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

## 实施拆分建议

本 spec 覆盖 7 块,建议在 writing-plans 阶段**拆成两份实施计划**,理由:验证节奏不同(UI 当天可目视验收,选池 KPI 需多交易日)、文件耦合不同(选池四块都改 `scoring.rs`/`report.rs`,需串行)。

- **Plan 1 — UI 组**: A1(双列新闻/舆论)+ A3(01 自适应)+ A4(7 天清理)。三块彼此近乎独立,可并行。
- **Plan 2 — 选池组**: B4(名次切档)→ B1(候选多信号)→ B2(五因子固定权重)→ B5(板块强度因子 + 东财数据源)→ B3(AI 复核)。**B4 最先做**——它最独立、最低风险,拿现有 `SymbolScore.total` 即可切档,且最直接兑现"观察池=最强 20 只"的核心洞察;B1/B2/B5/B3 在已改成名次切档的稳定视图上迭代打分逻辑。B5 是本轮选池优化的核心(取代 regime),含东财数据源接入 + 一对多板块映射表 + 低频刷新 cron,工作量最大。

两份 plan 独立实施、独立发版、独立可回滚。

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
- **窄屏断点**: `@media (max-width: 900px)` 时双列折叠为上下堆叠(左列在上、右列在下),各自仍独立滚动;避免两列被挤成极窄流。

**组件重构** (`src/lib/components/invest/`):
- 将 `EventWatchTab.svelte` 重构为双列容器,建议拆出两个子组件 `NewsFlashColumn.svelte`(左)与 `NewsDigestColumn.svelte`(右),各自持有筛选状态与滚动容器。
- 数据来源:
  - 左列复用 `investStore.fetchEvents()` 结果,前端按 `source==='jinshi_flash'` 过滤。
  - 右列的公告/个股新闻同样从 events 结果按 source 过滤;舆情需要新增读取路径(见下)。
- **舆情数据接入**: `sentiment_items` 表目前无前端读取路径,且 `storage::invest::sentiment` **没有**"取最近 N 条"的通用列举函数(现有只有 `list_sentiment_by_symbol` / `list_sentiment_by_sectors`)。因此需:① 在 `storage/invest/sentiment.rs` 新增 `list_recent_sentiment(conn, limit)`(按 `created_at` 倒序,已有 `idx_sentiment_created` 索引支撑);② 在 `commands/invest.rs` 新增 `get_sentiment_items(limit)` 命令调它;③ 在 `invest-store` 增加 `fetchSentimentItems()` + state。**决策:本轮接入,读现有表,有数据即显示**(表已由 `sentiment_collector` cron 每小时写入);表为空时右列公告+个股新闻照常,舆情类筛选显示空态。
- **雪球 scrapling 抓取不在本轮范围**(见 memory `project_xueqiu-scrapling-waf`),右列消费 `sentiment_items` 现有内容即可。

**i18n**: sub-tab label 键遵循现有模式,新增 `invest_committee_sub_news`(与 `invest_committee_sub_live/replay/...` 一致);另新增内容区 `news.*` 键(列标题、筛选 chip、来源标签、空态)。`en.json` 与 `zh-CN.json` 同步。保留原 `invest.eventWatch.*` 中仍复用的键;移除 `invest_system_sub_events` 引用。

### A3 — 盘前 01 模块(舆论/新闻先验)自适应布局

**目标**: `theme-wall` 从固定 4 列改为按卡片数自适应,消除留白/断行不平衡。

**规则** (`PremarketReportTab.svelte` 的 `.theme-wall`):
- n ≤ 4 → 一行 n 等分(`grid-template-columns: repeat(n, 1fr)`)。
- n = 5 → 第一个占满整行(`grid-column: 1 / -1`)+ 第二行 4 等分(**1+4 布局**)。
- n ≥ 6 → 每行 3 列(`repeat(3, 1fr)`)。
- 带"风险预警"标签的卡保留现有 `grid-column: 1 / -1`(占满整行)逻辑。
- **风险卡与 1+4 的冲突消解**: 若卡片集合中存在风险卡,**优先按风险卡规则排**(风险卡各自占满整行),其余非风险卡按每行 3 列铺;此时忽略 n=5 的 1+4 特殊规则(1+4 只在无风险卡时生效),避免风险卡不在首位时布局失控。
- 实现方式:根据 `commentary.sectors.length` 与"是否含风险卡"动态设置容器的 `grid-template-columns`,5 张(无风险卡)时对第一张单独加占满类。用 CSS `:has()` 或在 Svelte 里按 length 计算 class,择其一(实现阶段定)。

