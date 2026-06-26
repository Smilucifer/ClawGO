# 设计：修复 resume 误判 failed + 聊天页 Claude 订阅 usage 显示

日期：2026-06-26
状态：待评审

本设计涵盖两个独立但同批推进的改动：

1. 修复 resume/继续会话时出现 `Process exited with code Some(1073807364)` 并把会话误标为 `failed` 的 bug。
2. 在聊天页 top-bar 显示官方 Claude 订阅的额度（5h / 周窗口）使用情况。

两者互不依赖，可分别实现与回归。

---

## 第一部分：修复 resume 误判 failed

### 现象

actor 执行路径（Claude / Claude-兼容 provider）下，**resume 或继续历史会话时**偶发：

```
Process exited with code Some(1073807364)
```

会话被标记为 `failed`。`1073807364 = 0x40010004 = DBG_TERMINATE_PROCESS`，即子进程被外部 `TerminateProcess` 强制终止（不是崩溃）。

### 根因分析

报错来自 `src-tauri/src/agent/session_actor.rs` 的 `handle_eof()`（约 2178-2219 行）：

- claude CLI 在 actor 路径是**长生命周期进程**（`--input-format stream-json`，多回合复用同一进程），空闲时挂起等待 stdin。
- `handle_eof` 在「未收到 `result` 事件（`got_result_event == false`）+ 非取消（`self.cancel.is_cancelled() == false`）+ 退出码非 0」时一律判为 `failed`，并把原始退出码塞进报错信息（`format!("Process exited with code {:?}", exit_code)`，`{:?}` 格式化 `Option` 才会打印 `Some(...)`）。
- 注意：成功的 `result` 事件**不会**置 `got_result_event`（只有 error result 才置，见 `claude_protocol.rs` 测试断言）。因此一个跑完成功回合、处于空闲的 claude 进程被终止时，EOF 路径天然落入「无 result 事件」分支。
- `handle_stop()`（session_actor.rs:1287-1300）在停止时**只 `child.kill()` + `wait()`，不设置任何“有意终止”标志**，也不取消 `self.cancel`（`self.cancel` 只在 app 退出时被取消）。
- 正常停止走 cmd 分支：`handle_stop` 后立即 `break`，不会触发 `handle_eof`。但 `tokio::select!` 的 cmd 分支与 stdout-EOF 分支是竞争关系；当 EOF 因外部终止先就绪时，`handle_eof` 先执行，此时无从判断这次终止是不是“我们自己/teardown 引起的”，于是误判 `failed`。
- resume 必然先 `stop_actor`（`commands/session.rs:826`）拆掉旧进程，所以该误判在 resume 时高发。

**确定性边界（诚实交代）：**
- **可确定修复的**：上述「无法区分有意终止 vs 真崩溃」的**误分类**。
- **未 100% 坐实的**：`0x40010004` 这个具体退出码的**外部终止来源**。它不是 tokio 自身 `kill()` 的码（tokio `kill()` 用退出码 1），说明有别的机制（疑似 Windows Job Object / 进程组清理）在 teardown 前后终止了该进程。精确来源需一次复现日志坐实，故本设计加入诊断日志以备捕获。

### 修复方案（两层防御 + 诊断）

**1. 有意终止标志（消除误判，确定有效）**
- 在 actor 结构体增加字段 `stopping: bool`（默认 `false`）。
- `handle_stop()` 在**最开头**（`child.kill()` 之前）置 `self.stopping = true`。
- `handle_eof()` 把“有意终止”判定从 `self.cancel.is_cancelled()` 扩展为 `self.cancel.is_cancelled() || self.stopping`：满足则状态归为 `stopped`，不再产生 `failed` 与裸退出码报错。

**2. 防御性识别 Windows 强制终止码**
- 新增 helper `is_windows_termination_code(code: i32) -> bool`，覆盖已知强制终止码：`0x40010004`（DBG_TERMINATE_PROCESS）、`0xC000013A`（STATUS_CONTROL_C_EXIT）等。
- `handle_eof` 中，即使未命中 `stopping` 标志，若退出码命中该 helper，则归为 `stopped/terminated`（携带一条更友好的说明），而非 `failed` + 裸码。

**3. 诊断日志**
- `handle_eof` 命中“非 result 事件终止”路径时打印一条结构化日志：`run_id`、`stopping`、`cancel.is_cancelled()`、`is_resume`、`exit_code`、最近一行 stderr。
- 目的：若底层确有“进程被异常终止”的更深问题，下次复现可直接定位来源。

