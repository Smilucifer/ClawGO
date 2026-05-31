# Changelog / 更新日志

## Phase 10+ (2026-05-31)

### v4.0.0+ — openInvest 修复任务 + 代码审查

**P0 Bug 修复（Wave 1, 4 项）:**
1. 记忆管理页面切入即卡死（`$effect` 无限循环）
2. Tushare 搜索 null 解析（`stock_basic` 返回空 items）
3. LLM 配置"暂无配置"（`loadConfig` 初始化失败）
4. 立即扫描无反应（Event Watch scan 错误处理）

**Demo HTML（Wave 2, 3 项）:**
5. Pipeline Flow 动画 demo
6. Committee Roles 两栏布局 demo
7. Dashboard 持仓管理 demo

**P1 大改（Wave 3, 6 项）:**
8. Dashboard 持仓 — 新增"加入观望"操作（`addToWatch` + `TradeDialog` add_watch 模式）
9. Profile 从 Settings 迁移到 invest Dashboard 底部（`UserProfileSection` 组件）
10. 侧边栏 Settings 移至最后（index 8）
11. 直播"运行全部持仓"按钮 + "包含 WATCH" checkbox
12. Replay 下拉选持仓 + 试运行不留档
13. 角色配置 Tab 重写 — 两栏布局 + 4 角色卡片（QUANT/RISK 双轮 prompt）

**P2/P3（Wave 4, 5 项）:**
14. CommitteeLive 多资产并发总览表（symbol/状态/进度/裁决）
15. Dashboard 宏观快照 + 最新裁决摘要卡片
16. Memory Extraction i18n + 中文化（9 处硬编码中文）
17. /memory 默认选中 MEMORY.md
18. 文件路径可点击链接（`FilePathLinks` 组件）

**代码审查修复（15 项）:**
- **CRITICAL**: `add_watch` 搜索 UI 门控修复（功能完全不可用）
- **HIGH**: ProviderConfigPanel `$effect` 无限重试守卫、`addToWatch` try/finally 部分失败保护
- **MEDIUM**: holdHoldings 重复检查、状态派生一致性、wealth 角色恢复、Replay 手动输入、管线步数常量、allHoldings 去重
- **LOW**: Settings profile 深链接重定向、HoldingsTable getPnlPct 缓存、Tushare 错误信息保留

## Phase 10+ (2026-05-28)

### v3.0.0 — 记忆系统重构: 用户中心 + SQLite FTS5

**架构变更（Breaking）:**
- 存储层从 per-character LanceDB + JSONL 迁移至用户中心 SQLite FTS5
- 移除 LanceDB、petgraph、Embedding API 全部依赖
- `MemoryNode.character_id` 字段废弃（保留空字符串兼容）
- `EmbeddingConfig` 保留为 `embedding_config`（仅用 chat 字段做 LLM 提取）

**记忆提取:**
- LLM chat completions 提取对话中的用户信息（fact/preference/skill/feedback）
- FTS5 全文去重（`find_duplicates` Jaccard 0.8 阈值）
- 每日提取上限 20→50，5 分钟 debounce
- `get_extraction_config()` 简化为直接读 `embedding_config`

**记忆检索与注入:**
- SQLite FTS5 BM25 全文检索 + 标签匹配混合搜索
- `search_hybrid` 合并为单次 `with_conn` 调用（消除 TOCTOU 竞态）
- FTS5 查询净化（`sanitize_fts_query` 防止操作符注入）
- `inject_memories_into_prompt` 统一注入函数（群聊 + 私聊共用）
- 接入 `MemoryConfig.max_retrieval_count` 和 `relevance_threshold`
- `count_memories` SQL 注入修复（参数化查询）

**Dream 循环:**
- `memory_dream.rs`: 快照、合并、置信度衰减
- `text_similarity` 从字节级 trigram Jaccard 改为词级 Jaccard（CJK 兼容）
- `list_archived_memories` + `restore_archived_memory` 归档/恢复函数

**前端:**
- `UserMemoryPanel` + `user-memory-store`（替代 CharacterMemoryPanel）
- `memory-panel-helpers.ts` 提取共享工具（typeLabels/typeColors/sourceLabels/confidenceColor/formatDate）
- typeLabels 修正: "rule"→"feedback" 匹配后端 memory_type

