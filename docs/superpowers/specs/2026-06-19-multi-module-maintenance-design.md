# 设计:多模块维护批次(memory 修复 / 委员会直播重设计 / 功能清理 / usage 卡片)

日期:2026-06-19
状态:设计待审阅
关联 mockup:`docs/superpowers/specs/committee-live-redesign-mockup.html`

## 背景与目标

一次性处理 9 项跨前后端的维护需求。经探索后按"风险隔离 + 逻辑相关"拆成 4 个可独立验证、独立 commit 的批次。每个批次内的任务彼此相关或风险等级相近。

实现顺序:A(纯 bug,低风险)→ B(UI 重设计,需视觉验证)→ C(删除与迁移,改动面最大)→ D(usage 卡片)。

---

## 批次 A — 记忆系统 bug 修复

### A1. `/memory` 路由按 cwd 过滤失效(任务7)

**根因(已确认):** `MemoryFileCandidate` 结构体(`src-tauri/src/models.rs:26-34`)缺少 `#[serde(rename_all = "camelCase")]`,Tauri 序列化为 snake_case `project_slug`,而前端 TS 接口(`src/lib/types.ts:1-8`)按 camelCase `projectSlug` 读取,导致 `c.projectSlug` 恒为 `undefined`。前端过滤 `c.projectSlug === encodeCwdSlug(projectCwd)`(`src/routes/memory/+page.svelte:36-40`、`src/routes/+layout.svelte:317-320`)恒为 false,选中 cwd 后永远显示不出对应 `~/.claude/projects/<slug>/memory/*.md`。当 `projectCwd === ""` 时走"不过滤显示全部"分支,掩盖了该 bug。

**修复:** 给 `MemoryFileCandidate` 加 `#[serde(rename_all = "camelCase")]`(单处后端改动,无需动前端)。slug 编码逻辑两端已一致(TS `encodeCwdSlug` / Rust `encode_cwd`),无需改。

**验证:** 选中某 cwd,确认其 `memory/` 下文件正确列出;切换 cwd 时列表随之更新;全局/All Projects 模式不回归。

### A2. 记忆生成只在群聊触发(任务8)

**根因(已确认):** `auto_extract_memories`(`src-tauri/src/group_chat/memory_extraction.rs:119`)全仓库唯一调用点在 `src-tauri/src/group_chat/orchestrator.rs:531`(`run_group_chat_turn` 内)。普通 `/chat` 会话走 `commands/session.rs`,仅在第 800 行调 `inject_memories_into_prompt`(读路径),从不触发抽取。因此普通聊天产生零新记忆,`/memory-mgmt` 的抽取配置形同虚设。

**修复:** 在普通 `/chat` 会话一个回合结束后(`commands/session.rs` 中回合 settle 处)接线 fire-and-forget 的 `auto_extract_memories`,喂入该回合的 user 消息 + 助手回复。

- 复用现有去抖/上限机制(`memory_extraction.rs:46` `can_extract`),但去抖 key 从 `group_chat_id` 推广为通用 source key(普通会话用 run_id)。
- 现有全局 50/天上限(`memory_extraction.rs:57-67`)改为 per-source 计数,避免普通会话多开时被群聊额度挤占。
- 抽取出的记忆 `source` 字段标记会话来源(run_id),与群聊 `source_group_chat_id` 区分。

**验证:** 在普通 `/chat` 完成一轮对话后,确认 `memory.db` 出现新记忆;`/memory-mgmt` 列表能看到;去抖与 per-source 上限生效。

---

## 批次 B — 委员会直播 UI 重设计(任务3 + 4)

所有改动集中在 `src/lib/components/invest/CommitteeLiveTab.svelte`(当前 678 行单文件)。重设计时按职责拆出子组件:`SymbolCardHeader`、`PipelineBar`、`RegimeChip`、`StepCard`、`VerdictBlock`。数据来源不变(`invest-committee-store.svelte.ts` 的 `SymbolProgress` / `RegimeStepData` / `RoundOutputSummary`)。

