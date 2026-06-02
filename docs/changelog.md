# Changelog / 更新日志

## v5.2.4 (2026-06-02)

### 批量实时行情 API + 交易流程简化 + 返回率计算修复

**新功能 (2 项):**
1. **`get_realtime_quotes` 批量行情 API**: 新增 Tauri 命令 + `TushareClient::realtime_quotes()`，股票用 `rt_k`（盘中最新价），ETF 自动降级到 `fund_daily`；前端 `refreshPrices` 从逐符号循环改为单次批量调用，N 个持仓从 N 次 IPC 降为 1 次
2. **`RealtimeQuote` 类型**: Rust `RealtimeQuote` struct + TypeScript `RealtimeQuote` interface，字段包括 tsCode/name/open/high/low/close/preClose/vol/amount/tradeTime

**Bug 修复 (2 项):**
3. **`totalReturnPct` 计算修复**: 从 `totalAssets`（含现金）改为 `holdingsMarketValue`（仅持仓市值），收益率不再受现金余额稀释
4. **`CommitteeLiveTab` 最大持仓计算修复**: `maxHolding` 百分比基数从 `hv`（旧变量）改为 `total`，修复持仓集中度显示为 0 的问题

**交易流程简化 (2 项):**
5. **`buyStock` 移除手动 holding CRUD**: 不再手动调用 `add_holding`/`update_holding`，依赖 `record_trade` 触发 `recalculate_holdings_inner` 自动重建持仓表
6. **`sellStock` 移除手动 holding CRUD**: 不再手动调用 `delete_holding`/`update_holding`，同上

**CSS 设计系统 (1 项):**
7. **`--text-secondary` / `--text-tertiary` token**: `app.css` 新增两个暖色文字层次变量（`hsl(24 8% 72%)` / `hsl(24 7% 58%)`），invest 组件中非主要文本的对比度提升

**CI 修复 (1 项):**
8. **macOS/Windows CI build 修复**: `python-runtime/python/` 被 `.gitignore` 忽略导致 fresh checkout 后 Tauri resource glob 零匹配报错；所有 Python setup step 提前创建 `.gitkeep` 哨兵文件 + `curl -fSL` 让 HTTP 错误显式失败

**Simplify 审查修复 (3 项):**
9. **Efficiency**: `realtime_quotes` 双重迭代 → `partition` 单次遍历
10. **Efficiency**: ETF/stock fallback 串行 HTTP → `futures::future::join_all` 并发请求，多 ETF 场景延迟从 O(N×RTT) 降至 O(max RTT)
11. **Simplification**: `buyStock`/`sellStock` 移除 `this.cash` 死赋值（`loadAll()` 立即覆盖）

**涉及文件:**
- `src-tauri/src/tushare/client.rs` — `RealtimeQuote` + `realtime_quotes()` + `parse_realtime_quotes()` + `fallback_daily_quote()`
- `src-tauri/src/commands/invest.rs` — `get_realtime_quotes` Tauri command
- `src-tauri/src/lib.rs` — command registration
- `src/lib/types.ts` — `RealtimeQuote` interface
- `src/lib/stores/invest-store.svelte.ts` — `refreshPrices` 批量重写 + buyStock/sellStock 简化 + totalReturnPct 修复 + 死赋值清理
- `src/lib/components/invest/CommitteeLiveTab.svelte` — maxHolding 计算修复
- `src/lib/components/invest/TradeDialog.svelte` — `data-invest-scope` 属性
- `src/app.css` — `--text-secondary` / `--text-tertiary`
- `.github/workflows/ci.yml` — Python setup `.gitkeep` + `curl -fSL`
- `.github/workflows/release.yml` — Python setup `.gitkeep` + `curl -fSL`

---

## v5.2.3 (2026-06-02)

### /invest UI 修复 + 委员会 3 子页布局重构

v5.2.2 的"全模块 UI 设计系统统一"在两个层面没做干净：CSS token 语义跟 shadcn 冲突导致按钮文字看不见，3 个委员会子页（Replay / Archive / Tools）布局没按 mockup 重写。本次完整修复。

**CSS token 作用域修复（根因）：**
- `src/app.css` 的 `--accent` 是 shadcn 的暗灰 `#2a2827`（neutral surface），但 v5.2.2 的 28 个 invest 组件把 `var(--accent)` 当成金色品牌色用，导致 `bg-[var(--accent)] text-[var(--bg-base)]` 变成"暗灰底+暗色字"
- 新增 `[data-invest-scope]` 选择器，仅在 `/invest` 路由子树覆盖关键 token：`--accent` → 金色 `hsl(var(--primary))`、`--color-error` → 暖灰红 `#a87a7a`、`--bg-input` → `#2e2c29`（比 card 亮一档恢复输入框层次）、`--bg-card` / `--bg-elevated` / `--bg-sidebar` 精确对齐 demo design-system.css
- `/chat` `/settings` `/history` 等其他路由保持 shadcn 原语义不变

