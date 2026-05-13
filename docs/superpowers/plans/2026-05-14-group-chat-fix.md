# 群聊三个问题的修复方案（v3 — Hub 对比后修订）

## 背景

用户反馈群聊体验有三个问题：
1. 点发送后消息不立即显示，要等 AI 响应后才一起刷新
2. 角色 prompt 在 Actor 路径中被嵌入每条用户消息，N 轮后累积 N 份
3. 群聊参与者之间看不到彼此的上下文

---

## 多路审查结论（Claude / DeepSeek / Packy / Xiaomi Plan）

**4 路一致：有保留地同意。** 根因分析准确，方向正确。

### 共识
1. Fix 1 方案正确且低风险，直接实施
2. Fix 2 方向正确，但需先验证 `--append-system-prompt` 在长生命周期 Actor session 中是否持久生效
3. Fix 3 的 Debate 模式不应注入共享上下文（已有 peer context，会重复）
4. Fix 3 改变了 Fanout 产品语义，需明确为 intentional 变更
5. 分阶段实施：Fix 1 + Fix 2 先行，Fix 3 独立推进

### 分歧
1. Fix 2 实现方式：`start_session_impl` 参数 override (提案) vs RunMeta 字段 (Packy) vs `initial_message` 注入 (DeepSeek)
2. Fix 3 Fanout 默认行为：默认开启 (提案) vs 默认关闭 (Packy)

---

## claude-session-hub 对比分析

Hub 的 roundtable 实现提供了有价值的参考：

### 上下文共享
- Hub 使用 **injection matrix**：每个 agent 收到其他 agent 的**上一轮**回复（排除自己）
- Hub 有外部 `timeline.md` 文件，agent 可自行 `Read` 获取更早历史
- Hub 有 3 层持久记忆（个体 + 待审 inbox + 共识 profile）
- ClawGO 的 Fix 3 方案（最近 3 轮摘要）覆盖更广但截断更重

### 角色 prompt 注入
- Hub 使用 `--append-system-prompt-file` 在 session spawn 时注入（与 ClawGO Fix 2 方案等价）
- Hub 有 4 层 prompt 架构（BASE_RULES + scene + covenant + slot bias）
- **验证了 spawn 时注入 system prompt 的可行性**

### 前端响应
- Hub 有完整的 optimistic UI（立即显示 + partial update + 生命周期管理）
- ClawGO Fix 1 的预写空 turn 方案更简单，与现有轮询兼容

### 修订
- Fix 3 改为**只注入上一轮**（而非 3 轮），参考 Hub 的 injection matrix 模式
- 减少截断，保留后续扩展 timeline.md 式按需读取的可能

---

## Fix 1: 发送后消息立即显示

**根因：** orchestrator 在 spawn 前不写入任何数据。

**方案：** 在 spawn 前保存 `responses` 为空的初始 turn。JSONL dedup 确保后续覆盖。

**改动文件：** `src-tauri/src/group_chat/orchestrator.rs`

**改动量：** ~15 行

---

## Fix 2: 角色 prompt 只注入一次

**根因：** `execute_actor_turn` 把角色 prompt 拼接到每条用户消息中。Pipe 路径正确。

**方案：** `start_session_impl` 参数 override。

1. `start_session_impl` 新增 `append_system_prompt_override: Option<String>`
2. 在 `adapter_settings` 构建后、CLI 启动前应用 override
3. `create_group_chat_participant_impl` 中解析 AiCharacter role prompt 并传入
4. `execute_actor_turn` 移除 prompt 拼接
5. `build_role_system_prompt` 设为 pub

**前置验证：** 确认 `--append-system-prompt` 在 session resume 后仍生效。

**改动文件：**
- `src-tauri/src/commands/session.rs`
- `src-tauri/src/commands/group_chat.rs`
- `src-tauri/src/group_chat/orchestrator.rs`
- `src-tauri/src/web_server/dispatch.rs`

**改动量：** ~30 行，4 文件

---

## Fix 3: 跨参与者上下文共享（v3 修订）

**根因：** Fanout/SingleTarget/Private 无历史上下文。

**方案（参考 Hub injection matrix）：** 新增 `build_last_turn_context` 函数，注入到 Fanout 和 SingleTarget。

- **只注入上一轮**其他参与者的回复（排除自己），与 Hub 的 injection matrix 一致
- 每条 response 截断 1500 字符（Hub 用 2000，但 Hub 是英文场景）
- **Debate 不注入**（已有 peer context）
- **Private 不注入**（有意隔离）
- Fanout 指令改为"参考上下文但独立回答"
- 后续可扩展 timeline.md 式的按需读取路径

**改动文件：** `src-tauri/src/group_chat/orchestrator.rs`

**改动量：** ~50 行

---

## 待决问题

1. Fanout 的 shared context 默认开启还是关闭？
2. `--append-system-prompt` 持久化验证

---

## 验证

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml -- group_chat --nocapture
npm run check
```