### 受影响文件
- `src-tauri/src/agent/session_actor.rs`（字段、`handle_stop`、`handle_eof`、helper）。
- 可能新增/复用一处常量定义放退出码 helper。

### 行为变化
- resume/继续会话时不再弹 `Process exited with code Some(...)`、会话不再被无故标 `failed`，保持可继续使用。
- 真实的失败（claude 返回 error result、或真正崩溃码）仍正常标 `failed`，不被掩盖。

---

## 第二部分：聊天页 Claude 订阅 usage 显示

### 目标
在聊天页 top-bar 显示官方 Claude 订阅额度：5h 窗口利用率 + 周窗口利用率 + 各自 reset 时间 + 订阅类型（`subscriptionType`）+ `rateLimitTier`。**仅当当前会话 provider 为官方 Claude 订阅时显示**。

### 数据来源（后端）
- 读 `~/.claude/.credentials.json` 的 `claudeAiOauth.accessToken`，以 `Authorization: Bearer <token>` 调 Anthropic 的 OAuth usage 接口（即 `claude /usage` 背后调用的 endpoint）。
- **该 endpoint 路径与响应结构为非公开**：实现期先抓取一次真实响应核对后再固化 parser；本设计中 parser 字段标注「实现期核对」。
- **token 过期处理**：`accessToken` 带 `expiresAt`。过期或返回 401 时**不自行用 `refreshToken` 刷新**（避免破坏 CLI 登录态），而是优雅降级：返回带 `error`/`expired` 标记的结果。CLI 正常使用时会自行刷新 token，下个回合重新读文件即可恢复。
- 新增后端命令（建议 `src-tauri/src/commands/claude_usage.rs` 或并入 `balance.rs`），返回结构示意：

```
ClaudeSubscriptionUsage {
  five_hour:   { utilization: f64, resets_at: Option<String> },   // 字段名实现期核对
  weekly:      { utilization: f64, resets_at: Option<String> },
  subscription_type: Option<String>,
  rate_limit_tier:   Option<String>,
  fetched_at: String,
  error: Option<String>,   // 过期/网络失败等
}
```

### 前端展示
- 新增轻量 store：`src/lib/stores/claude-usage-store.svelte.ts`（持有最近一次结果、加载/失败状态、刷新方法）。
- 新增 top-bar 组件：紧凑徽标，例如 `5h 42% · 周 18%`，颜色随利用率分档（绿/黄/红）；点击或 hover 展开 popover，显示两条进度条、reset 倒计时、订阅类型与 tier。
- **显示条件**：当前聊天会话 provider 为官方 Claude 订阅才渲染（Claude-兼容、custom-\* 一律不渲染）。判定字段依据 §5 provider 身份，实现期对齐具体来源（session-store 中的 provider/platform 标识）。

### 刷新时机
- 进入 claude 会话时拉取一次打底。
- 之后**每个回合结束**（接 session-store 的 turn-idle/回合完成事件）刷新一次；空闲时不打接口。
- 刷新失败：静默保留上一次数据并标记 stale，不打断聊天。

### 受影响文件
- 后端：新增命令模块 + 注册到 IPC；`messages/en.json`、`messages/zh-CN.json` 补文案。
- 前端：新增 store + top-bar 组件；在聊天页 top-bar 接入；在回合结束处挂刷新 hook；`lib/api.ts` 加调用封装。

### 改动边界
- 不改 `/usage` 页、不改现有 `balance.rs` 三方余额逻辑。
- 新增：1 个后端命令 + 1 个 store + 1 个 top-bar 组件 + 1 处回合结束 hook + i18n 文案。

---

## 测试与验证
- **bug 修复**：为 `handle_eof` 的分类逻辑加单元测试——`stopping=true`/命中终止码 → `stopped`；真实 error/普通非 0 码 → 仍 `failed`。受 §11 Rust 测试运行时问题影响，至少保证 `cargo check` 通过；分类逻辑尽量抽成纯函数便于测。
- **usage**：后端 parser 对真实响应样本做单测；前端 store 状态机（加载/成功/失败/过期）单测；手动验证仅 claude 会话显示、回合结束刷新、过期降级。
- 全量 `npm run verify`（lint + fmt + i18n + tests + build + Rust checks）。

## 风险
- OAuth usage endpoint 非公开，可能随 CLI 升级变更——parser 做容错，失败时降级不影响聊天。
- bug 修复属防御性分类，理论上不会掩盖真实失败（error result 路径与普通非 0 码不变）；诊断日志确保深层问题可追踪。