**3 个委员会子页布局重构：**
- **Replay**：250px 标的列表 + 历史日期 / 1fr 报告内容 双栏（参照 mockup `invest-v2.html:849-880`）；replay 模式自动加载、simulate 模式独立流式
- **Archive**：左 250px 查询面板（symbol select + days input + 日期列表带 verdict badge）/ 右 1fr Markdown 详情；verdict badge regex 从 content 解析（BUY/ACCUMULATE/HOLD/TRIM/SELL/WATCH）
- **Tools**：9 工具 × 5 角色访问矩阵真表格，数据严格对照后端 `src-tauri/src/invest/committee/tools.rs:184-206` `role_tool_defs()`

**Simplify 审查（4 路并行 + 5 项修复）：**
- 抽出 `src/lib/utils/invest-verdict.ts` — `getVerdictBadgeStyle()` + `parseVerdictFromContent()`，统一 Live/Replay/Archive 三处独立实现，修复 HOLD 颜色不一致（LiveTab 用 `#c9a96e` 硬色，其他用 `var(--accent)` token，统一到后者）
- 抽出 `pipeline-config.ts` 新增 `ROLE_COLORS` / `getStepState()` / `getRoundForStep()` — Live 和 Replay byte-for-byte 重复的本地实现合并；ToolsTab 配色从 `{macro:#3b82f6, quant:#8b5cf6, risk:#f59e0b, cio:#10b981}`（与 DebateBlock/pipeline-config 不一致，macro/quant 颠倒）改用统一 `ROLE_COLORS`
- ToolsTab 死代码清理：`truncateResult()` 是 identity 函数 → 删除；`roleLabel()` 5-case switch → `ROLE_COLUMNS.find()` 一行
- ArchiveTab 性能：`$derived.by` 预计算 `verdictMap: Map<date, verdict>`，从"每次 selectedDate 点击都跑全列表 regex"降到"archives 改变才计算一次"
- ReplayTab race 修复：`$effect` 自动加载加 `loadGen` 计数器，快速切换标的时丢弃 stale 响应

**i18n：** 11 个新 key — 9 个 `invest_tool_*_desc`（工具中英文描述）+ `invest_tools_matrix_title` + `invest_tools_col_tool` + `invest_filter_all`；顺手把 `invest_trade_action`（不存在）改成已存在的 `invest_actions`，`invest_cio_verdict` 改成 `invest_replay_cio_verdict`

**修改文件清单：**
- 新增：`src/lib/utils/invest-verdict.ts`
- 修改：`src/app.css`、`src/routes/invest/+page.svelte`、`src/lib/components/invest/pipeline-config.ts`、`CommitteeLiveTab.svelte`、`CommitteeReplayTab.svelte`、`CommitteeArchiveTab.svelte`、`CommitteeToolsTab.svelte`、`HoldingsTable.svelte`、`messages/en.json`、`messages/zh-CN.json`

**验证：** svelte-check ✅（剩 3 个 errors 全是 CodeEditor.svelte:413 旧 bug，与本次无关）/ i18n:check ✅ 0 errors / Build ✅ 34s / cargo check ✅

---

## v5.2.2 (2026-06-02)

### /invest 全模块 UI 设计系统统一

**升级范围：**
- 28 个 Svelte 文件（27 组件 + 1 页面框架）全面从旧 Tailwind 风格迁移至暖色暗黑设计系统
- 覆盖全部 5 个一级 Tab（Dashboard / Committee / Strategy / Trades / System）+ 14 个子 Tab + 6 个通用组件

**设计系统变量映射：**
- 卡片容器：`rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)]`
- 表头文字：`text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]`
- 数值展示：`font-[var(--font-mono)]` 等宽字体
- Badge/状态：`rounded-[var(--radius-full)] px-3 py-1 text-[11px] font-bold` + 角色/状态对应颜色
- 颜色统一：success `#8a9a76` / error `#a87a7a` / warning `#b89a6a` / accent `#c9a96e` / blue `#3b82f6` / purple `#8b5cf6`

**组件清单：**
- Dashboard：KpiCard, MacroSnapshotCard, LatestVerdictCard, HoldingsTable, PnlChart
- Committee：CommitteeLiveTab, CommitteeReplayTab, CommitteeArchiveTab, CommitteeRolesTab, CommitteeAccuracyTab, CommitteeToolsTab
- Strategy：StrategyTab
- Trades：TradeLogTab
- System：SchedulerTab, SystemRegimeTab, EventWatchTab, SystemDatasourceTab, SystemPnlHistoryTab, InsightsFeed, SystemDreamsTab, UserProfileSection
- 通用：TradeDialog, EventTriggerDialog, DebateBlock, PipelineFlow, DreamingConfigPanel, ProviderConfigPanel
- 页面：+page.svelte（header + tab 导航 + sub-tab 导航）

**验证：** svelte-check ✅ / ESLint ✅ / Build ✅ (35s)

---

## v5.2.1 (2026-06-02)

