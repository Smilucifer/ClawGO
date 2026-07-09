# 委员会「新闻/舆论」+ 盘前观察优化 设计文档

**日期**: 2026-07-10
**范围**: 投资委员会 (`/invest`) 的两块改造——新闻/舆论双列视图,以及盘前观察选池/展示优化。
**不含**: 命中率反馈闭环(需先攒 1~1.5 个月历史数据,另开一轮)。

---

## 背景

用户提出的改造分两类:

- **UI/功能改动**:事件模块迁移改造(双列新闻/舆论)、盘前 01 模块自适应布局、快讯/新闻 7 天清理。
- **盘前选池优化**:候选池多信号化、因子权重随市况动态化、AI 终选复核,以及 SABC 观察池的选取模型重构。

三个后台研究(命中率闭环、候选池+权重、AI 复核)已确认:选池的历史反馈链目前是断的(盘前池从未被登记追踪),因此任何"用历史命中率反馈进选池"的方案都需先补数据存档再攒样本,本轮不做。本轮只做**不依赖历史数据、立刻见效**的三项选池优化。

---

## A. UI / 功能改动

### A1 — 委员会「新闻/舆论」双列视图

**目标**: 把"事件"从 `系统` 子标签迁到 `委员会` 子标签,改名"新闻/舆论",做成左右双列、各自独立滚动。

**导航变更** (`src/routes/invest/+page.svelte`):
- 从 `SystemSubTab` 移除 `events`;从 `systemSubTabs` 数组移除该项。
- 在 `CommitteeSubTab` 增加 `news`;在 `committeeSubTabs` 数组加入(位置置于 `archive` 之后、`premarket` 之前)。
- 委员会 sub-tab 渲染分支增加 `{:else if committeeSubTab === 'news'}`。
- 迁移后触发委员会不再需要跨主 tab 跳转(本就在委员会内),`onNavigateToCommittee` 回调简化为切到 `committeeSubTab='live'`。

**布局** (定稿于 v2 mockup):
- **左列 = 金十快讯**: `events` 表 `source='jinshi_flash'` 的紧凑流式列表。每行:严重度徽章(高/中/低/待判)+ 正文 + 立场(偏多红 `--up` / 偏空绿 `--down`,红涨绿跌)+ 时间 + 板块 chip + 「触发委员会」按钮(仅 `severity=high && !triggered`)。顶部一排筛选 chip:全部 / 仅高 / 偏多 / 偏空。
- **右列 = 新闻/舆论**: 其他来源的紧凑流式列表,与左列同密度。来源包括公告(`events` 表 `source='tushare_anns_d'`)、个股新闻(`events` 表 `source LIKE 'akshare:%'`)、舆情(`sentiment_items` 表)。每条:小来源标签(东财公告=金 / 个股=灰 / 雪球=青蓝)+ 标题 + 一行灰色摘要 + 立场 + 标的 + 时间。顶部筛选 chip:全部 / 公告 / 个股新闻 / 雪球舆情。
- 右列**不做垂直分组堆叠**(避免数据一多难以滚动定位),来源区分靠顶部筛选 chip + 每条的小来源标签。
- 两列各自 `overflow-y-auto`,独立滚动。顶部保留现有状态栏(总数/高/待触发/最新)+「立即扫描」按钮,横跨两列。

**组件重构** (`src/lib/components/invest/`):
- 将 `EventWatchTab.svelte` 重构为双列容器,建议拆出两个子组件 `NewsFlashColumn.svelte`(左)与 `NewsDigestColumn.svelte`(右),各自持有筛选状态与滚动容器。
- 数据来源:
  - 左列复用 `investStore.fetchEvents()` 结果,前端按 `source==='jinshi_flash'` 过滤。
  - 右列的公告/个股新闻同样从 events 结果按 source 过滤;舆情需要新增读取路径(见下)。
- **舆情数据接入**: `sentiment_items` 表目前无前端读取命令。需在 `commands/invest.rs` 新增 `get_sentiment_items(limit)` 命令(调 `storage::invest::sentiment` 已有的列举函数),并在 `invest-store` 增加 `fetchSentimentItems()` + state。若本轮暂不接舆情真实数据,右列可先只展示公告+个股新闻,舆情列为空态占位——**决策:本轮接入 `get_sentiment_items` 读现有表,有数据即显示**(表已由 `sentiment_collector` cron 每小时写入)。
- **雪球 scrapling 抓取不在本轮范围**(见 memory `project_xueqiu-scrapling-waf`),右列消费 `sentiment_items` 现有内容即可。

