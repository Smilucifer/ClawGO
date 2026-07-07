# 盘前观察报告（Pre-market Watch Report）设计

**日期**：2026-07-07
**范围**：openInvest 子系统新增功能，`/invest` 页新增「盘前观察」入口
**状态**：设计定稿，待评审

---

## 1. 目标

生成一份面向**下一交易日**的盘前观察报告，模式为**数据打底 + AI 点评**：硬数据由确定性代码采集与打分，AI 只负责语义聚合、排序解读与话术。报告以图文长图形式展示，可导出为 PNG / PDF。

报告分四段（复刻参考截图）：

1. **外部 / 舆论新闻先验** — 大量抓取东财+雪球新闻，AI 聚合成板块 + 评价标签
2. **资金与宏观双重确认** — 宏观快照 + 市场广度 + 板块资金流入榜 + 拥挤度雷达
3. **明日主线排序** — 板块聚合，前因 × 资金 × 股票池
4. **S/A/B/C 观察池 Top10** — 四因子加权打分分级

**配色**：本报告使用**红涨绿跌**（涨/正=红 `#c0524a`，跌/负=绿 `#4e9a5f`）。本期同时将**全 app 从绿涨红跌统一翻成红涨绿跌**（见第 10 节），使报告与其余 invest 页面配色一致。

---

## 2. 整体架构

数据流水线（触发 → 采集 → 打分 → AI → 组装 → 展示）：

```
定时(盘前 8:15 工作日) / 手动触发
   │
   ▼
[1] 数据采集层（纯数据，无 LLM）
   ├─ 新增: 舆论/新闻抓取(东财+雪球, Python RPC 桥, 可选 symbol)
   ├─ 新增: 板块资金流(AkShare, Python RPC 桥)
   ├─ 新增: 板块拥挤度(换手率分位 + 成交占比 + 龙头背离)
   ├─ 复用: build_macro_snapshot() 宏观快照
   ├─ 复用: moneyflow_dc 个股主力资金 + moneyflow_hsgt 北向
   ├─ 复用: list_events() 金十/公告/新闻
   ├─ 复用: regime + indicators 技术面
   └─ 复用: list_holdings() 股票池
   │
   ▼
[2] SABC 四因子打分（纯 Rust, 可单测, 独立于 verdict）
   舆论热度 + 资金面 + 技术面 + 事件催化 → 0-100 → S/A/B/C
   │
   ▼
[3] AI 点评层（一次 LLM 调用, 走委员会同款 CLI executor）
   ├─ 01: 新闻批量聚合成板块 + 评价标签 + 一句话点评
   ├─ 03: 主线板块排序 + 前因/资金/股票池理由
   └─ 04: 各档分级理由 + 赚钱效应解读（不改档位、不改分）
   │
   ▼
[4] 组装 md + 结构化 JSON → 存盘 {data_dir}/invest/reports/premarket_{date}.md(+.json)
   │
   ▼
[5] 前端: System 下新增「盘前观察」子 tab
   最新报告图文视图(无日期选择) + 立即生成 + 导出 PNG/PDF
```

### 新增独立单元

| 单元 | 位置 | 职责 |
|---|---|---|
| 舆论抓取脚本 | `src-tauri/python/sentiment_scraper.py` | 东财+雪球统一抓取，可选 symbol，输出标准 JSON |
| 板块资金脚本 | `src-tauri/python/sector_flow.py`（或并入上面） | AkShare 板块资金流 + 换手/成交占比 |
| 舆论数据接口 | `src-tauri/src/invest/sentiment.rs` | Rust 调 Python、归一化、短期缓存；**通用接口** |
| SABC 打分器 | `src-tauri/src/invest/premarket/scoring.rs` | 四因子加权，纯函数，可单测 |
| 板块拥挤度 | `src-tauri/src/invest/premarket/crowding.rs` | 拥挤度三指标计算 → 健康/偏热/过热 |
| 报告生成器 | `src-tauri/src/invest/premarket/mod.rs` | 编排：采集 → 打分 → AI → 组装 md/json |
| Tauri 命令 | `commands/invest.rs` 新增 | `fetch_sentiment`、`generate_premarket_report`、`list_premarket_reports`、`read_premarket_report` |
| 前端子 tab | `src/lib/components/invest/PremarketReportTab.svelte` | 图文视图 + 导出，复刻 `CommitteeArchiveTab` 读取模式 |

---

## 3. 数据采集层

### 3.1 舆论/新闻抓取（新增，通用接口）

**Python 脚本** `src-tauri/python/sentiment_scraper.py`，走现有 AkShare 同款 RPC 桥（`python/bridge.rs`，已处理 `PYTHONIOENCODING=utf-8`、懒加载）。

统一入口，provider 可选、symbol 可选：

