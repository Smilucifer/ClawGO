# Changelog / 更新日志

## Phase 9.x (2026-05-09)

- Room adapter timeout 重构：固定 5 分钟 `max_polls` 改为活动感知双层超时（10 分钟不活跃 + 30 分钟硬截止）
- `RunMeta.active_at` 字段：EventWriter 节流写入（1s 间隔），用于检测 run 是否仍在活跃
- `events.rs` lock scoping 改进：per-run 锁在调用 `update_active_at_throttled` 前释放，避免潜在死锁
- `cancel_room_turn` Tauri 命令：遍历 room participants 停止活跃 run，过滤非 Running 状态
- 前端 Cancel 按钮：turn 进行中时替换 Send 按钮，`cancelGeneration` 防止竞态
- 前端长时间运行警告：运行超过 5 分钟显示 amber 标签
- 前端最近活动显示：使用 `active_at` 优先于 event-derived `last_activity_at`
- `get_run()` 修复：SessionActor 运行的 `last_activity_at` 不再为 `undefined`
- Adapter 测试补充：`with_deadlines()` + 硬截止超时测试 + 不活跃超时测试
- Adapter I/O 优化：每次循环只读一次 `meta.json`，移除死代码 `read_outcome`

## Phase 9 (2026-05-08)

- History 页面重写：从 OpenCovibe runs 切换为直接读取 CC 原生会话（`~/.claude/projects/`）
- 过滤掉 `hasSubagents: true` 的子代理会话
- 简化 UI：仅显示 prompt、时间、项目路径、模型 badge、继续/导入按钮
- 支持文本搜索 + 项目 pill 过滤
- 已导入会话跳过重复导入，直接跳转
- 清理 ~30 条无用 i18n keys，新增 10 条 CC 历史相关 keys
- AskUserQuestion / elicitation 交互按钮显式设为 `type="button"`，避免多问题权限卡片重复提交

## Phase 8.x (2026-05-08)

- 聊天侧边栏预览修复：`summarize_events()` 改为反向扫描，显示最新消息而非最早消息
- 版本更新检查 GitHub 地址修正为 fork 仓库
- Provider 切换时自动更新默认模型，修复旧值残留导致的错误
- 新建圆桌会议室后显示可关闭的命令速查横幅（@debate、@summary、/dm、@Name）

## Phase 8 (2026-05-08)

- Gemini 彻底移除（~54 文件，前后端 + 测试 + 文档）
- Stepper mini-map 替换 History strip，支持逐轮回放与快照加载
- `@DisplayName message` SingleTarget 公开点名（仅被点名者回答）
- `/dm @Name message` 保留私有回合
- Room sidebar 虚拟"会议室"文件夹分组
- Roundtable seat prompt 英文证据约束
- Context events 跨 session 类型验证
- Code review 修复：snapshot 渲染、activeSnapshot 复位、Private handler 歧义检测、i18n、guard

## Phase 7.y (2026-05-07)

- Room 删除时停止 participant 并软删除 runs
- Roundtable 增量回合推送（JSONL 去重 + 1500ms 前端轮询）
- 右键"移除会话"上下文菜单（含 force-stop）
- Participant 状态本地化（pending→Starting..., running→Thinking...）
- Seat label 修改自动同步 prompt

## Phase 7.x (2026-05-07)

- Provider 配置完全动态化（从设置页读取而非硬编码模型/URL）
- Per-session 临时配置 JSON（`--settings session-{run_id}.json`）
- MiMo Pro provider 新增
- MiMo 余额/用量检查器（cookie 认证，双 API，琥珀色主题卡片）

## Phase 7 (2026-05-06)

- Codex PTY 原生 CLI 适配器
- Provider 设置页动态化
- Roundtable 三栏布局重设计
- 全局备忘面板重构

## Phase 6

- Driver MCP

## Phase 5.5

- Native CLI chat parity

## Phase 5

- Capability matrix

## Phase 4.5

- Research follow-up

## Phase 4

- Driver/Copilot

## Phase 3

- Roundtable implementation

## Phase 1

- Memo implementation
