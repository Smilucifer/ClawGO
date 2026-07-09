# 盘前观察报告改造设计

日期：2026-07-09
状态：设计已批准，待写实现计划

## 背景

盘前观察报告(`PremarketReportTab` + `report.rs`)当前存在多处问题：入口埋在系统页深处、导出按钮点了没反应、切 tab 丢生成状态、无耗时反馈、01 段布局窄、SABC 观察池只看本地持仓/自选而非全市场。同时 AI 点评未沿用委员会 provider、外部舆情只在生成时临时抓 40 条快照，海外指标依赖将被弃用的 Yahoo Finance。

本设计一次性覆盖 9 项改造。核心架构转变：**从"生成时临时拉数据"改为"盘后定时批量缓存 → 生成时只读缓存"**。

## 目标与非目标

**目标**：9 项改造(见下)全部落地，生成任务秒级完成，观察池覆盖全市场。

**非目标**：不改 SABC 四因子打分公式本身；不改委员会/宏观判断链路；不引入同花顺数据源。

## 改造项总览

| # | 项目 | 层 |
|---|------|-----|
| 1 | 入口迁移：reports 子 tab → committee/premarket | 前端 |
| 2 | 导出 PNG/PDF 修复(Tauri save) | 前端+后端 |
| 3 | 生成状态提到 store，切 tab 不丢 | 前端 |
| 4 | 显示生成耗时 | 前端 |
| 5 | 01 段加宽横排 + 画布放宽 | 前端 |
| 6 | SABC 观察池改全市场 + 盘后缓存架构 | 后端+Python |
| 7 | 盘前 AI 点评沿用委员会 provider | 后端 |
| 8 | 外部舆情定时采集(1h)+串联归一化打标 | 后端 |
| 9 | 删 Yahoo/yfinance + 死源清理 + 健康检查更新 | 后端+Python |

## 第 6 条 — SABC 观察池改全市场 + 盘后缓存架构(核心)

### 关键转变
- 观察池不再来自本地 Hold+Watch，改为**全市场**候选池。
- 数据不再在生成时临时拉，而是**盘后定时批量缓存到本地表**，生成任务只读缓存 → 秒级、无限流、用户零等待。

### A. 新增盘后缓存 job `premarket_cache`
cron 约 `盘后 16:30 工作日`(收盘后、次日盘前之间)。

1. **批量拉全市场当日**：扩展 tushare 封装支持按 `trade_date` 一次拉全市场(现仅按 `ts_code`)——
   - `daily`(全市场涨跌幅 `pct_chg` / 成交额 `amount`)
   - `moneyflow_dc`(全市场主力净流入)
   - ✅ **已实证无截断**(见"待验证假设 2")：单次按 `trade_date` 返回全市场 5500~6000 行,不需分页。走第三方代理(`TushareClient::with_token_and_proxy`)。
   - ⚠️ **目标交易日不靠代理 `trade_cal`**(其表滞后)：用"查 `daily(目标日)` 返回 0 行即非交易日/数据未出"判定,或本地交易日历。
2. **粗筛候选池(≤200)**：
   - 全市场按 `pct_chg` **降序** Top，与"近 3 日 sentiment 库命中股票代码集合"取**并集**。
   - 超 200 时：**舆情命中股优先全保留**，剩余名额用涨幅降序补齐到 200。
   - MiniQMT 全 A 快照在线时可作涨幅粗筛的加速源；离线降级 tushare `daily`。
3. **候选深度因子**：
   - 技术因子需 60 根 K 线，仅对入围候选逐只拉 K 线，用 `buffer_unordered` 限速慢拉(盘后时间充足)。
   - ⚠️ **capital 因子需重写**：现 `compute_capital` 是逐 symbol 打 tushare；本条已批量拉全市场 `moneyflow_dc`，故 capital 改为"从批量结果查表"，不复用逐只网络逻辑。sentiment/catalyst(读本地 sentiment 库)、technical(逐只 K 线)可复用现有 `compute_*`。
4. **落地缓存表** `premarket_factor_cache`(SQLite，`storage/invest/`，需加 schema migration)：
   `{trade_date, symbol, name, change_pct, amount, sentiment_score, capital_score, technical_score, catalyst_score, missing_factors, cached_at}`，主键 `(trade_date, symbol)`。
   - **`trade_date` = 数据所属的已收盘交易日**(即缓存 job 运行当日的 `get_invest_date()`，如周五盘后=周五)。

### B. 生成 job `premarket_report` 改为纯读缓存