### B1. 卡片头部行重排(任务3)

头部行新顺序:`名字 | 持仓/观察 | REGIME chip | 空档(spacer) | 进度条 | 判断 | 运行按钮 | 展开图标`。

- **REGIME 上移到头部:** REGIME 是确定的事实数据,做成头部一个 chip(状态名 + 关键指标 RSI-14 / MA20 方向 / 价格分位)。数据取 `SymbolProgress.regimeData`。当 `regime === 'unknown'` 或 `regimeData` 为空时,chip **淡化显示**(降低不透明度,不隐藏)。
- **进度条瘦身:** 从当前 `flex:1` 撑满中间改为固定宽度(约 148px,响应式见 B3),用 spacer 顶开。仍是 7 段等宽 segment(macro/regime/quant_r1/risk_r1/quant_r2/risk_r2/cio),颜色与状态逻辑不变(`pipeline-config.ts` `getStepState`)。

### B2. 展开后列布局 + 内部渲染(任务4)

- **移除展开体内独立 REGIME 框:** 当前 `CommitteeLiveTab.svelte:182-194` 的 `regime-box` step-card 删除(已上移头部)。展开后的 flow-grid 不再含 REGIME 格。
- **列宽调整:** 宏观 Macro / CIO / 最终判决 = 全宽(`grid-column: 1 / -1`);量化 Quant + 风控 Risk = 两列对半(`1fr 1fr`)。全宽块宽度等于量化+风控两列之和(即占满 grid 宽度),不再是当前的 65%/max-560 居中。
- **内部内容用方案 A(结构化字段卡):** 不依赖 Markdown。把后端解析的 `RoundOutputSummary.parsed` 字段映射成结构化视图:
  - signal/verdict chip(带 `sig-buy`/`sig-sell` 等既有配色)
  - 置信度 chip + 迷你进度条(`confidence` / `strength`)
  - 一句话结论(`oneLiner`)
  - key-value 字段表,按角色取对应字段:
    - 宏观:`marketPhase`、`emotionTemperature`、`marketPhaseReason`
    - 量化:`buyPointAssessment`、`valuationAssessment`、`moneyFlow`
    - 风控:`concentrationPct`、`dryPowderCny`、`pnlPct`、`stockRiskSummary`
    - CIO:`catalystTier`、`catalystSummary`、`executionMode`、`firstTrancheCny`
  - 最终判决块:verdict 徽章 + 置信度 + Gate1/Gate2 + reasoning + sentinel override(沿用现有 `getVerdictBadgeStyle`)
- **fallback:** 当 `parsed` 解析失败(`fallbackReason` 存在或字段缺失)时,退回显示 `parsed.rawText` 原始文本(即现状方案 B)。

### B3. 宽度响应式自适应(任务3/4)

采用**响应式断点**策略(非纯 clamp,非最小改动):

- **宽屏:** 头部 REGIME chip 显示完整(状态名 + RSI/MA20/分位);展开体 flow-grid 双列;全宽块占满。
- **中等屏:** flow-grid 用 `min-max` 区间 + 百分比让全宽块与双列平滑收缩。
- **窄屏(断点,沿用现有 `<700px` 思路):** REGIME chip 只保留状态名、隐藏指标;flow-grid 双列塌为单列;进度条可进一步收窄或换行。
- 用 CSS 媒体查询/容器查询实现断点;chip 内指标用条件渲染控制显隐。

**验证:** 在宽/中/窄三档屏宽下,头部行不溢出、进度条不被挤变形、展开列布局合理;`unknown` regime chip 正确淡化。

---

## 批次 C — 功能删除、数据源补全、llm_config 迁移(任务1 + 2 + 5 + 6)

### C1. 删除两个前端 tab(任务5)