**i18n**: 新增 `news.*` 键(列标题、筛选 chip、来源标签、空态),`en.json` 与 `zh-CN.json` 同步。保留原 `invest.eventWatch.*` 中仍复用的键;移除 `invest_system_sub_events`(或保留但不再引用)。

### A3 — 盘前 01 模块(舆论/新闻先验)自适应布局

**目标**: `theme-wall` 从固定 4 列改为按卡片数自适应,消除留白/断行不平衡。

**规则** (`PremarketReportTab.svelte` 的 `.theme-wall`):
- n ≤ 4 → 一行 n 等分(`grid-template-columns: repeat(n, 1fr)`)。
- n = 5 → 第一个占满整行(`grid-column: 1 / -1`)+ 第二行 4 等分(**1+4 布局**)。
- n ≥ 6 → 每行 3 列(`repeat(3, 1fr)`)。
- 带"风险预警"标签的卡保留现有 `grid-column: 1 / -1`(占满整行)逻辑。
- 实现方式:根据 `commentary.sectors.length` 动态设置容器的 `grid-template-columns`,5 张时对第一张单独加占满类。用 CSS `:has()` 或在 Svelte 里按 length 计算 class,择其一(实现阶段定)。

### A4 — 快讯/新闻 7 天清理

**目标**: `events` 与 `sentiment_items` 两表目前无限累积,加定时清理。

**清理策略**(用户确认「保留已触发/高价值」):
- `events`: `DELETE WHERE created_at < now-7d AND triggered = 0 AND severity != 'high'`(保留已触发委员会的、以及高价值的历史)。
- `sentiment_items`: `DELETE WHERE created_at < now-7d`(舆情无触发概念,一刀切)。
- 7 天为硬编码常量,不做可配置(YAGNI)。

**实现**:
- 新建 `src-tauri/src/storage/invest/news_cleanup.rs`,内含两个删除函数 + 单测(用 in-memory / 临时 DB 验证:超期未触发被删、已触发/high 保留、7 天内保留)。
- 在 `invest/scheduler/mod.rs` 的 `default_jobs` 增加第 14 个 cron(如 `0 30 3 * * *`,每日凌晨盘后),在 `runner.rs` 增加 dispatch 分支调用清理函数。
- 交易日限制:否(清理不必限交易日)。

---

## B. 盘前选池优化

### B1 — 候选池多信号化(P1 范围)

**问题**: `cache_builder.rs::select_candidates` 现状——舆情命中股全保,剩余名额纯按当日 `pct_chg` 降序补齐到 `CANDIDATE_CAP=200`。纯涨幅补池 = 系统性追高偏好。

**改法(P1,零新接口)**:
- `select_candidates` 改为多信号并联:
  - **S1 舆情命中**(全保,现有)。
  - **S2 主力净流入榜 Top60**:`moneyflow_dc_market` 的 `net_amount` map 已在 `build_cache` 内存中,排序取 Top60 union 进候选。**零新接口**。
  - **S7 涨幅兜底**:`daily_market` 的 `pct_chg` 降序,仅在候选不足 `CANDIDATE_CAP` 时补齐(从"主要补池方式"降级为"兜底")。
- 合并去重:按"命中信号数"降序,同命中数内按 `net_amount` 降序。
- `CANDIDATE_CAP` 维持 200(P1 不调)。
- 放量(S3)、涨停梯队(S5)、龙虎榜(S4)、突破新高(S6)等需新数据端点的信号,列为后续 P2~P5,**本轮不做**。

**收益**: 候选池从"当日涨幅榜"扩展为"涨幅榜 ∪ 主力净流入榜 ∪ 舆情",捕获"温和放量、主力介入但涨幅未爆"的品种,并降低候选池日换血率。

### B2 — 因子权重随市况(regime)动态化

**问题**: `scoring.rs` 四因子权重固定 `sentiment=0.30 / capital=0.30 / technical=0.25 / catalyst=0.15`,不随市况变化。

**改法**:
- 用**大盘(000001.SH)** 跑一次 `regime` 判断(5 态:uptrend / downtrend / range_bound / crash / unknown),查权重矩阵:

  | regime | sentiment | capital | technical | catalyst |
  |---|---|---|---|---|
  | uptrend | 0.20 | 0.35 | 0.30 | 0.15 |
  | range_bound | 0.30 | 0.25 | 0.20 | 0.25 |
  | downtrend | 0.15 | 0.35 | 0.35 | 0.15 |
  | crash | 0.10 | 0.30 | 0.50 | 0.10 |
  | unknown | 0.30 | 0.30 | 0.25 | 0.15(=当前默认) |

  (以上为基线值,实现阶段可微调;和恒为 1。)