**代码审查（15 项修复）:**
1. `list_memories` 参数顺序修正
2. `get_extraction_config` 简化
3. `count_memories` 参数化查询
4. FTS5 查询净化
5-6. memory_type/status 白名单校验
7. `inject_memories` auto_learn 门控
8. 接入 max_retrieval_count/relevance_threshold
9. 归档记忆列表/恢复函数
10. 日提取上限提升
11. 词级 Jaccard 相似度
12. feedback 标签修正
13. 共享 inject_memories_into_prompt
14. 前端去重 + 类型标签修正
15. search_hybrid TOCTOU 修复

## Phase 10+ (2026-05-27)

### v2.6.0 — Preview Panel Code Review (6 bug fixes)

**Bug fixes:**
- PreviewPanel: 全局键盘快捷键（Escape/Ctrl+S）限定为面板内生效，避免拦截其他输入框
- MonacoEditor: `$effect` 中 `language`/`theme` 在 bail 检查前读取，确保响应式依赖被追踪
- OfficePreview: DOMPurify `ADD_ATTR` 添加 `style`，保留 mammoth/xlsx 内联样式
- file-path-linkifier: 正则添加 `(?<!\/)` 负向后行断言，排除 URL 路径段误匹配
- preview-store: `open()` 返回 `boolean` 告知调用方前一文件是否有未保存更改
- api.ts: `readFileAsBase64` / `readFileAsBuffer` 大小守卫从 `1.4` 修正为 `4/3`

## Phase 10+ (2026-05-25)

### v2.5.0 — Doctor 诊断面板 + 文件面板树视图重写

**Doctor 诊断面板:**
- 新增右侧滑出诊断面板（DoctorPanel），从命令面板 "Check Agent CLI" 触发
- 折叠式分节展示：CLI 安装、认证、项目配置、MCP 服务、外部服务、系统状态
- 每项显示 pass/fail/warn 状态图标，支持一键复制完整报告
- DoctorStore 基于 Svelte 5 runes 管理 loading/error/report 状态

**FilesPanel 树视图重写:**
- 扁平列表改为可折叠目录树，显示子目录/文件计数
- 新增操作类型过滤栏（write/edit/read/persisted），带计数徽章
- 新增文件预览面板，通过 readTextFile 读取内容并 split-pane 展示
- 文件类型图标颜色编码（TS=蓝, JS=黄, RS=橙, Svelte=玫红 等）

**代码质量（7 项审查发现）:**
- FilesPanel previewFile 竞态条件：添加 stale-request 守卫
- DoctorStore catch 块丢失已成功的 rawReport：分离错误处理
- DoctorStore 双重 runDiagnostics IPC：buildDoctorReport 复用 rawReport
- 前端/后端自定义 provider 上下文窗口不一致（200K vs 900K）
- zhipu-intl 缺失于前端 provider 目录
- CommandPalette 遗留 check_agent_cli 死代码清理
- DoctorPanel 关闭时取消进行中的诊断请求

## Phase 10+ (2026-05-24)

### v2.4.0 — 1M 上下文窗口支持 + 群聊上下文精简

**1M 上下文窗口:**
- 后端 per-provider `CLAUDE_CODE_AUTO_COMPACT_WINDOW` 覆盖：DeepSeek/QWEN/MiMo/Custom → 900K，Kimi → 230K，GLM → 180K
- 前端 `MODEL_CONTEXT_WINDOWS` 静态映射表 + `getContextWindowForModel()` 查询函数
- `contextWindow` getter 改为 `Math.max(CLI报告值, 静态映射值)` fallback 策略
- 新增 `advisory` 上下文警告级别：仅 1M+ 模型在 25% 使用率时触发
- `contextStrategyMessage` getter 映射警告级别到 i18n 提示文案
- `SessionStatusBar` 新增 advisory（黄色）和 critical（红色）标签

**群聊上下文精简:**
- `CONTEXT_TURN_WINDOW` 从 3 轮减至 1 轮，减少上下文膨胀

**代码质量（3 路审查）:**
- Claude / DeepSeek / Xiaomi Plan 并行审查
- 修复 DRY 违反（`build_deepseek_env` 改用 `compact_window_for_platform`）
- 简化 `contextStrategyMessage` 冗余分支
- 补充 `CONTEXT_TURN_WINDOW` 变更注释

## Phase 10+ (2026-05-15)

### v2.3.0 — Memory Extraction chat_api_key 分离 + 群聊体验修复