- **委员会 - Tool 调用 tab:** 删 `CommitteeToolsTab.svelte`、`/invest` 中 `tools` sub-tab 注册、相关 i18n。直播卡内的简版 tool-strip(`CommitteeLiveTab.svelte:410-417`,同一份 `toolCallHistory` 的精简视图)一并移除。store 的 `toolCallHistory` 字段及 `tool_call` 事件处理保留(后端仍发送,前端不再展示;确认无其它消费方后可在后续清理)。
- **系统 - 市场 Regime tab:** 删 `SystemRegimeTab.svelte`、`/invest` 中 `regime` sub-tab 注册、`get_regime_classification` 命令(`commands/invest.rs:1354-1378` + `lib.rs:476` 注册)、相关 i18n。
  - **保留 `src-tauri/src/invest/regime.rs`**——委员会 pipeline 第 2 步、Quant/Risk/CIO 角色 prompt 硬规则、parser 字段、前端 7 节点 pipeline 全依赖它。只删独立的单股分类 UI 入口。

### C2. 孤儿后端命令清理 + 接线少数有用项(任务1)

探索确认 22 个"已注册但前端无 invoke"的命令。处理策略:**清理为主 + 接线 2 个真正有用的**。

- **删除(deprecated + 已被内部路径/record_trade 取代):** `add_holding`、`update_holding`(已标 `#[deprecated]`)、`delete_holding`、`save_event`、`save_event_source`、`get_event_sources`、`save_verdict`、`save_pnl_snapshot`、`set_initial_cash`、`update_cash`、`sync_trade_calendar`、`is_trading_day`、`get_daily_bars`、`get_strategy`、`get_scheduler_logs`、`get_tracking_status`、`list_all_tracking`、`generate_daily_report`、`list_daily_reports`、`run_committee`(非流式遗留)。删除函数定义 + `lib.rs` 注册项。
  - 注:`try_parse_provider_id` 被 `build_committee_config` 使用,若删除 LLM 配置相关命令需保留或内联它(见 C4)。
- **接线(真正缺 UI 的有用功能):**
  - `restart_python_runtime`(`commands/python_status.rs:60`):在 Python 状态相关 UI(`PythonSetupOverlay.svelte` 等)加"重启 Python 运行时"按钮,解决 Python 卡死时只能重启整个 app 的问题。
  - `save_memory`(`commands/memos.rs:58`):在 `/memory-mgmt` 加"手动新增记忆"入口,调用该命令写入 `memory.db`。

### C3. 数据源检测补全(任务6)

当前 `get_datasource_health`(`commands/invest.rs:1380-1564`)只检测 6 个源。补全实际在用但漏检的:

- 腾讯实时分笔(`http://qt.gtimg.cn`)
- 腾讯 CSI300 K线(`https://web.ifzq.gtimg.cn`)
- Tushare 新闻 API(`major_news` / `anns_d`,与行情 API 权限位不同)
- AkShare 债券收益率(`bond_yield_10y`)
- AkShare 市场统计(涨跌停家数 `market_stats`)
- Yahoo history(区别于已测的 quote)
- Python 运行时本身(`crate::python::require()`)——显式暴露根因,Python 死会同时打挂 yfinance/AkShare/Jin10 三类
- invest.db schema 校验(当前仅打开连接)

LLM provider 连通性检测:**取决于 C4 的迁移结果**。若 `llm_config.json` 迁移并删除,则数据源检测里的 "LLM Config" 行随之移除;LLM 走 CLI 后不再需要单独检测 endpoint。

### C4. event 分析迁移到 CLI + 删除 llm_config.json(任务6 关联)

**背景(已确认):** 委员会已全走 Claude CLI(`CliCommitteeExecutor`,凭据走 `UserSettings.platform_credentials`),不读 `llm_config.json` 的 api_key。但 `event_scanner` 和 `event_analyzer` 仍用 `OpenAiCompatClient` + `llm_config.json` 走 HTTP,经 `build_scan_clients`(`commands/invest.rs:1103`)每 10 分钟跑一次。