### Memory Extraction 设置迁移 + 全局记忆文件扩展

**设置迁移：**
- Memory Extraction 整个 tab（提取配置 + 记忆衰减与归档）从 `/settings` 迁移到 `/memory-mgmt` 页面
- `/memory-mgmt` 新增 "提取配置" tab，包含启用开关、Chat API Endpoint/Key/Model、记忆衰减与归档开关
- Settings 页面移除 Memory Extraction tab 及相关状态变量和函数

**记忆文件扫描扩展：**
- `MemoryFileCandidate` 新增 `project_slug` 字段，标识文件所属的 `~/.claude/projects/{slug}` 项目
- 后端扫描从单项目（依赖 cwd）改为遍历所有 `~/.claude/projects/*/memory/` 目录
- 新增 `~/.claude/memory/` 全局记忆目录扫描（scope `"global-memory"`）
- 前端 `/memory` 页面 Global 区域合并 `"global"` + `"global-memory"` scope
- Layout sidebar 按 `projectSlug` 过滤，每个项目文件夹只显示属于该项目的 memory 文件

### 委员会 7 项改进

**收益率计算：**
- 总收益率改为成本基准收益率：`(totalAssets - totalCostBasis) / totalCostBasis × 100%`
- 移除 `initialCash` 参与收益率计算的逻辑

**交易记录过滤：**
- TradeLogTab 默认隐藏系统操作（`cash_adjust`/`cost_edit`/`add_watch`/`delete_watch`）
- 新增 "显示系统操作" toggle 开关

**删除 UI 修复：**
- Dashboard watch 持仓删除从 `confirm()` 替换为自定义 ConfirmDialog 组件
- TradeLogTab 删除后移除乐观更新，改为依赖 `loadAll()` 刷新

**Dream Pipeline 修复：**
- `domain_insights.rs` SQLite `json()` 函数语法修复（key 名缺失 + `json_extract` 路径格式）

**事件中文化：**
- EventWatchTab/EventTriggerDialog 的 severity 和 stance 标签改为 i18n 翻译
- 新增 3 个 i18n 键：`invest.eventWatch.stanceBullish/Bearish/Neutral`
- 内容展示优先级调整：body（LLM 中文摘要）作为主显示，title 降为次要信息

**策略注入扩展：**
- 策略约束注入从仅 CIO 扩展到 CIO + Risk 角色

**L4 Officer 工具面板：**
- CommitteeToolsTab 新增 L4 Officer 角色条目（工具：`query_dreaming_insights`）

**i18n 新增键（6 个）：**
- `invest_confirm` / `invest_confirm_title` — 确认对话框
- `invest_trade_show_system` — 显示系统操作
- `invest.eventWatch.stanceBullish/Bearish/Neutral` — 立场标签

---

## v5.2.0 (2026-06-02)

### 委员会 Prompt L1-L4 策略框架升级

**核心变更：**
- 新增 L4 Officer 角色（执行控制官）：卫语句判定 + 情绪评估 + 行为红灯评分 + 买点合理性检查
- 重写全部 7 个 Prompt 模板（Macro/Quant R1/R2/Risk R1/R2/L4 Officer/CIO）
- L1-L4 四层嵌套决策体系：L1 全局底色 → L2 催化剂 → L3 技术执行 → L4 执行控制
- 催化剂层级框架（Tier1/Tier2/Tier3）集成到 CIO

**数据层扩展：**
- Tushare 客户端新增 4 个 API 方法：daily_basic、fina_indicator、report_rc、moneyflow_dc
- AssetContext 结构体：PE/PB/ROE/营收增速/净利增速/机构评级/风险新闻/资金流向
- macro_cache 扩展：涨跌停家数 + 两市成交额（15 个指标）
- 辅助函数：count_recent_trades、max_drawdown_for_symbol

**Parser 升级：**
- ParsedFields 新增 30+ 字段（Macro/Quant/Risk/L4 Officer/CIO 各角色专用字段）
- 新增 parse_l4_officer 解析函数
- 新增 compute_red_light_score 确定性评分函数（0-30 → green/yellow/red）
- hard_truncate 改进：行边界感知截断

**工具扩展：**
- 新增 4 个工具：get_moneyflow、get_company_info、get_company_news、get_recent_events
- 角色工具分配更新：Macro(+1)、Quant(+2)、Risk(+1)、L4 Officer(1)

**Pipeline 变更：**
- 8 步 pipeline：Macro → REGIME → Quant R1/R2 → Risk R1/R2 → L4 Officer → CIO
- L4 Officer 在 Risk R2 之后、CIO 之前执行
- 行为红灯评分由 Rust 端确定性计算（不依赖 LLM）

**前端改动：**
- PipelineFlow.svelte：8 节点 pipeline 可视化
- DebateBlock.svelte：L4 Officer 红色标识
- CommitteeReplayTab/CommitteeLiveTab：8 步进度显示
- CommitteeRolesTab：L4 Officer 角色卡片 + hard rules
- i18n：6 个新翻译键（en + zh-CN）