**Memory Extraction 改进:**
- `EmbeddingConfig` 新增独立 `chat_api_key` 字段，支持跨 provider 场景（如 DashScope embedding + DeepSeek chat）
- 说话人标注：提取上下文格式改为 `[角色名]: 内容`，LLM 只提取目标角色的记忆
- LLM 置信度：prompt 要求返回 0-100 confidence 字段，替代硬编码 70
- Embedding 复用：去重检查和向量写入共用同一次 embedding 计算，减少 API 调用
- 文件诊断日志：`log_to_file()` 写入 `logs/memory-extraction.log`，便于排查提取问题
- 中文 prompt：提取指令改为中文，要求输出与对话同语言

**群聊体验修复:**
- 侧边栏点击参与者 session 正确进入私聊视图（移除 `run_ids.includes(runId)` 过度匹配）
- 群聊时间线加载后自动滚动到底部
- `participantRunIds` 改为 `$derived` 避免逐事件数组分配
- 移除 orchestrator 中 `is_empty/is_orphan` 冗余守卫
- 提取 `debounce_key` 辅助函数，恢复 `max_tokens=2000`

**设置 UI:**
- Embedding 设置面板新增 chat_api_key 和 chat_model 独立配置项
- 连接测试支持 chat endpoint 独立验证

**代码质量（4 路审查）:**
- Code Reuse / Quality / Efficiency 三路并行审查
- 修复 BufWriter flush-per-call、LLM 响应无截断日志、WHAT 注释等问题

## Phase 10 (2026-05-14)

### v2.2.0 — 群聊体验优化 + Character Memory System 补全

**P0 Bug 修复:**
- Avatar 上传：Tauri v2 `file.path` 不可用回退到 `@tauri-apps/plugin-dialog`，新建角色支持即时上传
- 角色人设注入：`character_id` 链路打通（前端 → Tauri 命令 → 后端 participant 存储），`resolve_participant_system_prompt` 基于 ID 查找
- 多 @mention 解析：`MultiTarget` 变体支持同时 @mention 多个角色，全局扫描替代 `splitn(2)`
- Embedding API Key 回显：`apiKey` → `api_key` snake_case 对齐，`null` → `undefined` 匹配 serde skip

**P1 群聊核心体验:**
- Markdown 渲染：群聊消息复用 `MarkdownContent` 组件，支持标题/列表/代码块/表格
- 长文自动折叠：15 行阈值，`max-h-40` + mask-image 渐变，展开/折叠按钮
- Executor 路由过滤：Fanout 模式下 `role_type == "executor"` 自动排除，空 targets 返回明确错误
- 群聊上下文互相可见：最近 3 轮公开消息注入到 Fanout/SingleTarget/Private prompt，8000 字节 token 预算，CJK-aware 截断
- Summary/Debate 跳过重复 context 注入
- MultiTarget 可见性：对所有参与者可见（与 Fanout/Debate/Summary 一致）

**P2 性能优化:**
- `CLAWGO_SCENE=group_chat` 环境变量注入，superpowers 可跳过项目文件夹扫描

**P3 Character Memory System 补全:**
- `MemoryNode.status` 字段（pending/approved/rejected），serde default 向后兼容
- Review Queue：待审核记忆列表，approve/reject 操作
- LLM 自动提取：从 embedding config 派生 chat endpoint，OpenAI-compatible API 调用，结构化 JSON 输出
- 提取节流：5 分钟 debounce + 每角色每天 10 次上限
- 批量持久化：`append_memory_log_batch` 单锁写入
- 知识图谱可视化：sigma.js + graphology + ForceAtlas2 布局（lazy-loaded）
- Injection Config UI：`max_retrieval_count`(1-20)、`relevance_threshold`(0.0-1.0)、`graph_hops`(0-5) 可配置
- Embedding 配置扩展：`chat_endpoint`、`chat_model` 可选字段
- 前端 Embedding 状态指示：连接状态 + 快捷测试入口
- `MemoryConfig` 字段：`max_retrieval_count`、`relevance_threshold`、`graph_hops`