```python
def fetch_sentiment(provider="all", symbol=None, limit=20, sort="hot"):
    # provider: "eastmoney" | "xueqiu" | "all"
    # symbol:   None → 市场级热门流；传了 → 该股舆论
    # 返回标准 JSON list
```

**统一输出契约**（东财/雪球归一化到同一结构）：

```json
{
  "provider": "eastmoney",
  "symbol": "000001.SZ",
  "title": "...",
  "summary": "...",
  "url": "...",
  "published_at": "2026-07-07T09:30:00",
  "read_count": 12000,
  "comment_count": 340,
  "source_type": "post",
  "sentiment_hint": 0.6
}
```

- `symbol` 市场级为 null。
- `sentiment_hint`（-1.0~1.0）：脚本层用**简单词典**打的粗情绪分（利好/涨停/突破/增持 vs 利空/跌停/减持/爆雷，结合阅读/评论加权）。快、免费、可复现，供 SABC 打分直接使用。**精细情绪判断不在脚本层**，留给 AI 点评做有语境的解读。

**两 provider 内部私有实现，对外只暴露统一接口**：
- 东财：股吧帖子列表 API + 财经资讯接口（有 symbol → 个股吧；无 → 财经要闻）
- 雪球：个股讨论/热帖 API（脚本内部处理 Cookie/token）；无 symbol → 热门话题榜

**健壮性**：UA 头、请求间隔；单 provider 失败不影响另一个（返回该 provider 空列表 + warning）；整体永不因单点失败抛。脚本可 `python sentiment_scraper.py --provider eastmoney --symbol 000001.SZ` 独立跑验证。

**Rust 接口层** `src-tauri/src/invest/sentiment.rs`：

```rust
pub struct SentimentItem { /* 对应上面 JSON */ }
pub struct SentimentQuery { provider, symbol, limit, sort }
pub async fn fetch_sentiment(query: SentimentQuery) -> Result<Vec<SentimentItem>, String>
```

职责：调 Python 桥 → 反序列化 → 可选短期缓存（避免报告生成时重复抓）。**通用接口**：报告生成器、独立 Tauri 命令、以后的事件扫描都能复用，不绑死在报告里。

### 3.2 板块资金流 + 拥挤度（新增）

tushare 的 `moneyflow_dc` 只到个股，板块级资金流走 **AkShare**（同一 Python 桥）。

- **板块资金流入榜**：今日主力净流入 Top 板块 + 净流出 Top 板块。
- **拥挤度雷达（瓶颈预警，`crowding.rs`）**：一期只做「资金拥挤度/过热」单一维度（技术压力位维度留二期），三指标合成：
  1. **换手率分位** — 板块换手率在自身历史的百分位（越高越拥挤）
  2. **成交占比** — 板块成交额占全市场比例（畸高 = 资金过度集中）
  3. **龙头/板块背离** — 龙头涨但板块内跟风减少 = 见顶前兆
- 合成 → 三档徽章：**健康 / 偏热 / 过热**（过热 = 瓶颈预警）。挂在每个流入板块旁。

### 3.3 复用的现有能力

| 数据 | 复用函数 | 位置 |
|---|---|---|
| 宏观快照 + 市场广度 | `build_macro_snapshot()` | `storage/invest/macro_cache.rs` |
| 个股主力资金 | `TushareClient::moneyflow_dc` | `tushare/client.rs` |
| 北向资金 | `TushareClient::moneyflow_hsgt` | `tushare/client.rs` |
| 最近事件(24h) | `storage::invest::events::list_events` | `storage/invest/events.rs` |
| 单标的技术状态 | `regime::compute_regime_for_symbol` + `indicators.rs` | `invest/regime.rs`、`invest/indicators.rs` |
| 股票池 | `storage::invest::portfolio::list_holdings` | `storage/invest/portfolio.rs` |
| 5AM 日切的"今天" | `date_utils::get_invest_date()` | `invest/date_utils.rs` |
| 交易日判断 | `storage::invest::scheduler::is_trading_day` | scheduler |

---

## 4. SABC 四因子打分器（`premarket/scoring.rs`）

纯 Rust、纯函数、可单测。**独立于委员会 verdict**（verdict 未来可作第五因子挂入，但一期不依赖，即使委员会没跑报告也能分级）。

每因子归一化到 0-100，加权合成：

| 因子 | 权重 | 计算 | 数据来源 |
|---|---|---|---|
| 舆论热度 | 30% | `sentiment_hint`(-1~1 → 0-100) × 热度对数(阅读+评论)加成 | `sentiment.rs` |
| 资金面 | 30% | 个股主力净流入率 + 北向持股变动，分位归一 | `moneyflow_dc` + `moneyflow_hsgt` |
| 技术面 | 25% | regime 状态分 + RSI/MA 多头排列 + vol20 量能 | `regime.rs` + `indicators.rs` |
| 事件催化 | 15% | 关联该股的公告/新闻条数与新鲜度（强催化加满） | `list_events` |

