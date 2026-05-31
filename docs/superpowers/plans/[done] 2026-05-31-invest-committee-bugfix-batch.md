# 投资委员会批量 Bug 修复计划

> **状态: [done]** - 全部 30 个任务完成 (2026-05-31)
> - 后端 14 个任务 ✅
> - 前端 16 个任务 ✅
> - cargo check ✅
> - npm run build ✅

## 背景

投资委员会模块积累了大量 Bug 和未完成功能。本计划覆盖 Dashboard、委员会、角色配置、命中率追踪、策略配置、交易记录、系统、记忆管理、记忆页面等全部问题。

## 探索发现的关键问题

1. **REGIME 未注入**: `regime.rs` 计算层 + `parser.rs` 解析层已完成，但 `orchestrator.rs` 从未调用 `format_regime_context()` 将 REGIME 数据注入 Quant 的 user message
2. **Portfolio Summary 缺失**: Risk R1 prompt 引用了 `portfolio_summary`，但没有函数构建/注入它
3. **LLM Config 健康检查错误**: `get_datasource_health` 检查 strategy 表计数而非实际 LLM 配置
4. **Yahoo Finance 未在数据源健康检查中**: 只检查 Tushare、invest.db、LLM Config
5. **事件扫描可能静默失败**: 需要调查为何找不到任何内容
6. **委员会工具 Tab 是占位符**: `CommitteeToolsTab.svelte` 仅 11 行，显示"Phase 4 开发中"，后端工具系统已完整但前端未实现
7. **ETF 支持是持仓层面的改动**: `AssetType` 已有 Stock/ETF/Crypto/Other，但 UI 和逻辑只处理股票。ETF 需贯穿整个持仓系统
8. **Dreaming 定时任务与 invest 无关**: 用户记忆衰减与归档是通用记忆功能，应迁移到 Settings → Memory Extraction
9. **股票代码应显示中文名称**: 委员会多处显示 `300617.SZ` 而非 `安靠智电`，需从 holdings 获取中文名贯穿整个委员会 UI
10. **角色配置参数名应中文化**: 英文参数名是代码设计用的，LLM Prompt 输出和 UI 展示应使用中文

---

## 第 0 阶段：UI/UX 设计（最先执行）

**在任何实现之前**，调用 `ui-ux` skill 设计：
- 委员会直播 Tab 重新设计（新的流程流，包含 REGIME/Portfolio 步骤）
- 委员会回放 Tab 重新设计（真实回放 + 试运行模式，使用卡片 UI）
- Dashboard 布局变更（合并 HOLD/WATCH 表格）
- 角色配置布局变更（R2 prompt、供应商配置迁移）

用户必须审批 UI mockup 后才能继续实施。

---

## 第 1 阶段：后端修复（Rust）

### 1.1 REGIME 注入到编排器
**文件**: `src-tauri/src/invest/committee/orchestrator.rs`
- 在 `run_committee()` 中，Macro 阶段之后、Quant R1 之前，通过 `regime::compute_regime_for_symbol(symbol)` 计算 REGIME
- 将 `format_regime_context(&regime_result)` 注入 Quant R1 的 user message
- 存储 `regime_result` 供后续上下文使用

### 1.2 Portfolio Summary 注入
**文件**: `src-tauri/src/invest/committee/orchestrator.rs`
- 从 DB 构建 portfolio summary：持仓（symbol, shares, avg_cost, current_price, pnl_pct, concentration_pct）+ 现金（available, emergency_buffer）
- 注入 Risk R1 的 user message 作为结构化文本
- 注入 Macro 的 user message 提供上下文

### 1.3 流程流更新（Macro → REGIME → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO）
**文件**: `src-tauri/src/invest/committee/orchestrator.rs`, `events.rs`
- Macro 阶段之后添加 REGIME 计算步骤
- Portfolio 数据作为共享上下文注入（不在流程步骤中重复），在 Risk R1 和 Macro 的 user message 中提供
- 更新 `events.rs` 中的 step_index 映射：从 6 步更新为 7 步（新增 REGIME 步骤）

### 1.4 命中率自动追踪
**文件**: `src-tauri/src/invest/verdict_review.rs`
- 移除手动"运行评审"触发的依赖
- 决议归档后，自动调度每日价格追踪
- 追踪直到持仓被卖出或从观望中移除