**代码质量（3 轮多路审查，12 路 provider）:**
- Round 1: 3C + 6I + M/R 全部修复
- Round 2: 1C（confidence 回归）+ 1I（字节切片 CJK panic）+ M 全部修复
- Round 3: 4 个 Minor 全部修复（TS "skill" union、truncate_str off-by-one、skill tag、Summary 冗余 context）
- `truncate_str` CJK 安全重写，`keyword_match_score` BM25-style 评分
- 静态 `reqwest::Client` 连接池复用
- Memory type validation: fact/experience/preference/rule/relationship/skill
- Status validation: pending/approved/rejected
- Config clamping: max_retrieval_count(1-20), relevance_threshold(0.0-1.0), graph_hops(0-5)
- Toast z-index 统一为 `z-[60]`

### v2.0.3 — Character Memory System: Simplify Review 修复

**三路并行审查 (Simplify):**
- 42 findings total: 14 Code Reuse + 15 Code Quality + 13 Efficiency
- 7 fixes applied, 10 deferred, 17 skipped (non-issues or too-large scope)
- Deferred items tracked in: `docs/superpowers/plans/[todo] 2026-05-14-character-memory-simplify-review-deferred.md`

**修复内容:**
- `+page.svelte`: 移除死状态 `embeddingConfigLoaded`, Embedding 配置 `onblur` 添加 500ms 去抖
- `avatar.rs`: 移除 TOCTOU `exists()` 检查 + 冗余扩展名检查，仅保留幻数验证
- `characters.rs`: 移除 3 处函数内部冗余 `use std::io::Write`；提取 `clear_lancedb_index()` 消除 compact/retention 间重复逻辑
- `memory_injection.rs`: 提取 `cjk_token_weight()` 辅助函数，消除 `approx_tokens` 和 `format_memory_injection` 间 CJK 代码点范围重复
- `vectorstore.rs`: 提取 `memory_schema()` 辅助函数，消除 `vector_upsert` 和 `vector_batch_upsert` 间 Schema 构建重复

## Phase 10 (2026-05-13)

### v2.0.2 — 群聊消息即时显示 + 独立气泡

**消息即时显示:**
- `orchestrator.rs`：spawn 前预写空 turn（`responses: []`），用户消息在下次轮询（≤1.5s）内立即出现
- AI 回复到了就增量 pop，不需要等所有参与者完成

**前端独立气泡:**
- 移除 turn header（TURN N · mode），改为轮次间淡色分隔线
- 用户消息：右对齐蓝色圆角气泡
- AI 回复：左对齐独立卡片，带头像、角色标签、状态点
- 等待状态：三点跳动动画
- 修复 `GroupChatParticipantDetail` 属性访问类型错误（`pinfo?.role` → `pinfo?.participant.role`）

### v2.0.1 — Plan 移除 + MCP 自动批准

**Plan 功能移除:**
- 删除 `PlanPanel` 组件和 `commands/plans.rs` Tauri 命令模块
- 移除 `PlanArtifact`、`PlanTask`、`PlanStatus`、`TaskStatus` 类型定义（前后端）
- 移除 `GroupChat.active_plan_id` 字段和 `plan.json` 存储
- 移除 `build_bootstrap_context` 中的 plan 上下文注入
- 清理 21 个 i18n key（en/zh-CN）

**MCP 工具自动批准:**
- `SessionActor` 新增 `auto_approve_mcp: bool` 字段
- Group Chat 参与者创建时自动启用 MCP 工具审批跳过
- 工具名以 `mcp__` 开头的权限提示自动发送 allow 响应，无需手动点击
- 普通 chat 和 web server 路径不受影响

**Bug Fixes (Phase 10 后续修复):**
- 侧边栏群聊删除按钮
- 过滤工具调用和思维链从输出中移除
- 导航修复：始终使用 groupChatId 路由到 GroupChatLayout
- Settings 角色页下拉框样式修复（白字白底）
- Planner/Executor 默认 system prompt 优化

### v2.0.0 — Group Chat 重构

**Room → GroupChat 重命名:**
- 后端 `room/` → `group_chat/`，`storage/rooms.rs` → `storage/group_chats.rs`，`commands/rooms.rs` → `commands/group_chat.rs`
- 前端 `room-store` → `group-chat-store`，`RoomStepper` → `GroupChatStepper`
- 所有 i18n keys `room_*` → `groupChat_*`
- 旧 Room 页面删除，群聊入口整合至 `/chat` 路由 + 侧边栏

**Character Library (角色库):**
- `AiCharacter` 模型：label、role_type（planner/executor）、role_instruction、default_provider/model、icon
- 存储于 `UserSettings.ai_characters`
- Settings → Characters CRUD 页面（创建/编辑/删除）
- 前端 4 个 Tauri 命令：list/create/update/delete_character

