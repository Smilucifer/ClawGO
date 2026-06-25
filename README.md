# ClawGO

> 本地优先的 AI 协作桌面应用 —— 把单人聊天、多智能体群聊和量化投研放进同一个工作台。

ClawGO 是一个 Windows 优先的桌面应用,基于 Tauri + SvelteKit + Rust 构建。它在本地运行,数据保存在你自己的机器上,围绕 Claude、Codex 等 AI CLI 提供从单人对话到多智能体协作、再到投资决策辅助的完整工作流。

## 致谢

本项目在 [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe) 的本地优先桌面架构与 UI 基础上继续演进,并吸收了 [TianLin0509/claude-session-hub](https://github.com/TianLin0509/claude-session-hub) 在群聊、备忘录、协作协议和多 CLI 工作流上的产品启发。感谢这两个项目对本项目方向的帮助。

## 它能做什么

ClawGO 把几条原本分散的工作流收拢到一个桌面端:

- **单人聊天** —— 稳定的 `/chat` 路径,接入官方 Claude / Codex CLI 以及多家兼容 API。
- **多智能体群聊** —— 让多个 AI 角色在同一个会话里协作、辩论、分工。
- **角色库与角色记忆** —— 复用的角色模板,并让角色从对话中持续学习。
- **全局备忘** —— 随手记录偏好、约定和待办,跨重启保留。
- **openInvest 投研助手** —— 多角色投资委员会、持仓盯盘、行情与事件追踪。

数据本地优先:会话、群聊、备忘、投资数据都保存在你的机器上,不依赖云端账户。

## 核心概念

- **Run** —— 最小执行单元,一次被持久化的 AI 会话。
- **GroupChat(群聊)** —— 建立在一个或多个 Run 之上的协作编排层。
- **AiCharacter(角色)** —— 可复用的角色模板,定义名称、类型、自定义指令和默认 provider / 模型。
- **Provider 身份 ≠ 执行身份** —— 界面上显示的 provider 不一定就是底层真正执行的 agent(详见下文 Provider 一节)。

## 功能一览

### 单人聊天

接入官方 Claude / Codex CLI 与多家兼容 API,支持模型热切换、工具调用可视化、用量统计、权限与确认流程。聊天会话持久化在本地,可随时恢复或停止。

### 群聊 / Roundtable / Driver-Copilot

群聊是多智能体协作的入口。你可以创建群聊、添加 Claude / Codex participant,并把 participant 关联到已有或新建的 Run。删除群聊不会删除对应的 Run。每个 participant 可关联一个角色模板,会话启动时注入对应的系统提示。

常用玩法:

- 把相关的多个 Run 聚合到一个群聊里统一查看。
- 在 Roundtable 时间线里向多个活跃 participant 一次性分发同一个问题。
- `@debate` —— 让 participant 基于上一轮公开回复互相比较观点。
- `@summary @name` —— 指定某个 participant 总结公开群聊历史。
- `@DisplayName message` —— 公开回合,只由被点名的 participant 回答。
- `/dm @Name message` —— 私有回合,内容不出现在公开时间线。
- **Auto-chain** —— 当一条回复中 `@提及` 了其他 participant,系统会自动链式调用(最多 3 跳)。
- **Driver / Copilot** —— 一个 Driver 通过 `/review` 向一个或多个 Copilot 请求只读审查。
- **Stepper 回放** —— 逐回合点击加载历史快照,叠加展示当时的 pane 内容。

### 角色库

角色(AiCharacter)是可复用的智能体模板,定义角色名称、类型、自定义指令、默认 provider 和模型。在 Settings → Characters 管理角色库,创建群聊时为每个 participant 关联一个角色。

- **Planner** —— 只读角色,可读取文件、搜索代码辅助规划,但不能修改文件系统或运行命令。
- **Executor** —— 严格按计划执行任务,不偏离计划内容。

### 计划机制

每个群聊可以关联一个计划(PlanArtifact):标题、任务清单、状态(草稿 / 进行中 / 已完成)和备注。计划面板以可交互的任务清单展示,支持任务状态循环(待办 → 进行中 → 完成 / 阻塞)、approve / complete 操作和备注编辑。计划上下文会自动注入,确保会话切换后新会话了解当前进度。

### 角色记忆系统

每个角色拥有独立的持久化记忆,从群聊对话中自动学习事实、经验、偏好、规则和关系,通过 SQLite FTS5 全文检索,在编排时把相关记忆注入系统提示。

- **自动提取** —— 回合完成后由 LLM 自动提取有价值的记忆,带 5 分钟 debounce + 每角色每天上限。普通 `/chat` 回合同样会触发提取。
- **全文搜索** —— 基于 SQLite FTS5 的全文检索,支持关键词匹配与相关性排序。
- **Review Queue** —— 自动提取的记忆先进入待审核队列,可审批或拒绝。
- **注入配置** —— 每个角色可独立配置检索数量(1–20)与相关性阈值(0.0–1.0)。

### 全局备忘(Memo)

应用内的全局备忘录,用来保存长期偏好、项目约定、临时上下文和待办。备忘面板从页面右上角剪贴板图标滑出,不打断当前工作流,也可通过 Command Palette 打开。支持添加、复制、删除条目,每条显示内容与时间戳,跨重启保留。

### openInvest 投研助手

`/invest` 下的一套相对独立的量化 / 组合辅助子系统,数据持久化在本地 SQLite。

- **投资委员会** —— 多角色 LLM 辩论(宏观 / 量化 / 风控 / CIO),分两轮对每只标的给出结构化裁决,支持研究与实盘双模式。每个角色通过 Claude CLI 执行,provider / 模型来自委员会调参与凭据配置。
- **行情与数据** —— 接入 Tushare、腾讯实时行情、AkShare(经 Python 桥)、全球指数等多个数据源,内置统一的数据源编排层(按指标类别自动选源、判空降级、记录命中源)。支持可选的 miniQMT 本地行情源(开启后 K线/实时报价优先从本地获取,无限频)。内置 RSI/MA、市场 regime、宏观指标缓存(上证指数、涨跌家数、北向资金、融资余额等 17 项)等技术分析。
- **持仓盯盘** —— 持仓与观察列表、当日盈亏(现金流调整盯市法,正确覆盖新买/加仓/减仓/清仓)、清仓当日保持持仓 + 次日自动转观察等。支持可配置的手续费方案(佣金/印花税/过户费),佣金自动计入盈亏。
- **定时任务** —— 盈亏快照、事件扫描、每日报告、定期"dreaming"反思。
- **事件追踪** —— 高频 Jin10 事件流采集 + LLM 归一化与扫描。

### Provider 与认证

当前主力 provider:**Claude、Codex、DeepSeek、GLM、QWEN、KIMI、MiMo Pro**,以及任意数量的 **Custom Provider**。

- **官方 CLI(订阅)** —— Claude、Codex,使用官方 CLI 认证,原生执行。
- **Claude 兼容 API** —— DeepSeek、GLM、QWEN、KIMI、MiMo Pro 在界面上是一等 provider,但执行层复用 Claude Code 兼容会话,通过 `platform_id` 注入对应的 API 配置。
- **Custom Provider** —— 在 Settings → Connection 填写 Name、Base URL、API Key、Model 即可,使用与内置 API provider 相同的启动路径。

每次启动会从最新设置生成一份 per-session 临时配置,并自动合并你本地 `~/.claude/settings.json` 中的 hooks、插件和 MCP 服务器,确保自定义配置不会丢失。模型下拉显示分级标注(Opus / Sonnet / Haiku),支持热切换。

### MCP 服务器管理

在 Extensions 页面管理 MCP 服务器,支持 5 种来源:本地配置、用户级 `~/.claude.json`、用户级 `~/.claude/settings.json`、项目级 `.mcp.json`,以及 ClawGO 托管的服务器。托管服务器会自动注入到每次会话,并与你已有的 MCP 服务器合并,不会覆盖同名的本地或项目级配置。

### Windows 原生工具链支持

在 Windows 上从普通桌面窗口启动应用时,Claude / Codex 子进程通常拿不到 Visual Studio Developer Prompt 里的 `cl`、`link`、Windows SDK 等环境。ClawGO 会在明确需要原生工具链的项目中,自动为本地 CLI 子进程补充 MSVC 开发环境。Codex 若通过 npm `.cmd` shim 安装,会直接以 `node.exe + CLI js` 方式启动,避免对话时闪出临时 `cmd` 窗口。

模式可在 Settings 中切换:

- `auto`(默认)—— 仅在保守的原生项目信号下启用。
- `always` —— 强制为本地子进程启用。
- `off` —— 关闭自动注入。

注入成功时聊天状态栏会显示 `MSVC` 徽标(群聊会话出于隔离考虑禁用注入,不显示徽标)。

## 安装与开发

环境要求:Node.js ≥ 20、Rust 工具链、Windows + Visual Studio C++ build tools(原生工具链支持)。

```bash
# 安装依赖
npm install
npx svelte-kit sync

# 前端开发服务器(端口 1420)
npm run dev

# 桌面端运行
npm run tauri dev

# 打包(生成 .exe / .msi 安装包,产物在 src-tauri/target/release/bundle/)
npm run tauri build
```

版本号统一更新:

```bash
npm run release <version|patch|minor|major>
```

质量验证:

```bash
npm run verify        # lint + format + i18n + test + build + Rust check

# 单项检查
npm run lint
npm run build
npm run check
npm run i18n:check
npm run rust:check
npm test
```

## 当前限制

- 上下文管理为 MVP:会话切换检测已实现,但自动 spawn 新会话并注入 bootstrap context 的完整流程仍为 stub;bootstrap context 使用模板截断而非 LLM 摘要,token 估算为近似值。
- Driver / Copilot 为 MVP:Copilot 只读行为通过 review prompt 约束,危险操作审批与硬权限限制为后续工作。
- 余额查询仅覆盖部分 provider(DeepSeek、MiMo / PackyAPI),其余 provider 暂无余额检查。
- 群聊锁映射暂无驱逐机制,长期运行可能泄漏。
- 本机 Rust 单元测试受 VCRUNTIME140.dll 版本不匹配影响,通常用 `cargo check` 替代 `cargo test`。

## 许可证

本项目以 Apache-2.0 许可证发布。整合上游代码或迁移设计时,应保留必要的 attribution 与 license 说明,并遵守上游项目及其依赖的许可证要求。