**字符限制变更：**
- Macro: 400 → 600
- Quant: 250 → 350
- Risk: 250 → 350
- L4 Officer: 250（新增）
- CIO: 400 → 600

**Simplify 审查修复：**
- 提取 `MoneyflowDc::aggregate_moneyflow` + `format_moneyflow_summary` 共享 helper
- 提取 `fetch_valuation_data` helper 消除重复 API 调用
- `build_asset_context` API 调用并行化（tokio::join!）
- 提取 `concentration_for_symbol` helper 统一集中度计算
- `ParsedFields::is_red_light()` 派生方法替代冗余字段
- 提取 `pipeline-config.ts` 共享模块消除前端 STEP_DEFS 重复
- 合并 `count_recent_trades` / `count_all_recent_trades` 为单一函数

---

## Phase 10+ (2026-06-01)

### v5.1.2 — ETF 价格修复 + 事件扫描增强 + simplify 审查修复

**Bug 修复 (3 项):**
1. **ETF 价格获取修复**: `tushare/client.rs` 新增 `daily_api()` 模板方法，ETF 代码前缀（159/510/512/515/588/150/500/501/160-164）自动路由到 `fund_daily` API，`daily()` 和 `get_latest_price()` 共享逻辑
2. **事件扫描增强**: `event_scanner.rs` 新增英文关键词（tariff/sanctions/federal reserve/cpi/gdp 等 HIGH+MEDIUM 共 21 词），支持全球事件检测（美联储降息/贸易战/CPI 等）
3. **record_trade 持仓一致性**: `portfolio.rs` 的 `record_trade()` 插入交易后自动调用 `recalculate_holdings_inner()`，消除手动操作导致持仓不更新的问题

**Simplify 审查修复 (6 项):**
4. **Reuse**: `is_etf_code()` + 内联 if/else → `daily_api()` 模板方法消除重复分支
5. **Reuse**: `Severity` match 块提取为 `as_str()` 方法
6. **Simplification**: 诊断日志循环 + 保存循环合并为单循环
7. **Efficiency**: `chars().take(40).collect::<String>()` → `floor_char_boundary(40)` 零分配截断
8. **Efficiency**: `e.to_string().contains("UNIQUE")` → `e.contains("UNIQUE")` 避免冗余分配
9. **Altitude**: `orchestrator.rs` notional=0 时从 avg_cost×shares 兜底计算 + CIO Prompt 100 股倍数规则

**涉及文件:**
- `src-tauri/src/tushare/client.rs` — daily_api() ETF 路由
- `src-tauri/src/invest/event_scanner.rs` — Severity::as_str()/循环合并/英文关键词/零分配截断
- `src-tauri/src/invest/committee/orchestrator.rs` — notional 兜底计算
- `src-tauri/src/invest/committee/roles.rs` — CIO Prompt 100 股规则
- `src-tauri/src/storage/invest/portfolio.rs` — record_trade 自动 recalculate

---

### v5.1.1 — Bug 修复 + Chat UI 升级 + 代码审查优化

**Bug 修复 (3 项):**
1. **标题栏按钮修复**: `capabilities/default.json` 添加 `core:window:allow-minimize`/`allow-toggle-maximize`/`allow-close` 权限，最小化/最大化/关闭按钮恢复正常
2. **Python Setup Overlay 竞态修复**: 兼容 `"starting"`/`"verifying"` 两种 stage；mount 后主动调用 `get_python_status` 轮询当前状态，解决后端先于前端订阅事件的竞态条件；Rust 端新增 `ProgressSnapshot` + `LAST_PROGRESS` 全局存储
3. **Index 首页恢复**: 移除 `goto("/chat")` 重定向，重建功能性首页（Logo + 快捷操作 + 最近对话 + 功能入口网格）

**Chat UI 升级 (4 项):**
4. **消息区域**: 头像改为 30px 圆形（用户首字母 / AI ✦），移除用户消息背景色，添加 `fadeInUp` 入场动画
5. **输入框**: 背景 `bg-secondary`，focus 边框 `border-primary/30`（金色），send 按钮 `bg-primary/15` + `border-primary/30` 样式
6. **状态栏**: 背景 `bg-secondary`，状态点绿色 glow，新增 Running/Ready 状态 pill
7. **右侧面板**: 背景 `bg-sidebar`，宽度 280px→300px，工具图标 4x4→7x7

**CSS 设计系统桥接 (1 项):**
8. **Flat token 别名**: `app.css` 新增 50+ CSS 变量（`--bg-base`/`--text-primary`/`--accent-color`/`--font-sans`/`--font-mono`/`--radius-*`/`--space-*`/`--duration-*`），映射 Demo 设计系统到 Tailwind HSL 变量