### 1.5 事件扫描调试
**文件**: `src-tauri/src/invest/event_scanner.rs`
- 调查 `scan_events()` 为何找不到任何内容
- 检查 Tushare `major_news` API 是否返回数据
- 添加 Yahoo Finance 作为备用新闻源

### 1.6 数据源健康检查修复
**文件**: `src-tauri/src/commands/invest.rs`
- 修复 LLM Config 检查：读取 `~/.claw-go/invest/llm_config.json` 而非 strategy 表
- 添加 Yahoo Finance 作为数据源健康检查

### 1.7 交易记录删除/修改
**文件**: `src-tauri/src/storage/invest/portfolio.rs`
- 添加 `delete_trade()` 函数
- 添加 `update_trade()` 函数
- 交易修改/删除后重新计算持仓

### 1.8 策略注入到 Prompt
**文件**: `src-tauri/src/invest/committee/orchestrator.rs`
- 委员会运行前加载活跃策略
- 将策略目标和约束注入 CIO prompt 上下文

### 1.9 委员会归档策略
**文件**: `src-tauri/src/storage/invest/committees.rs`
- 委员会运行完成后自动归档（无需用户操作）
- 每天每个标的仅保留最新一次裁决，新运行覆盖旧数据
- 模拟执行（dry_run）不进入归档

### 1.9a dry_run 模式实现
**文件**: `src-tauri/src/invest/committee/orchestrator.rs`, `src-tauri/src/commands/invest.rs`
- `run_committee_stream` 命令新增 `dry_run: bool` 参数
- orchestrator 的 `run_committee()` 支持 dry_run 模式：执行完整流程但跳过归档步骤
- 前端传入 dry_run=true 时，结果仅通过 SSE 事件流返回，不写入数据库

### 1.9b R2 工具定义更新
**文件**: `src-tauri/src/invest/committee/tools.rs`
- `role_tool_defs()` 当前只给 R1 分配工具，R2 无工具
- 为 Quant R2 和 Risk R2 分配工具（与 R1 相同或子集）
- R2 轮需要工具来执行交叉审查（查询历史数据、多时间框架分析等）

### 1.9c REGIME 硬规则验证
**文件**: `src-tauri/src/invest/committee/roles.rs`, `orchestrator.rs`, `parser.rs`
- 验证 Quant R1 prompt 中的 REGIME 硬保护规则是否能正确获取解析结果
- 验证 `parser.rs` 中 `parse_quant()` 是否正确提取 REGIME 回填值
- 验证 `format_regime_context()` 输出格式是否与 prompt 中的描述一致
- 端到端测试：运行委员会后检查 REGIME 数据是否正确注入

### 1.9d CIO Sanity Check 验证
**文件**: `src-tauri/src/invest/committee/analysis.rs`, `roles.rs`
- 验证 CIO prompt 中的 Sanity Check 规则是否能被 CIO 正确获取
- 验证 `cio_sanity_check()` 的 4 个 Gate 是否正确读取 CIO 输出中的字段
- 验证 Gate 1-4 的数据来源路径是否完整
- 端到端测试：运行委员会后检查 Sanity Check 结果是否正确

### 1.10 Dreaming 定时任务迁移到记忆模块
**文件**: `src-tauri/src/invest/scheduler.rs`, `src-tauri/src/commands/settings.rs`
- 将"用户记忆衰减与归档"定时任务从 invest scheduler 迁移到通用记忆模块
- 在 Settings → Memory Extraction 下添加开关
- 从 invest scheduler 中移除该任务

---

## 第 2 阶段：前端修复（Svelte）

### 2.1 持仓系统 - ETF 全面支持 + 表格合并
**文件**: `src/lib/components/invest/HoldingsTable.svelte`, `src/lib/stores/invest-store.svelte.ts`, `src-tauri/src/storage/invest/portfolio.rs`
- 合并 HOLD 和 WATCH 为单一表格，增加状态列（HOLD 优先排列）
- ETF 全面支持：买入、卖出、持仓显示、价格刷新、委员会运行均需区分股票/ETF
- 持仓表显示资产类型标签（股票/ETF）
- 标签统一改为"标的"而非"股票"

