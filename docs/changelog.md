# Changelog / 更新日志

## v5.5.3 (2026-06-19)

### invest 定时调度器 9 项修复(根因 + 运行时实证)

读 invest.db 6.1 万行 scheduler_logs + scheduler.json 实证确认 11 个 bug,经 subagent 逐任务实现 + 两阶段审查 + 全分支 final review。完整计划见 `docs/superpowers/plans/2026-06-19-invest-scheduler-fixes.md`。

**根因(每分钟空转 / 漏执行):**

1. **cron 读取归一化(根因 #13)**: `config.rs::load_jobs_base()` 读取磁盘覆盖时未调 `normalize_cron_6field`(归一化只在 `update_cron`/`save_dream_config` 写入路径做)。任何被持久化成 5 字段的 cron(`scheduler.json` 里 `dream_invest` = `"0 3 * * *"`)→ `compute_next_run_for_job` 的 `Schedule::from_str` 失败 → 返回 `None` → 主循环 `None => true` 每 tick(~60s)触发。实证:dream_invest 在 DB 中 12377 条日志、9521 条 `insights_written:0` 空转快照。修复:读取时也归一化 + 解析失败加 `log::warn!`。
2. **skip 不推进 next_run(#14)**: `requires_trading_day` 任务非交易日被 skip 时不更新 next_run,下一 tick 仍判 due、每分钟刷 `skipped` 日志。新增纯函数 `config::should_fire(job, now)`,主循环 to_fire 收集块改为 for 循环,skip 分支推进 next_run + dirty/批量 save_jobs。

**双重调度(#1/#2):**

3. **删除 lib.rs pnl_snapshot 独立循环**: lib.rs 有一段硬编码 9:30/11:00/13:00/15:00(注释写 "Beijing time" 实际用 `chrono::Local`)的 spawn 循环,与 scheduler 的 pnl_snapshot 任务并存,导致同一时刻插两条快照 + 两份日志(status `success` vs `ok`)。删除副本,default_jobs cron 改为 `"0 30 9,11,13,15 * * 1-5"` 覆盖 4 个盘中时刻。
4. **删除 lib.rs event_scan 独立循环**: `spawn_event_scanner_cron`(每 30min)与 scheduler event_scan 任务重复扫描 Tushare/LLM。删除副本,event_scan 改由 scheduler 单一驱动。

**持久化与并发健壮性(#3/#4/#7/#8):**

5. **save_jobs 原子写 + 串行化锁**: `save_jobs` 从 `std::fs::write` 非原子覆盖改为 tmp+rename + PermissionDenied 重试 3 次(沿用 `committee/queue.rs::save_queue` 范本);新增模块级 `SCHEDULER_FILE_LOCK: Lazy<Mutex<()>>`,在 `load_jobs_base`/`save_jobs` 入口串行化(两段式 load→save 无嵌套,无重入死锁)。
6. **per-job 互斥 + panic 兜底 + CancellationToken**: 新增 `RUNNING_JOBS: Lazy<Mutex<HashSet<String>>>` + `try_acquire_job`/`release_job`/`JobGuard`(Drop 释放),主循环/dedicated 循环/`trigger_cron_job` 三处不再并发跑同一 job;dispatch 用 `tokio::spawn(fut).await` 包裹,task panic 由 JoinHandle 捕获为 error 日志而非杀死整个循环;`start` 接 `CancellationToken`,所有 sleep 改 `tokio::select!`,app 关闭时优雅退出 + `RUNNING` 复位。`run_job_guarded` 返回 bool,锁竞争跳过时主循环不误推进 next_run(避免 cron 触发被静默吞掉)。

**时区与交易日(#5/#6):**

7. **beijing_today + 交易日判定收紧**: 新增 `storage::invest::scheduler::beijing_today()`(`Utc::now() + 8h`,北京时间日历日,无 05:00 cutoff,区别于 `get_invest_date` 业务日),主循环交易日判定改用它,不再受宿主机时区影响;`is_trading_day` 在 trade_calendar 表无记录时加 `log::warn!` 后保留 weekday 回退(让节假日误跑可观测)。

**数据治理(#11):**

8. **dream_snapshots 保留清理**: 新增 `dream_snapshots::prune_keep_recent(20)`,用单条 DELETE + `ROW_NUMBER() OVER (PARTITION BY dream_type ORDER BY created_at DESC, id DESC)` 每类型保留最近 20 条;启动时一次性回收 #13 产生的 9500+ 空转快照。
9. **scheduler_logs 保留清理**: 新增 `scheduler::prune_scheduler_logs(30)`,删除 started_at 早于 30 天的记录(cutoff 用与 `log_task_start` 同一 `to_rfc3339_opts(Millis, true)` 保证字典序=时间序);启动时一次性清理(jin10 每 15s 一条,6 万行积压)。

**集成修复(全分支 final review 抓出)**: 删除主循环 fire 段冗余的 last_run/next_run 双写——`persist_job_status` 已是权威路径,外层用 tick 顶部陈旧快照再写会覆盖 dedicated loop(jin10/event_analyzer)期间的更新,导致 UI 时间戳偶发倒退。

**工作流记录**: 因与并行 committee session 共享主工作树发生分支交错事故,迁移到独立 git worktree 隔离后完成;dream_snapshots 表清理与 committee session 协调归本批处理。本机 Rust 单测因 STATUS_ENTRYPOINT_NOT_FOUND(CLAUDE.md §11)无法运行,验证以 `cargo check --tests` + 纯逻辑单测(供 CI)为准。

## v5.5.2 (2026-06-18)

### 委员会直播 settings 文件并发竞态修复 + 国际指标反序列化修复

**委员会 CLI settings 并发竞态 (P0):**

1. **`session-committee-*.json` 被并发删除导致 "Settings file not found"**: 直播页 `_drainQueue` 并发启动多个标的(`maxConcurrent`),每个标的调用 `run_committee_stream` → `build_committee_config` → `write_committee_settings_json`,后者每次都调 `cleanup_old_committee_settings()`。旧实现**无差别删除所有** `session-committee-*.json`;单个标的完整跑(4 角色 × 2 轮 + CIO,含重试/超时)要几分钟、全程复用同一份 `--settings` 文件,后启动的标的会把前一个**正在用**的配置文件删掉 → 该标的下一次角色 CLI 调用报 `claude CLI exited 1: Settings file not found`。修复:cleanup 改为**基于文件修改时间**,只回收超过 `COMMITTEE_SETTINGS_MAX_AGE`(2 小时)的陈旧文件;当前批次刚生成的文件 mtime 仅几秒,远在窗口内不会被误删,跨会话历史残留仍正常回收;读不到 mtime 时保守保留。
2. **并发扫描风暴 (simplify/efficiency)**: cleanup 每标的调一次,N 个标的几乎同时启动会触发 N 次同目录 `read_dir` + 逐文件 `stat`,而清理一批只需做一次。新增 `COMMITTEE_CLEANUP_COOLDOWN`(5 分钟)冷却门 + `AtomicU64` 记录上次扫描时间,把并发启动时的扫描收敛成单次。

**国际指标反序列化修复:**

3. **`BondYield10y` / `MarketStats` 去掉 `#[serde(rename_all = "camelCase")]`**: 两个结构体来自 AkShare(Python)返回的 snake_case 字段,camelCase 重命名与实际负载不匹配。沿用 v5.3.1 NewsItem 同类修复的根因处理(移除 rename_all,对齐 Python snake_case)。

**取舍记录**: 委员会 settings 文件采用基于年龄的回收,而非"完成即删"的生命周期清理——常规 session 路径(`session-{run_id}.json`)本就从不清理这些临时文件,无既定生命周期约定可对齐;且该文件整批共享、跨多个并发标的复用,真要"完成即删"需引入 Arc + Drop 引用计数这套代码库里不存在的新机制,只换来更及时清理、不换来正确性。2 小时阈值相对单标的最长约 20 分钟有 6× 安全余量。

## v5.5.1 (2026-06-18)

### 委员会直播修复 + invest Dashboard 优化 + 全模块金额精度统一

两个独立的 invest 模块迭代（委员会直播 6 项修复 + Dashboard 6 项优化），各自经子代理逐任务实现 + 独立审查 + 全分支审查；全分支审查额外抓到两个逐任务审查漏掉的真实功能 bug。完整设计见 `docs/superpowers/specs/2026-06-18-committee-live-and-dashboard-design.md`。

**委员会直播 (Part A, 6 项):**

1. **跨重启恢复完整进度**: `queue.rs` 的 `QueueItem` 新增不透明 `progress` 透传字段（后端只存不解析）；前端 store 拥有 `PersistedProgress` schema，把每标的跑完的完整进度（completedRounds/result/regimeData/failedSteps）序列化进队列文件。`loadQueue` 每进程只恢复一次（单例 store，切 tab 不再覆盖实时状态），被重启中断的 running/queued → aborted（不自动续跑，避免启动即调付费 LLM）。
2. **卡片独立运行/中止按钮**: 移除多选框 + 运行所选 + 全选；每张标的卡片改为单个"运行/中止"切换按钮（queued 时禁用），顶部保留全部运行/中止全部/并发选择器/Include Watch。
3. **解析误报弱化 + parser 字段对账**: `missing_critical_fields`（有原文、仅缺字段）不再用大黄条盖住内容，正常渲染原文 + 附小 chip；只有真空 fallback（worker_unavailable/empty_text/cli_error）才显示警告条。parser 补齐 CIO `execution_mode`/`first_tranche_cny` 与 Macro `signal_reason`/`market_phase_reason` 提取，顺带修复一个自 L4 移除起就无法编译的陈旧测试（`cargo check --tests` 才暴露）。
4. **prompt 精简输出**: 6 个角色 prompt 统一加"每字段值一句话结束、理由类 ≤ 一句话、结构化列表保持既定条数"约束，省 token。
5. **卡片排版修复**: `.step-body` 改纯文本 `white-space: pre-wrap`（对齐 demo），消除单换行折叠与 markdown 块级双空行。
6. **关键 chip + demo 视觉对齐**: 前端 `RoundOutputSummary.parsed` 接口扩展声明后端已序列化的全字段；每个 step 卡片顶部按角色渲染 signal/strength/verdict/买点/估值/资金流/集中度/催化剂等 chip（枚举值英文原样，标签走 i18n），配色对齐 demo。

**invest Dashboard (Part B, 6 项):**

7. **A 股 T+1 冻结股数**: `list_holdings` 读取时从"今日 buy trades"实时聚合算出 `frozen_shares`（不加 DB 列、不存储——`record_trade` 本就从 trades 全量回放，故跨交易日天然解冻、多笔买入天然累加、零迁移），随 `Holding` 返回前端。
8. **store 派生值**: 新增 `dailyPnl`/`dailyPnlPct`（组合当日收益，实时聚合非快照）、`todayTradedShares`（今日买卖股数）、`latestVerdictMap`（每标的最新委员会评级）。
9. **持仓明细表扩展**: HoldingsTable 从 7 列扩成 ~16 列（名称/代码/类型/持仓/可用/冻结/成本/现价/市值/盈亏/盈亏%/当日盈亏/当日%/仓位/今买/今卖/评级），watch 行非持仓列显示 `—`，容器横向滚动。
10. **KPI 卡重构**: 总收益率 → 总收益（大字金额 + 小字百分比），新增当日收益卡，删除宏观快照卡 + 最新裁决卡（含删除两个组件文件）。
11. **评级新鲜度过滤**: 持仓表评级列只在委员会 verdict 为最近一个交易日内（4 自然日窗口容忍周末）才显示 chip，过期显示 `—`，复用已加载的 `verdicts`（无新命令）。
12. **归档文件名带股票名称**: 委员会归档 `{symbol}.md` → `{symbol}_{name}.md`（sanitize 文件系统非法字符 + 控制字符 + 尾部点/空格，保留中文），markdown 标题带名称，`load_archive` 兼容新旧两种文件名。

**全分支审查抓到的真实 bug (2 项):**

13. **同日买卖少算可用股数**: T+1 冻结量按"今日买入"算未扣同日卖出，做 T 后可用股数被错误显示为 0。修正为 `frozen = min(今日买入, 持仓)`（不采纳审查建议的"买−卖净额"——对 T+1 语义是错的，会把已交收股标记为可卖）。
14. **UTC/本地日期错配凌晨丢单**: trades 的 `created_at` 存 UTC，但 invest 统计日期是本地（5AM 截止），SQL 用 `substr(created_at,1,10)` 取 UTC 日期 → A 股早盘 00:00–08:00 下的单匹配不上，冻结量/今日买卖列静默读成空。修正为 `date(created_at, 'localtime')` 先转本地再取日期，前端今日买卖聚合同源对齐。

**全模块金额精度统一:**

15. **所有 ¥ 金额统一 3 位小数**: 抽共享 `formatYuan()`（千分位 + 固定小数 + 可选正负号）与 `normalizeConfidencePct()`，覆盖 KPI 卡/持仓表/CommitteeLiveTab/TradeDialog/SystemPnlHistoryTab/TradeLogTab/PnlChart；消除同一现金在不同页面显示 2 位 vs 3 位的不一致。CSV 导出保留裸 `toFixed(3)`（避免 ¥/千分位逗号破坏列），技术指标（RSI/MA/波动率/延迟/strength）保持原有精度。

**取舍记录**: T+1 冻结采用"读取时算"而非存储列；评级列复用已加载 verdicts 而非新增批量命令；`isVerdictFresh` 保留 4 天窗口（刻意的周末/节假日容差）；`trades(action, trade_date)` 索引留待 trades 表增大后再加。

## v5.5.0 (2026-06-18)

### 委员会直播 UI 重构 — Debate Flow Card + 动态执行队列 + Abort

把委员会直播页从"批量一键运行 + 7 步纵向圆点列表"重构为"动态执行队列(可追加/可中止/可重试) + 菱形 debate flow 卡片布局"，后端通过 `CancellationToken` 真正取消进行中的 pipeline。

**后端取消机制 (3 项):**

1. **`queue.rs` 持久化模块**: 新建 `committee/queue.rs`，把直播队列 + portfolio 快照序列化到 `~/.claw-go/invest/committee-queue.json`（tmp+rename+PermissionDenied 重试，与 `storage::runs::save_meta` 同模式）。前端是状态真相源，本模块只做 load/save。3 单元测试。
2. **`CancellationToken` 注入 orchestrator**: `run_committee` 新增 `cancel` 参数，在每个 phase 边界（macro/regime/debate/CIO 前）调用 `check_cancellation`；取消时先 emit `SymbolAborted` 事件再返回 `Err("aborted: {symbol}")`。`run_debate_rounds` 循环首行检查。`events.rs` 新增 `SymbolAborted` 变体。
3. **Tauri 命令**: `CommitteeCancelRegistry`(`Arc<Mutex<HashMap<String, CancellationToken>>>`) 作为 Tauri state；`run_committee_stream` 改造为每个 symbol 注册 token；新增 `abort_committee_symbol`/`abort_committee_all`/`load_committee_queue`/`save_committee_queue` 4 命令。

**前端队列调度器 (3 项):**

4. **store 队列状态机**: `InvestCommitteeStore` 导出 class，从"单批 runCommittee"改造成并发队列调度器——维护 `queue: QueueItem[]`，每次对单个 symbol fire-and-forget 调用 `run_committee_stream([symbol])`，前端 `maxConcurrent` 控制并发；`addToQueue`/`abortSymbol`/`abortAll`/`retrySymbol`/`setMaxConcurrent`/`loadQueue` 方法 + 300ms debounce 持久化。7 Vitest 测试(真 TDD)。
5. **`getStepState` aborted 状态**: pipeline-config 新增 `'aborted'` 返回值——已完成步骤保持 done，未完成步骤在 symbol 中止后显示为 aborted。3 Vitest 测试。
6. **CommitteeLiveTab 重写**: 菱形 debate flow grid 布局（macro→regime 分叉→quant/risk R1→R2→CIO 汇聚），SVG 连接线；每个 symbol 卡片可展开/折叠，运行中显示 ⏹ abort、完成后显示 ↻ retry；页面内并发选择器；队列进度文字。

**清理 (3 项):**

7. **并发设置迁移**: ProviderConfigPanel 移除并发上限设置（迁移到直播页），删除 store 的 `runCommittee` 兼容 shim。
8. **删孤儿组件**: `PipelineFlow.svelte`(0 引用) 被 debate flow card 取代后删除。
9. **i18n**: 14 个新 key(en/zh)。

**Simplify 代码审查修复 (4 路审查):**

10. **abort 不再误判为 failed**: `batch_stream` 的 `Err(e)` 分支对 `aborted:` 前缀的 error 跳过 `Error` 事件 emit（取消已由 `SymbolAborted` 表达，避免前端状态从 aborted 被覆盖成 failed）。
11. **regime phase 前补 check_cancellation**: macro→regime 之间存在未覆盖的 phase 边界（regime 含 Tushare HTTP 调用），补齐检查点。
12. **queue.rs 紧凑 JSON**: 机器读取文件改用 `to_string`（非 pretty），省 ~30% 字节。
13. **渲染热路径优化 + 死代码清理**: CommitteeLiveTab 用 `$derived` 的 `queueMap`/`toolMap` 消除 each 块内 O(assets×n) 重复扫描；`STEP_ICONS`/`STEP_ROUND` 合并进 `STEP_DEFS`(icon/round 字段)；`buildSnapshot` 双 map 合并为单次；删除 store 死字段 `activeSymbols` + LiveTab 冗余 `results.find` fallback。

**取消粒度权衡**: 取消在 phase 边界生效，不中断进行中的单次 LLM 调用（避免 HTTP 请求中途强杀）。stream 路径下后端 Semaphore 退化为 1，真正并发由前端控制。

## v5.4.3 (2026-06-17)

### 宏观指标 Tencent/AkShare 集成+委员会 L4 移除+Sanity Check 简化+CIO 输出修复+Prompt 精简

**宏观指标数据源集成 (7 项):**

1. **CSI300 Tencent K-line fallback**: `fetch_csi300` Tushare 失败或返回空时，自动 fallback 到腾讯财经 K-line API (`web.ifzq.gtimg.cn`)，同时获取 close + 20日年化波动率。
2. **10Y 国债 AkShare fallback**: `fetch_cgb_10y` Tushare 失败时，fallback 到 AkShare `bond_zh_us_rate`（Python RPC），仅请求近 90 天数据。
3. **3 个新指标**: `limit_up_count`(涨停家数)、`limit_down_count`(跌停家数) 通过 AkShare `stock_zt_pool_em/dtgc_em`；`two_market_volume`(两市成交额) 通过腾讯指数行情并发查询上证+深证。
4. **`tencent_quotes.rs` 扩展**: 新增 `IndexQuote`、`fetch_index_quote`、`fetch_csi300_kline`、`pub fn compute_vol20`（共享波动率计算），3 个单元测试。
5. **`international.rs` 扩展**: 新增 `BondYield10y`、`MarketStats` 结构体，`fetch_akshare_bond_yield`、`fetch_akshare_market_stats` RPC 方法。
6. **`akshare_market.py` 新 provider**: `bond_yield_10y()` + `market_stats(date)`，注册到 server.py。`market_stats` API 全部失败时返回 `{}`（非交易日不缓存 0）。
7. **`macro_cache` 15 指标全覆盖**: `ALL_INDICATORS` 全部有对应的 fetch 逻辑，测试断言从 12 更新为 15。

**CIO 输出缺失修复 (4 项):**

8. **CIO prompt 结构重组**: 16 个字段列表从 prompt 末尾移至最前面（紧接角色描述之后），确保模型优先输出 KEY: VALUE 格式。删除冗余"职责范围"段落和"现金仓位机会成本规则"。`length_constraint_suffix` 对 CIO 返回空（格式已在 prompt 中）。
9. **空 verdict 漏洞修复**: `detect_fallback_reason` 从 `is_none()` 改为 `is_blank()` 统一检查空字符串/空白，Macro/Quant/Risk 的 critical field 同步加固。
10. **Risk 用户行为模式分析删除**: Risk R1 删除 `query_dreaming_insights` 工具、情绪状态评估、历史模式；Risk R2 删除情绪化追涨规则、情绪重校准字段。减少 ~500 token prompt + 输出。
11. **CIO prompt L4 引用清理**: 删除"红灯规则"、"安全阀"、"情绪稳定"字段，CIO 不再引用已移除的 L4 Officer。

**委员会 L4 Officer 移除 (8 项):**

12. **角色精简 5→4**: 移除 L4 Execution Officer 角色及全部关联代码。Pipeline 从 8 步缩减为 7 步（Macro R1 → Quant R1 → Risk R1 → Quant R2 → Risk R2 → CIO）。
13. **`orchestrator.rs`**: 删除 `run_l4_officer_phase` 函数 + L4 启动/完成事件 + L4 相关 tool-call 逻辑（-160 行）。
14. **`roles.rs`**: 删除 `L4_OFFICER_PROMPT`、L4 角色定义、`load_prompt_for_round` L4 分支（-137 行）。
15. **`parser.rs`**: 删除 `parse_l4_officer`、`compute_red_light_score`、L4 相关解析逻辑（-154 行）。
16. **`cli_executor.rs`**: 删除 `build_cli_l4_prompt`、dreaming cache/formatting（-95 行）。
17. **前端 5 文件**: CommitteeRolesTab 移除 L4 卡片、CommitteeToolsTab 5→4 列、DebateBlock/PipelineFlow 移除 L4 颜色/步骤、pipeline-config backendIdx 7→6。
18. **`events.rs`**: `step_index_for_role` L4Officer 改为 sentinel 99（防碰撞），CIO 改为 6。
19. **`ParsedFields` 清理**: 移除 10 个 dead L4 字段（`l4_guard_clause`/`l4_guard_reason`/`l4_emotion_assessment`/`l4_red_light`/`l4_buy_point_ok`/`l4_check_emotion`/`execution_red_light_score`/`red_light`/`emotional_state`/`emotion_recalibrated`）+ `is_red_light()` 方法。

**CIO Sanity Check 简化 (3 项):**

20. **4 Gates → 2 Gates + Fallback**: 移除 Gate 3（子弹降级，v5.4.1 已标记始终 true）和 Gate 4（集中度调整）。新增 Gate 2 三重恶化守卫（macro risk_off + quant bearish + 深度亏损 → SELL）。Fallback 检查 WORKER_UNAVAILABLE + fallback_reason → HOLD。
21. **`cio_sanity_check` 签名简化**: 6 参数 → 4 参数（移除 `_min_cash_reserve`、`_actual_cash_cny`、`actual_concentration`，新增 `macro_strength`）。`SanityCheckResult` 移除 `gate3_pass`、`gate4_pass` 字段。
22. **`analysis.rs` 精简**: Gate 2 从集中度百分比检查改为三重恶化组合检查。

**Simplify 审查修复 (15 项):**

23. **vol20 代码去重**: `compute_vol20(&[f64])` 提取到 `tencent_quotes.rs` 为共享函数，`macro_refresh.rs` 和 `fetch_csi300_kline` 均调用同一实现。
24. **`fetch_two_market_volume` 并发化**: 两个腾讯 HTTP 请求从串行改为 `tokio::try_join!` 并发。
25. **Match arms 错误分离**: `fetch_csi300` / `fetch_cgb_10y` 的 `_` catch-all 拆分为 `Ok(_)`（空数据，info 日志）+ `Err(e)`（错误，warn 日志）。
26. **Python `bond_yield_10y` 性能**: `start_date=""` 改为近 90 天，从 19 页 HTTP 请求缩减为 1 页。
27. **Python `market_stats` 失败语义**: 两个 API 全部失败时返回 `{}` 而非 0 填充字典，Rust 侧产生错误而非缓存假数据。
28. **`compute_vol20` 签名**: `Vec<f64>` → `&[f64]`，消除调用侧不必要的堆分配。
29. **内联单次调用函数**: `fetch_csi300_tencent` → `csi300_tencent_fallback`（语义更清晰），`fetch_cgb_10y_akshare` → `cgb_10y_akshare_fallback`。
30. **测试简化**: vol20 测试直接调用 `compute_vol20()`，新增不足数据返回 `None` 测试。
31. **DebateBlock ROLE_COLORS 复用**: 本地重复定义改为 `import { ROLE_COLORS } from './pipeline-config'`。
32. **CIO prompt 重复格式指令消除**: `length_constraint_suffix` 对 CIO 返回空（格式已在 prompt 中）。
33. **Dead dreaming cache 删除**: `format_dreaming_insights_for_prompt`、`DREAMING_CACHE`、`clear_dreaming_cache` 及 orchestrator 两处调用。
34. **Broken tests 修复**: 更新 `test_length_constraint_mentions_verdict_for_cio`（空断言）、删除 `test_length_constraint_mentions_guard_clause_for_l4`。
35. **L4Officer step_index 碰撞修复**: 返回 sentinel 99 而非与 CIO 共享 6。
36. **detect_fallback_reason L4 arm**: 从 `l4_guard_clause.is_none()`（永远 true）改为显式 `return Some("l4_removed")`。
37. **Risk parser dead 提取清理**: 移除 `emotional_state`/`emotion_recalibrated` 提取行（prompt 已不输出这些字段）。

## v5.4.2 (2026-06-17)

### 委员会 R2 Fallback 误报修复+CIO 总资产注入

**R2 fallback 误报修复 (3 项):**

1. **`detect_fallback_reason` 轮次感知**: 函数签名加入 `round: u8` 参数。Quant R2（round≥2）只检查 `signal`，不再要求 `regime`（R2 prompt 不输出 REGIME，之前 100% 触发 `missing_critical_fields`）。
2. **CLI 路径增加重试**: `run_role_phase` 中解析触发 fallback 时，附加格式提醒重新调用 CLI 一次，仅在重试成功时替换结果。API 路径已有此机制，CLI 路径之前缺失。
3. **Quant R2 新增 2 个单元测试**: 验证 `regime=None` 不再触发 fallback（`test_detect_fallback_quant_r2_no_regime_ok`）、`signal=None` 仍正确触发（`test_detect_fallback_quant_r2_missing_signal`）。原有 10 个测试全部适配 round 参数。

**CIO 总资产数据注入 (2 项):**

4. **`build_cli_cio_prompt` 注入 portfolio 数据**: 新增 `portfolio_summary: &str` 参数，CIO prompt 包含持仓表、总市值、现金、总资产。修复 CIO 无数据时幻觉"总资产约 1,562 CNY"的根因。
5. **`build_portfolio_summary` 新增总资产行**: 输出末尾增加 `总资产: {:.2} CNY`，确保 CIO 和 Risk 角色使用一致的数据。

**附带修复 (1 项):**

6. **`portfolio.rs` 测试编译修复**: `trade_date` 字段 `.into()` → `Some(...)` 适配 `Option<String>` 类型。

### 委员会 CIO 回显误报修复+R2 解析增强+安全检查加固

**CIO `[WORKER_UNAVAILABLE]` 回显修复 (3 项):**

7. **`format_round_outputs_for_prompt` 使用原始 raw_text**: 不再将有 fallback 的输出替换为 `[WORKER_UNAVAILABLE]` 标记注入 prompt。旧方式导致 LLM 回显标记，`detect_fallback_reason` 误判 CIO 自身为 `worker_unavailable`。`cli_executor.rs` 和 `orchestrator.rs` 同步修改。
8. **CIO prompt 删除两条 `[WORKER_UNAVAILABLE]` 规则**: Hard Rules 中的 "worker 输出含标记 → HOLD+confidence≤0.4" 和安全阀中的第 3 条。这些安全逻辑已由 `analysis.rs` post-merge 检查兜底，prompt 中重复声明是回显 bug 的根因。
9. **`cio_sanity_check` 增加 `fallback_reason` 检查**: 原检查仅 `raw_text.contains("[WORKER_UNAVAILABLE]")`，`missing_critical_fields` 和 `empty_text` 不含标记不触发。现在同时检查 `fallback_reason.is_some()`，覆盖所有 fallback 场景。新增 `test_sanity_fallback_reason_without_marker` 测试。

**Quant R2 解析增强 (2 项):**

10. **`apply_r2_signal_override` 增加 signal key 变体**: 新增 `调整后信号`、`信号调整`。移除含空格的 `ADJUSTED SIGNAL`（`is_structured_key_line` 拒绝含空格的 key，会被 `merge_continuation_lines` 误判为续行）。
11. **`parse_quant` 补上 `调整买点` 提取**: R2 prompt 要求输出 `调整买点` 但 parser 完全没提取。新增 `调整买点`/`ADJUSTED_BUY_POINT` key，R1 的 `买点评估` 优先（`is_none()` 守卫）。

**i18n+文档 (2 项):**

12. **角色配置 UI 描述更新**: `invest_roles_sanity_3` 和 `invest_roles_hard_cio_4` 从 `[WORKER_UNAVAILABLE]` 改为 "任何角色输出异常"。zh-CN、en、roles-config-demo.html 同步。
13. **7 个新单元测试**: R2 signal 变体×3、调整买点×2、format_round_outputs×1、cio_sanity_check fallback_reason×1。

## v5.4.1 (2026-06-17)

### 委员会置信度逻辑重构+CLI 静默+hard_truncate 移除

**移除 hard_truncate 字符截断 (3 项):**

1. **完全移除 `hard_truncate`**: CLI 模式下 Claude 自身管理输出长度，prompt guidance 足够。硬截断是"缺少关键字段"解析失败的根因。
2. **删除相关函数**: `max_chars()`、`critical_field_keys()`、`strip_bold_markers()`、`hard_truncate()` 及 14 个相关测试。
3. **`length_constraint_suffix` 简化**: 移除具体字符数引用，改为 guidance-only：`"保持简洁，先输出关键字段，再输出详细分析"`。

**CLI 进程静默 (1 项):**

4. **`cli_executor.rs` 添加 `.hide_console()`**: 委员会 CLI spawn 设置 `CREATE_NO_WINDOW`，Windows 下不再弹出 cmd 窗口。

**置信度逻辑重构 (6 项):**

5. **移除 Gate 3（子弹降级）**: `cio_sanity_check` 不再因 `dry_powder < min_cash_reserve` 强制 `verdict=HOLD + confidence≤0.4`。子弹不足是仓位状态，不是决策质量问题。`gate3_pass` 字段保留（backward compat），始终为 `true`。
6. **红灯评分移除 dry_powder 规则**: `compute_red_light_score` 删除 `dry_powder_cny` 参数，移除 `dry_powder<1000 → k_score=8` 分支。k_score 仅基于集中度：`>60%→10, >40%→6, else→2`。
7. **Quant R2 prompt**: 删除"子弹 ≤ 单笔最小 cap → 可改 neutral"规则。信号评估应独立于资金状态。
8. **Risk R2 prompt**: 删除"DRY_POWDER_CNY < 1000 → 流动性风险升级"规则。
9. **CIO prompt**: `子弹的 10%` → `可用现金的 10%`，`子弹 50%` → `可用现金 50%`，`子弹不足时的 default` → `满仓时的 default`。
10. **`orchestrator.rs`**: 移除 `dry_powder_cny` 提取逻辑（L4 Officer phase），`compute_red_light_score` 调用从 4 参数改为 3 参数。

**代码审查修复 (1 项):**

11. **Mutex 中毒恢复**: `CLI_EXECUTOR.lock().ok()?` → `.lock().unwrap_or_else(|e| e.into_inner())`，中毒后可自动恢复而不永久不可用。

## v5.4.0 (2026-06-17)

### 委员会 CLI Executor 代码审查修复 — 15 项全量修复

**🔴 正确性 (5 项):**

1. **`run_macro_phase` 优雅降级**: CLI executor 未初始化时返回 `[WORKER_UNAVAILABLE]` degraded output 而非传播 Err 中断整个 pipeline。
2. **`run_role_phase` 优雅降级**: 同上，CLI 调用失败也返回 degraded output（含 `fallback_reason`），pipeline 继续执行后续角色。
3. **`tokens_used` 文档**: `run_macro_phase` 和 `run_role_phase` 添加文档说明 CLI 模式下 token 计数始终为 0（`claude --print` 不报告 usage）。
4. **Settings JSON 失败表面化**: `build_committee_config` 改为返回 `Result`，非默认供应商配置生成失败时返回明确中文错误，不再静默回退到错误供应商。
5. **RoleStart/RoleComplete 配对**: 通过 Fix 2 间接解决 — `run_role_phase` 永不返回 Err，debate 循环中 RoleComplete 总能发出。

**🟡 性能/资源 (5 项):**

6. **OnceLock → Mutex**: `CliCommitteeExecutor` 全局单例改用 `Mutex<Option<...>>`，允许 claude 安装后自动重试初始化，无需重启应用。`CliCommitteeExecutor` 实现 `Clone`（`Semaphore` 包裹 `Arc`）。
7. **Quant R1 prompt 去重**: 移除 `format_asset_context_for_prompt` 和 `precomputed_indicators` 的重复追加 — `load_prompt_for_round` 已通过 `{{placeholder}}` 替换注入。
8. **CIO prompt 去重**: 同上，移除重复的 asset context 追加。
9. **Dreaming 缓存**: 新增 `DREAMING_CACHE`（`once_cell::sync::Lazy<Mutex<HashMap>>`），Risk R1 和 L4 共享同一 symbol 的 DB 查询结果，batch 运行前 `clear_dreaming_cache()` 清空。
10. **Spawn 并发化**: `acquire_owned().await` 从 for 循环体移入 `tokio::spawn` 块内，task 创建不再串行阻塞。

**🟢 代码质量 (5 项):**

11. **原子文件写入**: `patch_settings_model` 改用 tmp+rename 模式，避免 crash 时部分写入。
12. **Temp 文件清理**: `write_committee_settings_json` 调用前自动清理旧 `session-committee-*.json` 文件。
13. **`resolve_provider` 去除 dead_code**: 该函数在 archive 路径中确实被使用，移除错误的 `#[allow(dead_code)]`。
14. **`format_risk_metrics_for_prompt` 去重**: 改为委托调用 orchestrator 的 `build_risk_metrics_context`，消除 ~50 行重复逻辑。`build_risk_metrics_context` 标记 `pub(crate)` 并去除 `#[allow(dead_code)]`。
15. **原子写入附带**: `patch_settings_model` 使用 `path.with_extension("json.tmp")` + `fs::rename`。

**涉及文件 (3):**
- `src-tauri/src/invest/committee/orchestrator.rs` — Fix 1/2/3/5/6/10/13
- `src-tauri/src/invest/committee/cli_executor.rs` — Fix 6/7/8/9/11/12/14
- `src-tauri/src/commands/invest.rs` — Fix 4

---

## v5.3.9 (2026-06-17)

### 委员会 CLI Executor Phase 2 — 全角色 CLI 模式 + 供应商统一 (7 tasks)

**Phase 2 核心改动:**

1. **Task 1 — 供应商统一 + CLI `--settings`**: `build_committee_config` 调用 `write_committee_settings_json` 生成临时 settings JSON，从 `UserSettings.platform_credentials` 读取供应商凭据。`CliCommitteeExecutor::run_role` 新增 `settings_path` 参数，注入 `--settings` 到 CLI 参数。
2. **Task 2 — 可配置并发**: `CommitteeConfig.max_concurrent_symbols` 替代硬编码 `MAX_CONCURRENT_SYMBOLS = 5`，前端 `ProviderConfigPanel` 新增并发数选择器（1/2/3/5/8/10）。
3. **Task 3 — CLI prompt 构建器**: 新增 `build_cli_quant_r1_prompt`、`build_cli_quant_r2_prompt`、`build_cli_risk_r1_prompt`、`build_cli_risk_r2_prompt`、`build_cli_l4_prompt`、`build_cli_cio_prompt` 6 个构建器，将预取数据嵌入 system prompt。新增 `format_asset_context_for_prompt`、`format_risk_metrics_for_prompt`、`format_round_outputs_for_prompt`、`format_dreaming_insights_for_prompt`、`fetch_company_news_for_prompt` 5 个数据格式化函数。
4. **Task 4 — Orchestrator CLI-first 重构**: `run_committee`、`run_debate_rounds`、`run_role_phase`、`run_l4_officer_phase`、`run_macro_phase` 移除 `client` 参数。`run_role_phase` 改为根据角色/轮次调用 CLI prompt 构建器 + `cli.run_role()`。`run_macro_phase` 移除 API fallback，纯 CLI 模式。
5. **Task 5 — API 代码清理**: `build_llm_config`、`resolve_provider`、`llm_call_with_retry`、`build_context_messages`、`retry_on_fallback`、`run_with_tool_loop`、`build_risk_metrics_context`、`default_role_temperature` 标记 `#[allow(dead_code)]`（API 调用链已断开，函数保留供未来参考）。
6. **Task 6 — 重试 UI**: `CommitteeLiveTab` 符号标签新增 🔄 重试按钮，调用 `store.runCommittee([symbol])`。
7. **Task 7 — 验证**: `cargo check` 零警告通过，`npm run check` + `npm run i18n:check` 通过。

**CLI prompt 注入数据清单:**
- **Quant R1**: AssetContext 全量 + 预计算技术指标 + REGIME 上下文 + 近期裁决
- **Quant R2**: 前序轮次输出（Quant R1 + Risk R1）
- **Risk R1**: 预计算风险指标 + AkShare 个股新闻 + 投资洞察 + 近期裁决 + 策略上下文 + 用户档案
- **Risk R2**: 前序轮次输出（Quant R1/R2 + Risk R1）
- **L4 Officer**: 前序轮次输出 + 投资洞察
- **CIO**: 前序轮次输出 + AssetContext + 策略上下文 + 用户档案 + 数据质量警告

**涉及文件 (8):**
- `src-tauri/src/invest/committee/cli_executor.rs` — 6 个 prompt 构建器 + 5 个格式化函数 + `fetch_company_news_for_prompt`(async)
- `src-tauri/src/invest/committee/orchestrator.rs` — CLI-first 重构 + API dead code 标记 + PortfolioData 字段 pub(crate)
- `src-tauri/src/commands/invest.rs` — 移除 OpenAiCompatClient 创建 + run_committee/run_committee_stream 简化
- `src/lib/components/invest/ProviderConfigPanel.svelte` — 供应商统一 + 并发数选择器
- `src/lib/components/invest/CommitteeLiveTab.svelte` — 🔄 重试按钮
- `src/lib/stores/invest-committee-store.svelte.ts` — maxConcurrentSymbols 类型
- `messages/en.json` + `messages/zh-CN.json` — 3 个新 i18n key

---

## v5.3.8 (2026-06-17)

### 委员会直播状态保留 + 并发限制 + 字符限制提升 (3 commits)

**状态保留 (Part A, 4 项):**
1. **`runCommittee` 限定重置范围**: `results`、`perSymbolProgress`、`toolCallHistory` 只清除本次运行的符号，保留其他符号已有状态。Run All 增量模式（跳过已完成），Run Selected 尊重用户选择（可重新运行已完成的）。
2. **`symbol_complete` 替换而非追加**: 事件处理器用 `findIndex + map` 替换已有结果，避免重复条目。
3. **`completedCount` 限定当前批次**: 只计算 `activeSymbols` 中已完成的符号，避免跨批次计数溢出。
4. **事件处理器优化**: `tool_call`/`committee_start`/`done` 等非变更事件提前 return，避免无意义的 `perSymbolProgress` Map 全量拷贝。

**并发限制 (Part B, 2 项):**
5. **`MAX_CONCURRENT_SYMBOLS = 5`**: `run_committee_batch` 和 `run_committee_batch_stream` 使用 `tokio::sync::Semaphore` 限制同时运行的 pipeline 数量，for 循环内 `acquire_owned().await` 阻塞直到有空位。
6. **Governor 互补**: 符号级限制控制 pipeline 并发，Governor（per-provider 8 并发）限制 LLM API 调用并发，两者不冲突。

**字符限制提升 (Part C, 3 项):**
7. **`max_chars()` 提升**: Quant/Risk 550→700，CIO 600→700，缓解 LLM 输出截断问题。
8. **`strip_bold_markers` 预处理**: `hard_truncate` 入口去除 `**` 粗体标记，节约字符预算给实际内容。
9. **测试断言同步**: `test_max_chars`、`test_length_constraint_suffix`、`test_hard_truncate_actual` 断言更新。

**代码审查修复 (3 项):**
10. **Fix 1: Run Selected 进度分母**: `selectedSymbols.size` → `store.activeSymbols.length`，避免运行中选择变化导致分母错误。
11. **Fix 2: 冗余 `runSetLocal` 移除**: 复用已有的 `runSet`，消除重复 Set 创建。
12. **Fix 3: 非变更事件 Map 拷贝优化**: `tool_call`/`committee_start`/`done` 事件不再触发 `perSymbolProgress` 全量拷贝和 Svelte 重渲染。

**涉及文件 (4):**
- `src/lib/stores/invest-committee-store.svelte.ts` — runCommittee 状态保留 + symbol_complete 替换 + 事件处理器优化
- `src/lib/components/invest/CommitteeLiveTab.svelte` — Run All 过滤已完成 + completedCount 限定批次 + 进度分母修复
- `src-tauri/src/invest/committee/orchestrator.rs` — Semaphore(5) 并发限制
- `src-tauri/src/invest/committee/roles.rs` — max_chars 700 + strip_bold_markers + 测试更新

## v5.3.7 (2026-06-12)

### 委员会单元测试修复 (1 项)

1. **`roles.rs` 单元测试断言同步**: `test_max_chars` 断言从 `600/350/350` 更新为 `500/550/550`（匹配 v5.3.6 中 `max_chars()` 实现值变更）；`test_critical_field_keys_risk` 断言从 `["SIGNAL", "信号"]` 更新为 `["SIGNAL", "信号", "风险信号"]`（匹配 Risk 角色新增关键字段）。

**涉及文件 (1):**
- `src-tauri/src/invest/committee/roles.rs` — 4 处测试断言更新

## v5.3.6 (2026-06-12)

### 委员会解析器增强 + 代码审查修复 (14 commits)

**解析器格式容错 (PR-A, 3 项):**
1. **`extract_field` 支持 6 种格式变体**: 分层 `strip_prefix` 匹配 `KEY: value`、`KEY：value`、`**KEY**: value`、`**KEY**：value`、`KEY=value`、`**KEY**=value`，消除 LLM 输出格式不一致导致的解析失败。
2. **`extract_list_field_any` 复用 `matches_key_line`**: 列表字段提取统一使用共享格式匹配函数，消除内联重复。
3. **`matches_key_line` 共享函数**: 从 `extract_field`、`extract_list_field_any`、`hard_truncate` 三处提取单一格式匹配源，新增格式变体只需修改一处。

**前端错误状态提示 (PR-B, 4 项):**
4. **`fallback_reason` 字段 + `detect_fallback_reason` 函数**: `ParsedFields` 新增 `fallback_reason`，检测 `worker_unavailable` / `empty_text` / `missing_critical_fields` 三种回退原因。
5. **orchestrator 管道接入**: `run_with_tool_loop` 两条成功路径均调用 `detect_fallback_reason`。
6. **前端 `failedSteps` + `getStepState` failed 状态**: `SymbolProgress` 新增 `failedSteps: Set<number>`，`getStepState` 支持 `'failed'` 返回值。
7. **CommitteeLiveTab 友好错误展示**: `WORKER_UNAVAILABLE` 等哨兵文本替换为本地化友好提示，使用 `t()` 国际化。

**hard_truncate 关键字段保留 (PR-C, 3 项):**
8. **`critical_field_keys()` 方法**: 每个角色定义关键字段列表（Macro→SIGNAL, Quant→SIGNAL+REGIME, Risk→SIGNAL, CIO→VERDICT, L4→GUARD_CLAUSE）。
9. **`hard_truncate` 保留关键字段**: 截断时优先保留关键字段行，非关键内容优先截断。关键字段超长时封顶到 `max_chars`。
10. **prompt 约束优化**: `length_constraint_suffix` 指示 LLM 先输出关键字段再输出详细分析。

**代码审查修复 (8 项):**
11. **Fix 1: error 路径接入 `detect_fallback_reason`**: orchestrator 三条 `[WORKER_UNAVAILABLE]` early-return 路径均调用 `detect_fallback_reason`，`fallback_reason` 不再为 `None`。
12. **Fix 2: `hard_truncate` max_chars 封顶**: 关键字段本身超长时，仅保留第一个关键字段截断到 `max_chars - 3` + `...`，不再超出硬上限。
13. **Fix 3: budget off-by-one 修复**: 非关键块与首个关键行之间的 `\n` 分隔符正确计入预算。
14. **Fix 4: `failedSteps` 填充**: store 的 `role_complete` 事件处理器检查 `fallbackReason`，有值时将对应步骤索引加入 `failedSteps`。
15. **Fix 5: Quant 回退 AND→OR**: 任一关键字段缺失即触发回退（原逻辑需两者同时缺失）。
16. **Fix 6: 测试修复**: `test_hard_truncate_preserves_*` 输入文本加长到超过角色 `max_chars`，确保截断逻辑真正执行。
17. **Fix 7: 格式匹配去重**: 三处独立的 6 变体格式匹配统一为 `matches_key_line` 共享函数。
18. **Fix 8: `getFallbackMessage` 国际化**: 硬编码中文替换为 `t()` 调用 + 4 个 i18n key。

**涉及文件 (9):**
- `src-tauri/src/invest/committee/parser.rs` — extract_field/matches_key_line/detect_fallback_reason/fallback_reason 字段
- `src-tauri/src/invest/committee/roles.rs` — critical_field_keys/hard_truncate/length_constraint_suffix
- `src-tauri/src/invest/committee/orchestrator.rs` — detect_fallback_reason 接入 (5 个调用点)
- `src/lib/stores/invest-committee-store.svelte.ts` — RoundOutputSummary/SymbolProgress 类型扩展 + failedSteps 填充
- `src/lib/components/invest/pipeline-config.ts` — getStepState failed 状态
- `src/lib/components/invest/CommitteeLiveTab.svelte` — 友好错误展示 + i18n + renderMarkdown
- `src/lib/components/invest/DebateBlock.svelte` — blockState 类型扩展
- `messages/en.json` — 4 个 fallback i18n key
- `messages/zh-CN.json` — 4 个 fallback i18n key

### Bug 修复: Preview 面板 + 项目记忆 + 记忆提取配置 (3 项)

**Preview 面板 HTML 预览修复 (1 项):**
1. **`HtmlPreview.svelte` 预览方式重写**: 移除 blob URL + `$effect` 生命周期管理，改用 `srcdoc` 属性直接注入 HTML 内容。`srcdoc` 天然具有独立 origin，无需 `allow-same-origin`，安全性更高且代码更简洁。

**项目记忆解析修复 (1 项):**
2. **`memory/+page.svelte` 传递 `project_paths` 参数**: `refreshCandidates()` 和 `autoSelectFirst()` 调用 `api.listMemoryFiles()` 时新增 `projectPaths` 参数（`[projectCwd]`），后端据此扫描 `~/.claude/projects/{slug}/memory/` 目录下的项目记忆文件。之前只传 `cwd` 导致后端无法定位项目记忆目录。

**Memory Extraction 配置状态修复 (1 项):**
3. **`memory-mgmt/+page.svelte` 启用状态初始值修复**: `memoryExtractionEnabled` 默认值从 `true` 改为 `false`；`loadExtractionConfigFromSettings` 新增 `else` 分支，当 `embedding_config` 为 `None` 时重置所有字段；`ec.enabled` 回退值从 `?? true` 改为 `?? false`。根因：UI 显示"已启用"但实际从未配置过，用户无法通过关闭再开来修复（后端收到 `undefined` 清除配置，但下次加载 UI 仍显示"已启用"）。

**涉及文件 (3):**
- `src/lib/components/preview/HtmlPreview.svelte` — sandbox 属性
- `src/routes/memory/+page.svelte` — listMemoryFiles project_paths 参数
- `src/routes/memory-mgmt/+page.svelte` — 启用状态初始值 + 加载逻辑

### 现金管理增强: 银证转入/转出 + 微调修正 + 7 项审查修复

**现金操作重构 (4 项):**
1. **TradeAction 枚举扩展**: 新增 `TransferIn` / `TransferOut` 变体，`cash_delta_for_trade` 正确处理转入（+金额）和转出（-金额）。
2. **HoldingKind 枚举扩展**: 新增 `Cash` 变体，支持 `kind='cash'` 的交易记录。
3. **DB Schema 迁移**: holdings/trades 表 CHECK 约束更新（kind 含 'cash'，action 含 transfer_in/transfer_out），`check_is_current` 探测字符串同步更新。
4. **`recalculate_holdings_inner`**: TransferIn/TransferOut 为 no-op（不影响持仓计算）。

**TradeDialog 重设计 (3 项):**
5. **三子模式选择器**: 银证转入（绿色）、银证转出（红色）、微调修正（金色），通过 `CASH_MODES` 数据驱动渲染。
6. **`handleSubmit` 重写**: 所有现金操作通过 `recordTrade()` 记录交易（保留审计轨迹），`fine_tune` 映射到存储层 `cash_adjust`。
7. **`submitLabel` 提取**: 9 层嵌套三元替换为 `SUBMIT_LABELS` 查找表 + `$derived`。

**TradeLogTab 更新 (2 项):**
8. **SYSTEM_ACTIONS 扩展**: 新增 `transfer_in`、`transfer_out`，默认隐藏。
9. **徽章颜色 + 方向过滤**: 转入/卖出=绿色，转出/买入=红色；过滤下拉框新增转入/转出/微调选项；CASH 符号显示本地化标签。

**代码审查修复 (7 项):**
10. **Fix 1: `fine_tune` → `cash_adjust` 映射**: UI 层 `fine_tune` 在提交时映射到存储层 `cash_adjust`，避免静默降级为 Unknown。
11. **Fix 2: 注释/code 不匹配修复**: `check_is_current` 注释从 `edit_holding` 更新为 `transfer_out`。
12. **Fix 3: 死方法 `updateCash()` 删除**: 现金操作统一走 `recordTrade()`，旧直接覆写路径移除。
13. **Fix 4: 死三元分支折叠**: TradeLogTab 徽章颜色表达式移除冗余分支（`sell` 正确显示绿色）。
14. **Fix 5: `submitLabel` 提取**: 9 层嵌套三元替换为 `SUBMIT_LABELS` + `$derived`。
15. **Fix 6: pill-tab 按钮提取**: 3 个重复按钮替换为 `CASH_MODES` + `{#each}` + `{@const}`。
16. **Fix 7: `cash_adjust` 方向过滤**: 过滤下拉框新增微调修正选项。

**涉及文件 (8):**
- `src-tauri/src/storage/invest/portfolio.rs` — TradeAction/HoldingKind 枚举 + cash_delta_for_trade + holdings no-op
- `src-tauri/src/storage/invest/mod.rs` — DB CHECK 约束 + check_is_current 注释修复
- `src/lib/components/invest/TradeDialog.svelte` — 三子模式重设计 + submitLabel + CASH_MODES
- `src/lib/components/invest/TradeLogTab.svelte` — SYSTEM_ACTIONS + 徽章颜色 + 方向过滤 + CASH 标签
- `src/lib/stores/invest-store.svelte.ts` — 删除 updateCash() 死方法
- `src/lib/types.ts` — TradeAction 类型扩展
- `src/routes/invest/+page.svelte` — 按钮文案改为"现金管理"
- `messages/en.json` + `messages/zh-CN.json` — 12 个新 i18n key

## v5.3.5 (2026-06-11)

### 委员会指标预计算 + 工具清理 + 2 轮代码审查修复

**核心架构变更 (1 项):**
1. **`indicators.rs` 共享指标模块**: 新建 `src-tauri/src/invest/indicators.rs`，提供 `compute_ma`、`compute_ma_series`、`compute_rsi14`、`compute_volatility`、`compute_price_percentile`、`classify_trend` 六个纯数学函数，消除 `regime.rs` 与 `tools.rs` 之间的代码重复。包含 12 个单元测试。

**委员会管道优化 (3 项):**
2. **Quant R1 预计算指标注入**: `build_asset_context` Step 9 计算 MA5/20/60/120、RSI-14、HV20 年化波动率、价格分位数、趋势分类，通过 `{{precomputed_indicators}}` 注入 Quant R1 prompt。LLM 直接引用，无需工具调用。
3. **`get_company_info` 工具移除**: 数据已通过 `{{pe_ttm}}`/`{{pb}}`/`{{roe}}` 等 placeholder 注入，工具调用完全冗余。Quant 工具列表 5→4。
4. **`get_company_news` AkShare 迁移**: 从 Tushare `major_news`（市场级快讯，无个股筛选）改为 AkShare `fetch_akshare_stock_news`（个股新闻）。

**第一轮代码审查修复 (9 项):**
5. **价格分位双倍缩放修复**: `compute_price_percentile` 返回 0-100，`regime.rs` 存储时 `/100.0` 转为 0.0-1.0 分位数，避免 `format_regime_context` 再乘 100 导致 7200% 错误。
6. **零收盘守卫**: `compute_volatility` 对零收盘价（停牌/退市/数据错误）返回 0.0，防止 NaN/Inf 传播到 HV20 和 Prompt。
7. **`mean_all` 回退**: MA5/20/60 数据不足时用全量均值回退，而非 `latest`，避免 `classify_trend` 自相矛盾。
8. **RSI 平坦价格守卫**: 全部收盘价相等时（avg_gain=0, avg_loss=0）返回 50.0（中性），而非 100.0（超买）。
9. **HV20 不足数据提示**: 波动率=0.0 且 bars<21 时显示 `N/A (仅N日数据)` 而非误导性的 `0.0%`。
10. **Quant Prompt 更新**: 添加预计算指标说明，工具描述改为"补充"，移除 `get_company_info` 引用，估值评估改为"系统注入"。
11. **CommitteeToolsTab 更新**: 移除 `get_company_info` 条目，工具矩阵 9→8。

**第二轮代码审查修复 (3 项):**
12. **`compute_ma` period=0 守卫**: 防止除零产生 NaN。
13. **零收盘守卫缩窄**: 从扫描全部 750 根 K 线改为仅检查最近 21 根收盘价（用于 20 日收益计算），避免远期数据错误影响当前波动率。
14. **价格分位窗口动态化**: 硬编码 "500日" 改为 `pct_window = closes_desc.len().min(750)`，显示实际窗口大小。

**涉及文件 (8):**
- `src-tauri/src/invest/indicators.rs` — 新建，12 单元测试
- `src-tauri/src/invest/mod.rs` — 添加 `pub mod indicators`
- `src-tauri/src/invest/regime.rs` — 复用 indicators，删除私有 RSI
- `src-tauri/src/invest/committee/tools.rs` — 移除 get_company_info，迁移 get_company_news，清理重复代码
- `src-tauri/src/invest/committee/roles.rs` — Quant R1 prompt 注入预计算指标
- `src-tauri/src/invest/committee/orchestrator.rs` — build_asset_context Step 9
- `src/lib/components/invest/CommitteeToolsTab.svelte` — 工具矩阵更新
- `docs/committee-precompute-indicators.md` — 实施计划

## v5.3.4 (2026-06-11)

### Code Review 修复 (10 项) + PnL 计算兜底

**PnL 计算修复 (1 项):**
1. **`run_pnl_snapshot` notional 兜底**: 当 `get_latest_price()` 失败时，回退到持仓的 `notional` 值而非跳过，与前端 `totalAssets` 行为对齐。修复实际总资产 86638.28 vs PnL 计算 81572.88 的差异。

**后端修复 (6 项):**
2. **`process_edit_holding` name/asset_type 传播**: 编辑持仓时，`name` 和 `asset_type` 字段现在正确传播到 `MemHolding`。
3. **迁移 `convert_hold_to_watch` → `'unknown'`**: 之前映射为 `'cost_edit'` 导致语义丢失，现改为 `'unknown'` 避免误导性数据。
4. **嵌套事务修复**: `convert_watch_to_hold` 内调用 `recalculate_holdings_inner` 的嵌套 BEGIN/COMMIT 导致外层事务提前提交。新增 `manage_tx` 参数和 `recalculate_holdings_inner_no_tx` 变体。
5. **CHECK 约束添加 `'unknown'`**: `TradeAction::Unknown` 不在 DB CHECK 约束中，写入会失败。
6. **event_scanner kind 过滤恢复**: 被移除的 `kind == "hold" || kind == "watch"` 过滤重新加入。
7. **`created_at` 保留**: `recalculate_holdings_inner` 重算时保留原始 `created_at`，避免每次重算覆盖创建时间。

**前端修复 (1 项):**
8. **TradeDialog `||` → `??`**: `holdingAvgCost || h.avgCost` 在值为 0 时错误回退，改用 nullish coalescing。

**已确认无需修复 (2 项):**
9. **`update_trade` created_at**: 存储层 UPDATE SQL 不包含 `created_at`，值已保留。添加注释说明。
10. **`asset_type_map` 类型**: holdings 表 `asset_type TEXT NOT NULL DEFAULT 'stock'`，`String` 类型正确。

**涉及文件 (6):**
- `src-tauri/src/lib.rs` — PnL notional 兜底
- `src-tauri/src/storage/invest/portfolio.rs` — process_edit_holding, manage_tx, created_at 保留
- `src-tauri/src/storage/invest/mod.rs` — 迁移修复, CHECK 约束
- `src-tauri/src/invest/event_scanner.rs` — kind 过滤
- `src-tauri/src/commands/invest.rs` — 注释
- `src/lib/components/invest/TradeDialog.svelte` — `||` → `??`

## v5.3.3 (2026-06-11)

### Invest 交易逻辑全面重构 (PR1+PR2) + Simplify 审查

**核心架构变更 (3 项):**
1. **`sql_string_enum!` 宏**: 提取 Display/FromSql/ToSql 通用实现，消除 TradeAction + HoldingKind 约 50 行样板代码。支持 variant-level 属性（如 `#[serde(other)]`）。
2. **类型安全枚举**: `TradeAction` (7 variants + Unknown) 和 `HoldingKind` (Hold/Watch) 实现 `FromStr` trait + `Default`，调用方使用 `.parse().unwrap_or_default()` 惯用写法。
3. **`convert_watch_to_hold` 原子化命令**: 单事务完成 delete_watch + buy，替代之前有原子性缺陷的两步 IPC 模式。

**后端变更 (6 项):**
4. **`add_holding` / `update_holding` 标记 `#[deprecated]`**: 分别被 `record_trade(action="add_watch")` 和 `record_trade(action="edit_holding")` 替代。
5. **DB 迁移**: CHECK 约束移除 `convert_hold_to_watch`，新增 `edit_holding`。旧记录自动转换 (`convert_watch_to_hold→buy`, `convert_hold_to_watch→cost_edit`)。`check_is_current` 守卫避免每次启动全表重建。
6. **`process_*` 函数签名统一**: dispatch 循环预构建 `(symbol, currency, kind)` key 传入，消除 6 次冗余 clone。`process_delete_watch` 签名与其他函数对齐。
7. **`recalculate_cash_inner` 保留为 `pub(crate)` 恢复工具**: 正常路径不触碰 cash，仅用于数据修复。
8. **`event_scanner.rs`**: 移除冗余 `kind == "hold" || kind == "watch"` 过滤（CHECK 约束已保证）。
9. **`lib.rs`**: PnL 快照过滤改用 `HoldingKind::Hold` 枚举比较，`run()` 函数添加 `#[allow(deprecated)]`。

**前端变更 (6 项):**
10. **`addToWatch` / `deleteWatch` / `convertWatchToHold` / `updateHoldingMeta`**: 统一为单次 `record_trade` 或 `convert_watch_to_hold` IPC，消除双路径操作和 try/finally 包装。
11. **HoldingsTable UI**: hold/watch 分行动作按钮 — HOLD 行: Edit | 买入 | 卖出 | 转观望；WATCH 行: Edit | 买入建仓 | 删除。共享 `clsAction` 基础样式类防止样式漂移。
12. **`TradeAction` 联合类型**: `types.ts` 新增 `TradeAction` 类型，`Trade.kind` 收窄为 `'hold' | 'watch'`，`Trade.action` 收窄为 `TradeAction`。
13. **`+page.svelte`**: 新增 `convertToWatch` 和 `deleteWatchFromTable` 回调，传递给 HoldingsTable。
14. **i18n**: 新增 `invest_convert_to_watch` ("转为观望") 和 `invest_delete_watch` ("删除观望") key。

**Simplify 审查修复 (5 项):**
15. **Reuse**: `sql_string_enum!` 宏消除两个 enum 的 Display/FromSql/ToSql 重复实现
16. **Simplification**: `FromStr` trait 替代 inherent `from_str`；`process_delete_watch` 签名对齐
17. **Efficiency**: 预构建 key 传入 `process_*` 函数，消除 6x 冗余 `(String, String, String)` 分配
18. **Altitude**: Edit 按钮使用 `clsAction` 基础类，防止与表格其他按钮样式不同步

**涉及文件 (12):**
- `src-tauri/src/storage/invest/portfolio.rs` — 宏+枚举+函数拆分+key 传递 (核心重构)
- `src-tauri/src/storage/invest/mod.rs` — CHECK 约束迁移+旧记录转换+early-return 守卫
- `src-tauri/src/commands/invest.rs` — deprecated 标记+`convert_watch_to_hold` 命令+`.parse()` 适配
- `src-tauri/src/group_chat/orchestrator.rs` — 无实际变更（diff 为空）
- `src-tauri/src/invest/event_scanner.rs` — 移除冗余 kind 过滤
- `src-tauri/src/lib.rs` — `HoldingKind::Hold` 枚举比较+`#[allow(deprecated)]`
- `src/lib/stores/invest-store.svelte.ts` — 4 个方法统一为单次 IPC
- `src/lib/types.ts` — `TradeAction` 联合类型+`Trade.kind` 收窄
- `src/lib/components/invest/HoldingsTable.svelte` — hold/watch 分行按钮+样式类
- `src/routes/invest/+page.svelte` — convertToWatch/deleteWatch 回调
- `messages/en.json` / `messages/zh-CN.json` — 2 个新 i18n key

## v5.3.2 (2026-06-11)

### 金十快讯高频采集 + 事件分析器 + Scheduler 集成

**架构设计:** 两阶段事件处理流水线 — 高频采集器 (15s) 写入原始事件，低频分析器 (10min) LLM 归一化。

**新增模块 (2 项):**
1. **`jin10_collector.rs`**: 每 15 秒轮询金十 A 股快讯，写入 events 表 (`analyzed=false`)。内存 HashSet 去重 + DB UNIQUE 索引双重去重。`cleanup_seen_ids` 防内存泄漏 (>5000 时清理至 2000)。
2. **`event_analyzer.rs`**: 每 10 分钟查询未分析事件，批量 LLM 归一化 (severity/stance/symbols)，更新 DB。跳过 LLM 重新分类为 LOW 的事件（仍标记 analyzed 避免重复处理）。

**Events 表扩展 (3 字段):**
3. **`analyzed` / `analyzed_at` / `channels`**: 新增列 + DB 迁移。`list_unanalyzed_events` 查询 `WHERE analyzed=0`。`update_event_analysis` 更新 severity/stance/symbols 并设置 `analyzed=1`。

**Scheduler 集成 (2 jobs):**
4. **`jin10_collector` job**: `*/15 * * * * *` (6 字段秒级 cron)，`requires_trading_day=false`。
5. **`event_analyzer` job**: `0 */10 * * * *`，`requires_trading_day=false`。

**Python Jin10 Provider 增强 (4 项):**
6. **`_clean_html()`**: HTML → 纯文本，处理 `<br>`、`&nbsp;` 等实体。
7. **`_should_skip()`**: 5 条过滤规则 — 广告、HTML 列表、空/短内容、标题党、摘要长文。
8. **Channel 参数**: `news()` 新增 `channel` 参数，客户端过滤。`news_a_share()` 便捷函数 (`channel=2`)。
9. **`fetch_jinshi_a_share_news`**: Rust 端 `InternationalClient` 新增 A 股快捷方法。

**Simplify 审查修复 (13 项):**
- **Reuse (6)**: `parse_normalized_response` 泛型化、`fallback_normalize_from` 共享、`short()` 公共化、`format_provider_timestamp` 共享、`JIN10_COUNT` 单一位置、`row_to_event` 提取
- **Simplification (2)**: `row_to_event` 消除 list_events/list_unanalyzed_events 重复映射、`has_column` 标识符验证
- **Efficiency (4)**: `cleanup_seen_ids` 效率优化、`Local::now()` 循环前预计算、`load_jobs()` N+1→单次 load、`sources_scanned.push` 移出 async block
- **Altitude (1)**: `migrate_trades_table` schema 匹配时提前返回

**涉及文件 (15):**
- `src-tauri/src/invest/jin10_collector.rs` — 新模块
- `src-tauri/src/invest/event_analyzer.rs` — 新模块
- `src-tauri/src/invest/event_scanner.rs` — 共享 helpers 提取
- `src-tauri/src/invest/mod.rs` — 模块声明
- `src-tauri/src/invest/scheduler/mod.rs` — 2 个默认 job
- `src-tauri/src/invest/scheduler/runner.rs` — dispatch + 单次 load 重构
- `src-tauri/src/invest/international.rs` — channel 参数 + A 股方法
- `src-tauri/src/storage/invest/events.rs` — schema 扩展 + row_to_event
- `src-tauri/src/storage/invest/mod.rs` — 迁移 + 标识符验证 + schema 提前返回
- `src-tauri/src/commands/invest.rs` — channel 参数适配
- `src-tauri/python-runtime/scripts/providers/jinshi.py` — HTML 清理 + 过滤 + channel
- `src/lib/components/invest/EventWatchTab.svelte` — pending 状态 UI
- `src/lib/types.ts` — InvestEvent 新字段
- `messages/en.json` / `messages/zh-CN.json` — "invest.eventWatch.pending"

### 事件去重 + 排序修复 + 专用定时器 + Simplify 审查

**Bug 修复 (3 项):**
1. **DB 去重迁移守卫**: `DELETE FROM events` 去重 SQL 改为仅在 UNIQUE 索引不存在时执行（`sqlite_master` 检查），避免每次启动全表扫描。
2. **事件排序修复**: 前端 `filteredEvents` 排序移除 severity 优先级（`pending` 被误排到与 `high` 同级），改为纯 `created_at` 降序。
3. **金十快讯不丢弃**: `event_analyzer` 移除 `overwrite_low` 参数，LLM 分类为 LOW 的事件保留原始 severity（jin10 事件保持 `"pending"`），仅标记 `analyzed=1`。

**架构改进 (2 项):**
4. **专用定时器**: `jin10_collector` 和 `event_analyzer` 从主 scheduler 循环分离，各运行独立 tokio spawn 循环，精确 15s/10min 间隔。
5. **CronJob `dedicated` 字段**: 新增 `dedicated: bool` 数据驱动标记，主循环用 `!j.dedicated` 过滤，消除硬编码 job ID 列表。

**Simplify 审查修复 (7 项):**
- **Reuse (2)**: 专用循环复用 `dispatch_job()` 消除内联逻辑重复、提取 `persist_job_status()` + `execute_and_log()` 共享辅助函数
- **Simplification (2)**: `event_analyzer` 两个 LOW 分支合并为单分支、移除始终为 `false` 的 `overwrite_low` 参数
- **Efficiency (2)**: 状态更新改用 `load_jobs_base()` 跳过无用 cron 计算、`Instant::now()` 扣除执行时间消除 sleep 漂移
- **Altitude (1)**: `dedicated` 字段替代硬编码过滤列表，防止新增 job 时遗漏

**涉及文件 (5):**
- `src-tauri/src/invest/scheduler/mod.rs` — CronJob `dedicated` 字段 + 默认值
- `src-tauri/src/invest/scheduler/config.rs` — `load_jobs_base` 改为 `pub(crate)`
- `src-tauri/src/invest/scheduler/runner.rs` — 全面重写: 专用循环 + 共享辅助 + 精确计时
- `src-tauri/src/invest/event_analyzer.rs` — LOW 分支合并 + 移除 `overwrite_low`
- `src-tauri/src/storage/invest/mod.rs` — 去重迁移索引存在性守卫
- `src/lib/stores/invest-store.svelte.ts` — 排序改为纯时间降序

### 事件时间戳时区修复 (UTC→东八区)

**Root Cause:** `event_scanner.rs::format_provider_timestamp` 使用 `DateTime::from_timestamp(ts, 0)` 生成 UTC 时间后直接格式化为 `"%Y-%m-%dT%H:%M:%S"`（无时区后缀），前端按本地时间显示导致所有事件时间比东八区晚 8 小时。

**修复:** 加入 `.with_timezone(&chrono::Local)` 将 UTC 转为本地时区后再格式化，与 invest 模块其他时间戳生成点（`jin10_collector.rs`、`storage/invest/events.rs`）保持一致。

**涉及文件 (1):**
- `src-tauri/src/invest/event_scanner.rs` — `format_provider_timestamp` UTC→Local

---

## v5.3.1 (2026-06-10)

### Python RPC UnicodeEncodeError 修复: Jin10/AkShare 数据源崩溃

**Root Cause:** Python server 在 Windows 上 stdout 默认编码为 GBK，当 Jin10/AkShare 返回的新闻包含 GBK 无法编码的 Unicode 字符（如 U+200B 零宽空格）时，`print()` 抛出 `UnicodeEncodeError`，server 进程崩溃退出，Rust 端收到 `BrokenPipe` 错误。

**修复 (2 项):**
1. **`bridge.rs`**: spawn Python 进程时设置 `PYTHONIOENCODING=utf-8` 环境变量，强制 UTF-8 编码 stdin/stdout/stderr。
2. **`server.py`**: `_safe_print` 的 except 增加 `UnicodeEncodeError` 捕获，作为防御性措施。

**失败方案记录:** `docs/discarded_sol.md`

### 定时任务 cron 格式修复: 5 字段→6 字段 + 后端归一化

**Root Cause:** Rust `cron` crate v0.15 要求 6 字段格式 (`秒 分 时 日 月 周`)，但前端 SchedulerTab 生成和预设的 cron 表达式为 5 字段（无秒字段），导致 `update_cron_schedule` 保存时 `Schedule::from_str` 解析失败。DreamingConfigPanel 的自由文本输入和 `DreamConfig::default()` 也存在同样的 5 字段问题。

**修复 (5 项):**
1. **`SchedulerTab.svelte` PRESETS**: 7 个预设从 5 字段 (`'0 17 * * 1-5'`) 改为 6 字段 (`'0 0 17 * * 1-5'`)。
2. **`SchedulerTab.svelte` fieldsToCron**: 输出前补 `0` 作为秒字段。
3. **`SchedulerTab.svelte` stripSeconds**: 提取共享辅助函数，消除 `parseCronToFields` 和 `humanCron` 之间的重复秒字段剥离逻辑。
4. **`config.rs` normalize_cron_6field**: 后端归一化函数，在 `update_cron` 和 `save_dream_config` 入口处自动将 5 字段表达式补上秒字段 `0`，防御性保护所有前端调用方。
5. **`dreaming/mod.rs` DreamConfig::default()**: 默认 `invest_cron` 从 `"0 3 * * *"` (5 字段) 改为 `"0 0 3 * * *"` (6 字段)，与 scheduler 默认统一。

**Simplify 审查 (4 路):**
- Reuse: 提取 `stripSeconds()` 消除 2 处重复
- Simplification: `isPresetCron` round-trip 跳过（亚微秒开销）
- Efficiency: 无问题
- Altitude: 后端归一化 + DreamConfig 默认修复

### NewsItem 反序列化修复 + 结构体重命名 + 绝对时间展示

**Root Cause:** `YahooNewsItem` 结构体使用 `#[serde(rename_all = "camelCase")]`，Rust 端期望 `providerPublishTime`，但 Python 端返回 `provider_publish_time`（snake_case），导致反序列化失败。

**修复 (3 项):**
1. **`international.rs` NewsItem 重命名**: `YahooNewsItem` → `NewsItem`，反映多数据源共用事实。移除 `rename_all = "camelCase"`（Python 端全部返回 snake_case），消除 `related_tickers` 等字段的潜在同类问题。
2. **`EventWatchTab.svelte` 绝对时间**: 导入 `fmtDateTime`，在相对时间 tooltip 展示绝对时间（如"6/10 14:30"）。
3. **全局注释同步**: 5 个 Python provider + changelog 中 `YahooNewsItem` 引用更新为 `NewsItem`。

**Simplify 审查 (4 路):**
- Reuse: 无问题（`fmtDateTime` 已有函数）
- Simplification: 无问题
- Efficiency: 无问题
- Altitude: 发现 `rename_all = "camelCase"` 对 `related_tickers` 等字段的隐患 → 移除 `rename_all` 根本解决

---

## v5.3.0 (2026-06-10)

### 定时任务调度面板重设计 + 后端 next_run + 状态映射合并

**调度面板重设计 (1 项):**
1. **SchedulerTab Card 布局**: 表格 → 卡片布局，状态圆点+倒计时+展开详情面板。7 个预设调度 + "Custom" 5 字段可视化 cron 生成器（分钟/小时/日/月/星期），运行时间线，启用/禁用开关。

**后端 next_run 计算 (2 项):**
2. **`compute_next_run_for_job`**: `config.rs` 新增函数，基于 `cron` crate 计算下次触发时间（支持 interval 和 cron 两种模式）。`load_jobs()` 结尾自动填充每个 job 的 `next_run` 字段。
3. **Runner 简化**: `runner.rs` 的 `should_fire` 45 行调度逻辑替换为 `job.next_run <= now` 5 行比较。执行后更新合并为单次 load+save（原 3 次读 2 次写 → 1 次读 1 次写）。

**状态映射合并 (1 项):**
4. **`invest-status.ts` 共享模块**: `investStatusDotClass()` + `investStatusTextClass()` 统一查找表，覆盖 ok/completed/error/failed/skipped/rolled_back。SchedulerTab 和 DreamingConfigPanel 移除本地重复函数。

**代码审查优化 (4 项 simplify):**
5. **`load_jobs_base` 提取**: `config.rs` 共享覆盖逻辑提取为基础函数，`load_jobs` 和原 `load_jobs_raw`（已删除）共用，消除 35 行复制。
6. **`persist_next_run` 删除**: 该函数实际为空操作（`next_run` 从未写入 `JobOverride`，仅在 load 时计算）。runner 改为在 save 前直接 recompute。
7. **`should_fire` 调度逻辑消除**: runner 改用 `job.next_run` 比较，与 `compute_next_run_for_job` 单一真相源。
8. **`STATUS_MAP` 查找表**: `invest-status.ts` 双 if-chain 合并为单一 Record，新增状态只需改一处。

**i18n (18 keys):**
- `invest_scheduler_next_run/countdown/paused/trading_day/presets/custom/recent_runs/no_runs/view_all_logs/footer`
- `invest_scheduler_cron_minute/hour/day/month/weekday/preview/generated`

**涉及文件:**
- `src/lib/components/invest/SchedulerTab.svelte` — 全面重写（269→600+ 行 card 布局+cron builder）
- `src-tauri/src/invest/scheduler/config.rs` — `load_jobs_base` + `compute_next_run_for_job` + 删除 `persist_next_run`/`load_jobs_raw`
- `src-tauri/src/invest/scheduler/runner.rs` — `should_fire` 简化 + 单次 load+save
- `src/lib/utils/invest-status.ts` — 新共享模块
- `src/lib/components/invest/DreamingConfigPanel.svelte` — 使用共享状态函数
- `messages/en.json` / `messages/zh-CN.json` — 18 个新 i18n key

### convert_watch_to_hold 移除 + 现金管理重构 + 时间格式统一

**convert_watch_to_hold 移除 (3 项):**
1. **前端 `convertWatchToHold` 重构**: 4 次 IPC 调用 (`delete_holding` + `add_holding` + `record_trade(convert_watch_to_hold)`) 精简为 2 次 `record_trade` (`delete_watch` + `buy`)。移除冗余 `delete_holding`/`add_holding`（recalculate 会从 trade 历史重建 holdings）。回滚改为 best-effort（首条 trade 已持久化）。
2. **后端 `convert_watch_to_hold` 分支移除**: `recalculate_holdings_inner_body` 中删除 `convert_watch_to_hold` match 分支；`recalculate_cash_inner` 中移除 `convert_watch_to_hold` 现金等价逻辑；CHECK 约束移除该 action。
3. **CHECK 迁移兼容**: `migrate_trades_table` 在 `INSERT OR IGNORE` 前执行 `UPDATE trades SET action='buy' WHERE action='convert_watch_to_hold'`，避免历史记录被静默丢弃。

**现金管理重构 (4 项):**
4. **增量现金替代全量重算**: `record_trade`/`delete_trade`/`update_trade` 改用 `apply_cash_delta_sql`（单条 `UPDATE cash SET available = available + ?`）代替 `recalculate_cash_inner`（从 `initial_balance` 全量重算），解决 `cash_adjust` 被忽略导致现金被重置的根因。
5. **`cash_delta_for_trade` 提取**: `apply_cash_delta`/`reverse_cash_delta`/`recalculate_cash_inner` 三处共享同一 action→delta 映射，新增 action 只需改一处。
6. **`get_trade_by_id` 提取**: `delete_trade`/`update_trade` 中 12 行重复的 SELECT-by-ID 提取为共享函数。
7. **`recalculate_holdings_inner` 合并**: `recalculate_holdings_inner_without_cash`（40 行复制）合并为 `recalculate_holdings_inner(conn, recalc_cash: bool)`，消除 DRY 违规。
8. **`get_cash_inner` 错误处理修复**: 从静默返回 0.0 恢复为 `QueryReturnedNoRows => 0.0` + 其他错误传播，避免 DB 损坏时的静默数据错误。

**时间格式统一 (2 项):**
9. **invest 模块 `created_at` 统一为毫秒精度**: `chrono::Utc::now().to_rfc3339()` → `to_rfc3339_opts(SecondsFormat::Millis, true)`，统一为 29 字符格式 `2026-06-10T00:48:14.808+00:00`。覆盖 `portfolio.rs`(4)、`commands/invest.rs`(7)、`scheduler.rs`(2)、`strategy.rs`(1) 共 14 处。
10. **DB 历史数据归一化**: Python 脚本将 97 条 trades + 9 条 holdings + 1 条 cash + 1 条 strategy 的时间戳从混合精度（6 位微秒/9 位纳秒）截断为 3 位毫秒。

**代码审查优化 (7 项 simplify):**
11. **`cash_delta_for_trade` 单一真相源**: 3 处 action→delta match 合并为一个函数。
12. **`apply_cash_delta_sql` 原子 UPDATE**: 从 read-modify-write（SELECT+UPSERT）改为单条 UPDATE，消除额外查询。
13. **`get_trade_by_id` 提取**: 12 行重复查询合并为共享 helper。
14. **`recalculate_holdings_inner` 合并**: 40 行复制函数合并为带 bool 参数的单一函数。
15. **`get_cash_inner` 错误传播**: 区分 NoRows 和真实 DB 错误。
16. **CHECK 迁移数据保护**: 迁移前 UPDATE 旧 action，防止 `INSERT OR IGNORE` 静默丢弃。
17. **前端 IPC 精简**: 4 次 IPC → 2 次，移除被 recalculate 覆盖的冗余操作。

**涉及文件:**
- `src/lib/stores/invest-store.svelte.ts` — `convertWatchToHold` 重构（4→2 次 IPC）
- `src-tauri/src/storage/invest/portfolio.rs` — 现金增量管理 + 函数合并 + `get_trade_by_id` + 时间格式
- `src-tauri/src/storage/invest/mod.rs` — CHECK 约束 + 迁移数据保护
- `src-tauri/src/commands/invest.rs` — 时间格式统一（7 处）
- `src-tauri/src/storage/invest/scheduler.rs` — 时间格式统一（2 处）
- `src-tauri/src/storage/invest/strategy.rs` — 时间格式统一（1 处）

## v5.2.19 (2026-06-09)

### Python RPC 崩溃修复 + Provider 共享工具 + 代码清理

**Python RPC 崩溃修复 (5 项):**
1. **Lazy import 模式**: `jinshi.py` 和 `eastmoney.py` 的 `import requests` 从模块级移至 `_get_session()` 内部，缺失时返回 None 而非崩溃整个 RPC 子进程。
2. **server.py ImportError 处理**: `get_provider()` 捕获 ImportError 并转为 ValueError，附带缺失依赖提示。
3. **bridge.rs 简化**: 移除 `exit_status` 字段（仅存硬编码字符串），stderr 日志从 3 个 `contains` 判断简化为 `log::info!` 全量记录，error message 预格式化避免 N 次重复分配。
4. **server.py BaseException 保护**: `handle_request()` 外层 `except BaseException` 防止任何未捕获异常（含 `SystemExit`、`MemoryError`）杀死 RPC 主循环；`_safe_print()` 封装 stdout 写入，`BrokenPipeError` 时优雅退出而非崩溃。
5. **server.py stderr 保护**: `BaseException` handler 内的 stderr `print` 本身用 `try/except (BrokenPipeError, OSError): pass` 保护，避免二次崩溃。

**Provider 共享工具 (3 项):**
6. **`providers/utils.py` 提取**: `matches_query()`、`parse_timestamp()`、`create_session()` 三个共享函数消除 `jinshi.py`/`eastmoney.py`/`akshare_news.py` 中的重复代码（`_matches_query`×3、`_parse_*_time`×3、`_get_session`×2）。
7. **`LazySession` 类**: 哨兵模式 (`_UNINITIALIZED`) 的延迟初始化 session，失败时只打印一次 stderr 日志，后续调用直接返回 None。`jinshi.py` 和 `eastmoney.py` 统一使用，消除 `eastmoney.py` 每次调用 retry 的 latent bug。
8. **`clean_dataframe()` 函数**: EastMoney 数据通用清洗——`fillna("")` + `replace("-", "")`，`akshare_news.py` 使用，消除 DataFrame 中的 "nan" 和 "-" 占位符。

**PnL 快照修复 (1 项):**
9. **`get_previous_day_snapshot`**: `run_pnl_snapshot` 使用前一天快照而非最近快照计算日收益，避免同日多次运行时的重复计算。

**资金流向当日注入 (1 项):**
10. **资金流向双注入**: `MoneyflowDc` 新增 `format_moneyflow_summary_latest`（仅取最新一天）+ `MoneyflowCachePayload` typed 结构体 + `to_cache_json()` 方法。`AssetContext` 新增 `money_flow_daily_summary` 字段，Quant R1 prompt 同时注入当日+近5日资金流向（避免方向相反的误导信号）。旧缓存无 `daily_summary` 字段时 `#[serde(default)]` 兼容 fallback。`exec_moneyflow` 工具仍用 5 天汇总。

**事件源修复 (1 项):**
11. **Jin10 错误日志**: `fetch_jinshi_all_news` 从 `if let Ok` 改为 `match` + `log::warn!`，Python RPC 失败时记录错误而非静默返回空列表。

**代码清理 (2 项):**
12. **删除 17 个临时文件**: 12 个 test_*.py 测试脚本 + tushare_config.py + simplify_parsers.py + dragon_tiger_cache.py + screener_cache.db + output.txt + output_utf8.txt（含 2 个硬编码 Tushare API token）。
13. **`.gitignore` 更新**: 新增 `output*.txt` 和 `*.db` 规则，清理 340 MB 陈旧 worktrees。

**涉及文件:**
- `src-tauri/src/python/bridge.rs` — exit_status 移除 + stderr 简化
- `src-tauri/src/python/mod.rs` — get_client 简化
- `src-tauri/python-runtime/scripts/server.py` — ImportError 处理 + BaseException 保护 + _safe_print
- `src-tauri/python-runtime/scripts/providers/utils.py` — 新共享工具模块 + LazySession + clean_dataframe
- `src-tauri/python-runtime/scripts/providers/jinshi.py` — lazy import + 共享 utils + LazySession
- `src-tauri/python-runtime/scripts/providers/eastmoney.py` — lazy import + 共享 utils + LazySession
- `src-tauri/python-runtime/scripts/providers/akshare_news.py` — 共享 parse_timestamp + clean_dataframe
- `src-tauri/src/invest/international.rs` — Jin10 错误日志
- `src-tauri/src/invest/committee/orchestrator.rs` — money_flow_daily_summary + cache 扩展 + MoneyflowCachePayload typed 反序列化 + to_cache_json 去重
- `src-tauri/src/invest/committee/roles.rs` — prompt 双占位符（当日+近5日）
- `src-tauri/src/invest/committee/tools.rs` — exec_moneyflow typed 反序列化
- `src-tauri/src/tushare/client.rs` — format_moneyflow_summary_latest + MoneyflowCachePayload + to_cache_json
- `src-tauri/src/lib.rs` — PnL 快照使用 get_previous_day_snapshot
- `.gitignore` — output*.txt + *.db 规则

## v5.2.18 (2026-06-09)

### 事件源重构: Jin10 全量快讯 + AkShare 个股新闻

**数据源替换 (3 项):**
1. **移除 Yahoo Finance 新闻**: 删除 `fetch_yahoo_news`、`fetch_china_finance_news`、`fetch_eastmoney_news`、`fetch_eastmoney_quote` 4 个死方法。Yahoo 429 限流问题彻底解决。
2. **移除 Tushare 新闻**: `event_scanner` 不再调用 `major_news` API（该端点在自定义代理上返回 0 条）。Tushare 公告 (`anns_d`) 保留。
3. **新增 Jin10 + AkShare 双源架构**: Jin10 (`channel=-8200`) 获取全量快讯（宏观+国际），AkShare (`stock_news_em`) 对持仓/观望标的逐个获取个股新闻。两源并发 (`tokio::join!`)，归一化为 `NewsItem` schema 后去重+严重性分类+LLM 归一化入 DB。

**新增 Python Provider (1 项):**
4. **`akshare_news.py`**: 通过 AkShare 库调用东财搜索 API，返回 `NewsItem` 格式。`fillna("")` + `to_dict("records")` 简化 DataFrame 处理。

**代码质量优化 (5 项 simplify):**
5. **`rpc_call<T>` 泛型方法**: `international.rs` 7 个 RPC 方法从 3 行体变为 1 行调用，消除机械重复。
6. **`probe_news` helper**: `commands/invest.rs` 健康检测 AkShare+Jin10 两个重复块合并为 3 行调用。
7. **`fallback_time` 局部变量**: `event_scanner.rs` 消除 3x 重复 `now.format(...)` 表达式。
8. **`JIN10_COUNT` / `AKSHARE_PER_STOCK_COUNT` 常量**: 魔法数字移至模块级。
9. **健康检测更新**: 5 源（Tushare 行情 / invest.db / LLM Config / AkShare 个股 / 金十数据），移除 Tushare 新闻/东财/同花顺。

**涉及文件:**
- `src-tauri/src/invest/event_scanner.rs` — Jin10+AkShare pipeline + 常量提取
- `src-tauri/src/invest/international.rs` — `rpc_call<T>` + 移除死代码 + 新方法
- `src-tauri/src/commands/invest.rs` — 健康检测重构
- `src-tauri/python-runtime/scripts/server.py` — 注册 akshare，移除 ths
- `src-tauri/python-runtime/scripts/providers/akshare_news.py` — 新 Python provider

## v5.2.17 (2026-06-09)

### Dashboard 按钮统一 + 做 T 成本均摊

**Dashboard UI 改进 (1 项):**
1. **持仓/观望按钮统一**: HoldingsTable 的 hold 和 watch 条目统一显示"编辑、买入、卖出"三个按钮。移除 `onConvert`、`onDeleteWatch` props，新增 `onBuy` prop。watch 买入直接调用 `buyStock`（自动转 hold）。移除 `confirmDeleteWatch` 死代码和 `dialogMode` 中未使用的 `'convert'`、`'add_trade'` 类型。

**做 T 成本均摊 (1 项):**
2. **卖出→买入 P&L 均摊**: `recalculate_holdings_inner_body` 新增 `PnlTracker` 结构，跟踪每个 symbol 的实现盈亏和清仓日期。卖出时计算 `pnl = shares × (sell_price - avg_cost)` 并累加；买入时将累计 P&L 均摊到新成本价：`adjusted_cost_basis = existing_cost_basis + buy_cost - realized_pnl`。日期边界规则：清仓后间隔 ≥ 2 天（自然日）P&L 过期，不再均摊（近似"中间隔出一个交易日"）。

**Simplify 审查修复 (3 项):**
3. **buy 分支统一**: 消除 `if entry.shares > 0` / `else` 两个分支，统一为单一公式（当 shares=0 时 existing_cost_basis=0，数学等价）。
4. **`_amount` 死变量移除**: buy 分支中未使用的 `_amount` 计算删除。
5. **`entry_date` clone 优化**: `add_watch` 分支中 `entry.entry_date.clone().or_else()` 改为 `get_or_insert_with()`，避免已有值时的无意义 clone。

**涉及文件:**
- src-tauri/src/storage/invest/portfolio.rs — `PnlTracker` + `is_pnl_expired()` + buy/sell 分支重写
- src/lib/components/invest/HoldingsTable.svelte — 按钮统一 + props 简化
- src/routes/invest/+page.svelte — 新增 `openBuyFromHolding` + 移除死代码
- src/lib/stores/invest-store.svelte.ts — 已有 `buyStock` 自动处理 watch→hold

## v5.2.16 (2026-06-09)

### invest 统计日期 5:00 AM 截止 + 委员会批量优化 + 收盘价格修复

**统计日期截止 (1 项):**
1. **invest 统计日期 05:00 截止逻辑**: 新增 `date_utils` 集中模块，凌晨 05:00 前运行的任务归属前一天统计日期。影响 PnL 快照、委员会归档、verdict 日期、每日报告、Dreaming 管道、调度器 trading day 检查共 8 处调用点。前端 `getInvestDate()` 使用本地时区构造（修复 `toISOString()` UTC bug）。Dashboard header 显示日期规则提示。

**委员会批量优化 (3 项):**
2. **PortfolioData 批量共享**: `run_committee_batch` / `run_committee_batch_stream` 预加载一次 `PortfolioData`（含价格刷新），通过 `Arc<PortfolioData>` 共享给所有 symbol 任务，消除 N 次重复 DB 读取和 Tushare API 调用。新增 30 秒超时保护。
3. **PortfolioData dry_run 模式**: `load_and_refresh_prices(dry_run)` 参数控制是否将更新后的 notional 写回 DB。dry_run=true 时仍获取价格（用于准确市值估算）但不持久化。
4. **notional_is_estimated 警告**: 当持仓市值基于成本价估算（未获取到实时价格）时，Risk R1 上下文注入 ⚠️ 警告，提示盈亏比和集中度数据可能不准确。

**G3 子弹数据兜底 (1 项):**
5. **cio_sanity_check Gate 3 fallback**: `actual_cash_cny: Option<f64>` 参数传入 `PortfolioData.cash`，Gate 3 检查链变为 CIO 解析 → 各轮输出 → 实际现金余额，消除"子弹数据不可用"误跳过。Gate 4 复用 Gate 2 的 `concentration` 变量（-7 行重复代码）。

**收盘价格修复 (1 项):**
6. **收盘后行情跳过 rt_k**: `get_latest_price` 和 `realtime_quotes` 新增 `is_a_share_market_open()` 判断（基于 `trade_calendar` 交易日历 + 北京时间 9:15-11:30/13:00-15:00），收盘后直接使用 `daily`/`fund_daily` 日线数据，避免 rt_k 返回非最终收盘价。

**数据完整性 (2 项):**
7. **卖出自动转 Watch**: `recalculate_holdings_inner` 的 sell 分支，当卖出导致 shares ≈ 0 时自动转为 watch 条目（保留名称、成本价、资产类型）。`MemHolding::copy_core_fields_from()` 辅助方法统一 hold↔watch 转换逻辑。
8. **卖出/删除/修改交易后现金自动同步**: `recalculate_cash_inner()` 在 `recalculate_holdings_inner` 事务内自动从交易历史重算现金，前端不再需要手动调用 `update_cash`。

**Simplify 审查修复 (5 项):**
9. **date_utils 共享内部函数**: 提取 `invest_date_naive()` 内部函数，三个公开函数从它派生，消除三重复制。
10. **committees.rs 使用 `get_invest_date_compact()`**: 替代 `.replace('-', "")` 内联实现。
11. **archive.rs 直接调用 `get_invest_naive_date()`**: 消除 `String → parse_from_str → NaiveDate` 往返。
12. **format.ts 时区修复**: `toISOString()` (UTC) 改为 `getFullYear()`/`getMonth()`/`getDate()` (本地时区)。
13. **format.ts 常量提取**: `INVEST_DATE_CUTOFF_HOUR` 常量替代魔数 `5`，注释对齐 Rust 同名常量。

**涉及文件:**
- src-tauri/src/invest/date_utils.rs — 新建，集中日期工具模块（3 函数 + 5 测试）
- src-tauri/src/invest/mod.rs — 注册 date_utils 模块
- src-tauri/src/lib.rs — PnL 快照改用 `get_invest_date()` + `get_previous_day_snapshot()`
- src-tauri/src/invest/scheduler/runner.rs — 调度器 today 改用 `get_invest_date()`
- src-tauri/src/invest/committee/archive.rs — 3 处调用改用 invest date
- src-tauri/src/storage/invest/committees.rs — verdict_date 改用 invest date
- src-tauri/src/invest/daily_report.rs — 报告日期改用 `get_invest_date()`
- src-tauri/src/invest/dreaming/pipeline.rs — dreaming today 改用 `get_invest_naive_date()`
- src-tauri/src/invest/committee/orchestrator.rs — PortfolioData Arc 共享 + dry_run + timeout
- src-tauri/src/invest/committee/analysis.rs — `actual_cash_cny` 参数 + Gate 4 去重
- src-tauri/src/storage/invest/portfolio.rs — `recalculate_cash_inner` + `copy_core_fields_from` + sell auto-convert
- src-tauri/src/storage/invest/scheduler.rs — `is_a_share_market_open()` 函数
- src-tauri/src/storage/invest/verdicts.rs — `get_previous_day_snapshot()` + `row_to_pnl_snapshot()`
- src-tauri/src/tushare/client.rs — 收盘后跳过 rt_k
- src/lib/i18n/format.ts — `getInvestDate()` 前端工具函数
- src/lib/components/invest/TradeDialog.svelte — 默认日期改用 `getInvestDate()`
- src/routes/invest/+page.svelte — Dashboard 日期规则提示
- src/lib/stores/invest-store.svelte.ts — store 方法简化
- src/lib/components/invest/CommitteeArchiveTab.svelte — UI 更新
- src/lib/components/invest/CommitteeReplayTab.svelte — UI 更新
- messages/en.json + messages/zh-CN.json — `invest_date_rule` i18n key

### 现金负数修复 + 备用金重构 + Watch 复活修复

**Bug 修复 (3 项):**
14. **交易操作导致现金变负数 (-100000)**: `set_cash_inner` 的 INSERT SQL 原本将 `initial_balance` 设为与 `available` 相同值，当现金行被清除后重建时会覆盖初始资本。修复为 INSERT 时 `initial_balance = NULL`，只有 `set_initial_cash` 才写入该列。
15. **卖出后已删除的 Watch 复活**: `recalculate_holdings_inner_body` 重放所有交易时，sell 全部卖出会自动转 watch，但不检查用户是否曾通过 `delete_watch` 明确删除过。新增 `watch_deleted: HashSet<String>` 集合追踪已删除的 watch 符号，sell 分支转换前检查该集合。
16. **Watch 转 Hold 后 Dashboard 不显示**: `convert_watch_to_hold` replay 逻辑从 watch 条目复制 shares（始终为 0），导致新 hold 条目 shares=0 被过滤跳过。修复为从交易记录 `t.shares` 读取实际数量，并调用 `recompute_notional()` 重算持仓金额。

**备用金重构 (6 项):**
17. **删除 `emergency_buffer_cny` 独立配置**: 从 `UserProfile` 结构体/SQL、`InvestLlmConfig` 结构体/JSON 序列化、`CommitteeConfig` 结构体三处完全移除。
18. **动态计算 buffer**: `effective_buffer = max(min_cash_pct across strategies) × total_assets`，无策略时 fallback 0.0 并输出 `log::warn`。
19. **风险偏好从账户类型推导**: `build_user_profile_context()` 根据 `account_purpose`（零花钱/长线/退休金/教育金）推导风险偏好描述注入 LLM prompt，无需额外设置。
20. **前端清理**: `ProviderConfigPanel` 删除 buffer 输入框，`UserProfileSection` 删除 buffer 输入/验证，`CommitteeLiveTab` 删除未使用的 buffer 计算，`InvestLlmConfig` / `UserProfile` TS 接口同步更新。
21. **i18n 清理**: 删除 4 个 i18n key（中英文各 `settings_profile_emergency_buffer`、`_desc`、`_invalid_buffer`、`invest_committee_emergency_buffer`）。
22. **参数重命名**: 全链路 `emergency_buffer_cny` → `min_cash_reserve`，LLM 上下文标签改为"最低现金储备"。

**Simplify 审查修复 (5 项):**
23. **PortfolioData timeout+fallback 提取**: 三处重复的 `tokio::time::timeout(30s, load_and_refresh_prices)` + 空 PortfolioData fallback 提取为 `PortfolioData::load_with_timeout()` + `Default` impl。
24. **无效绑定删除**: `let config = config;` 无效自绑定删除。
25. **多策略取最保守值**: `list_strategies().into_iter().next()` 改为 `fold(f64::max)` 取所有策略中最大的 `min_cash_pct`。
26. **fallback 日志**: 无策略配置时 `log::warn` 提示 Gate 3 buffer = 0。

**涉及文件 (本次追加):**
- src-tauri/src/storage/invest/portfolio.rs — `set_cash_inner` NULL 修复 + `watch_deleted` 集合
- src-tauri/src/storage/invest/user_profile.rs — 删除 `emergency_buffer_cny` 字段
- src-tauri/src/commands/invest.rs — 删除 `InvestLlmConfig.emergency_buffer_cny`
- src-tauri/src/invest/committee/orchestrator.rs — 动态 buffer 计算 + 风险偏好注入 + timeout 提取 + 参数重命名
- src-tauri/src/invest/committee/analysis.rs — 参数重命名
- src/lib/stores/invest-committee-store.svelte.ts — 删除 `emergencyBufferCny` 接口字段
- src/lib/components/invest/ProviderConfigPanel.svelte — 删除 buffer 输入
- src/lib/components/invest/UserProfileSection.svelte — 删除 buffer 输入/验证
- src/lib/components/invest/CommitteeLiveTab.svelte — 删除 buffer 计算
- src/lib/types.ts — 删除 `UserProfile.emergencyBufferCny`
- messages/en.json + messages/zh-CN.json — 删除 4 个 i18n key

---

## v5.2.15 (2026-06-07)

### 卖出自动转 Watch + G3 子弹数据兜底修复

**Bug 修复 (2 项):**
1. **卖出全部持仓后自动转为 Watch 状态**: `recalculate_holdings_inner_body` 的 sell 分支中，当卖出导致 `shares ≈ 0` 时，将 hold 条目自动转为 watch 条目（保留名称、成本价、资产类型），而非直接删除。用户可在 watch 列表中手动删除或重新买入。
2. **委员会 G3 检查"子弹数据不可用"修复**: `cio_sanity_check` 的 Gate 3 原本仅从 LLM 输出中解析 `dry_powder_cny`，LLM 经常不输出该字段导致永远跳过检查。新增 `actual_cash_cny: Option<f64>` 参数作为兜底，解析链变为 CIO 输出 → 各轮输出 → `PortfolioData.cash` 真实现金余额。

**Simplify 审查修复 (2 项):**
3. **concentration 重复计算消除**: Gate 2 和 Gate 4 的 `concentration_pct` fallback 链完全相同，Gate 4 复用 Gate 2 的 `concentration` 变量（-7 行）。
4. **hold↔watch 字段拷贝去重**: 提取 `MemHolding::copy_core_fields_from()` 辅助方法，sell auto-convert、`convert_watch_to_hold`、`convert_hold_to_watch` 三处转换逻辑统一调用，新增字段只需改一处。

**涉及文件:**
- src-tauri/src/storage/invest/portfolio.rs — sell 分支自动转 watch + `copy_core_fields_from()` 辅助方法
- src-tauri/src/invest/committee/analysis.rs — `cio_sanity_check` 新增 `actual_cash_cny` 参数 + concentration 去重
- src-tauri/src/invest/committee/orchestrator.rs — 调用处传入 `Some(portfolio_data.cash)`

---

## v5.2.14 (2026-06-06)

### PnL 快照每日收益计算修复 + 卖出现金同步修复

**Bug 修复 (2 项):**
1. **`run_pnl_snapshot()` daily_pnl 计算逻辑修复**: 同一天多次运行快照任务时，`daily_pnl` 错误地计算为与同一天前一次快照的差值，而非与前一天的差值。新增 `get_previous_day_snapshot(current_date)` 函数，通过 `WHERE snapshot_date < ?1` 确保始终与前一天比较。
2. **卖出/删除/修改交易后现金余额自动同步**: `record_trade`、`delete_trade`、`update_trade` 之前只重建持仓表不更新现金表，导致卖出后现金不增加。新增 `recalculate_cash_inner()` 函数，在 `recalculate_holdings_inner` 事务内自动从交易历史重算现金。

**Simplify 审查修复 (4 项):**
3. **SQL 重复消除**: 提取 `get_initial_cash_inner(conn)` 和 `set_cash_inner(conn, amount)` 辅助函数，`recalculate_cash_inner` 复用而非内联重复 SQL。
4. **`convert_watch_to_hold` 现金同步**: 该操作等同买入（扣现金），`recalculate_cash_inner` 现在正确识别并扣除。
5. **移除前端冗余 `update_cash`**: `buyStock`/`sellStock`/`convertWatchToHold` 不再手动调用 `update_cash`，由后端 `recalculate_cash_inner` 统一处理，消除双写竞争。
6. **`updateCash` 简化**: 移除 `cash_adjust` 交易记录（与后端重算冲突），改为直接 `set_cash`。

**收盘价格修复 (1 项):**
7. **收盘后行情显示非最终收盘价**: 腾讯 API 失败时降级到 Tushare `rt_k`（盘中接口），收盘后返回的是最后一笔盘中价而非官方收盘价。新增 `is_a_share_market_open()` 函数（基于 `trade_calendar` 交易日历+北京时间时段判断），收盘后跳过 `rt_k` 直接使用 `daily`/`fund_daily` 日线数据。

**Simplify 审查修复 (3 项):**
8. **PnlSnapshot 行映射重复消除**: 提取 `row_to_pnl_snapshot()` 辅助函数（复用已有 `row_to_verdict` 模式），`list_pnl_snapshots` 和 `get_previous_day_snapshot` 共用。
9. **市场时段判断提升到 scheduler 模块**: `is_a_share_market_open()` 从 `TushareClient` 移至 `storage::invest::scheduler`，消除 HTTP 客户端对存储层的反向依赖。
10. **`get_latest_price` 收盘后跳过 `rt_k`**: 与 `realtime_quotes` 保持一致，收盘后直接降级到日线收盘价。

**涉及文件:**
- src-tauri/src/lib.rs — `run_pnl_snapshot()` 改用 `get_previous_day_snapshot`
- src-tauri/src/storage/invest/verdicts.rs — 新增 `get_previous_day_snapshot()` 函数
- src-tauri/src/storage/invest/portfolio.rs — 新增 `recalculate_cash_inner()` + `get_initial_cash_inner` + `set_cash_inner` 提取
- src/lib/stores/invest-store.svelte.ts — 移除冗余 `update_cash` 调用，简化 `updateCash`
- src/lib/components/invest/TradeDialog.svelte — 移除 `add_trade` 模式冗余 `updateCash`

---

## v5.2.13 (2026-06-03)

### 工具调用解析器增强 + encode_cwd 冒号兼容 + 13 单元测试

**XML 解析器 (4 项):**
1. **parse_function_calls() 解析器**: 新增 function_calls/invoke 格式支持，覆盖 LLM 偶发 XML 工具调用
2. **parse_fn_tag_body() 辅助函数**: 处理 function=name + parameter=value 变体格式
3. **infer_json_value() 类型推断**: 从字符串自动推断 JSON 值类型(bool/i64/f64/string)，消除重复逻辑
4. **TOOL_CALL_TAG_PAIRS 常量**: 共享的工具调用标签对列表，供解析器和残留清理共用

**残留标签清理安全网 (1 项):**
5. **strip_residual_tool_call_tags() 安全网**: 所有解析器未匹配但 XML 标签残留时，自动剥离防止泄露到角色输出

**DSML 单竖线规范化 (1 项):**
6. **parse_dsml_tool_calls() 单竖线支持**: 自动检测并规范化单竖线 DSML 变体为双竖线

**collect_stream() 四级降级链 (1 项):**
7. **降级链升级**: DSML -> parse_function_calls -> parse_xml_tool_calls -> strip_residual

**encode_cwd() Windows 盘符兼容 (2 项):**
8. **Rust encode_cwd() 冒号替换**: Windows 路径编码从 C:-Users 修正为 C--Users，匹配 Claude CLI 实际目录命名
9. **TypeScript encodeCwdSlug() 同步**: 前端 slug 生成同步加入冒号替换，保持前后端一致

**单元测试 (13 项):**
10. test_parse_function_calls_basic — 基本解析 + bool 推断
11. test_parse_function_calls_multiple — 多 invoke 解析
12. test_parse_function_calls_numeric_params — 数值参数推断
13. test_parse_function_calls_no_tags — 无标签返回 None
14. test_parse_function_calls_with_surrounding_text — 包裹文本中解析
15. test_parse_xml_fn_eq_basic — function=name 基本解析
16. test_parse_xml_fn_eq_with_params — function=name 带参数
17. test_strip_residual_function_calls_tags — 残留清理
18. test_strip_residual_tool_call_tags — 残留清理
19. test_strip_no_tags_unchanged — 无标签不变
20. test_parse_dsml_single_bar — DSML 单竖线解析
21. encode_cwd 冒号测试更新 — Windows 路径断言修正

**涉及文件:**
- src-tauri/src/invest/llm/types.rs — 3 个新解析器 + 辅助函数 + 安全网 + 13 测试
- src-tauri/src/storage/cli_sessions.rs — encode_cwd() 冒号替换 + 测试更新
- src/lib/utils/format.ts — encodeCwdSlug() 冒号替换

---

## v5.2.12 (2026-06-03)

### Tushare moneyflow_dc Schema 迁移 + XML 工具调用解析 + Changelog 查看器 + 委员会批量运行

**Tushare `moneyflow_dc` API Schema 迁移 (5 项):**
1. **`MoneyflowDc` 结构体重写**: 字段从 `*_vol`(买/卖分开, 11 字段) 改为 `*_amount`(净额, 7 字段)，匹配 Tushare 上游 API 变更
2. **`moneyflow_dc()` 解析器更新**: 字段名查找从 `buy_sm_vol`/`sell_sm_vol` 等改为 `buy_sm_amount`/`buy_md_amount` 等
3. **`aggregate_moneyflow()` 简化**: 从 `buy - sell` 对减改为直接累加净额（API 已预计算）
4. **`format_moneyflow_summary()` 单位修正**: 从"万手"改为"亿元"（÷10000），保留两位小数
5. **`get_moneyflow_def()` 描述更新**: 工具说明反映新数据格式（净额万元/汇总亿元）

**Plain XML 工具调用解析器 (4 项):**
6. **`parse_xml_tool_calls()` 解析器**: 新增对 `<tool_call>{"name":"...","arguments":{...}}</tool_call>` 格式的支持，覆盖 LLM 偶发的纯文本工具调用
7. **`finish_tool_parse()` 共享函数**: 提取 DSML 和 XML 解析器共用的日志+Option 包装逻辑，消除重复代码
8. **`collect_stream()` 二级降级**: DSML 格式检测失败后尝试 plain XML 格式，确保工具调用不丢失
9. **7 个单元测试**: 覆盖基本解析、多调用、畸形 JSON、空参数、缺字段、缺闭合标签等场景

**AboutModal Changelog 查看器 (UI 重写):**
10. **从 README 渲染改为 changelog.md 解析**: `parseChangelog()` 支持 `vX.Y.Z (date)` 和 `Phase X` 两种头部格式
11. **版本折叠/展开**: 每个版本独立折叠，当前版本自动高亮+展开
12. **搜索过滤**: 按版本号、标题、正文内容实时过滤
13. **懒解析**: 首次打开 Modal 时才解析 changelog，避免启动开销
14. **i18n**: 新增 `about_changelog`/`about_searchVersions`/`about_expandAll`/`about_collapseAll`/`about_noMatching` 5 个 key

**委员会直播页批量运行 (UX 增强):**
15. **复选框选择**: 每个持仓卡片前新增 checkbox，支持多选标的
16. **"运行选中"按钮**: 只运行勾选的标的，与"运行全部"并列
17. **全选/取消选择**: 一键批量操作，运行状态时禁用
18. **i18n**: 新增 `invest_run_selected`/`invest_select_all`/`invest_clear_selection`/`invest_select_symbol` 4 个 key

**委员会 UI 细节优化 (3 项):**
19. **`buildVerdictMap()` 共享函数**: `CommitteeArchiveTab` 和 `CommitteeReplayTab` 的 verdict 预计算逻辑提取到 `invest-verdict.ts`
20. **MarkdownContent 渲染**: ArchiveTab 和 ReplayTab 归档内容从纯文本改为 Markdown 渲染
21. **ReplayTab verdict 徽章**: 侧边栏日期列表和详情页头部显示 verdict 色标徽章

**工具函数提取 (2 项):**
22. **`encodeCwdSlug()` 提取**: 从 `+layout.svelte` 内联函数提取到 `format.ts` 共享模块，memory 页面复用
23. **Memory 页面项目范围过滤**: `scopeMemory` 按 `encodeCwdSlug(projectCwd)` 过滤，只显示当前项目的记忆条目

**涉及文件:**
- `src-tauri/src/tushare/client.rs` — `MoneyflowDc` 结构体+解析器+聚合+格式化
- `src-tauri/src/invest/committee/tools.rs` — `get_moneyflow_def()` 描述
- `src-tauri/src/invest/llm/types.rs` — `parse_xml_tool_calls()` + `finish_tool_parse()` + 测试
- `src-tauri/src/invest/committee/orchestrator.rs` — DSML 注释更新
- `src/lib/components/AboutModal.svelte` — Changelog 查看器重写
- `src/lib/components/invest/CommitteeLiveTab.svelte` — 批量运行
- `src/lib/components/invest/CommitteeArchiveTab.svelte` — verdictMap + MarkdownContent
- `src/lib/components/invest/CommitteeReplayTab.svelte` — verdictMap + MarkdownContent + 徽章
- `src/lib/utils/invest-verdict.ts` — `buildVerdictMap()` 共享函数
- `src/lib/utils/format.ts` — `encodeCwdSlug()` 共享函数
- `src/routes/+layout.svelte` — 使用共享 `encodeCwdSlug`
- `src/routes/memory/+page.svelte` — 项目范围过滤
- `messages/en.json` — 10 个新 i18n key
- `messages/zh-CN.json` — 10 个新 i18n key

---

## v5.2.11 (2026-06-03)

### 记忆管理重构 + DSML 工具调用格式兼容 + 委员会 UI 修复

**记忆管理重构 (8 项):**
1. **`list_memory_files` 定向扫描**: 新增 `project_paths` 参数，从全量扫描 `~/.claude/projects/*/memory/` 改为按已知项目路径定向查询，减少无关项目噪声
2. **`remove_memory` 命令**: 新增 Tauri IPC 命令，前端可直接删除记忆条目
3. **`archive_memory` 命令**: 新增 Tauri IPC 命令，将记忆状态设为 `archived`
4. **`restore_memory` 命令**: 新增 Tauri IPC 命令，将已归档记忆恢复为 `approved`（置信度 60）
5. **`memory_store` 归档/恢复函数**: `archive_memory()` 和 `restore_memory()` 从 `memory_dream.rs` 迁移到 `memory_store.rs`，统一存储层职责
6. **`EmbeddingConfig` 设置持久化**: `embedding_config` 从 localStorage 迁移到 `UserSettings`，`settings.rs` 新增 `apply_embedding_config` 处理函数
7. **`memory_dream_enabled` 设置持久化**: `memory_dream_enabled` 从 localStorage 迁移到 `UserSettings`
8. **记忆管理页面重写**: `/memory-mgmt` 页面从 localStorage 读取配置改为通过 `updateUserSettings` 持久化到后端，新增 `saveSettingsPatch` 通用辅助函数

**DSML 工具调用格式兼容 (3 项):**
9. **`parse_dsml_tool_calls` 解析器**: 新增 DeepSeek/MiMo 原生 DSML 格式工具调用解析，支持 `<｜｜DSML｜｜invoke>` + `<｜｜DSML｜｜parameter>` 嵌套标签结构
10. **`collect_stream()` 层规范化**: DSML 格式检测和解析下沉到 `collect_stream()` 后置步骤，`CollectedResponse` 契约保证 `.tool_calls` 已填充，所有下游消费者自动受益
11. **`orchestrator.rs` 简化**: `run_with_tool_loop()` 移除 DSML 特殊逻辑，解构 `CollectedResponse` 消除 clone，`tc.arguments.to_string()` 缓存为局部变量

**委员会 UI 修复 (3 项):**
12. **`CommitteeAccuracyTab` 加载修复**: `$effect` 改为 `onMount`，避免 Svelte 5 响应式循环导致的重复加载
13. **`CommitteeLiveTab` 紧急缓冲金**: 新增 `emergencyBufferCny` 指标，从 `llmConfig` 读取并展示
14. **现金显示精度修复**: `formatCash` 从整数 K 改为一位小数 K（如 ¥12.3K），标签从 `invest_committee_emergency` 改为 `invest_cash`

**涉及文件:**
- `src-tauri/src/commands/files.rs` — `list_memory_files` 新增 `project_paths` 参数
- `src-tauri/src/commands/memos.rs` — 新增 `remove_memory`/`archive_memory`/`restore_memory`
- `src-tauri/src/storage/memory_store.rs` — 新增 `archive_memory`/`restore_memory`
- `src-tauri/src/group_chat/memory_dream.rs` — 移除 `restore_archived_memory`（迁移至 memory_store）
- `src-tauri/src/storage/settings.rs` — 新增 `apply_embedding_config`/`apply_bool_field`
- `src-tauri/src/invest/llm/types.rs` — 新增 `parse_dsml_tool_calls` + `collect_stream` DSML 规范化
- `src-tauri/src/invest/committee/orchestrator.rs` — `run_with_tool_loop` 简化
- `src-tauri/src/lib.rs` — 注册新 Tauri 命令
- `src-tauri/src/web_server/dispatch.rs` — `list_memory_files` 调用适配
- `src/lib/api.ts` — `listMemoryFiles` 新增 `projectPaths` 参数
- `src/lib/types.ts` — 新增 `EmbeddingConfig` 接口
- `src/lib/components/invest/CommitteeAccuracyTab.svelte` — `$effect` → `onMount`
- `src/lib/components/invest/CommitteeLiveTab.svelte` — `emergencyBuffer` + `formatCash` 修复
- `src/routes/+layout.svelte` — `listMemoryFiles` 传入 `knownPaths`
- `src/routes/memory-mgmt/+page.svelte` — 配置持久化重写

## v5.2.10 (2026-06-03)

### 委员会直播页面崩溃修复 — watch/hold 持仓去重

**Bug 修复 (3 项):**
1. **`watchHoldings` store 层去重**: `invest-store.svelte.ts` 的 `watchHoldings` 从简单 kind 过滤改为 `$derived.by` + `holdSymbolSet` 排除已在 `holdHoldings` 中的 symbol；根因是 `buyStock` 直接买入已在 watch 列表中的股票时，不会自动清理 watch 条目，导致同一 symbol 同时存在于 holdHoldings 和 watchHoldings，`{#each} (asset.symbol)` 重复 key 触发 Svelte 运行时崩溃
2. **`CommitteeLiveTab.allAssets` 简化**: store 保证去重后，`allAssets` 从手动 `seen` Set + 双循环恢复为简洁的 `.map()` 形式
3. **`CommitteeReplayTab.allHoldings` 简化**: 同上，移除组件级冗余去重逻辑

**Simplify 审查修复 (3 项):**
4. **Altitude 修复**: 去重从组件层（消费者自行 dedup）提升到 store 层（数据源自动过滤），所有消费者自动受益，无需重复实现
5. **Reuse 修复**: 消除 CommitteeLiveTab 和 CommitteeReplayTab 之间重复的 `seen` Set 去重模式
6. **Simplification**: 两个循环体合并为单一 `.map()` 调用

**涉及文件:**
- `src/lib/stores/invest-store.svelte.ts` — `watchHoldings` 加入 holdSymbolSet 去重
- `src/lib/components/invest/CommitteeLiveTab.svelte` — `allAssets` 简化
- `src/lib/components/invest/CommitteeReplayTab.svelte` — `allHoldings` 简化

## v5.2.9 (2026-06-03)

### 腾讯行情 API 集成 + ETF 价格修复 + asset_type 全链路修复 + DB 迁移安全修复 + 代码审查优化

**腾讯行情 API 集成 (3 项):**
1. **`tencent_quotes` 模块**: 新建 `src-tauri/src/tencent_quotes.rs`，实现 `fetch_quotes()` 批量获取实时行情；API `http://qt.gtimg.cn/q={symbols}` 免费、无需认证、支持 ETF+股票批量查询；响应 `~` 分隔字段解析（name/close/pre_close/open/high/low/vol/amount/trade_time）
2. **`realtime_quotes` 优先腾讯**: TushareClient::realtime_quotes() 调用链改为 腾讯 API → 部分成功降级 → Tushare rt_k → fund_daily/daily fallback 四层降级；腾讯完全成功时直接返回，部分成功时对缺失符号降级到 Tushare
3. **共享 `reqwest::Client`**: `fetch_quotes` 接受 `&reqwest::Client` 参数复用 TushareClient 的连接池，避免每次调用创建新 TLS 客户端的 ~5-15ms 开销

**ETF 价格显示修复 (2 项):**
4. **`resolve_close_idx` 辅助方法**: 提取统一的价格列定位函数，优先 `price_field(ts_code)`（ETF→adj_nav, 股票→close），兜底 `"close"`；`daily()`、`fallback_daily_quote()`、`get_latest_price()` 三处调用统一
5. **`get_latest_price` fallback 修复**: fields 字符串从 `{pf}` 改为 `{pf},close`，`resolve_close_idx` 替代无兜底的 `position(|f| f == pf)`，解决 ETF 调用 `adj_nav` 字段不存在时的错误

**DB 迁移安全修复 (3 项):**
11. **`init_with_fallback` 备份策略**: 迁移失败时先备份数据库文件（带时间戳），再删除重试，避免直接删除导致数据丢失
12. **`backup_db_files` 函数**: 创建带时间戳的备份副本（`invest.db.backup_20260603_123456`），包含 WAL/SHM sidecars
13. **`migrate_trades_table` 宽容迁移**: 动态检测旧表列结构，只复制存在的列，缺失列用 NULL 填充，防止列不匹配导致迁移失败

**Simplify 审查修复 — DB 迁移安全 (7 项):**
17. **`conn.transaction()` 替换手动 `BEGIN/COMMIT`**: RAII 自动回滚，防止迁移失败时连接中毒和 `trades_new` 僵尸表
18. **`get_table_columns` 返回 `Result`**: 列表内省失败时立即报错，不再静默返回空 Vec 导致全表数据被 NULL 覆盖（静默数据擦除风险）
19. **`DB_SIDECAR_EXTS` 常量**: `backup_db_files` 和 `delete_db_files` 共享扩展名列表，消除重复
20. **`HashSet<String>` 替换 `Vec::contains`**: O(1) 查找替代 O(n) 线性扫描，消除 13 次/轮的 String 分配
21. **`TRADES_COLUMNS` 静态常量**: 13 个列名不再每次调用堆分配，`column_list` 只 join 一次复用于 INSERT 语句
22. **`get_table_columns` + `has_column` 语义分离**: 各司其职，不合并（单列检查 vs 全列内省）
23. **删除 20 行死代码**: `has_column` 对 trades.name/trade_date/asset_type 的 fallback 迁移块，已被 `migrate_trades_table` 覆盖，每次 init 执行无意义 I/O

**asset_type 全链路修复 (5 项):**
6. **`Trade.asset_type` 字段**: Trade 结构体新增 `asset_type: Option<String>`；SQLite trades 表新增 `asset_type TEXT` 列；DB migration 自动从 holdings 回填现有交易
7. **`update_trade` SQL 修复**: UPDATE 语句从 11 参数补齐至 12 参数（补 `asset_type=?12`），修复编辑交易时 asset_type 丢失的 bug
8. **`resolve_asset_type()` 推导函数**: 优先使用 trade 提供的值，兜底从 symbol 前缀推导（`is_etf_symbol`）；buy/add_watch/convert_watch_to_hold 三处统一调用
9. **前端 `assetType` 透传**: `recordTrade`/`updateTrade`/`buyStock`/`sellStock`/`addToWatch`/`convertWatchToHold` 6 处 IPC 调用补齐 `assetType` 参数；TradeDialog `add_trade` 模式传入 `assetType`
10. **`is_etf_symbol` 共享函数**: 提取到 `storage/invest/mod.rs` 作为 `pub fn`，`portfolio.rs` 和 `tushare::client.rs` 统一调用，消除 13 项 ETF 前缀列表的重复维护风险

**Simplify 审查修复 (6 项):**
11. **移除重复 `RealtimeQuote` 结构体**: `tencent_quotes.rs` 删除本地 `RealtimeQuote`，直接使用 `tushare::client::RealtimeQuote`，消除 ~32 行冗余代码和手动字段映射
12. **`fetch_quotes` 复用 client**: 改为接受 `&reqwest::Client` 参数，不再每次调用创建新客户端
13. **`is_etf_symbol` 共享化**: ETF 前缀匹配逻辑从 2 处独立实现收敛为 1 个共享函数
14. **`parse_quote_line` 长度守卫修复**: 最小字段数从 `<33` 改为 `<38`，修复访问 `parts[37]` 时的潜在 panic
15. **部分成功降级**: 腾讯返回数 < 请求数时自动对缺失符号降级到 Tushare，不再静默丢弃
16. **`RealtimeQuote` 注释更新**: doc comment 从 "via rt_k API" 更新为 "Sources: Tencent API (primary) or Tushare rt_k (fallback)"

**涉及文件:**
- `src-tauri/src/tencent_quotes.rs` — 新建，腾讯行情 API 客户端
- `src-tauri/src/tushare/client.rs` — realtime_quotes 腾讯优先 + resolve_close_idx + is_etf_code 委托共享函数 + 注释更新
- `src-tauri/src/storage/invest/portfolio.rs` — asset_type 字段 + resolve_asset_type + is_etf_symbol 改用共享函数
- `src-tauri/src/storage/invest/mod.rs` — trades 表 asset_type 迁移 + pub is_etf_symbol + DB 迁移安全重构(备份/宽容迁移/transaction/HashSet/常量/死代码清理)
- `src-tauri/src/commands/invest.rs` — record_trade/update_trade 补 asset_type 参数
- `src-tauri/src/lib.rs` — 注册 tencent_quotes 模块
- `src/lib/stores/invest-store.svelte.ts` — 6 处 IPC 补 assetType
- `src/lib/components/invest/TradeDialog.svelte` — add_trade 模式传入 assetType
- `src/lib/types.ts` — Trade 接口补 assetType

---

## v5.2.8 (2026-06-03)

### invest DB 迁移修复 + Watch 价格刷新 + 委员会 Bug 修复 + 代码审查优化

**迁移修复 (3 项):**
1. **`trades_new` 列数修复**: 3 处迁移块的 `CREATE TABLE trades_new` 从 10 列补齐至 12 列（补 `name TEXT, trade_date TEXT`），解决 `SELECT * FROM trades` 返回 12 值但目标表仅 10 列导致的 `has 10 columns but 12 values were supplied` 错误
2. **显式列名列表**: 所有迁移的 `INSERT ... SELECT *` 替换为显式 12 列名列表，消除列顺序依赖
3. **`CREATE_TABLES_SQL` 同步**: 首次建库的 trades 表定义同步补上 `name TEXT, trade_date TEXT`，避免新库 schema 与代码 INSERT 12 列不一致

**Fallback 机制 (3 项):**
4. **`init_with_fallback`**: 包装 `init_db_inner`，迁移失败时自动删除 `.db` + `-wal` + `-shm` 文件后重试一次，用户无需手动删除残缺数据库
5. **`delete_db_files` 辅助函数**: 统一清理 SQLite 主文件及 WAL/SHM 侧文件，供 fallback 和 lazy init 复用
6. **Lazy init 路由统一**: `with_conn` / `with_conn_mut` 的 lazy init 路径同样走 `init_with_fallback`，保证任何入口初始化失败都能自愈

**Watch 价格刷新修复 (2 项):**
7. **`refreshPrices` 收盘守卫修复**: `Object.keys(this.priceMap).length > 0` 改为 `syms.every(s => s in this.priceMap)`，只要任一持仓没缓存就不跳过，解决新 watch 项在收盘后永远拿不到价格的 bug
8. **`addToWatch` 价格预填**: 添加 watch 后立即将用户输入的价格写入 `priceMap`，不等下一次刷新周期，解决添加后显示"—"的问题

**Bug 修复 (3 项):**
9. **EventWatchTab 类型修复**: `t()` 的 `scanResult` 参数 `fetched`/`filtered`/`saved` 从 `number` 包装为 `String()`，修复 3 个 `svelte-check` 类型错误
10. **ETF 实时价格 `adj_nav` fallback**: `rt_k` 接口对 ETF 返回 `close=0` 时自动 fallback 到 `adj_nav`（复权单位净值）；仅 `close > 0` 时标记为已处理，确保未命中 ETF 正确降级到 `fund_daily`
11. **PnL 快照手动触发后前端不刷新**: `SchedulerTab.runNow()` 触发 `pnl_snapshot` 后未刷新 `investStore.pnlSnapshots`，导致 PnL 历史页显示为空。修复：新增 `refreshPnlSnapshots()` 轻量方法（1 次 IPC 替代 `loadAll` 的 7 次），按 job 类型定向刷新 + `loadJobs` 并行化

**Simplify 审查修复 (6 项):**
10. **`invest_db_path` 提取**: db_path 构建从 3 处重复收敛为 1 个 helper，`ensure_dir` 错误处理统一（lazy init 路径不再静默吞错）
11. **`ensure_conn` 提取**: `with_conn` / `with_conn_mut` 的 7 行 lazy init 块收敛为 `ensure_conn(&mut guard)` 单行调用
12. **`has_column` 提取**: 5 处 `pragma_table_info` 列检查迁移块从 6-8 行收敛为 `if !has_column(&conn, table, col)`，返回类型统一为 `bool`
13. **冗余迁移块删除**: 迁移块 2（add_watch）和迁移块 3（delete_watch）删除 — 第一个迁移块已包含全部 8 个 action，后续为无用 no-op，每次冷启动多跑 2 个空事务
14. **`delete_db_files` 简化**: `["", "-wal", "-shm"]` + 条件分支改为 `["db", "db-wal", "db-shm"]` 直接迭代
15. **`init_db` 签名调整**: `data_dir` 参数改为 `_data_dir`（内部通过 `invest_db_path()` 获取），统一路径来源

**Simplify 审查修复 (3 项):**
16. **`refreshPnlSnapshots()` 轻量方法**: store 新增单独刷新 pnl_snapshots 的方法，避免 `loadAll()` 的 7 次 IPC 全量加载
17. **SchedulerTab 按 job 类型定向刷新**: `pnl_snapshot` → `refreshPnlSnapshots()`（1 IPC），`verdict_review`/`dream_invest` → `loadAll()`，其他 job → 不刷新
18. **`loadJobs` + store 刷新并行化**: `Promise.all([loadJobs(), storeRefresh])` 替代顺序 await，减少手动触发延迟

**委员会 Bug 修复 (2 项):**
19. **Quant R1 资金流向无数据**: `build_asset_context()` 缓存完整性检查从粗粒度 `cache_entries.len() >= 3` 升级为按数据类型逐一检查（`has_daily_basic`/`has_fina`/`has_moneyflow`）；核心数据齐全但 `moneyflow_dc` 缺失时调用定向 `refresh_moneyflow_cache()` 针对性刷新；刷新失败时日志提示检查 Tushare API 权限；股票无资金流向时自动添加 `"资金流向=N/A"` 数据质量警告
20. **Risk R1 集中度不一致**: 后端集中度公式 `个股市值 / 总持仓市值 × 100`（不含现金）改为 `个股市值 / (总持仓市值 + 现金) × 100`，与前端 `CommitteeLiveTab` 的 `totalAssets` 分母对齐；`build_portfolio_summary` 组合概览表集中度列同步修正

**Simplify 审查修复 (6 项):**
21. **`PortfolioData::total_assets()` 方法**: `total_notional + cash` 公式收敛为单一来源，消除 `build_portfolio_summary` 和 `concentration_for_symbol` 中的重复计算
22. **`refresh_moneyflow_cache()` 辅助函数**: 从 `build_asset_context` 提取 40 行内联资金流向刷新逻辑，减少函数嵌套深度
23. **内存追加代替 DB 重读**: `refresh_moneyflow_cache` 成功后直接 `entries.push()` 而非 `load_all_latest_for_symbol()` 重新查询 SQLite
24. **单次 `chrono::Utc::now()` 调用**: 修复 `today`/`five_days_ago` 中重复调用导致潜在时钟跨越
25. **`has_type` 闭包**: 3 处 `cache_entries.iter().any(|(t, _, _)| t == "xxx")` 收敛为 `let has_type = |t| ...; has_type("xxx")` 模式
26. **`concentration_for_symbol` 简化**: 直接使用 `portfolio_data.total_assets()` 替代本地计算 `total_notional + cash`

**涉及文件:**
- `src-tauri/src/storage/invest/mod.rs` — 迁移修复 + fallback + 6 项 simplify（invest_db_path/ensure_conn/has_column/迁移块删除）
- `src-tauri/src/lib.rs` — 启动日志级别
- `src-tauri/src/tushare/client.rs` — ETF rt_k adj_nav fallback + handled 守卫
- `src-tauri/src/invest/committee/orchestrator.rs` — Quant 资金流向修复 + Risk 集中度修复 + 6 项 simplify
- `src/lib/stores/invest-store.svelte.ts` — refreshPrices 守卫 + addToWatch 价格预填 + refreshPnlSnapshots()
- `src/lib/components/invest/EventWatchTab.svelte` — String() 类型包装
- `src/lib/components/invest/SchedulerTab.svelte` — PnL 快照刷新 + 按 job 类型定向刷新 + 并行化
- `src/lib/components/invest/*.svelte`（25 文件）— CSS `border-[var(--border)]` → `border-border` 统一

---

## v5.2.6 (2026-06-02)

### 持仓名称持久化 + 收盘价格修复 + 代码搜索 + 数据初始化 + 持仓编辑 + 手动交易

**核心架构 (3 项):**
1. **`trades.name` / `trades.trade_date` 字段**: Trade 结构体+SQLite 表新增 `name`（中文名）和 `trade_date`（用户指定日期）列；DB migration 自动迁移+从 holdings 回填现有交易名称
2. **`recalculate_holdings_inner` 名称恢复**: buy/add_watch 操作从 `trade.name` 设置持仓名称，确保卖出后名称不丢失；convert_watch_to_hold 保留已有名称
3. **`investStore.nameMap` 三源合并**: `$derived` 计算 `Map<symbol, name>`，合并 holdings + priceMap + trades 三源，所有组件统一使用（替代 4 处本地 nameMap）

**价格修复 (2 项):**
4. **`refreshPrices` 智能守卫**: 移除 `isMarketOpen()` 硬拦截，改为"收盘后已有缓存则跳过"策略 — 首次收盘后获取一次日线收盘价，后续 60s 轮询自动跳过，避免重复 API 调用
5. **ETF 实时价格修复**: `get_latest_price` 先尝试 `rt_k` 盘中实时价（股票+场内 ETF 均支持），失败再降级到 `daily`/`fund_daily`；`fund_daily` 无 `close` 字段，统一使用 `price_field()` 映射到 `adj_nav`（复权单位净值）；`realtime_quotes` stock/ETF 路径统一为"rt_k 批量→跟踪 missing→fallback"循环，提取 `fallback_many` 辅助方法；`parse_realtime_quotes` 增加 `adj_nav` fallback

**股票搜索增强 (2 项):**
6. **`stock_basic` ts_code 精确匹配**: 检测 6 位数字+可选 `.SH`/`.SZ` 格式后优先查 `ts_code` 参数，失败再 fallback 到 name 模糊搜索和 symbol 搜索
7. **`fund_basic` 代码匹配**: ETF 搜索同时匹配 `ts_code` 和基金名称（支持输入 `510300` 直接定位 ETF）

**新功能 (3 项):**
8. **`init_invest_data` 命令**: 系统页新增数据初始化按钮，一键同步交易日历（2 年）+设置初始余额
9. **持仓编辑**: HoldingsTable 新增编辑按钮，支持修改持仓的买入日期/成本价/数量/备注（通过 `update_holding` 命令）
10. **手动添加交易**: TradeLogTab 新增"手动添加交易"按钮，支持买入/卖出方向选择+自定义日期+备注

**UI 改进 (3 项):**
11. **TradeDialog 重构**: 新增 `add_trade`/`edit_holding` 两种模式；所有交易操作传入 `name`/`tradeDate`；`title`/`needsSearch`/`canSubmit` 提取为 `$derived` 派生值
12. **TradeLogTab 增强**: 使用 `investStore.nameMap`（含卖出后名称）；表格显示"中文名 + 代码"双行；CSV 导出增加名称列；`tradeDate()` helper 复用
13. **CommitteeArchiveTab/CommitteeAccuracyTab/StrategyTab**: 统一使用 `investStore.nameMap` 替代本地 nameMap，卖出后的标的也能显示中文名

**Simplify 审查修复 (12 项):**
14. **Reuse**: `add_trade`/`edit_holding` 移除动态 `import('$lib/transport')`，改为 `recordTrade()`/`updateHoldingMeta()` store 方法
15. **Efficiency**: `refreshPrices` 收盘后智能守卫（已有缓存则跳过），`isMarketOpen()` 不再是死代码
16. **Altitude**: `looks_like_code` 检测从脆弱字符检测改为精确 6 位数字+.SH/.SZ 匹配
17. **Simplification**: `portfolio.rs` 提取 `TRADE_COLUMNS` 常量+`trade_from_row()` 辅助函数，消除 3 处 12 列重复
18. **Simplification**: `mod.rs` 合并两处 `pragma_table_info` 迁移块为一次查询
19. **Simplification**: `TradeLogTab` CSV 导出复用 `tradeDate()` helper
20. **Reuse**: `daily()`/`fallback_daily_quote()` 统一使用 `price_field()` 替代 inline `.or_else(|| adj_nav)` 重复
21. **Simplification**: `get_latest_price()` rt_k 路径 4 层嵌套改为 `parse_realtime_quotes()` + `find()`，2 行
22. **Simplification+Altitude**: Stock/ETF `realtime_quotes` 两段重复 try-then-fallback 合并为统一循环
23. **Reuse**: 提取 `fallback_many()` 辅助方法，消除 stock/ETF 两处 `join_all(fallback_daily_quote)` 重复
24. **Efficiency**: ETF `HashSet<String>` 改为 `Vec<String>` + `any()`，小集合无需哈希开销
25. **Altitude**: `parse_realtime_quotes` 增加 `adj_nav` fallback，ETF rt_k 响应也能正确解析

**Bug 修复 (5 项 + 7 项审查修复):**
26. **数据初始化重写**: `init_invest_data` 改为清空所有 invest 表 → 同步交易日历 → 设初始余额；`clear_all_invest_data` 从 `sqlite_master` 动态枚举表名（无硬编码列表），单事务批量删除，`sqlite_sequence` 全量重置
27. **命中率统计单位**: `verdict_tracking::start_tracking` 增加 `(symbol, verdict_date)` 去重 — 同日同 symbol 的多个 CIO 裁决只追踪一条
28. **Cron 6 字段修复**: 全部 6 个默认定时任务 cron 表达式从 5 字段 UNIX 格式修正为 6 字段（补秒字段）；`cron` crate v0.15 要求秒字段，此前所有定时任务静默跳过未执行
29. **事件扫描语言感知**: `scan_events` 增加 `language` 参数，LLM 归一化 prompt 按语言切换（中/英文）；前端 `triggerScan` 传递 `currentLocale()`
30. **扫描结果反馈**: `triggerScan` 保存 `ScanResult`，EventWatchTab 展示扫描完成摘要（条数统计/错误信息）

**Simplify 审查修复 (7 项):**
31. **Altitude**: `clear_all_invest_data` 改为 `conn.transaction()` + `sqlite_master` 动态表名，消除硬编码列表 + 缺 `trade_calendar` + `sqlite_sequence` 漂移
32. **Simplification**: `humanCron` 按字段数（5 vs 6）判断替代 regex `/^0\s+/` 误匹配
33. **Reuse**: `"zh-CN"` 提取为 `event_scanner::DEFAULT_LANGUAGE` 常量，3 处调用统一引用
34. **Simplification**: `normalize_events` 签名 `Option<&str>` → `&str`，消除双层默认
35. **Simplification**: 扫描警告 `console.warn` → `console.debug`（UI 已展示）
36. **Simplification**: 初始化成功后清空余额输入框
37. **Altitude**: `text-[#b89a6a]` → `text-[var(--color-warning)]` 设计系统一致性

**涉及文件:**
- 修改：`src-tauri/src/storage/invest/portfolio.rs`、`mod.rs`、`verdict_tracking.rs`、`src-tauri/src/commands/invest.rs`、`src-tauri/src/tushare/client.rs`、`src-tauri/src/lib.rs`
- 修改：`src-tauri/src/invest/event_scanner.rs`、`src-tauri/src/invest/scheduler/mod.rs`、`runner.rs`
- 修改：`src/lib/stores/invest-store.svelte.ts`、`src/lib/types.ts`、`src/routes/invest/+page.svelte`
- 修改：`src/lib/components/invest/TradeDialog.svelte`、`TradeLogTab.svelte`、`HoldingsTable.svelte`、`CommitteeArchiveTab.svelte`、`CommitteeAccuracyTab.svelte`、`StrategyTab.svelte`、`EventWatchTab.svelte`、`SchedulerTab.svelte`
- 修改：`messages/en.json`、`messages/zh-CN.json`

---

## v5.2.5 (2026-06-02)

### 委员会数据缓存 + 8 段注入优化 + 前端名称显示

**核心架构 (3 项):**
1. **`stock_data_cache` 永久缓存表**: invest.db 新增 `(symbol, data_type, data_date)` 三元主键表，存储 daily_basic/fina_indicator/report_rc/moneyflow_dc/industry 5 类数据，永久保留不设 TTL；`batch_upsert` 单事务写入，5 次 fsync 降为 1 次
2. **`build_asset_context()` 重写为 cache-first**: 先查 DB → miss 调 `refresh_asset_data()` 批量 API 回写 → `realtime_quotes(rt_k)` 获取盘中实时价（不缓存）；JSON 解析从 `serde_json::Value` 无类型访问改为 `DailyBasic`/`FinaIndicator`/`ReportRc` typed 反序列化，编译期检查字段名
3. **`load_prompt_for_round()` 统一占位符替换**: 接收 `&AssetContext` 参数，一次性替换 17 个 `{{placeholder}}`；删除 `run_role_phase` 中 5 个角色的 ~100 行重复 user message 追加块

**委员会 Prompt 注入 (2 项):**
4. **Risk prompt 资产上下文**: 新增完整 `**资产上下文**` 段，包含最新价/昨收/PE/PB/ROE/ROA/营收增速/净利增速/负债率/总市值/流通市值/机构评级，全部从 AssetContext 占位符注入，不再通过 user message 追加
5. **CIO 数据质量警告**: system prompt 末尾追加 `【数据质量警告】` 列出缺失字段，CIO 据此调低置信度

**工具层优化 (2 项):**
6. **`exec_company_info` cache-first**: 先查 `stock_data_cache::load_all_latest_for_symbol`（单次 DB 查询），miss 才调 API；提取 `format_valuation()` 共享函数消除缓存/API 两路径的格式化重复
7. **`exec_moneyflow` cache-first**: 当天缓存直接返回，miss 调 API

**前端优化 (4 项):**
8. **持仓/交易/策略中文名显示**: HoldingsTable/TradeLogTab/StrategyTab/CommitteeAccuracyTab/CommitteeReplayTab/MacroSnapshotCard 统一用 `nameMap` 显示中文名（`h.name || h.symbol`），symbol 降为 monospace 副文本 + title tooltip
9. **InvestStore.nameMap**: `$derived` 计算 `Map<symbol, name>`，合并 holdings + priceMap 双源
10. **盘中交易时段检测**: `isMarketOpen()` 判断 A 股交易时段（9:15-11:30 / 13:00-15:00 CST 工作日），盘外跳过 `rt_k` 调用避免无效 API 请求
11. **`lastRefreshAt` 时间戳**: 记录最近一次成功刷新时间

**Cron 表达式清理 (2 项):**
12. **前端 sanitize**: DreamingConfigPanel + SchedulerTab 保存前 trim + 正则过滤非 cron 字符
13. **后端 sanitize**: `update_cron` + `save_dream_config` trim + 过滤，防止非法字符写入

**Simplify 审查修复 (6 项):**
14. **Bug**: `data_quality` PE=N/A 重复推送 → 删除 post-parse 守卫，只保留 cache-miss 时推送
15. **Efficiency**: `load_all_latest_for_symbol` 双调用 → `mut` 变量复用，只在 refresh 后重读
16. **Simplification**: 7 元素 tuple 含死变量 `_close_from_daily` → 删除，tuple 缩为 5 元素
17. **Reuse**: `serde_json::Value` 无类型解析 → typed `DailyBasic`/`FinaIndicator`/`ReportRc` 反序列化
18. **Reuse**: `exec_company_info` 两路径重复格式化 → `format_valuation()` 共享函数
19. **Efficiency**: `refresh_asset_data` 5 次独立 `upsert_cache` → `batch_upsert` 单事务

**涉及文件:**
- 新增：`src-tauri/src/storage/invest/stock_data_cache.rs`
- 修改：`src-tauri/src/storage/invest/mod.rs`、`src-tauri/src/invest/committee/orchestrator.rs`、`src-tauri/src/invest/committee/roles.rs`、`src-tauri/src/invest/committee/tools.rs`、`src-tauri/src/invest/scheduler/config.rs`、`src/lib/stores/invest-store.svelte.ts`、6 个 invest Svelte 组件

**验证：** cargo check ✅ 0 warning / npm build ✅

---

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