**代码审查修复 (6 项 — /simplify 四路审查):**
9. **Reuse**: `+page.svelte` 的 `formatTime()`/`truncate()` 改用 `$lib/utils/format` 的 `relativeTime`/`truncate`
10. **Simplification**: `SessionStatusBar` Running/Ready pill 合并为单个元素 + 三元表达式
11. **Simplification**: `PythonSetupOverlay.handleProgress` 三分支简化为 `if (ready) ... else ...`
12. **Altitude**: `portfolio.rs` 三处 `entry.notional = avg_cost * shares` 提取为 `Holding::recompute_notional()` 方法
13. **Altitude**: `orchestrator.rs` `load_with_prices()` 重命名为 `load_and_refresh_prices()`，明确写副作用语义
14. **Efficiency**: 价格获取从 O(N) 串行 HTTP 改为 `futures_util::stream::buffer_unordered(3)` 并发，~3x 提速

**涉及文件:**
- `src-tauri/capabilities/default.json` — 窗口权限
- `src-tauri/src/python/bootstrap.rs` — ProgressSnapshot + LAST_PROGRESS
- `src-tauri/src/python/mod.rs` — pub bootstrap
- `src-tauri/src/commands/python_status.rs` — progress 字段
- `src/routes/+page.svelte` — Index 首页
- `src/app.css` — Flat token 别名 + fadeInUp 动画 + 暖色滚动条
- `src/lib/components/ChatMessage.svelte` — 消息 UI
- `src/lib/components/PromptInput.svelte` — 输入框 UI
- `src/lib/components/SessionStatusBar.svelte` — 状态栏 UI
- `src/lib/components/ToolActivity.svelte` — 右侧面板 UI
- `src/lib/components/PythonSetupOverlay.svelte` — 竞态修复
- `src-tauri/src/invest/committee/orchestrator.rs` — 并发价格获取 + 重命名
- `src-tauri/src/invest/committee/tools.rs` — 小数精度统一
- `src-tauri/src/invest/committee/analysis.rs` — 小数精度统一
- `src-tauri/src/invest/daily_report.rs` — 小数精度统一
- `src-tauri/src/invest/regime.rs` — 小数精度统一
- `src-tauri/src/invest/international.rs` — serde alias
- `src-tauri/src/storage/invest/portfolio.rs` — recompute_notional 方法

---

### v5.1.0 — UI 设计系统统一: 暖色暗黑主题 + 自定义标题栏 + Inter 字体

**UI 重构 (2026-06-01):**

**设计系统统一 (3 项):**
1. **暖色暗黑固定主题**: `app.css` CSS 变量全面替换为 Demo 设计系统色值 — `#1a1918` 底色、`#c9a96e` 强调色、`#ebe8e4` 文字、`#2e2c2a` 边框；移除 light mode、scheme-neutral 分支
2. **Inter 字体全局应用**: Google Fonts Inter 作为首选字体，添加到 `app.html`；splash 页同步更新
3. **自定义窗口标题栏**: Tauri `decorations: false` + 自定义标题栏（`data-tauri-drag-region`）含 ClawGO 标题 + 最小化/最大化/关闭按钮

**功能清理 (2 项):**
4. **移除主题切换**: 删除 `themeMode` / `colorScheme` / `cycleTheme()` / `cycleScheme()` 及相关 UI 按钮，固定暖色暗黑模式
5. **移除色彩方案切换**: 删除 warm/neutral 方案切换，清理 5 个废弃 i18n key

**侧边栏优化 (2 项):**
6. **Settings 底部分隔**: Settings 图标移至 Icon Rail 底部，用分隔线与主导航隔开，匹配 Demo 设计
7. **Icon Rail 样式**: 背景色统一使用 `var(--sidebar-border)` 变量

**Checklist 回归: 46 项功能入口全部保留 ✓**

**涉及文件:**
- `src/app.css` — CSS 变量重写
- `src/app.html` — Inter 字体 + 暖色 splash
- `src-tauri/tauri.conf.json` — `decorations: false` + CSP Google Fonts
- `src/routes/+layout.svelte` — 标题栏 + 移除主题切换 + Settings 分隔
- `messages/en.json` / `messages/zh-CN.json` — 清理废弃 key

---

## Phase 10+ (2026-06-01)

### v5.0.5 — 代码审查修复: Yahoo 认证加固 + Tushare 代理验证 + 并发安全

**11 项代码审查修复 (2026-06-01):**

**正确性 (3 项):**
1. **TushareClient URL scheme 验证**: `new()` 公开构造函数新增 `validate_url_scheme()` 检查，`from_settings()`/`with_token()` 在使用 `tushare_proxy_url` 前验证必须以 `http://` 或 `https://` 开头，无效时 fallback 到官方 API 并 log::warn
2. **reqwest::Proxy::all() 静默失败修复**: `InternationalClient::new()` 中无效代理 URL 从 `if let Ok` 静默跳过改为 `match` 分支 + `log::warn` 警告，用户可看到代理未生效的原因
3. **Mutex 中毒恢复**: `InternationalClient` 所有 `.lock().unwrap()` 改为 `.unwrap_or_else(|poisoned| poisoned.into_inner())`，避免前序 panic 导致级联崩溃