### 2.2 Dashboard - 用户档案移到系统 Tab
**文件**: `src/routes/invest/+page.svelte`
- 从 dashboard tab 移除 `UserProfileSection`
- 在 system tab 下添加 `profile` 子 tab

### 2.3 Dashboard - 添加日线图
**文件**: `src/lib/components/invest/PnlChart.svelte`
- 使用 Tushare 日线数据添加日线 PnL 图表

### 2.4 委员会直播 - 移除供应商配置，添加到角色配置
**文件**: `CommitteeLiveTab.svelte`, `CommitteeRolesTab.svelte`
- 从 CommitteeLiveTab 移除 ProviderConfigPanel
- 将 ProviderConfigPanel 添加到 CommitteeRolesTab（作为新区域）

### 2.5 委员会直播 - 移除手动代码输入
**文件**: `src/lib/components/invest/CommitteeLiveTab.svelte`
- 移除股票代码文本输入
- 添加"运行全部"按钮，运行所有 HOLD + WATCH 标的
- 不需要手动输入 - 只有批量运行逻辑

### 2.6 委员会直播 - UI 重新设计
**文件**: `src/lib/components/invest/CommitteeLiveTab.svelte`
- 重新设计以清晰展示标准流程流（7 步：Macro → REGIME → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO）
- 每个步骤作为带状态指示器的卡片
- Portfolio 持仓摘要作为共享卡片在顶部展示
- 结果自动归档，无归档按钮
- 每天每个标的仅保留最新一次裁决

### 2.7 委员会回放 - 两种模式重新设计
**文件**: `src/lib/components/invest/CommitteeReplayTab.svelte`
- **真实回放（Real Replay）**：
  - 从数据库调取该标的最近一次的委员会讨论内容
  - 展示全部 7 步流程：Macro → REGIME → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO
  - Portfolio 持仓摘要作为共享卡片在顶部展示
  - 使用卡片 UI 展示每个步骤的输出（类似直播但无动画）
  - 显示最终裁决、置信度、Gate 结果
- **模拟执行（Dry Run）**：
  - 对该标的立刻执行一次委员会，结果不进入归档
  - 用户可选择 round 次数：1（最小）/ 2（Cross-Examine）/ 4（推荐）/ 6（极限）/ 8（实验）
  - 使用卡片 UI 展示流程动画（类似直播）
  - 结果仅展示不保存，带有"模拟"水印标识
- 移除纯文本显示，统一使用结构化 DebateBlock 卡片

### 2.8 委员会归档 - 下拉搜索
**文件**: `src/lib/components/invest/CommitteeArchiveTab.svelte`
- 用 HOLD + WATCH 列表的下拉框替换文本输入
- 标签改为"标的代码"而非"股票代码"

### 2.9 委员会工具 Tab - 实现工具调用监控
**文件**: `src/lib/components/invest/CommitteeToolsTab.svelte`
- 当前是 11 行占位符，显示"Phase 4 开发中"
- 后端工具系统已完整：Macro=5 工具，Quant=3，Risk=2，CIO=0，R2+ 无工具
- 需要实现：显示每个角色可用的工具列表、工具调用历史、工具执行结果
- 从 `run_committee_stream` 事件流中捕获 tool_call 事件并展示

### 2.10 委员会全局 - 股票代码显示中文名称
**文件**: `CommitteeLiveTab.svelte`, `CommitteeReplayTab.svelte`, `CommitteeArchiveTab.svelte`, `CommitteeToolsTab.svelte`
- 所有显示 `300617.SZ` 的地方改为显示 `安靠智电`（中文名称）
- 从 holdings/assets 表获取中文名称映射
- 裁决下拉、回放下拉、归档搜索、工具调用日志均需使用中文名称
- 保留代码作为 tooltip 或副标题

### 2.11 角色配置 - 添加 R2 Prompt + LLM 输出中文化
**文件**: `src/lib/components/invest/CommitteeRolesTab.svelte`, `src-tauri/src/invest/committee/roles.rs`
- 为 quant_r2 和 risk_r2 添加 prompt 区域（UI 参考 Demo 的 R1/R2 Tab 布局）
- 实际内容保留现有专业指标信息，不做简化
- 在 Prompt 模板中要求 LLM 使用中文输出关键字段（信号、置信度、风险等级、裁决等）
- 前端展示 LLM 返回结果时，标签用中文，数值保持原样

