# Claude Session Hub Remaster / Claude Session Hub 重制版

> English follows Chinese in each section.
> 每个章节均先中文、后英文。

## 致谢 / Acknowledgements

本项目基于 [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe) 的本地优先桌面架构与 UI 基础继续演进，并吸收了 [TianLin0509/claude-session-hub](https://github.com/TianLin0509/claude-session-hub) 在会议室、备忘录、协作协议和多 CLI 工作流上的产品启发。感谢这两个项目对本 remaster 方向的帮助。

This project evolves from the local-first desktop architecture and UI foundation of [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe), while drawing product inspiration from [TianLin0509/claude-session-hub](https://github.com/TianLin0509/claude-session-hub), especially around rooms, memos, collaboration protocols, and multi-CLI workflows. Many thanks to both projects for shaping this remaster.

## 项目定位 / Project Positioning

Claude Session Hub Remaster 的目标不是重写 Claw GO，也不是直接搬运 Claude Session Hub。它以 Claw GO 当前的 Tauri + Svelte + Rust 架构为主体，逐步叠加 Claude Session Hub 中有差异化价值的协作能力。

Claude Session Hub Remaster is not a rewrite of Claw GO and not a direct port of Claude Session Hub. It keeps Claw GO's current Tauri + Svelte + Rust architecture as the foundation, then incrementally adds the collaboration ideas that make Claude Session Hub distinctive.

当前架构原则：

- `Run` 继续作为最小执行单元。
- `GroupChat`（原 Room）作为协作编排层，建立在 `Run` 之上。
- `AiCharacter` 作为可复用的角色模板，支持 role_type / role_instruction / 默认 provider 配置。
- Memo、Group Chat、Roundtable、Character Library、Character Memory System（SQLite FTS5 全文搜索 + LLM 自动提取 + 记忆注入）、上下文管理已全部落地。
- 现有 `/chat` 路径保持稳定。

Current architecture principles:

- `Run` remains the smallest execution unit.
- `GroupChat` (formerly Room) serves as the orchestration layer above `Run`.
- `AiCharacter` is a reusable persona template with role_type, role_instruction, and default provider config.
- Memo, Group Chat, Roundtable, Character Library, Character Memory System (SQLite FTS5 full-text search + LLM auto-extraction + memory injection), and Context Management are fully implemented.
- The existing `/chat` path remains stable.

## 当前功能 / Current Features

### Memo

你可以在应用内使用快速备忘（全局备忘录），用来保存长期偏好、项目约定、临时上下文和后续待办。备忘面板从页面右上角剪贴板图标滑出，不干扰当前工作流。也可以通过 Command Palette 打开。

You can use Quick Memo (global memos) inside the app for preferences, project conventions, temporary context, and follow-up notes. The memo panel slides out from a clipboard-icon button in the top-right corner of every page, staying out of your way. Also accessible via Command Palette.

支持：

- 添加、复制、删除备忘条目。
- 每条备忘显示内容与时间戳。
- 重启后保留备忘内容。
- 非聊天页顶部工具栏和聊天页 SessionStatusBar 均有入口。

Supported:

- Add, copy, and delete memo entries.
- Each memo shows text and timestamp.
- Keep memo content after restart.
- Accessible from the top bar on non-chat pages and SessionStatusBar on the chat page.

### Group Chat, Roundtable, and Driver/Copilot

Group Chat（群聊）是多智能体协作的入口。你可以创建群聊、添加 Claude / Codex participant，并把 participant 关联到已有或新建的 Run。群聊删除不会删除对应 Run。每个 participant 可关联一个 AiCharacter 角色模板，在会话启动时注入角色系统提示。participant 会话在侧边栏中以虚拟"群聊"文件夹单独分组，与项目会话分离。

Group Chat is the entry point for multi-agent collaboration. You can create a group chat, add a Claude / Codex participant, and link that participant to an existing or newly created Run. Deleting a group chat does not delete the linked Run. Each participant can link to an AiCharacter persona template, injecting a role-based system prompt at session launch. Participant sessions are grouped in a virtual "Group Chats" folder in the sidebar, separate from project sessions.

当前适合用来：

- 把相关 Run 聚合到一个群聊中查看。
- 在 Roundtable 时间线里向多个活跃 participant 分发同一个问题。
- 使用 `@debate` 让 participant 基于上一轮公开回复互相比较观点。
- 使用 `@summary @name` 指定一个 participant 总结公开群聊历史。
- 使用 `@DisplayName message`（SingleTarget）发送公开回合，仅由被点名的 participant 回答。
- 使用 `/dm @Name message` 发送私有回合，私有内容不会出现在公开时间线中。
- 通过 Stepper 逐轮回放——点击任意回合加载快照，紫色横幅 + pane 内容叠加显示。
- **Auto-chain**：SingleTarget 回复中 `@提及` 其他 participant 时自动链式调用（最多 3 跳）。
- 创建 Driver 群聊，让一个 Driver 通过 `/review` 向一个或多个 Copilot 请求只读审查。
- `.arena` 文件是本地运行上下文镜像（Room 时代遗留），可能包含 run references、memo 和最近的公开 preview；不要把它当成对外分享材料。

Currently useful for:

- Grouping related Runs in a group chat.
- Sending one prompt to multiple active participants in a Roundtable timeline.
- Using `@debate` to ask participants to compare positions based on previous public replies.
- Using `@summary @name` to ask one participant to summarize the public group chat history.
- Using `@DisplayName message` (SingleTarget) to send a public turn answered only by the named participant.
- Using `/dm @Name message` for private turns that do not appear in the public timeline.
- Turn-by-turn Stepper for group chat replay — click any turn to load its snapshot with purple banner and pane content overlay.
- **Auto-chain**: when a SingleTarget response `@mentions` another participant, the system automatically chains to them (up to 3 hops).
- Creating a Driver group chat where one Driver asks one or more Copilots for read-only review with `/review`.
- `.arena` files are local runtime context mirrors (legacy from Room era) and may include run references, memo text, and recent public previews; do not treat them as shareable artifacts.

### Character Library (角色库)

AiCharacter 是可复用的智能体角色模板，定义了角色名称、类型（planner / executor）、自定义指令、默认 provider 和模型。你可以在 Settings → Characters 页面管理角色库。创建群聊时，每个 participant 可关联一个角色，系统会在会话启动时通过 `--append-system-prompt` 注入角色约束。

AiCharacter is a reusable agent persona template defining a label, role type (planner / executor), custom instruction, default provider, and model. You can manage the character library from Settings → Characters. When creating a group chat, each participant can link to a character, and the system injects role constraints via `--append-system-prompt` at session launch.

- **Planner**：只读角色，可以读取文件和搜索代码辅助规划，但不可执行修改文件系统或运行命令的工具。
- **Executor**：严格按计划执行任务，不可偏离计划内容。

- **Planner**: read-only role — can read files and search code to assist planning, but cannot execute tools that modify the filesystem or run commands.
- **Executor**: strictly executes tasks according to the plan, cannot deviate from plan content.

### Plan Mechanism (计划机制)

每个群聊可以关联一个计划（PlanArtifact），包含标题、任务清单、状态（draft / active / completed）和用户备注。计划面板在群聊界面中展示为可交互的任务清单，支持任务状态循环（Todo → InProgress → Done / Blocked）、approve / complete 操作和用户备注编辑。计划上下文会自动注入到 Bootstrap Context 中，确保 session handoff 后新会话了解当前进度。

Each group chat can link to a plan (PlanArtifact) containing a title, task checklist, status (draft / active / completed), and user notes. The plan panel is displayed as an interactive task checklist in the group chat UI, supporting task status cycling (Todo → InProgress → Done / Blocked), approve / complete operations, and user notes editing. Plan context is automatically injected into the Bootstrap Context, ensuring new sessions understand current progress after session handoff.

### Providers and CLI Authentication

### Character Memory System (角色记忆系统)

每个 AiCharacter 拥有独立的持久化记忆系统。角色从群聊对话中自动学习事实、经验、偏好、规则和关系，通过 SQLite FTS5 全文搜索进行检索，在群聊编排时注入相关记忆到系统提示中。

Each AiCharacter has an independent persistent memory system. Characters automatically learn facts, experiences, preferences, rules, and relationships from group chat conversations, retrieve relevant memories via SQLite FTS5 full-text search, and inject them into system prompts during orchestration.

支持：

- **自动提取**：群聊回合完成后，LLM 自动从对话中提取有价值的记忆，5 分钟 debounce + 每角色每天 10 次上限。
- **全文搜索**：基于 SQLite FTS5 的用户级全文检索，支持关键词匹配和相关性排序。
- **Review Queue**：自动提取的记忆先进入待审核队列，用户可审批或拒绝。
- **Injection Config**：每个角色可独立配置 `max_retrieval_count`(1-20)、`relevance_threshold`(0.0-1.0)。
- **数据生命周期**：保留期策略、启动时自动维护。

Supported:

- **Auto-extraction**: After group chat turns, LLM automatically extracts valuable memories with 5-min debounce + 10/day per character cap.
- **Full-text search**: SQLite FTS5-based user-centric full-text retrieval with keyword matching and relevance ranking.
- **Review queue**: Auto-extracted memories enter a pending review queue; users can approve or reject.
- **Injection config**: Per-character `max_retrieval_count`(1-20), `relevance_threshold`(0.0-1.0).
- **Data lifecycle**: Retention policy, startup maintenance.

当前主力 provider：Claude、Codex、DeepSeek、GLM、QWEN、KIMI、Xiaomi Plan、Xiaomi API。Codex 通过 PTY 原生 CLI 执行，默认带 `--dangerously-bypass-approvals-and-sandbox`；DeepSeek、GLM、QWEN、KIMI、Xiaomi 作为一等 provider 显示，执行层复用 Claude Code compatible session，并通过 `platform_id` 注入对应 API 配置。你还可以在设置页添加任意数量的 **Custom Provider**——填写 Name、Base URL、API Key 和 Model 即可，使用与内置 API provider 相同的启动路径。

The current primary providers are: Claude, Codex, DeepSeek, GLM, QWEN, KIMI, Xiaomi Plan, and Xiaomi API. Codex uses PTY-based native CLI execution and defaults to `--dangerously-bypass-approvals-and-sandbox`. DeepSeek, GLM, QWEN, KIMI, and Xiaomi are first-class providers in the UI, but execute through Claude Code compatible sessions with provider configuration injected by `platform_id`. You can also add any number of **Custom Providers** from the settings page — just fill in Name, Base URL, API Key, and Model.

设置页提供每个 provider 独立的模型和认证配置。Claude、Codex 使用官方 CLI 认证；DeepSeek、GLM、QWEN、KIMI、Xiaomi 及 Custom Provider 支持 API key / base URL / model 动态配置，每次启动从最新设置生成 per-session 临时配置。临时配置会自动合并你本地 `~/.claude/settings.json` 中的 hooks、插件和 MCP 服务器，确保自定义配置不会丢失。Codex 使用 PTY 原生 CLI 执行路径，通过 transcript 文件监听完成判定而非进程退出语义。

The settings page provides independent model and auth configuration per provider. Claude and Codex use official CLI authentication. DeepSeek, GLM, QWEN, KIMI, Xiaomi, and Custom Providers support dynamic API key / base URL / model configuration, generating a per-session temp config from the latest settings on each launch. The temp config automatically merges your local `~/.claude/settings.json` hooks, plugins, and MCP servers, so your custom configuration is never lost. Codex uses a PTY-based native CLI execution path with transcript-based completion detection rather than process-exit semantics.

Claude-compatible API profiles can still be represented under `user.cc_agent_profiles` in `~/.claw-go/settings.json` for backward compatibility. API providers should keep provider identity separate from the Claude execution agent:

为了向后兼容，Claude-compatible API profile 仍可保留在 `~/.claw-go/settings.json` 的 `user.cc_agent_profiles` 下。API provider 应保持 provider identity 和 Claude execution agent 分离：

```json
{
  "user": {
    "cc_agent_profiles": [
      {
        "id": "qwen-via-claude",
        "label": "QWEN via Claude",
        "agent": "claude",
        "platform_id": "qwen",
        "model": "qwen3-coder-plus",
        "prompt": "You are the QWEN seat in this roundtable.",
        "role": "participant",
        "enabled": true
      },
      {
        "id": "kimi-via-claude",
        "label": "KIMI via Claude",
        "agent": "claude",
        "platform_id": "kimi",
        "model": "kimi2",
        "prompt": "You are the KIMI seat in this roundtable.",
        "role": "researcher",
        "enabled": true
      },
      {
        "id": "native-codex",
        "label": "Native Codex",
        "agent": "codex",
        "model": "gpt-5.5",
        "prompt": "You are the Codex CLI seat in this roundtable.",
        "role": "participant",
        "enabled": true
      },
      {
        "id": "xiaomi-plan-via-claude",
        "label": "Xiaomi Plan via Claude",
        "agent": "claude",
        "platform_id": "mimo-plan",
        "model": "mimo-v2.5",
        "prompt": "You are the Xiaomi Plan seat in this roundtable.",
        "role": "participant",
        "enabled": true
      }
    ]
  }
}
```

`agent` 可为 `claude` 或 `codex`，省略时默认 `claude`。API provider（DeepSeek/GLM/QWEN/KIMI/Xiaomi）通过 `agent: "claude"` 加 `platform_id` 表达。`model` 只应在用户明确选择模型或 API provider 需要模型时作为 per-run snapshot；不要把官方 CLI provider 的展示默认值强塞进启动参数。

`agent` can be `claude` or `codex`; omitted values default to `claude`. API providers (DeepSeek/GLM/QWEN/KIMI/Xiaomi) are represented as `agent: "claude"` plus `platform_id`. `model` should become a per-run snapshot only when explicitly selected by the user or required by an API provider; do not inject display defaults into official CLI launches.

### MCP Server Management

你可以在 Extensions 页面管理 MCP 服务器。Claw GO 支持 5 种来源：本地配置、用户级 `~/.claude.json`、用户级 `~/.claude/settings.json`、项目级 `.mcp.json`、以及通过 Claw GO 托管的服务器。托管服务器会自动注入到每次会话中，并与你本地 `~/.claude/settings.json` 中已有的 MCP 服务器合并，不会覆盖同名的本地或项目级配置。

You can manage MCP servers from the Extensions page. Claw GO supports 5 sources: local config, user-level `~/.claude.json`, user-level `~/.claude/settings.json`, project-level `.mcp.json`, and Claw GO managed servers. Managed servers are automatically injected into every session and merged with your existing MCP servers from `~/.claude/settings.json`, without overriding same-name local or project-level entries.

### Windows Native Toolchain Support

在 Windows 上，如果你从普通桌面窗口启动应用，Claude / Codex 子进程通常拿不到 Visual Studio Developer Prompt 里的 `cl`、`link`、Windows SDK 等环境。当前版本会在明确需要 native toolchain 的项目中，自动为本地 CLI 子进程补充 MSVC developer environment。Codex 如果通过 npm `.cmd` shim 安装，应用会在 Windows 上直接以 `node.exe + CLI js` 方式启动，避免对话时闪出临时 `cmd` 窗口。

On Windows, when the app is launched from the normal desktop environment, Claude / Codex child processes usually do not inherit the `cl`, `link`, Windows SDK, and related variables from a Visual Studio Developer Prompt. This version can automatically add an MSVC developer environment for local CLI child processes when the current project clearly needs native tooling. When Codex is installed through npm `.cmd` shims, the app launches `node.exe + the CLI js` directly on Windows to avoid transient `cmd` windows during chat.

**Auto-detection markers**（`auto` 模式下的项目根目录检测）：

- Tauri 项目：`src-tauri/` 目录、`binding.gyp`
- Node native 依赖：`package.json` 中含 `sharp`、`node-sass`、`bcrypt` 等
- Rust native 项目：`Cargo.toml` 含 `cc` build-dependency、`build.rs` 存在
- CMake / vcpkg：`CMakeLists.txt`、`vcpkg.json`
- Visual Studio / Qt：`*.sln`、`*.vcxproj`、`*.pro`、`*.pri`

检测仅限项目根目录，不递归子目录。若项目在子目录中有 `.sln` 等文件，请切换到 `always` 模式。

**Chat / Group Chat 策略分离**：Chat 会话允许 MSVC 注入（按模式判断）；Group Chat participant 会话由后端策略强制禁用注入（`MsvcPolicy::Disabled`），确保隔离。

**MSVC 状态徽标**：Chat 状态栏会在确认注入成功时显示 `MSVC` 徽标（amber 样式），鼠标悬停可查看 tooltip。Group Chat 会话不显示此徽标。

模式：

- `auto`：默认，仅在保守 native project 信号下启用。
- `always`：强制为本地子进程启用。
- `off`：关闭自动注入。

Modes:

- `auto`: default, enabled only for conservative native-project signals.
- `always`: force injection for local child processes.
- `off`: disable automatic injection.

你可以在 Settings 中切换模式，并查看当前项目的 MSVC 环境状态。状态提示会说明是否已注入、无需注入、已关闭或需要安装 Visual Studio C++ build tools；不会展示完整环境变量。

You can switch the mode in Settings and view the MSVC environment status for the current project. The status tells you whether injection is active, unnecessary, disabled, or blocked by missing Visual Studio C++ build tools; it does not expose the full environment values.

## 当前限制 / Current Limitations

- Context Management MVP：session handoff 检测已实现，但自动 spawn 新 session 并注入 bootstrap context 的完整流程仍为 stub（当前仅重置 session 计数）。
- Bootstrap context 使用模板截断而非 LLM 摘要，token 估算为近似值（~4 chars/token）。
- Lock map（`GROUP_CHAT_LOCKS`、`GROUP_CHAT_ORCHESTRATION_LOCKS`）无驱逐机制，长期运行可能泄漏。
- Driver/Copilot 目前是 MVP：Copilot 只读行为通过 review prompt 约束，危险操作审批和硬权限限制仍在后续阶段。
- 余额查询仅覆盖 DeepSeek（API key 认证）和 MiMo（cookie 认证），其余 provider 暂无余额检查。
- Rust 单元测试受限于本地 VCRUNTIME140.dll 版本不匹配（VS 18 BuildTools vs System32），需用 `cargo check` 替代 `cargo test`。
- Lock map 驱逐、LLM 摘要、Driver/Copilot 硬权限——这些在后续版本改进。

Current limitations:

- Context Management MVP: session handoff detection is implemented, but the full flow of auto-spawning a new session with bootstrap context injection is still a stub (currently only resets session count).
- Bootstrap context uses template truncation rather than LLM summarization; token estimation is approximate (~4 chars/token).
- Lock maps (`GROUP_CHAT_LOCKS`, `GROUP_CHAT_ORCHESTRATION_LOCKS`) have no eviction mechanism and may leak over long-running sessions.
- Driver/Copilot is currently an MVP: copilot read-only behavior is guided by the review prompt, while dangerous-operation review and hard permission enforcement remain later work.
- Balance queries only cover DeepSeek (API key auth) and MiMo (cookie auth); other providers lack balance checking.
- Rust unit tests are blocked by a local VCRUNTIME140.dll version mismatch (VS 18 BuildTools vs System32); use `cargo check` instead of `cargo test`.
- Lock map eviction, LLM summarization, Driver/Copilot hard permissions — planned for future releases.

## 开发 / Development

当前版本：**v5.2.5** · Current version: **v5.2.5**

```bash
npm install
npx svelte-kit sync
npm run dev
```

桌面端运行：

```bash
npm run tauri dev
```

打包（生成 `.exe` / `.msi` 安装包）：

```bash
npm run tauri build
```

产物路径：`src-tauri/target/release/bundle/`

版本号统一更新：

```bash
npm run release <version|patch|minor|major>
```

全量验证（lint + format + i18n + test + build + Rust check）：

```bash
npm run verify
```

单点验证：

```bash
npm run lint
npm run build
npm run rust:check
npm run i18n:check
npm test -- src/lib/stores/memo-store.test.ts
cargo test --manifest-path src-tauri/Cargo.toml storage::memos::tests:: -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::memos::tests:: -- --nocapture
```

Desktop run:

```bash
npm run tauri dev
```

Packaging (produces `.exe` / `.msi` installers):

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/`

Version bumping across all config files:

```bash
npm run release <version|patch|minor|major>
```

Full verification (lint + format + i18n + test + build + Rust check):

```bash
npm run verify
```

Common checks:

```bash
npm run lint
npm run build
npm run rust:check
npm run i18n:check
npm test -- src/lib/stores/memo-store.test.ts
cargo test --manifest-path src-tauri/Cargo.toml storage::memos::tests:: -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::memos::tests:: -- --nocapture
```

## 许可证 / License

本 remaster 仍应遵守上游项目及其依赖的许可证要求。合并上游代码或迁移 Claude Session Hub 设计时，应保留必要的 attribution 与 license 说明。

This remaster should continue to respect the license requirements of upstream projects and dependencies. When integrating upstream code or migrating Claude Session Hub ideas, keep the required attribution and license notes.