> **🔴 缓存 key 日期规则(避免错位)**：`get_invest_date()` 规则是"05:00 前算昨天、之后算今天"。盘后 16:30 缓存 key=周五；盘前 09:00 生成时 `get_invest_date()`=周一,**两者对不上**。
> **解法**：生成时**不按今天日期查**，而是读缓存表中**最新一天** `MAX(trade_date)` 的整批行(即最近一个已收盘交易日的池子),并加**新鲜度守卫**(最新缓存日必须在 N 个自然日内,如 ≤4 天,否则视为缓存缺失走兜底)。这样无需交易日历算"下一交易日",跨周末/假期也正确。

- `report.rs` 的 `collect_pool()` → 新 `collect_pool_from_cache()`：读 `premarket_factor_cache` 最新 `trade_date` 的行 → 直接 `score()` 组装 SABC。无网络调用。
- **缓存缺失/过期兜底**(盘后 job 没跑/失败/超新鲜度):现场触发一次 A 的粗筛+慢拉(耗时较长),报告标注"实时拉取(缓存缺失)"。粗筛+打分逻辑须**在 cache job 与兜底间共享**(抽公共函数),避免两份实现漂移。
- **名称解析**：daily 无中文名时用 `stock_industry` 表批量查，回退代码。

### C. 数据源
tushare 为主(批量友好)；MiniQMT 全 A 快照为涨幅粗筛的可选加速源，在线用、离线降级 tushare `daily`。不强制 MiniQMT。

## 第 7 条 — 盘前 AI 点评沿用委员会 provider

现状：`report.rs::ai_commentary` → `event_analyzer::cli_complete` → `run_role(..., settings_path=None)`，走默认 `~/.claude` provider，未沿用委员会设置。

改动：
- 新增 `cli_complete_with_settings(system, user, settings_path)`(或给 `cli_complete` 加可选参数)。
- `ai_commentary` 调用前用 `macro_verdict::resolve_settings_path()` 同款逻辑(读 `committee_tuning.selected_provider + model` → `write_committee_settings_json` 生成 `--settings` 路径)拿到 path 并传入。
- 委员会未配置 provider → path 为 None，回退默认。与宏观判断链路保持一致。

## 第 8 条 — 外部舆情定时采集 + 串联归一化打标

现状：`sentiment_items` 仅在生成时 `collect_all_sentiment(None, 20)` 临时抓每源 20 条；01 段 AI 只吃 `list_recent_sentiment(近1天, 40)` 快照，故每方向仅十几/几条。去重已现成(`make_sentiment_id = SHA256(provider|url|title)` + `INSERT OR IGNORE`)，归一化打标(`analyze_pending(Sentiment)`)已存在但只在生成时跑。

改动：
- **新增 scheduler job `sentiment_collector`**，cron `每 1 小时`(如 `0 0 8-22 * * *` 量级)，`requires_trading_day = false`(周末/假期也有影响下一交易日的新闻，需持续采)：
  1. 调 `collect_all_sentiment` 抓四源(雪球/东财/自媒体/…) → 去重存 `sentiment_items`，累积一整天。
  2. **同 job 内串联**执行 `analyze_pending(AnalyzeTable::Sentiment)` 归一化 + 打行业/symbol 标签(全市场口径，已有逻辑)。**不单独定时**。
- **生成时 01 段读当天累积、但设上限**：现 `build_news_block_for_ai` / `list_recent_sentiment` 取近1天40条。改为取"当天已归一化 sentiment"，但**加上限(如按 severity + 时间取 Top 100-150)**，避免一整天几百上千条撑爆 AI prompt(超 context/变慢/费用)。上限值实现时按实际条数调。

## 第 9 条 — 删 Yahoo/yfinance + 死源清理 + 健康检查更新

### 现状盘点
- Yahoo 唯一实际用途：`macro_refresh::fetch_international` 拉 6 个海外指标(VIX / 美10Y国债 TNX / 美元指数 DXY / 国际金价 / 国际油价 / USD/CNY)进 `macro_cache`。
- `yahoo.news` provider 方法未被任何调用(死代码)。
- Tushare `major_news`(新闻)只在健康检查探针出现，实际新闻走 akshare + 金十(死代码)。

### 覆盖已验证(2026-07-09)
6 指标全部有新鲜源(akshare 4 + 东财 2，见"待验证假设 1")。实现时先跑通新源写入 `macro_cache` 并对拍值合理，再删 Yahoo 代码路径。