### 2.12 命中率追踪 - 移除运行评审按钮
**文件**: `src/lib/components/invest/CommitteeAccuracyTab.svelte`
- 移除手动"运行评审"按钮
- 显示自动追踪的数据
- 修复 tushare token 显示问题

### 2.13 交易记录 - 添加删除/修改
**文件**: `src/lib/components/invest/TradeLogTab.svelte`
- 每行添加删除按钮
- 每行添加编辑按钮（打开 TradeDialog 编辑模式）

### 2.14 系统 - 数据源健康
**文件**: `src/lib/components/invest/SystemDatasourceTab.svelte`
- 将 Yahoo Finance 添加到数据源列表
- 修复 LLM Config 显示

### 2.15 记忆页面 - 显示 Claude Code 记忆
**文件**: `src/routes/memory/+page.svelte`
- 列出 `~/.claude/memory/` 下的文件（全局记忆）
- 列出当前项目的 `~/.claude/projects/<encoded-path>/memory/` 下的文件（项目记忆）
- 显示两个范围的 MEMORY.md 索引文件
- 支持编辑所有记忆 .md 文件
- 两段式布局：全局记忆 + 项目记忆

### 2.16 记忆管理 - 复用设置配置 + Dreaming 迁移
**文件**: `src/routes/memory-mgmt/+page.svelte`, `src/routes/settings/`
- 移除重复的提取配置
- 链接到 设置 > 记忆提取 Tab
- 在 Settings → Memory Extraction 下添加"用户记忆衰减与归档"开关（从 invest scheduler 迁移）

---

## 第 3 阶段：UI/UX 审查

所有变更完成后，调用 `ui-ux` skill 审查实现是否符合审批通过的设计。

---

## 关键文件清单

**后端：**
- `src-tauri/src/invest/committee/orchestrator.rs` - REGIME + Portfolio 注入，流程流
- `src-tauri/src/invest/committee/events.rs` - step_index 映射
- `src-tauri/src/invest/verdict_review.rs` - 自动追踪
- `src-tauri/src/invest/event_scanner.rs` - 调试扫描
- `src-tauri/src/invest/scheduler.rs` - Dreaming 任务迁移
- `src-tauri/src/storage/invest/committees.rs` - 归档策略（每天覆盖）
- `src-tauri/src/commands/invest.rs` - 数据源健康，交易 CRUD
- `src-tauri/src/commands/settings.rs` - Dreaming 开关
- `src-tauri/src/storage/invest/portfolio.rs` - 交易删除/修改，ETF 支持
- `src-tauri/src/invest/committee/tools.rs` - R2 工具定义
- `src-tauri/src/invest/committee/parser.rs` - REGIME 解析验证
- `src-tauri/src/invest/committee/analysis.rs` - Sanity Check 验证

**前端：**
- `src/routes/invest/+page.svelte` - Tab 结构
- `src/lib/components/invest/CommitteeLiveTab.svelte` - 直播 UI 重新设计
- `src/lib/components/invest/CommitteeReplayTab.svelte` - 回放重新设计（真实回放 + 模拟执行）
- `src/lib/components/invest/CommitteeArchiveTab.svelte` - 下拉搜索
- `src/lib/components/invest/CommitteeRolesTab.svelte` - R2 prompt，供应商配置，参数中文化
- `src/lib/components/invest/CommitteeToolsTab.svelte` - 工具调用监控（当前为占位符）
- `src/lib/components/invest/CommitteeAccuracyTab.svelte` - 自动追踪 UI
- `src/lib/components/invest/HoldingsTable.svelte` - 合并 HOLD/WATCH，ETF 支持
- `src/lib/components/invest/TradeLogTab.svelte` - 删除/修改
- `src/lib/components/invest/SystemDatasourceTab.svelte` - yfinance
- `src/routes/memory/+page.svelte` - Claude Code 记忆
- `src/routes/settings/` - Dreaming 开关

---

## 验证清单

1. `cargo check --manifest-path src-tauri/Cargo.toml` - Rust 编译
2. `npm run check` - Svelte 类型检查
3. `npm run lint` - 代码规范
4. `npm run build` - 完整构建
5. `npm run i18n:check` - i18n 验证
6. 手动测试每个修复的功能