- **权重 EMA 平滑**(α=0.3):regime 切换时权重不突变,与上一日 EMA 混合后归一,避免观察池天天大换血。新建 `src-tauri/src/invest/premarket/weight_state.rs` 持久化 EMA 状态到 `~/.claw-go/invest/premarket_weight_state.json`,节假日不推进。
- **关键约束**: `technical` 因子内部已经吃了**个股** regime(`compute_technical`),权重矩阵用的是**大盘** regime,两者分开,不二次放大。
- **保留手动权重**: `PremarketConfig` 增加 `weight_source: "auto" | "manual"`。`manual` 时用用户在权重面板设的固定值(现有行为);`auto` 时用 regime 矩阵 + EMA。前端权重面板加"自动/手动"开关。
- **不做分位阈值**:因为 SABC 改为按名次切分(见 B4),档位不再由绝对分阈值决定,分位阈值无意义。

### B3 — AI 终选复核

**目标**: 在纯量化选池后加一层 AI「证伪」复核,剔除有明显利空/纯情绪炒作/基本面不支撑的标的。

**流程**:
1. 量化(B1+B2 打分)选出 **top25 候选**(比最终 20 略多,给 AI 留剔除余量)。
2. 一次**批量**喂给 LLM(单次 CLI 调用,非逐只并发)做证伪:每只输出 `action ∈ {keep, downgrade, drop}` + 一句 `reason` + `risk_flag ∈ {none, regulatory, sentiment_only, weak_fundamental, other}`。**AI 只能保留/降档/剔除,不能加分或提档**。
3. 剔除 `drop` 的,剩余按量化总分取 top20 进入最终观察池。

**架构约束(硬性)**:
- `SymbolScore.total` 与档位**永远只由量化决定**。AI 结果走独立可选字段 `ai_review: Option<AiReview>`,不回写 `total`。
- 前端展示层消费 `ai_review`:`drop` 的从池中剔除(或折叠到"AI 剔除"区),`downgrade` 仅作展示提示,`reason` 显示为卡片下一行小字,`risk_flag` 显示小徽标。
- **关闭 AI 时**:前端不消费 `ai_review`,回退纯量化,干净无副作用。

**执行路径**:
- 复用 `event_analyzer::cli_complete_with_settings` + `macro_verdict::resolve_settings_path()`,**走现有委员会配置**(`CommitteeTuning.selected_provider + model` + `platform_credentials`),与 `ai_commentary`、`macro_verdict` 一致,**零新配置**。
- 60s 超时(盘前时间敏感,不用默认 180s)。
- 输入组装:每只标的喂 `名称 + 行业 + 四因子分 + 近3日 sentiment 命中(读本地 `sentiment_items`,不发新 API)`。约 ~180 tokens/只,25 只 ~4.5k input。

**开关与降级**:
- `PremarketConfig.enable_ai_review: bool`,**默认 true**。做成**前端设置面板的显式 toggle**(用户可随时开关),非仅配置文件。
- 三重降级,任一触发退回纯量化 top20:
  1. 开关关闭 → 跳过 AI pass。
  2. CLI 超时/错误/JSON 解析失败 → 全部 `ai_review=None`,报告标注"AI 精筛失败(不影响选池)"。
  3. **熔断**: AI `drop` 率 > 50%(如 25 只砍 >12 只)或全 drop → 视为异常,整体作废,记 warn 日志。
- 单只在 JSON 中缺失或 action 非法 → 该只 `ai_review=None`,其余照用。

### B4 — SABC 观察池:先选池 → 打分排序 → 按名次切档(核心模型重构)

**问题(用户核心洞察)**: 现状是"先按绝对分阈值分档,再每档 `slice(0,3)`"。这会**强制凑档**——市场最强的票若都够 S 档,只取前几只 S,然后被迫去 A/B/C 捞 top20 之外更差的票填满格子。等于"为填满档位格子主动纳入更差股票,还挤掉更好的票",与"观察池=最值得看的一批"的初衷相反。

**新模型(用户确认)**:
1. **先选池**: B1+B2+B3 选出全市场**总分最高的 20 只**——这 20 只是一个精英整体,是"市场上最值得追的 20 只"。
2. **打分排序**: 这 20 只按总分从高到低排名。
3. **按名次切档**(不是按绝对分阈值):
   - 排名 1-5 → **S**
   - 排名 6-10 → **A**
   - 排名 11-15 → **B**
   - 排名 16-20 → **C**
   - 每档固定 5 只。