**可靠性 (3 项):**
4. **重试循环末尾退避消除**: `fetch_chart_raw`/`fetch_yahoo_news`/`call_api` 三个重试循环在最后一次迭代（`attempt + 1 >= max_retries`）时跳过 sleep，避免无意义的 8s/8s/4s 等待
5. **ensure_session 惊群效应修复**: 写锁内添加双重检查（double-checked locking），多个并发任务在 session 过期时不再各自独立 fetch cookie+crumb
6. **Mutex → RwLock**: `InternationalClient.session` 从 `Mutex<Option<YahooSession>>` 改为 `RwLock`，快路径（session 有效时）允许多读者并发

**代码质量 (5 项):**
7. **with_token_and_proxy() 新方法**: 接受显式 `(token, proxy_url)` 参数，避免已加载 settings 的调用点（lib.rs 两处）重复读取 settings.json
8. **resolve_local_proxy_url() 共享辅助函数**: 提取到 `storage/settings.rs`，消除 `updates.rs` 与 `international.rs` 的重复代理 URL 构建逻辑
9. **tushare_proxy_url URL 格式验证**: `from_settings()` 和 `with_token()` 在构建 TushareClient 前验证 URL scheme，无效值 fallback 到官方 API
10. **代理端口范围检查**: `resolve_local_proxy_url()` 使用 `u16` 类型天然限制 0–65535，`>= 1` 检查排除端口 0
11. **代理函数语义改进**: 共享函数命名为 `resolve_local_proxy_url()` 替代各处 `resolve_proxy_url()`，语义更清晰（本地 HTTP 隧道代理）

**涉及文件:**
- 后端: `tushare/client.rs` / `invest/international.rs` / `storage/settings.rs` / `commands/updates.rs` / `lib.rs`

---

### v5.0.4 — Yahoo Finance 429 二次修复 + 扫描增强 + add_watch + fund_basic

**4 项修复 + 4 项代码审查改进 (2026-06-01):**

**修复 (4 项):**
1. **macro_refresh 并发改串行**: `fetch_international()` 从 `join_all` 同时发 6 个 Yahoo 请求改为串行执行，消除 429 根因
2. **Yahoo 请求间隔统一 500ms**: `fetch_all_quotes`/`fetch_china_finance_news`/`fetch_international` 三处间隔统一为 `YAHOO_REQUEST_INTERVAL_MS` 常量
3. **扫描错误报告增强**: `ScanResult` 新增 `errors` 字段，记录 Tushare/Yahoo 失败原因，前端 `console.warn` 输出
4. **Yahoo 兜底阈值**: 从 `< 5` 改为 `< 3`（`YAHOO_FALLBACK_MIN_EVENTS` 常量），减少不必要的 Yahoo 调用

**代码审查改进 (4 项):**
1. **YAHOO_REQUEST_INTERVAL_MS 常量**: 提取 500ms 魔法数字为 `international.rs` 公开常量，3 处引用统一
2. **YAHOO_FALLBACK_MIN_EVENTS 常量**: 提取 `< 3` 魔法数字为命名常量 + doc comment
3. **log+push 去重**: `event_scanner.rs` 中 `log::warn!` + `errors.push(format!(…))` 合并为单次 `format!`
4. **ScanResult TypeScript 接口**: 从内联类型提取为 `types.ts` 中的 `ScanResult` 接口

**附带修复 (来自之前未提交的改动):**
5. **add_watch action 迁移**: `trades` 表 CHECK 约束新增 `add_watch` 值，DB 迁移脚本
6. **fund_basic 客户端过滤**: Tushare `fund_basic` API 忽略 `name` 参数，改为客户端模糊匹配

**涉及文件:**
- 后端: `international.rs` / `macro_refresh.rs` / `event_scanner.rs` / `storage/invest/mod.rs` / `tushare/client.rs`
- 前端: `invest-store.svelte.ts` / `types.ts`

---

## Phase 10+ (2026-05-31)

### v5.0.3 — 委员会中文化 + REGIME 展示 + Parser 双语 + Profile 双注入

**5 项功能改进 + 6 项代码审查修复 (2026-05-31):**

**功能改进 (5 项):**
1. **Gate notes + 归档报告中文化**: Gate 1-4 notes 从英文改为中文（信号一致性/集中度/子弹充足/仓位合理），归档报告全文中文化
2. **REGIME 实时展示**: RegimeStep 事件扩展（regime/reason/strategy_hint/metrics），前端 REGIME 卡片展示市场状态、原因、策略建议和量化指标（RSI-14/MA20/MA60/波动率/价格分位数）
3. **Parser 双语支持**: extract_field_any/extract_f64_any/extract_bool_any/extract_list_field_any 支持中英文字段名同时解析，CIO 裁决识别买入/加仓/持有/减仓/卖出
4. **Prompt 字段名中文化**: 6 个 prompt 模板（MACRO/QUANT/QUANT_R2/RISK/RISK_R2/CIO）输出格式指令从英文改为中文
5. **Profile 双注入 + Risk 数值预计算**: 用户档案同时注入 Risk R1 和 CIO，Risk R1 预计算 CONCENTRATION_PCT/PNL_PCT/DRY_POWDER_CNY