### 改动
1. **海外 6 指标改 akshare + 东财**(已验证覆盖，见"待验证假设 1")：
   - **akshare 4 个**(VIX/金/油/USDCNY)：`providers/akshare_market.py` 新增函数(`futures_foreign_hist("VX"/"GC"/"CL")` + `fx_spot_quote`)。
   - **东财直连 2 个**(DXY `100.UDI`、美10Y `171.US10Y`)：走已有 `providers/eastmoney.py`，`push2` stock/get 抓取，`f43/10^f59` 解码。
   - Rust `international.rs` 新增 `fetch_overseas_indicators`(内部按指标分派 akshare/东财)。
   - `macro_refresh::fetch_international` 改调之。
2. **删 Yahoo**：删 `providers/yahoo.py`、`fetch_yahoo_quote`/`fetch_yahoo_history`、`YahooQuote`/`YahooBar` 类型、`INTERNATIONAL_SYMBOLS` 的 Yahoo 符号；注册表 `Category::Overseas` 源链 `Yahoo` → `Akshare`；删 `SourceId::Yahoo`。
3. **删 Tushare 死新闻**：删 client `major_news` 方法 + 健康检查"Tushare 新闻"探针。
4. **删 yfinance 包**：从 `python-runtime` 移除 yfinance 依赖；`PythonStatus.yfinance_version` 字段去掉或改名，前端 `SystemDatasourceTab` / python status 同步。
5. **更新健康检查** `get_datasource_health`：Yahoo 两条探针 → "AkShare 海外指标"探针；"Python 运行时"注释里 yfinance 依赖描述改掉。

## 第 1 条 — 入口迁移

`PremarketReportTab` 从 `system → reports` 子 tab 移到 `committee` 子 tab，排在 `accuracy`(命中率)之后。

改动(`src/routes/invest/+page.svelte`)：
- `CommitteeSubTab` 类型加 `'premarket'`；`committeeSubTabs` 数组在 accuracy 后加一项。
- committee 渲染块加 `{:else if committeeSubTab === 'premarket'}<PremarketReportTab />`。
- 从 `SystemSubTab`、`systemSubTabs`、system 渲染块移除 `reports` 项。
- i18n 加 `invest_committee_sub_premarket`(zh + en)。

## 第 2 条 — 导出 PNG/PDF 修复

现状：`link.click()` + dataURL 在 Tauri webview 被拦截，点击无反应。

改动(`PremarketReportTab.svelte` + 后端)：
- html2canvas → canvas → `toDataURL` 取 base64。
- `@tauri-apps/plugin-dialog` 的 `save()` 选路径(过滤 png/pdf)。
- 新后端命令 `write_binary_export(path, base64_data)`(仿 `write_html_export`，白名单 `.png`/`.pdf`，base64 解码写文件)，注册进 `lib.rs`。
- PDF 分支：jsPDF 输出 `arraybuffer` → base64 → 同一命令写盘。

## 第 3 条 — 生成状态提到 store

现状：`generating` 是组件本地 `$state`，切 tab 组件卸载即丢；后端 `trigger_cron_job` 阻塞式 await。

改动：
- 在 invest store(或新建 premarket store)持有 `generating`、`startedAt`、`elapsedMs`、`lastError` 等状态(模块级单例)。
- 组件挂载时从 store 读，生成动作写 store；切 tab 回来重渲染即恢复。后端 job lock 保证不重复触发。

## 第 4 条 — 显示生成耗时

- 生成状态(第3条)一并记 `startedAt` / `elapsedMs`。
- 生成中显示秒表(每秒 tick)；完成后在工具栏/报告头显示"本次生成用时 Xs"。

## 第 5 条 — 01 段加宽横排 + 画布放宽

现状：`#report-canvas` / `.toolbar` / `.settings-panel` / `.err-strip` / `.empty` 写死 `720px`；`.theme-wall` 写死 `repeat(2, 1fr)`。

改动(`PremarketReportTab.svelte` `<style>`)：
- 写死的 `720px` 提为变量 `--report-w`(如 `1080px`)放宽画布，各处引用变量。
- `.theme-wall` 从 `repeat(2, 1fr)` 改 `repeat(4, 1fr)`(或 `auto-fill minmax(...)`)，一行放 4-5 个卡片。
- 风险预警卡保留 `grid-column: 1 / -1` 通栏。其余段落网格随宽度自适应。

## 待验证假设(部分已实证 2026-07-09)

### 1. 海外 6 指标覆盖(阻断第9条)——✅ 已实证全覆盖(bundled akshare 1.18.64 + 东财直连)