### A4 — 快讯/新闻 7 天清理

**目标**: `events` 与 `sentiment_items` 两表目前无限累积,加定时清理。

**清理策略**(用户确认「保留已触发/高价值」):
- `events`: `DELETE WHERE created_at < now-7d AND triggered = 0 AND severity != 'high'`(保留已触发委员会的、以及高价值的历史)。
- `sentiment_items`: `DELETE WHERE created_at < now-7d`(舆情无触发概念,一刀切)。
- 7 天为硬编码常量,不做可配置(YAGNI)。
- **引用消歧**: committee 引用某条舆情/事件(如 verdict.reasoning 里贴了原文)不阻止清理——reasoning 已冗余抄写原文,原表删除不影响回溯。

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
- **S2 数据源降级**: `moneyflow_dc_market` 当日拉取失败(网络/限流)→ net_amount map 为空 → S2 命中数为 0,静默退化为 S1+S7,并在报告 `sections_status` 标注 `capital_flow: unavailable`(与 memory `project_premarket-followups-v568` 的降级可观测性待办合并)。
- `CANDIDATE_CAP` 维持 200(P1 不调)。
- 放量(S3)、涨停梯队(S5)、龙虎榜(S4)、突破新高(S6)等需新数据端点的信号,列为后续 P2~P5,**本轮不做**。

**收益**: 候选池从"当日涨幅榜"扩展为"涨幅榜 ∪ 主力净流入榜 ∪ 舆情",捕获"温和放量、主力介入但涨幅未爆"的品种,并降低候选池日换血率。

### B2 — 五因子固定权重(不做 regime 动态权重)

**背景决策**: 早期设计曾拟用大盘(000001.SH)regime 动态调权,但经讨论**放弃**(路线乙)。理由:
1. 单一上证综指代表性差(被权重股主导,结构性行情下背离);换成市场广度也解决不了"吸血行情被误判成熊市、权重调反"的问题(广度差 + 集中度高的吸血行情,在只看方向的 regime 眼里与真熊市无异,却需要完全相反的权重)。
2. 真正缺的是**个股所属赛道强不强**这个维度——由新增的 **B5 板块强度因子**从个股级直接解决,比市场级 regime 判断更靠谱,且不引入"多交易日才验证得出、分类准不准存疑"的黑盒。

**改法**:
- 四因子扩为**五因子**(新增 B5 `sector`),采用**固定权重**基线:
  `sentiment=0.25 / capital=0.25 / technical=0.15 / catalyst=0.10 / sector=0.25`(和为 1,实现阶段可微调)。
- `PremarketConfig` 权重字段从 4 个扩到 5 个(新增 `weight_sector`)。
- **保留 `weight_source: "auto" | "manual"`**: `auto` = 上述固定基线;`manual` = 用户在权重面板自定义五个权重。前端权重面板加"自动/手动"开关 + 第五个权重滑块。
- **不做** regime 权重矩阵、EMA 平滑、`weight_state.rs`、crash 旁路——这些随路线乙一并移除(见"明确移除/不做的项")。
- **保留约束**: `technical` 因子内部仍吃**个股** regime(`compute_technical` 现状不变),这是因子分内部逻辑,与权重无关。
- **不做分位阈值**: SABC 改为按名次切分(见 B4),档位不由绝对分阈值决定。

### B3 — AI 终选复核

**目标**: 在纯量化选池后加一层 AI「证伪」复核,剔除有明显利空/纯情绪炒作/基本面不支撑的标的。

**流程**:
1. 量化(B1+B2 打分)选出 **top25 候选**(比最终 20 略多,给 AI 留剔除余量)。
2. 一次**批量**喂给 LLM(单次 CLI 调用,非逐只并发)做证伪:每只输出 `action ∈ {keep, drop}` + 一句 `reason` + `risk_flag ∈ {none, regulatory, sentiment_only, weak_fundamental, other}`。**AI 只能保留或剔除,不能加分/提档/降档**(名次切档下"降档"无实际语义,故不设 downgrade)。
3. 剔除 `drop` 的,剩余按量化总分取 top20 进入最终观察池。