**代码审查修复 (6 项):**
1. **PortfolioData 消除重复 DB 查询**: 新增 PortfolioData 结构体，build_portfolio_summary 和 build_risk_metrics_context 共享同一份数据
2. **Profile 注入合并**: CIO 和 Risk R1 的 profile 注入从两个 if 块合并为单个条件
3. **REGIME 发射扁平化**: 成功/失败分支的重复 emit 调用合并为先计算再统一发射
4. **预计算指标键对齐**: build_risk_metrics_context 输出从英文键改为中文键（集中度/盈亏比/可用子弹）
5. **REGIME 卡片条件标准化**: LiveTab 和 ReplayTab 的条件顺序统一
6. **CIO to_uppercase 注释**: 添加说明大写转换仅对英文有效

**涉及文件:**
- 后端: `roles.rs` / `parser.rs` / `orchestrator.rs` / `events.rs` / `analysis.rs` / `archive.rs`
- 前端: `invest-committee-store.svelte.ts` / `CommitteeLiveTab.svelte` / `CommitteeReplayTab.svelte` / `DebateBlock.svelte`
- i18n: `messages/zh-CN.json` / `messages/en.json`

---

## Phase 10+ (2026-05-31)

### v5.0.2 — 代码审查修复: 数据完整性 + 正确性 + 死代码清理

**14 项代码审查修复 (2026-05-31):**

**修复 (10 项):**
1. **exec_history_data_tushare bars 排序反转**: daily() 返回降序但 `.rev()` 导致取到最旧数据，pct_change 符号反转、recent_5 显示错误
2. **旧版 account purpose 值迁移**: short_term/speculation/dividend/hedge 加载时标准化为新版值，避免显示/数据不一致和 i18n key 缺失
3. **initial_balance 列只读不写**: set_cash 首次 INSERT 时写入 initial_balance，新增 set_initial_cash 命令
4. **family_support 迁移集中化**: 从 get_profile/save_profile 内联移至 init_db() 集中管理，错误正确传播
5. **save_llm_config/get_llm_config 不对称**: 未知 provider 配置丢失；get_llm_config 改为遍历 JSON 所有 provider key
6. **build_committee_config 未传递 model**: CommitteeConfig 新增 model_override，build_llm_config 优先使用用户配置
7. **parse_provider_id 内联重复**: 提取 try_parse_provider_id() 统一使用
8. **build_user_profile_context 吞没 DB 错误**: 增加 log::warn! 日志
9. **fund_basic 未过滤基金类型和状态**: 增加 fund_type=E 和 status=L，只返回上市 ETF
10. **SystemPnlHistoryTab loading 状态竞态**: 移除本地 state，直接使用 store.loading

**优化 (5 项):**
11. **Verdict 行映射重复**: 提取 row_to_verdict() 辅助函数，消除 7 处重复代码
12. **UserProfile TypeScript 接口补全**: 新增 familySupport 字段
13. **ProviderConfigPanel saveTimer 泄漏**: 组件卸载时清理定时器
14. **invest-store loadAll 错误吞没**: .catch 增加 console.warn 日志

---

### v5.0.2 — Yahoo Finance 429 限流修复

**Yahoo Finance 客户端重试 + 限流 (2026-05-31):**

1. **fetch_chart_raw 重试**: 429/5xx 自动重试 3 次，指数退避 (1s/2s/4s)
2. **fetch_yahoo_news 重试**: 同样的重试策略应用于新闻搜索接口
3. **fetch_all_quotes 串行化**: 6 个国际指标请求从 `join_all` 并发改为串行，间隔 300ms
4. **fetch_china_finance_news 串行化**: 4 个新闻查询从并发改为串行，间隔 300ms

**根因:** Yahoo Finance API 对同一域名的并发请求限流严格，原实现无重试且全量并发，频繁触发 429。修复后与 Tushare 客户端保持一致的重试模式。

---

### v5.0.1 — 代码审查修复: 数据完整性 + UI 正确性 + 死代码清理

**9 路审查, 15 项修复 (2026-05-31):**

**P0 — 数据完整性 (5 项):**
1. **asset_type 列迁移**: 现有数据库 `ALTER TABLE holdings ADD COLUMN asset_type` 兼容，使用 `pragma_table_info` 幂等检查
2. **notional 数据保护**: `recalculate_holdings_inner` 删除前查询现有 notional 值，重建时保留原值
3. **事务包装**: DELETE+INSERT 循环使用 `BEGIN`/`COMMIT`/`ROLLBACK`，防止中途失败导致数据丢失
4. **dry_run 前端透传**: `runCommittee()` 新增 `dryRun` 参数，simulate 模式不再归档真实判决
5. **Pipeline 索引对齐**: 前端 `STEP_DEFS.backendIdx` 匹配后端 7 步流程 (macro=0, regime=1, quant_r1=2, ...)