**迁移:**
1. 把 `event_scanner::normalize_events`(`event_scanner.rs:165-205`)和 `event_analyzer::analyze_pending_events`(`event_analyzer.rs:113-159`)的 LLM 调用从 `OpenAiCompatClient::chat_stream` 迁移到 `CliCommitteeExecutor`(它们本就是短文本补全调用),凭据改走 `UserSettings.platform_credentials`。
2. 迁移后删除:
   - `src-tauri/src/invest/llm/client.rs`(`OpenAiCompatClient` + `resolve_api_key` + `get_llm_config_path`)
   - `orchestrator.rs` 中死代码:`build_llm_config`(663)、`llm_call_with_retry`(694)、`retry_on_fallback`(1210)、`run_with_tool_loop`(1258)及相关 import
   - `build_scan_clients`(1103)、`get_datasource_health` 的 LLM-Config 分支(1442-1497)
   - `get_llm_config` / `save_llm_config`(661-770)及 `InvestLlmConfig` / `InvestLlmProviderConfig` 结构、`default_llm_config`、相关 parse 辅助
   - `lib.rs:440-441` 命令注册
   - 前端 `ProviderConfigPanel.svelte`、`invest-committee-store.svelte.ts:495-509`(`loadConfig`/`saveConfig`/`llmConfig`)、`SystemDatasourceTab.svelte` 的 LLM provider 部分
   - i18n key:`invest_committee_llm_config`、`invest_committee_provider`、`invest_committee_provider_default`、`invest_committee_provider_hint`、`invest_committee_debate_rounds`、`invest_committee_no_config`、`invest_system_ds_llm_none`
3. **保留委员会调参:** `debate_rounds` / `selected_provider` / `timeout_secs` / `max_concurrent_symbols` 当前嵌在 `InvestLlmConfig`,删除前迁移到一个更小的结构或 `UserSettings`,供 `build_committee_config` 继续使用。

**风险控制:** 必须先完成步骤 1(迁移)再做步骤 2(删除)。若先删,委员会不受影响但 event_scan/event_analyzer 会每 10 分钟报"无可用 LLM provider",事件监控自动分类停更。

### C5. 数据库清理(任务2)

**死代码 + 遗留目录(可逆性高,先做):**
- 删 `src-tauri/src/storage/invest/round_cache.rs`(整个模块 `#[allow(unused)]`,全仓库无调用,声明的 `round-cache/` 目录实际不存在)
- 删 `~/.claw-go/rooms/` 遗留目录(代码库无任何引用,已迁移到 `group-chats/`)

**只写不读的表数据(删行不可逆,需逐项核对后删):**
- `daily_reports` 表存量行(UI 改用 `reports/daily_*.md` 文件,`list_daily_reports` 已删)
- `event_sources` 表存量行(无配置 UI,运行时不读)

**孤儿 IPC 写入的脏数据(逐项核对后删):**
- `verdicts` 表中历史上通过 `save_verdict` 写入、绕过 daily-overwrite 的混杂行
- `pnl_snapshots` 表中通过 `save_pnl_snapshot` IPC 写入的测试数据

**空转的 dreaming 记录(用户补充):**
- `domain_insights` 表中 dreaming 周期空转产生的几万行无效 insight。清理标准:实现时先查询确认特征(如空内容、无关联 verdict、批量重复),核对后删除。

**清理方式:** 提供一个一次性清理命令或脚本(只读核对 → 报告数量 → 用户确认 → 删除),不在正常启动路径自动删。删行操作前必须先 SELECT 报告将删除的行数与样本。

---

## 批次 D — /usage 余额卡片(任务9)

### D1. 新增 PackyAPI 余额卡片