- 合成分 = Σ(因子分 × 权重)。
- 缺失因子按中性 50 填充并降权，记 `missing_factors` 写进报告透明化。
- **档位阈值**（做成 `PremarketConfig` 常量，可调）：S ≥ 78、A 62–78、B 45–62、C < 45。

输出结构：

```rust
pub enum Grade { S, A, B, C }
pub struct FactorBreakdown { sentiment: f64, capital: f64, technical: f64, catalyst: f64 }
pub struct SymbolScore {
    symbol: String, name: String,
    total: f64,                    // 0-100
    grade: Grade,
    factors: FactorBreakdown,      // 四分项
    missing_factors: Vec<String>,
}
```

**关键取舍**：打分完全确定、可复现、可单测——给定输入必得同一档位，不调 LLM。AI 只在拿到这些分后写理由和排序微调。

**主线排序（03 段）= 按板块聚合**：把股票池个股按所属板块聚合，用同一批因子数据（舆论 + 资金 + 催化）算板块热度分并排序。个股排序天然由 SABC 合成分决定。

---

## 5. AI 点评层（`premarket/mod.rs` 内，一次 LLM 调用）

复用委员会同款 CLI executor（`committee/cli_executor.rs`），provider/model 从 `CommitteeTuning` + `platform_credentials` 取，不引入新的 LLM 配置通道。

**输入**（结构化上下文拼成一个 prompt）：宏观快照 + 市场广度、板块资金流 + 拥挤度、当日大量新闻（原始标题/摘要）、最近事件(24h)、SABC 打分结果（四因子分项 + 档位）、板块聚合热度分、股票池。

**输出**（结构化片段，解析后嵌入 md 各段）：

- **01 板块聚合**：把大量新闻归类成板块/题材，每板块给一个**评价标签**（见下）+ 关联新闻数 + 一句话说明；末尾一句舆情基调总述。
- **03 主线排序**：板块排序 + 每条主线「前因 × 资金 × 股票池」理由。
- **04 分级解读**：各档分级理由 + 赚钱效应一句话。

**01 评价标签体系**（机会向 + 风险向两类，覆盖政策面负面先验）：

| 标签 | 含义 | 色（红涨绿跌下） |
|---|---|---|
| 新闻强 | 新闻数量/热度高，舆论在发酵 | 金（accent） |
| 催化强 | 有明确催化事件（政策/订单/立项） | 红（up） |
| 情绪强 | 讨论情绪一边倒 | 青蓝（grade-b） |
| 分歧大 | 多空争论激烈 | 绿（down） |
| **风险预警** | 收纳下行风险新闻：监管收紧、政策转向、处罚/退市、爆雷、地缘扰动 | 红底描边警示 |

`风险预警` 板块整行展示，专门保留政策/监管类新闻，避免漏掉负面先验。

**约束**：AI **不改档位、不改分数**（那是打分器定的），只写理由和排序。prompt 明确「以下分级已定，你负责解释和排序主线，不得推翻」。硬数据可复现，AI 只负责可读性。

**降级**：AI 调用失败时报告照常生成——各 AI 段落显示「AI 点评生成失败」占位，数据段和 SABC 分级完整保留。

---

## 6. 错误处理

盘前报告刻意做成**尽力而为**（区别于 `daily_report.rs` 的「任一步失败即 propagate」）：

- 任一数据源失败 → 该段标注「数据缺失」但不中断整体。
- 舆论单 provider 失败 → 用另一个 provider 的数据，标注来源缺失。
- AI 失败 → 数据段 + SABC 分级仍完整产出。
- 报告永远能产出（哪怕残缺），因为它数据源多、盘前时效性强，宁可残缺也要出。

---

## 7. 存储与触发

**存储**：md 文件写 `{data_dir}/invest/reports/premarket_{date}.md`，配套结构化 `premarket_{date}.json`（前端图文视图渲染用，md 用于归档/复制）。与 `daily_report` 共用 `reports/` 目录。**不进 DB**（`daily_reports` 表已被文档标为「只写不读/清理目标」）。日期统一走 `date_utils::get_invest_date()`（5AM 日切）。列表靠扫目录。

**触发**：

- **定时**：`scheduler/mod.rs::default_jobs()` 加 `premarket_report` job，cron `0 0 9 * * 1-5`（盘前 9:00），`requires_trading_day: true`；`runner.rs::dispatch_job` 加分支调 `generate_premarket_report(&data_dir)`。
- **手动**：前端「立即生成」按钮走现成 `trigger_cron_job("premarket_report")`，无需新命令。