**P1 — UI 正确性 (5 项):**
6. **$derived 修复**: `roleStats`/`nameMap` 改用 `$derived.by()` 替代 `$derived(() => expr)`
7. **R1 Prompt 路径**: `get_role_prompts` 读取 `quant_r1.txt` 替代 `quant.txt`
8. **deleteTrade 刷新**: 删除交易后调用 `loadAll()` 同步持仓/现金状态
9. **regime_step 成功检查**: 失败时设置 `error` 状态，不再递增 `completedSteps`
10. **TradeDialog 验证**: edit 模式不再跳过数量/价格校验

**P2 — 死代码/回归 (5 项):**
11. **archive_decision 死代码移除**: 移除旧函数，保留 `committees::archive_verdict` 唯一路径
12. **events.jsonl 日覆盖**: 与 DB 归档策略一致，过滤同日同 symbol 旧条目
13. **多日期回放**: ReplayTab 恢复日期选择器，支持浏览历史判决
14. **手动审查按钮**: AccuracyTab 恢复 "Run Review" 按钮
15. **verdict ID 查询**: 替换 `list_verdicts(limit=1)` 为 `get_verdict_by_id()` 直接查询

**审查方法:**
- 9 路独立 finder (A-I 角度) 并行扫描
- 15 项候选 1-vote 验证 (14 CONFIRMED, 1 REFUTED)
- Phase 3 gap sweep 补充 2 项

---

### v5.0.0 — openInvest Phase 5: 委员会 LLM 工具 + Prompt 全面升级

**架构变更（9 Phase, 15 项审查修复）:**

**Phase 0 — 角色精简:**
- CommitteeRole enum 从 7 变体简化为 4: Macro/Quant/Risk/Cio
- 新增 Round enum (R1/R2)，load_prompt_for_round 支持轮次+占位符渲染
- 管线从 7 步改为 6 步（去掉 Wealth），PipelineFlow/前端同步更新
- round_cache 存储层（跨次查询工具用）

**Phase 1 — Regime 计算模块:**
- 独立 regime.rs，从 commands/invest.rs 提取
- 新增 RSI-14 Wilder 计算、价格分位数（500 日窗口）
- REGIME 注入 Quant R1 user message

**Phase 2 — Tushare 宏观接口:**
- 新增 moneyflow_hsgt/margin_detail/shibor/cn_bond_yield 4 个方法

**Phase 3 — Yahoo Finance 客户端:**
- 新建 international.rs，6 个国际指标（VIX/TNX/DXY/Gold/Oil/USDCNY）
- fetch_yahoo_history 支持任意 Yahoo 符号历史日线

**Phase 4 — macro_cache 存储层:**
- 12 个宏观指标 UPSERT 表，is_stale 检查

**Phase 5 — 调度+工具重写:**
- macro_refresh cron job（交易日每 15 分钟）
- exec_macro_snapshot 从 macro_cache 读取，过期 fallback 实时拉取
- exec_history_data 双数据源（Tushare A 股 + Yahoo 国际）
- exec_multi_timeframe 增强：RSI-14、价格分位数、MA120

**Phase 6 — 工具分角色开放:**
- role_tool_defs(role, round) 按角色返回工具集
- run_with_tool_loop 共享函数，R1 有工具 R2 无工具

**Phase 7 — Prompt 替换:**
- 6 个新 prompt：Macro/Quant R1/Quant R2/Risk R1/Risk R2/CIO
- REGIME 硬保护规则、portfolio 注入、反幻觉约束

**Phase 8 — Parser + Analysis:**
- ParsedFields 新增 10 个字段（regime/key_data/pnl_pct 等）
- CIO Gate 4：零仓位 HOLD 降 confidence、低集中度 HOLD 降 confidence

**15 项代码审查修复:**
- **CRITICAL**: prompt save/load 路径统一（quant_r1.txt 而非 quant.txt）、is_stale() NaiveDateTime 解析
- **HIGH**: asset_name 查询 holdings 表、i18n 重复键删除
- **MEDIUM**: Gate 1 neutral 处理、PipelineFlow skipped 状态、字节越界防护、重复逻辑提取
- **LOW**: HV20 数据不足检查、$effect cleanup、括号优先级、前端类型同步

**新建文件:**
- `src-tauri/src/invest/regime.rs` — Regime 计算模块
- `src-tauri/src/invest/international.rs` — Yahoo Finance 客户端
- `src-tauri/src/invest/macro_refresh.rs` — 宏观数据刷新调度
- `src-tauri/src/storage/invest/macro_cache.rs` — 宏观指标缓存表
- `src-tauri/src/storage/invest/round_cache.rs` — 轮次输出缓存

---

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