**接口(已实测确认):** PackyAPI 是 New-API 系平台。
- 端点:`GET https://www.packyapi.com/api/user/self`
- 必需:`Cookie: session=<...>; TDC_itoken=<...>` 三值之一 + 请求头 `New-Api-User: <用户ID>`
- 响应:`data.quota`(剩余额度)、`data.used_quota`(已用)、`data.display_name`、`data.group` 等。单位换算 **500000 = $1**。
- 实测样本:quota=52819275(≈$105.6)、used_quota=397680725(≈$795.4)。

**前端:** 在 `routes/usage/+page.svelte` 的 Balance Card 内,DeepSeek 面板右侧新增 PackyAPI 面板。网格从 `md:grid-cols-2` 调整为容纳三个 provider(DeepSeek / PackyAPI / 小米),小米保持独占一行。复用 `balanceStatusText(source)` 显示逻辑,后端写 `cache["packyapi"]` 即可。凭据输入区(session / TDC_itoken / New-Api-User 用户ID 三个字段)做成可折叠,类似小米。

**后端:** `commands/balance.rs`:
- `refresh_balance_status` 的 source 枚举加 `"packyapi"`(`balance.rs:347` + 前端 `api.ts:480-485`)
- 新增 `query_packyapi_balance`,模仿 `query_mimo_balance`:带三个 cookie/header 值 GET `/api/user/self`,解析 quota → 格式化为 `"$105.60 (剩) / $795.40 (已用)"`,写回 `helper.cache["packyapi"]`
- 凭据字段加到 `BalanceHelperSettings`(`models.rs:493-519`)+ `apply_balance_helper`(`settings.rs:702-739`)
- i18n:`messages/zh-CN.json` + `en.json` 加 `settings_balance_packyapi` 等键

### D2. cookie 自动续期(小米 + PackyAPI)

**根因(已确认):** `commands/balance.rs:353-356` 的 `reqwest::Client` 未启用 `cookie_store`,也不解析响应 `Set-Cookie`。小米 `serviceToken` 是短寿命 JWT,平台靠 `slh`/`ph` 滚动 cookie 在每次响应 `Set-Cookie` 续期;静态 cookie 过期(几小时到一天)即失效,用户被迫手动重取。PackyAPI 的 session/TDC_itoken 同理会过期。

**修复:** 采用 `cookie_store` 自动续期:
- `reqwest::Client::builder().cookie_store(true)`,请求前注入存盘的 cookie 到 cookie jar
- 请求后从 cookie jar 取回最新的 `serviceToken`/`slh`/`ph`(小米)与 `session`/`TDC_itoken`(PackyAPI),回写到 `BalanceHelperSettings` 持久化
- 失效时(401/403 或 body 业务错误码如小米 `code != 0`)给前端明确提示,引导重取
- 这条修复同时覆盖小米与 PackyAPI 两个卡片

**验证:** 粘贴一次有效 cookie 后,持续刷新一段时间(跨越原 token 寿命),确认 cookie 被自动续期、不再频繁要求手动重取;失效时提示清晰。

---

## 测试与验证策略

- 每个批次独立 commit,Conventional Commit 风格。
- Rust 改动用 `cargo check`(本机 Rust 单测有 `STATUS_ENTRYPOINT_NOT_FOUND` 运行时问题,见 CLAUDE.md §11)。
- 前端:`npm run check`、`npm run lint`、相关 `*.test.ts`、`npm run build`。
- 全局:`npm run i18n:check`(所有删除/新增 i18n key 后必跑)。
- 删行类 DB 操作:先 SELECT 报告 → 用户确认 → 删除,不可逆操作前留样本。
- 收尾:`npm run verify`。

## 实现顺序

1. **批次 A**(纯 bug,低风险)
2. **批次 B**(UI 重设计,需视觉验证)
3. **批次 C**(改动面最大;C4 内部务必"先迁移后删除";C5 删行前核对)
4. **批次 D**(usage 卡片)

每批完成后更新相关 `docs/`。