**Context Management MVP (上下文管理):**
- `ParticipantMeta`：delivery_cursor、session_turn_count、session_seq
- `filter_visible_messages`：按每条 turn 自身的 mode 过滤可见性（Private/SingleTarget 仅 sender+target 可见）
- `check_handoff`：turn 计数阈值（25 turns）触发 session handoff
- `build_bootstrap_context`：模板截断（~2000 tokens）构建新 session 引导上下文
- `reset_session_after_handoff`：重置 session 状态

**Role System Prompt (角色系统提示):**
- `build_role_system_prompt`：根据 role_type（planner/executor）+ role_instruction 生成系统提示
- `resolve_participant_system_prompt`：查找匹配的 AiCharacter，注入 `--append-system-prompt`
- planner 角色：只读，可规划但不可执行
- executor 角色：严格按计划执行

**Auto-chain Routing (自动链式路由):**
- SingleTarget 回复中扫描 `@Label` 提及，自动链式调用（最多 3 跳）
- 循环检测：`HashSet` 记录已链式参与者
- `CancellationToken` 传播支持取消

**其他改进:**
- `list_turns_jsonl` 返回结果按 `idx` 排序（修复 HashMap 迭代顺序不确定问题）
- `participant_id` 路径遍历校验（拒绝包含 `../` 或 `/` 的 ID）
- 群聊存储使用 per-ID mutex 锁保证并发安全
- 侧边栏群聊分组：折叠列表 + "新群聊"按钮 + 创建对话框（名称 + CWD 选择器）
- 首次使用引导：自动创建 Planner 角色

## Phase 9.z (2026-05-12)

### Custom Provider 支持 + Native Config Merge + Managed MCP

**Custom Provider:**
- 后端 `provider_claude_config.rs` 新增 `custom-*` 平台路由：`is_custom_platform()`、`leak_custom_id()`（带 Mutex 缓存避免重复 leak）、`platform_to_provider_id()` 返回自身、`requires_explicit_base_url/model()` 要求必填、`provider_env_from_credential()` 委托 `build_parameterized_env`
- 前端 Settings → Connection 新增 Custom Providers 卡片：表单（Name / Base URL / API Key / Model / Effort Level）、CRUD 操作、已有 custom provider 列表展示
- 碰撞防护：`Date.now()` + `Math.random()` 随机后缀；URL 格式校验（仅允许 http/https）
- Custom form API key 可见性独立于全局 `showApiKey` 状态（`customShowApiKey`）
- 4 个新测试：`custom_platform_maps_to_self`、`custom_platform_requires_base_url_and_model`、`custom_platform_valid_with_all_fields`、`custom_platform_builds_parameterized_env`

**Native Config Merge:**
- `provider_config_json_from_env` 重构：以 native `~/.claude/settings.json` 为基底，strip 敏感 key（`apiKey`/`primaryApiKey`），叠加 provider env/permissions/MCP，保留 hooks/plugins/enabledMcpjsonServers 等用户配置
- `SENSITIVE_KEYS` 从 `cli_config.rs` 提取为 `pub const`，`session.rs` 和 `provider_claude_config.rs` 共享引用，消除重复常量
- 6 个新测试覆盖：native hooks 保留、API key 剥离、MCP 合并、env 覆盖、native env 保留、superpowers 插件强制启用

**Managed MCP Injection:**
- `mcp_registry.rs` 新增第 5 来源：Claw GO 托管服务器（`UserSettings.mcp_servers`），scope="managed"
- 托管服务器替换同名 `scope="user"` 条目，保留 `local`/`project` scope
- Extensions 页面配置列表正确显示托管 MCP 服务器

**其他:**
- `provider_config_json_from_env` 硬覆盖字段（thinking/includeCoAuthoredBy/language 等）补充设计意图注释

## Phase 9.y (2026-05-09)

### v1.1.7 — 第三方 session provider 显式配置校验与 Xiaomi 共用模型配置