**AiReview 契约(三处一次定死)**:
- **LLM 输出 JSON schema**: 顶层 `{ "decisions": [ {"symbol": "600519.SH", "action": "keep|drop", "reason": "≤30字", "risk_flag": "none|regulatory|sentiment_only|weak_fundamental|other"}, ... ] }`。数组,每只一条,`symbol` 为 ts_code。prompt 约束 `reason` ≤ 30 汉字。
- **Rust 结构**(`scoring.rs`,serde camelCase):
  ```rust
  pub struct AiReview { pub action: String, pub reason: String, pub risk_flag: String }
  // SymbolScore 增: pub ai_review: Option<AiReview>  (serde skip_serializing_if = None)
  ```
- **前端 TS**(`types.ts`): `aiReview?: { action: 'keep' | 'drop'; reason: string; riskFlag: string }`。

**架构约束(硬性)**:
- `SymbolScore.total` 与档位**永远只由量化决定**。AI 结果走独立可选字段 `ai_review: Option<AiReview>`,不回写 `total`。
- **AI 剔除的展示(决策:显示剔除区)**: 04 模块正常显示最终 top20 精英(名次切档 SABC);其下方增加一块**「AI 剔除」折叠区**,列出被 `drop` 的票 + `reason` + `risk_flag` 小徽标。透明、可回溯、AI 误杀时用户能看见并人工判断。`reason` 显示为小字,前端 CSS 截断兜底防溢出。
- **关闭 AI 时**:前端不消费 `ai_review`,不显示剔除区,回退纯量化 top20,干净无副作用。
- **落盘(决策:落盘存档)**: `ai_review` 随 `scores` 写进盘前报告存档 JSON,为以后命中率闭环留历史信号(即使前端某处不展示也落盘)。

**执行路径**:
- 复用 `event_analyzer::cli_complete_with_settings` + `macro_verdict::resolve_settings_path()`,**走现有委员会配置**(`CommitteeTuning.selected_provider + model` + `platform_credentials`),与 `ai_commentary`、`macro_verdict` 一致,**零新配置**。
- 60s 超时(盘前时间敏感,不用默认 180s)。
- 输入组装:每只标的喂 `名称 + 行业 + 四因子分 + 近3日 sentiment 命中(读本地 `sentiment_items`,不发新 API)`。约 ~180 tokens/只,25 只 ~4.5k input。

**开关与降级**:
- `PremarketConfig.enable_ai_review: bool`,**默认 true**。做成**前端设置面板的显式 toggle**(用户可随时开关),非仅配置文件。
- 三重降级,任一触发退回纯量化 top20:
  1. 开关关闭 → 跳过 AI pass。
  2. CLI 超时/错误/JSON 解析失败 → 全部 `ai_review=None`,报告标注"AI 精筛失败(不影响选池)",`sections_status` 标 `ai_review: failed`。
  3. **熔断**: `drop_count >= 13`(top_k=25,即 drop 率 ≥ 52%)或全 drop → 视为异常判断,整体作废(全部 `ai_review=None`),记 warn 日志,`sections_status` 标 `ai_review: circuit_broken`。
- **LLM 输出异常处理**:
  - 返回输入 25 只之外的 ts_code → 忽略该条。
  - 25 只输入但只返回部分 → 未返回的按 `keep`(`ai_review=None`),**不判 drop**。
  - `action` 值非 `keep`/`drop` → 该只按 `keep`(`ai_review=None`)。
  - `risk_flag` 不在枚举内 → 归为 `other`。

### B4 — SABC 观察池:先选池 → 打分排序 → 按名次切档(核心模型重构)

**问题(用户核心洞察)**: 现状是"先按绝对分阈值分档,再每档 `slice(0,3)`"。这会**强制凑档**——市场最强的票若都够 S 档,只取前几只 S,然后被迫去 A/B/C 捞 top20 之外更差的票填满格子。等于"为填满档位格子主动纳入更差股票,还挤掉更好的票",与"观察池=最值得看的一批"的初衷相反。

**新模型(用户确认)**:
1. **先选池**: B1 多信号候选 → 五因子打分(B2 固定权重 + B5 板块因子)→ B3 AI 复核,选出全市场**总分最高的 20 只**——这 20 只是一个精英整体,是"市场上最值得追的 20 只"。
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
- **切档规则的精确表述**: `pool` = AI 剔除后的最终股票数。`pool >= 20` → 取 top20,每档正好 5 只(1-5=S / 6-10=A / 11-15=B / 16-20=C)。`pool < 20` → 按序填 S→A→B→C,每档最多 5 只,末档不满 5(不补更差的票)。`pool < 20` 的**唯一触发路径**是 B3 AI 砍了 6~12 只(K≤5 时 25→20 无损;K≥13 触发熔断整体作废,不会发生),即 drop 数落在 [6,12] 的窄区间。
- **不足 20 只时**(pool<20):如上,末档可能不满 5 只,不凑数。