| 指标 | akshare 接口 | 结果 |
|------|-------------|------|
| VIX | `ak.futures_foreign_hist(symbol="VX")` | ✅ 17.9 @2026-07-08 新鲜 |
| 美10Y国债 | `ak.bond_zh_us_rate(start_date=...)` 取 `美国国债收益率10年` | ✅ 4.55 @2026-07-07，**须 dropna 取最后非空**(最新行常为 nan) |
| 国际金价 | `ak.futures_foreign_hist(symbol="GC")` | ✅ 4078 @2026-07-08 |
| 国际油价 | `ak.futures_foreign_hist(symbol="CL")` | ✅ 73.8 @2026-07-08 |
| USD/CNY | `ak.fx_spot_quote()` 取 `USD/CNY` 行 / 或 `ak.currency_boc_sina("美元")` | ✅ 6.8053 新鲜 |
| **美元指数 DXY** | **东财直连** `push2.eastmoney.com/api/qt/stock/get?secid=100.UDI` | ✅ 100.99 新鲜(akshare `DX` 期货冻结 2019 弃用) |

**美10Y 备选**：除 akshare `bond_zh_us_rate`(需 dropna),东财直连 `secid=171.US10Y` 也可(=4.57，实时、无需 dropna)，更简单，**推荐用东财**。

**东财直连解码**：`f43`=原始值、`f59`=小数位，真值 = `f43 / 10^f59`；`f58`=名称。项目已有 `eastmoney.py` provider,加 DXY/US10Y 两个 secid 的抓取即可,架构一致。

**四源对比结论**(应"看看同花顺/雪球/东财/tushare"要求)：
- **tushare**：美10Y(`us_tycr`)、美元指数均返回 **40203 无此接口权限** → 出局。
- **东财**：DXY(`100.UDI`)、美10Y(`171.US10Y`)直连均 ✅ 新鲜 → **采用**。
- 雪球/同花顺：东财已满足,无需再接(避免多引一套鉴权/抓取)。

**结论**：海外 6 指标**全部有可用新鲜源**(akshare 4 个 + 东财 2 个),**无需舍弃任何指标**,可安全删 Yahoo。

### 2. tushare 按 trade_date 批量行数(影响第6条A)——✅ 已实证(第三方代理 `http://111.170.140.159:8020`)

- 全 A 上市总数 5537；`daily` 单交易日 = **5514~5519 行**、`moneyflow_dc` = **5970 行**。**均远超 5000，单次按 `trade_date` 返回全市场，无截断，不需分页。** ✅
- **数据新鲜**：`daily @ 20260708`(昨/已收盘)=5519 行,`@ 20260709`(今/盘中)=0 行 → 代理数据同步到最近收盘日。
- ⚠️ **坑：代理的 `trade_cal` 表滞后**(测试时区间内只返回到 0701)。**cache job 不要依赖代理 `trade_cal` 判"最近交易日"**——改用"查 `daily(目标日)`,返回 0 行即视为非交易日/数据未出",或用本地交易日历。
- **token 在第三方代理**(非官方 `api.tushare.pro`)：项目 `TushareClient` 已支持 `proxy_url`(见 `with_token_and_proxy` / `resolve_base_url`),缓存 job 复用该路径即可。官方 `us_tycr`/`index_global` 无权限,已由东财直连覆盖(见假设1)。

### 3. 缓存日期规则(第6条B)——✅ 已确认
`get_invest_date()` 源码确认"05:00 前算昨天、之后算今天",故盘后缓存 key 与盘前读取 key 必错位。设计已改为读 `MAX(trade_date)` + 新鲜度守卫,规避该问题。

## 测试与验证

- **假设验证**(见上,进计划前完成)。
- 后端：`premarket_cache` 粗筛+缓存写入单测(mock tushare 批量返回)；`collect_pool_from_cache` 读 `MAX(trade_date)` + 新鲜度守卫单测；注册表 `Category::Overseas → Akshare`、删 `SourceId::Yahoo` 后 `cargo build` + 现有源链测试通过。
- 前端：导出走 Tauri save 手动验证 PNG/PDF 落盘；切 tab 生成状态保持手动验证。
- 健康检查：`get_datasource_health` 手动跑一次确认 AkShare 海外探针 ok、无 Yahoo/Tushare新闻 残留。

## 实施顺序建议

**先验证"待验证假设"** → 后端数据层(6/8) → provider 替换+健康检查(9,先验证 akshare 覆盖再删 Yahoo) → LLM provider(7) → 前端(1/2/3/4/5)。