- 第三方 session provider 新增统一显式配置校验结果结构：`ProviderIssue`、`ProviderValidationResult`、`ValidatePlatformCredentialsResponse`
- 后端在 `src-tauri/src/agent/provider_claude_config.rs` 中新增统一校验入口 `validate_provider_credential` / `validate_platform_credentials`，覆盖 DeepSeek、GLM、QWEN、KIMI、Xiaomi（`mimo-plan` / `mimo-api`）
- `build_deepseek_env` / `build_parameterized_env` 在生成临时 session JSON 前先执行统一校验；配置不完整时直接阻止 provider config 生成
- 新增 settings IPC：`validate_platform_credentials`，并在 `src-tauri/src/lib.rs` 注册
- Settings → Connection 页新增“应用并校验配置”按钮：保存当前 `platform_credentials` 后立即调用后端统一校验，并在 provider 卡片内联展示字段级问题列表
- DeepSeek 卡片补充提示语义：明确要求显式填写完整模型配置
- Xiaomi 双 provider 卡片收口：`mimo-plan` 与 `mimo-api` 共享 6 个模型配置输入（`ANTHROPIC_MODEL`、三档 tier、`CLAUDE_CODE_SUBAGENT_MODEL`、`CLAUDE_CODE_EFFORT_LEVEL`），输入变更双写到两份 `extra_env`；`api_key` 与 `base_url` 仍分别保存在各自 credential 中
- Xiaomi / provider 校验成功文案从“配置完整，可启动”收窄为“配置校验通过”，避免对运行态做过度承诺
- Rust 测试代码补充：新增 `kimi` / `deepseek` / `mimo-api` 的显式校验覆盖；本机仍受既有 `0xc0000139` 环境问题影响，验证以 `cargo check` 为主
- Xiaomi 共用模型配置一致性修复：Settings 页共用模型面板改为共享视图（优先 `mimo-plan.extra_env`，缺失时回退 `mimo-api.extra_env`），后端 `migrate_platform_credentials` 新增共享字段补齐逻辑，自动修复历史上 `mimo-plan` / `mimo-api` 模型字段分叉导致的 `mimo-api` 校验缺项问题

### v1.1.6 — 旧 ID 彻底清理

- 移除所有旧 provider ID 支持：`mimo-pro`、`xiaomi`、`mimo` 从前端 `providerIdForRun` + 后端 `platform_to_provider_id`/`provider_env_from_credential`/`default_base_url`/`is_phase7_claude_compatible_api_platform`/`known_provider_defaults`/`auth_fixes` 同步删除
- 移除旧 ID 迁移逻辑（`migrate_platform_credentials` 中 mimo-pro→mimo-plan、mimo/xiaomi→mimo-api 的迁移代码）
- `mimo-plan` provider label 从 `"Xiaomi"` 改为 `"Xiaomi (Plan)"`，与 `"Xiaomi (API)"` 明确区分
- `session-store.test.ts` 新增 `preserves raw multi-question AskUserQuestion options on tool_start` 测试
- 全局 `rustfmt` 格式化统一：多行断言、函数签名、match arm 缩进

### v1.1.5 — Provider 预设清理与白名单机制

- 移除 5 个无后端支持的 provider 预设：kimi-coding、doubao、minimax、minimax-cn、mimo（前端 platform-presets.ts + 后端 onboarding.rs/settings.rs 同步清理）
- `PlatformCredential.extra_env` 白名单机制：`ALLOWED_EXTRA_ENV_KEYS` 限制用户可覆盖的环境变量（模型 tier + effort level），防止误覆盖稳定性变量
- `merge_extra_env` 合并函数：stability_env_vars → extra_env 覆盖顺序，空值过滤，6 个单元测试覆盖
- Settings 页 CC Session provider 卡片重设计：API Key 始终可见 + 可折叠高级配置面板（6 个 env var 字段：5 文本框 + 1 effort level 下拉框）
- Chat 页模型下拉菜单显示 tier 标签（Opus/Sonnet/Haiku），使用 `expandModelsToTiers` 展开，支持 extra_env 覆盖
- 第三方 provider 模型热切换：移除 `!isThirdParty` 限制，`set_model` control protocol 经 DeepSeek 和 MiMo Pro 实测有效
- extra_env 输入框统一为 `onblur` 持久化，与 API Key 字段行为一致
- EFFORT_LEVEL 下拉框改用 Svelte 受控 `value` 绑定
- placeholder 使用 tier 展开结果，修复 2 模型配置下 sonnet/haiku 显示错误的问题

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

- History 页面重写：从 Claw GO runs 切换为直接读取 CC 原生会话（`~/.claude/projects/`）
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