**已知取舍(可接受)**: 名次切分有"临界感"——第 5 名(S 末)与第 6 名(A 首)分数可能接近却戴不同徽章。这是排名榜固有特性,作为展示标签可接受,远优于老的凑档逻辑。

### B5 — 个股所属板块强度因子(第五因子)

**目标**: 新增一个个股级因子——身处强势赛道(尤其被资金吸血的板块)的题材龙头自动加分浮上来。这从个股层面解决了市场级 regime 在结构性/吸血行情下失灵的问题(取代 B2 的 regime 动态权重,见路线乙决策)。

**为什么解吸血行情**: 大多数板块普跌、资金吸血到半导体+通信时,身处这些强板块的票 `sector_strength` 拿高分而浮上来;那些"个股自身没跌但所处板块也无资金"的防御性杂毛板块分低被压下。这是市场级 regime 做不到的区分。

**数据源统一到东财(根治板块名不匹配)**:
- **问题**: 现状个股行业名来自 Tushare(`stock_basic.industry`,申万风格单一字符串),板块强度来自东财(`stock_sector_fund_flow_rank`),两套编码 join 会有一批匹配不上。
- **改法**: 个股→板块映射也改从**东财**取,与板块强度同源,直接 join,无需名称映射层。
  - 行业板块成分:`stock_board_industry_cons_em`。
  - 概念板块成分:`stock_board_concept_cons_em`(支持一只股票属**多个**概念,如"锂电池"/"新能源车"/"储能")。
  - 需在 `python-runtime/scripts/providers/akshare_sector.py`(或新建 provider)新增这两个端点,并在 Rust 侧封装。
- **一对多映射 + 低频缓存**: 新建 SQLite 表 `stock_board_map(ts_code, board_name, board_type)`(一只股票多行,`board_type ∈ {industry, concept}`),仿现有 `stock_industry` 模式。板块归属变化慢,**低频刷新**——新增每周一次 cron(或并入现有 `refresh_stock_basic` 手动触发),平时打分读缓存,**零盘前开销**。新股上市后下次刷新自动纳入。

**因子计算**:
- 板块强度分:`fetch_sector_flow()`(已有,一次调用拿全部东财板块当日涨跌幅 + 主力净流入)→ 合成 0-100 分/板块。build_cache 里加这一次调用(成本低)。
- 个股 `sector_strength`:查 `stock_board_map` 拿该股所属所有板块,**取其中强度分最高的那个**(用户确认:取最强板块,最贴合吸血行情)。
- **兜底**: 无板块归属(次新股/映射缺失)→ 给中性分 **50**,不惩罚。实现时先打印一次映射覆盖率作为验收(目标 >80%)。

**并入打分**:
- `FactorScores`(`scoring.rs`)新增字段 `sector_strength: f64`。
- 五因子加权(见 B2 固定权重,sector 占 0.25)。
- 前端 04 模块因子标签增一个"板块强"chip(`sector_strength >= 60` 时显示)。

**V1 / V2 分界**:
- **V1(本轮)**: 纯**当日**板块强度(涨跌幅 + 主力净流入)。足以解"当天谁在被吸血"。
- **V2(后续,不做)**: 板块 **N 日动量/加速度**(识别板块刚启动加速),需新增 akshare `stock_board_industry_hist_em` 或 tushare `sw_daily` 板块历史端点,中等成本。

---

## 明确移除/不做的项

- **A2(SABC 显示名字 + tooltip 代码)**: 当前已是"名字(主)+ 后面小灰代码",符合用户预期,**无需改动**,从范围移除。若后续觉得代码字太大再单独微调 `.stk-code` 样式。
- **A5(每档最多 5 只作为配额)**: 被 B4 的名次切分取代(每档正好 5 只是名次切分的结果,不是"最多 5 的配额上限"),概念作废。
- **AI `downgrade` action**: 名次切档下"降档"无实际语义(不改分、不改档),留着是装饰。AI action 简化为 `keep` / `drop` 两态。
- **regime 动态权重整套(路线乙决策)**: 大盘/广度 regime 权重矩阵、EMA 平滑、`weight_state.rs`、crash 旁路——全部**不做**。市况维度改由 B5 板块强度因子从个股级表达。B2 退化为五因子固定权重。理由见 B2 段。
- **板块 N 日动量/加速度(B5 V2)**: 需新增 akshare/tushare 板块历史端点,本轮只做当日板块强度。
- **命中率反馈闭环 / 因子权重自适应 / 逻辑回归模型**: 需先建 `premarket_selection_history` 表并攒 1~1.5 个月数据,本轮不做。
- **候选池 P2~P5 信号(放量/涨停梯队/龙虎/新高)**: 需新数据端点,本轮不做。
- **雪球 scrapling 抓取落地**: 本轮右列只消费 `sentiment_items` 现有内容。