---

## 8. 前端展示

`/invest` → System 下新增 `reports`（「盘前观察」）子 tab（sub-tab 列表加一项）。组件 `PremarketReportTab.svelte`：

- **默认展示最新一份报告的图文视图**（无日期选择器——预测的就是下一交易日）。历史报告可留一个折叠入口（次要）。
- **图文视图**：读结构化 JSON → 渲染成 demo 那样的长图（四段），固定 720px 宽的 `#report-canvas` 作为导出目标节点。
- 顶部工具栏：**立即生成**（`trigger_cron_job`）、**导出 PNG**、**导出 PDF**。
- 导出技术选型：PNG 走 `html2canvas` 对 `#report-canvas` 截图；PDF 走 `jspdf`（把 canvas 塞进 PDF）或浏览器打印。依赖新增到 `package.json`。
- md 内容用现成 `renderMarkdown()`（marked v15 + hljs）作为「复制/纯文本」备用视图。

**UI demo**：`docs/ui-demo/premarket-report-demo.html`（已定稿，四段完整，红涨绿跌，含风险预警标签）。前端实现严格按此 demo 的类名与 token。

**新增 Tauri 命令**（共 4 个）：
- `fetch_sentiment(provider, symbol, limit, sort)` — 舆论通用接口，前端可单独查某股舆论、独立验证抓取。
- `generate_premarket_report()` — 手动生成（或复用 `trigger_cron_job`）。
- `list_premarket_reports(limit)` — 扫目录返回报告日期列表。
- `read_premarket_report(date)` — 读某日报告的 md + json。

---

## 9. 测试

- **SABC 打分器**（`scoring.rs`）纯函数单测：各因子归一化边界、缺失因子降权、档位阈值切换、合成分计算。
- **拥挤度**（`crowding.rs`）单测：三指标合成 → 三档映射边界。
- **舆论归一化**（`sentiment.rs`）单测：东财/雪球 JSON → `SentimentItem` 解析、`sentiment_hint` 词典打分。
- **Python 脚本**：可独立 CLI 跑验证（`--provider --symbol`）。
- **前端**：Vitest（node env），沿用现有约定。
- **i18n**：新增 UI 文本中英文 key 同步补齐（`en.json` + `zh-CN.json`）。

Rust 测试在本机走 `cargo check` 或 cmd.exe（见 CLAUDE.md §11 已知运行时问题）。

---

## 10. 全局红涨绿跌翻色（纳入本期范围）

现有 invest 模块整体是**绿涨红跌**，本期统一翻成**红涨绿跌**，全 app 一致。范围与风险：

- **改动点**：
  - `app.css` 的 `[data-invest-scope]` token —— 交换涨跌语义色（`--color-success`/`--color-error` 在涨跌语境的用法，或引入统一的 `--up`/`--down` 语义变量并全量替换）。
  - 所有引用涨跌色的 invest 组件：`MacroSnapshotCard`、`KpiCard`、`HoldingsTable`、`PnlChart`、`TradeLogTab`、committee 相关组件、以及本报告组件。
  - `macro-card-demo.html` 等 demo 里的"本 app 惯例"注释同步更正。
- **策略**：优先**引入语义变量** `--up`（红）/ `--down`（绿），把散落的 `--color-success`/`--color-error` 在**涨跌语境**下的用法替换为 `--up`/`--down`；保留 `--color-success`/`--color-error` 用于**非涨跌**的通用成功/错误状态（如校验通过、连接失败），避免一刀切改错语义。
- **风险**：涨跌色和"成功/错误状态色"在现有代码里可能混用同一变量，需逐处判断语境，不能全局替换字符串。改完需人工过一遍 invest 全部页面确认无语义错色（绿色的"成功"提示不能变成"跌"）。
- **验证**：`npm run build` + 人工巡检 `/invest` 各 tab（dashboard/committee/strategy/trades/system）截图确认。

## 11. 明确排除（YAGNI / 留待二期）

- **NLP 情绪模型**（一期不做）。舆论情绪一期用脚本层**关键词词典**打粗分（`sentiment_hint`），够喂 SABC 打分；精细语境判断已由 AI 点评那一步覆盖。引入本地情绪模型（FinBERT 类）会给 Windows 桌面 app 增加 torch/模型文件重依赖，逐条调 LLM 判情绪则成本翻数十倍、收益有限，均不划算。若后续词典误判影响分级，优先"AI 聚合时顺带返回每板块情绪分"，而非引入本地模型。
- 拥挤度雷达的「技术压力位」维度（二期）。
- SABC 引入委员会 verdict 作为第五因子（二期）。
- 历史报告的富交互浏览（一期只做「最新报告 + 折叠历史入口」）。