4. **SABC 语义变为"在这 20 只精英内的相对名次标签"**,不再是"绝对质量门槛 + 选取配额"。C 档不再是"全市场捞的差票",而是"这 20 只精英里相对靠后的 5 只",依然是全市场前 20。

**这解决了什么**: 永远不会伸手去 top20 之外捞更差的票。老逻辑的"凑档"矛盾彻底消除。

**连带变更**:
- **废弃绝对阈值** `threshold_s/a/b/c`:改为纯名次切分。
- **前端** `PremarketReportTab.svelte` 04 模块:保留 S/A/B/C 分组的 2×2 视觉,但分组来源改为"按名次切的 5 只",不再是"按绝对分阈值分桶 + slice(0,3)"。组内按分降序。
- **展示上限**: 每档正好 5 只(总 20),取代原来的每档 3 只(总 12)。
- **不足 20 只时**(候选/AI 剔除后不满 20):按实际数量从高到低填 S→A→B→C,末档可能不满 5 只。

**已知取舍(可接受)**: 名次切分有"临界感"——第 5 名(S 末)与第 6 名(A 首)分数可能接近却戴不同徽章。这是排名榜固有特性,作为展示标签可接受,远优于老的凑档逻辑。

---

## 明确移除/不做的项

- **A2(SABC 显示名字 + tooltip 代码)**: 当前已是"名字(主)+ 后面小灰代码",符合用户预期,**无需改动**,从范围移除。若后续觉得代码字太大再单独微调 `.stk-code` 样式。
- **A5(每档最多 5 只作为配额)**: 被 B4 的名次切分取代(每档正好 5 只是名次切分的结果,不是"最多 5 的配额上限"),概念作废。
- **命中率反馈闭环 / 因子权重自适应 / 逻辑回归模型**: 需先建 `premarket_selection_history` 表并攒 1~1.5 个月数据,本轮不做。
- **候选池 P2~P5 信号(放量/涨停梯队/龙虎/新高)**: 需新数据端点,本轮不做。
- **雪球 scrapling 抓取落地**: 本轮右列只消费 `sentiment_items` 现有内容。

---

## 涉及文件清单

**前端**:
- `src/routes/invest/+page.svelte` — sub-tab 迁移(events → committee/news)。
- `src/lib/components/invest/EventWatchTab.svelte` → 重构为双列(+ 新增 `NewsFlashColumn.svelte`、`NewsDigestColumn.svelte`)。
- `src/lib/components/invest/PremarketReportTab.svelte` — 01 自适应布局(A3)、04 名次切档展示(B4)、AI 复核字段展示 + 设置面板 toggle(B3)。
- `src/lib/stores/invest-store.svelte.ts` — 新增 `fetchSentimentItems()` + state。
- `src/lib/types.ts` — `SymbolScore` 加 `aiReview?`;新增 sentiment item 类型。

**后端**:
- `src-tauri/src/commands/invest.rs` — 新增 `get_sentiment_items`。
- `src-tauri/src/storage/invest/news_cleanup.rs` — 新建(A4 清理 + 单测)。
- `src-tauri/src/invest/scheduler/mod.rs` + `runner.rs` — 新增清理 cron(A4)。
- `src-tauri/src/invest/premarket/cache_builder.rs` — `select_candidates` 多信号化(B1)。
- `src-tauri/src/invest/premarket/scoring.rs` — `PremarketConfig` 加 `weight_source` / `enable_ai_review`;`AiReview` 结构 + `SymbolScore.ai_review`;废弃绝对阈值切档,改名次切档(B2/B3/B4)。
- `src-tauri/src/invest/premarket/weight_state.rs` — 新建(B2 EMA 状态)。
- `src-tauri/src/invest/premarket/report.rs` — 大盘 regime 取 + 权重 resolve(B2);top25 选取 + AI 复核 pass + 熔断降级(B3);名次切档(B4)。
- `src-tauri/src/messages/en.json` + `zh-CN.json` — i18n。

---

## 验证

- 前端: `npm run build`、`npm run check`、`npm run i18n:check`。
- 后端: `cargo check`、`cargo clippy -- -D warnings`;`news_cleanup` 与名次切档、AI JSON 解析的单测(本机裸 `cargo test` 有已知 0xc0000139 问题,用 `npm run rust:test` 或 `cargo check` 验证)。
- 选池效果观察指标(上线后跟踪): 观察池日换血率、S/A 档次日命中率、追高股(pct_chg>7%)占比。
- AI 复核降级路径逐条验证:关开关、CLI 失败、drop 率熔断。