---

## 涉及文件清单

**前端**:
- `src/routes/invest/+page.svelte` — sub-tab 迁移(events → committee/news)。
- `src/lib/components/invest/EventWatchTab.svelte` → 重构为双列(+ 新增 `NewsFlashColumn.svelte`、`NewsDigestColumn.svelte`);双列在 ≤900px 折叠上下堆叠。
- `src/lib/components/invest/PremarketReportTab.svelte` — 01 自适应布局 + 风险卡冲突消解(A3)、04 名次切档展示(B4)、AI 复核字段展示 + 「AI 剔除」折叠区 + 设置面板 toggle(B3)。
- `src/lib/stores/invest-store.svelte.ts` — 新增 `fetchSentimentItems()` + state。
- `src/lib/types.ts` — `SymbolScore` 加 `aiReview?`;新增 sentiment item 类型。

**后端**:
- `src-tauri/src/commands/invest.rs` — 新增 `get_sentiment_items`;新增 `refresh_stock_board_map`(或并入现有 `refresh_stock_basic`)(B5)。
- `src-tauri/src/storage/invest/sentiment.rs` — 新增 `list_recent_sentiment(conn, limit)`(A1)。
- `src-tauri/src/storage/invest/news_cleanup.rs` — 新建(A4 清理 + 单测)。
- `src-tauri/src/storage/invest/stock_board_map.rs` — 新建(B5 一对多板块映射表 `stock_board_map` + CRUD)。
- `src-tauri/src/invest/scheduler/mod.rs` + `runner.rs` — 新增清理 cron(A4)+ 板块映射低频刷新 cron(B5,每周一次);均需手动加 dispatch match 分支。
- `src-tauri/src/invest/premarket/cache_builder.rs` — `select_candidates` 多信号化 + S2 降级(B1);build_cache 里加 `fetch_sector_flow()` + 板块强度 map + 个股取最强板块分(B5)。
- `src-tauri/src/invest/premarket/scoring.rs` — `PremarketConfig` 五因子权重(加 `weight_sector`)+ `weight_source` + `enable_ai_review`;`FactorScores` 加 `sector_strength`;`AiReview` 结构 + `SymbolScore.ai_review`;废弃绝对阈值切档,改名次切档(B2/B3/B4/B5)。
- `src-tauri/src/invest/premarket/report.rs` — 五因子加权(B2/B5);top25 选取 + AI 复核 pass + 熔断降级 + `sections_status` 标注(B3);名次切档(B4);`ai_review` 落盘存档。
- `src-tauri/python-runtime/scripts/providers/akshare_sector.py` — 新增 `stock_board_industry_cons_em` / `stock_board_concept_cons_em` 端点(B5)。
- `src-tauri/src/messages/en.json` + `zh-CN.json` — i18n(含 `invest_committee_sub_news`、板块强 chip)。

---

## 验证

- 前端: `npm run build`、`npm run check`、`npm run i18n:check`。
- 后端: `cargo check`、`cargo clippy -- -D warnings`;`news_cleanup` 与名次切档、AI JSON 解析的单测(本机裸 `cargo test` 有已知 0xc0000139 问题,用 `npm run rust:test` 或 `cargo check` 验证)。
- AI 复核降级路径逐条单测(开发期用 mock 即可,不必等实盘):关开关、CLI 失败、`drop_count>=13` 熔断、LLM 幻觉/缺失/非法值。
- B4 名次切档单测:`pool>=20` 每档 5 只、`pool<20` 末档不满、组内降序。
- **plan 完成态定义**: 代码 + 单测 + `npm run build`/`check`/`i18n:check` + `cargo check`/`clippy` 全过即算 done。**选池效果 KPI(观察池日换血率、S/A 档次日命中率、追高股 pct_chg>7% 占比)是发版后跟踪项,不阻塞 plan close**(这些指标需多交易日样本,且当前无历史基线)。
