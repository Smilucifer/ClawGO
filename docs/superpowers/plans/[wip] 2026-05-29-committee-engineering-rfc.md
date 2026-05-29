# Committee Engineering RFC — openInvest 移植实施级补充

> 状态：[wip] 等待主人批准后进入 Phase 3a 实施
> 创建：2026-05-29
> 配套主方案：`[wip] 2026-05-28-openinvest-investgui-port.md`
> 多路审查：Claude（架构）+ MiMo Plan（A 股金融）+ DeepSeek（Rust 工程）2026-05-29

## 背景

主方案在产品功能和数据流上完成度高，但 3 路 reviewer 一致指出 4 个实施级 gap 需在 Phase 3a 启动前补齐：

1. **LLM 直调层完全缺失设计**（Blocker，Claude + DeepSeek）
2. **VIX/TNX Yahoo 接口不可靠 + A 股宏观逻辑未本地化**（Blocker，MiMo Plan）
3. **`record_external_trade` 跨文件原子性是伪命题**（Blocker，DeepSeek）
4. **A 股交易日历守卫缺失，cron 在节假日误触发**（High，MiMo Plan）

本 RFC 同时承载 Phase 3 拆分（3a/3b/3c）和锁定的设计决策。

---

## 主人锁定的决策

| ID | 决定 | 说明 |
|----|------|------|
| D1 | LLM 直调限定 OpenAI 兼容模式 | 仅 DeepSeek + MiMo Plan + MiMo API |
| D2 | A 股宏观指标替换 Yahoo | 采纳 MiMo 视角的本土化方案 |
| D3 | holdings/trades 迁入 SQLite | 用 `BEGIN EXCLUSIVE` 保证原子性 |
| D4 | `memory.db` 与 `invest.db` 隔离 | 避免写锁竞争 |
| D5 | 应用未运行时接受任务丢失 | 不注册 Windows 任务计划 |
| D6 | 委员会代理走 ClawGO 现有 proxy 配置 | 不重复实现 |
| D7 | 辩论轮数 6 个下拉选项 | 默认 4 轮，1/2/3/4/6/8 |
| D8 | LLM 全局 Semaphore = 8 | per-provider 独立计数 |
| D9 | 输出长度约束 | 辩论段 200 汉字 / Macro 400 / CIO 300 |
| D10 | 路径 A Dreaming 也做快照 | + UI 增加「已归档」视图 |
| D11 | 委员会运行直接做 streaming | 不分两阶段 |

---

## 一、LLM 调用抽象层

### 1.1 仅支持的 Provider 矩阵

```
DeepSeek      base_url: https://api.deepseek.com/v1            chat
MiMo Plan     base_url: https://token-plan-cn.xiaomimimo.com/v1  Plan 订阅
MiMo API      base_url: https://api.xiaomimimo.com/v1            API key 付费
```

**所有 Provider 走 OpenAI ChatCompletion schema**，包括：
- `POST /v1/chat/completions`
- `messages`: `[{role, content}]`
- `tools`: OpenAI function calling 标准格式（Macro 角色用，其余 4 角色禁用）
- `stream: true` 走 SSE
- 鉴权：`Authorization: Bearer <api_key>`

GLM/QWEN/KIMI 不进入委员会通道（避免 tool calling 协议适配复杂度）。普通群聊不受影响。

### 1.2 `InvestLlmClient` Trait 设计

```rust
// src-tauri/src/invest/llm/mod.rs

pub struct LlmConfig {
    pub provider: ProviderId,         // DeepSeek | MiMoPlan | MiMoApi
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub timeout_secs: u64,            // 默认 60
}

pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub enum StreamChunk {
    Delta { content: String },
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, args_delta: String },
    ToolCallEnd { id: String },
    Finished { finish_reason: String, usage: Usage },
    Error { message: String },
}

#[async_trait]
pub trait InvestLlmClient: Send + Sync {
    async fn chat_stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: Option<&[ToolDef]>,
        config: &LlmConfig,
    ) -> Result<BoxStream<'static, StreamChunk>, LlmError>;
}
```

**实现：单一 `OpenAiCompatClient`，通过 `base_url` 切换 Provider。**

### 1.3 并发控制

```rust
// src-tauri/src/invest/llm/governor.rs

pub struct LlmGovernor {
    semaphores: HashMap<ProviderId, Arc<Semaphore>>,  // 每 Provider 独立
}

impl LlmGovernor {
    pub fn new() -> Self {
        let mut semaphores = HashMap::new();
        for provider in [ProviderId::DeepSeek, ProviderId::MiMoPlan, ProviderId::MiMoApi] {
            semaphores.insert(provider, Arc::new(Semaphore::new(8)));  // D8
        }
        Self { semaphores }
    }

    pub async fn acquire(&self, provider: ProviderId) -> OwnedSemaphorePermit {
        self.semaphores[&provider].clone().acquire_owned().await.unwrap()
    }
}
```

**最大并发 8（D8）**：5 symbols × 3 角色 = 15 并发请求会被压到 8，剩余 7 个排队，避免 429。

### 1.4 错误处理与重试

```rust
pub enum LlmError {
    RateLimit { retry_after_ms: Option<u64> },   // 429
    Timeout,                                      // > timeout_secs
    NetworkError(String),
    ParseError(String),
    Unauthorized,                                  // 401
    InvalidRequest(String),                        // 400
    ServerError(u16),                              // 5xx
}

// 重试策略
pub async fn call_with_retry<F, Fut, T>(f: F) -> Result<T, LlmError>
where F: Fn() -> Fut, Fut: Future<Output = Result<T, LlmError>>
{
    let mut delay = Duration::from_millis(500);
    for attempt in 0..3 {
        match f().await {
            Ok(v) => return Ok(v),
            Err(LlmError::RateLimit { retry_after_ms }) => {
                let d = retry_after_ms.map(Duration::from_millis).unwrap_or(delay);
                tokio::time::sleep(d).await;
                delay *= 2;
            }
            Err(LlmError::Timeout) | Err(LlmError::NetworkError(_)) | Err(LlmError::ServerError(_)) => {
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => return Err(e),  // 401/400 不重试
        }
    }
    Err(LlmError::Timeout)  // 重试耗尽兜底
}
```

**重试耗尽 → 该角色输出 `[WORKER_UNAVAILABLE]` → CIO Gate 3 自动降级为 HOLD（confidence ≤ 0.4）**。

### 1.5 Streaming 实现（D11）

**前后端协议**：Tauri event channel（不是 HTTP SSE，因为 Tauri 不支持）

```
后端 src-tauri/src/invest/committee/runner.rs
  ├─ 调 InvestLlmClient::chat_stream
  ├─ 每收到 StreamChunk → app.emit_to(window, "committee:stream", payload)
  └─ payload: { run_id, symbol, role, round, kind, content }

前端 src/lib/stores/invest-committee-store.svelte.ts
  ├─ listen('committee:stream', handler)
  ├─ 按 (symbol, role, round) 累积 content delta
  └─ 推 PipelineFlow 节点状态：pending → active(脉冲) → done
```

**事件类型枚举**：

```rust
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommitteeStreamEvent {
    PhaseStarted { phase: String, symbol: String },
    RoleStarted { role: CommitteeRole, round: u8, symbol: String },
    RoleDelta { role: CommitteeRole, round: u8, content: String, symbol: String },
    RoleCompleted { role: CommitteeRole, round: u8, symbol: String, full_text: String, parsed: ParsedFields },
    RoleFailed { role: CommitteeRole, round: u8, symbol: String, error: String },
    ConvergenceReached { round: u8, symbol: String },
    VerdictReady { symbol: String, verdict: Verdict },
    Cancelled { symbol: String },
}
```

### 1.6 代理支持（D6）

```rust
// 复用 ClawGO 现有 proxy 配置
let proxy_url = settings.network.proxy_url.clone();  // 已有字段
let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(60));
if let Some(url) = proxy_url {
    builder = builder.proxy(reqwest::Proxy::all(&url)?);
}
let client = builder.build()?;
```

不在委员会模块重复实现 proxy 配置 UI。

### 1.7 输出长度约束（D9）

prompt 系统提示固定追加：

```
【输出长度约束】
- 辩论轮（Round 1..N，Quant/Risk/Wealth）:不超过 200 汉字
- Macro 首轮（数据消化 + SIGNAL）:不超过 400 汉字
- CIO 终局裁决:不超过 300 汉字

超出长度的部分会被截断,要点请放在前部。
```

**后端硬截断兜底**：解析 LLM 输出时按角色 + 轮次截断（按字符数，超长打 warning log）。

**8 轮极限估算**：
- 辩论 tokens：8 × 3 × 300 = 7,200
- 固定开销：4,000
- 合计：11,200 tokens — DeepSeek 128K / MiMo 假定 32K+ 均安全

---

## 二、A 股宏观指标替代方案（B2）

### 2.1 替换映射表

| 原 openInvest 指标 | A 股本土替代 | Tushare MCP 工具 |
|--------------------|------------|--------------------|
| VIX（CBOE 波动率） | 上证 50ETF 历史波动率比值（HV20/HV60） | `index_dailybasic` 计算 + `daily` |
| TNX（10Y 美债收益率） | 10 年期国债 ETF (511260.SH) 价格变化 | `fund_daily` |
| Fed 政策利率 | DR007 银行间 7 日回购利率 | （Tushare 暂无,降级:用 Shibor 替代,需独立 HTTP） |
| S&P 500 percentile | 沪深 300 (000300.SH) percentile | `index_daily` |
| Gold 期货 (GC=F × USDCNY) | 黄金 ETF (518880.SH) | `fund_daily` |
| DuckDuckGo News | Tushare 新闻 | `news`, `major_news` |
| US Treasury curve | 国债 ETF 久期梯队 | `fund_daily` |

### 2.2 A 股新增指标（原版没有,但应纳入)

| 指标 | 含义 | Tushare 工具 |
|------|------|---------------|
| 北向资金净流入 | 外资情绪 5 日累计 | `moneyflow_hsgt` |
| 融资余额变化 | 杠杆资金情绪 | `margin` |
| 涨停股数 / 跌停股数 | 市场宽度 | `limit_list_d` |
| 成交额 vs 5 日均 | 流动性强度 | `index_daily` 大盘指数 amount |
| 龙虎榜机构净买入 | 机构情绪 | `top_inst` |

### 2.3 `get_macro_snapshot` 工具新输出 schema

```json
{
  "snapshot_date": "2026-05-29",
  "market_breadth": {
    "csi300_pct_change": 0.0,
    "csi300_percentile_60d": 0.0,
    "limit_up_count": 0,
    "limit_down_count": 0,
    "amount_ratio_5d": 1.0
  },
  "volatility": {
    "hv20": 0.0,
    "hv60": 0.0,
    "hv_ratio": 0.0
  },
  "rates": {
    "bond_etf_511260_pct_30d": 0.0,
    "dr007": null
  },
  "flows": {
    "northbound_net_5d_cny": 0.0,
    "margin_net_5d_cny": 0.0,
    "top_inst_net_buy": 0.0
  },
  "commodities": {
    "gold_etf_518880_pct_30d": 0.0
  },
  "regime_hint": "uptrend|downtrend|range_bound|crash|recovery"
}
```

### 2.4 Crash regime A 股本地化定义

```rust
pub fn classify_regime(snapshot: &MacroSnapshot, csi300_history: &[Bar]) -> Regime {
    let pct_5d = csi300_history.last_n_pct_change(5);
    let limit_down_ratio = snapshot.market_breadth.limit_down_count as f64
                          / total_a_share_count() as f64;

    if pct_5d < -0.08 && limit_down_ratio > 0.05 {
        Regime::Crash
    } else if pct_5d < -0.10 {
        Regime::Crash
    } else if pct_5d > 0.05 && csi300_history.is_above_ma60() {
        Regime::Uptrend
    } else if pct_5d < -0.03 && !csi300_history.is_above_ma60() {
        Regime::Downtrend
    } else if csi300_history.last_recovered_from_drawdown(0.10) {
        Regime::Recovery
    } else {
        Regime::RangeBound
    }
}
```

**核心思想**：用「连续 5 日跌幅 + 跌停股占比」替代 VIX>30 的多市场判定。

### 2.5 委员会 prompt 本地化

`~/.claw-go/invest/prompts/macro.md` 的 A 股版需替换：
- VIX 阈值 → HV20/HV60 比值阈值（建议 > 1.5 触发警戒）
- Fed → PBOC（中国人民银行）
- S&P 500 → 沪深 300
- 增加北向资金 / 融资余额 / 涨跌停 breadth 字段

保留 prompt 编辑能力,用户可在「角色配置」Tab 自定义。

---

## 三、SQLite 事务方案（B3）

### 3.1 数据库分布（D4）

```
~/.claw-go/memory.db                    （现有,记忆系统）
~/.claw-go/invest/invest.db             （新增,投资专用)
  ├─ holdings           （从 portfolio.json 迁出）
  ├─ trades             （从 history.jsonl 迁出）
  ├─ cash               （多币种现金）
  ├─ verdicts           （委员会裁决归档)
  ├─ pnl_snapshots      （PnL 时序）
  ├─ events             （Event Watch 事件流)
  ├─ event_sources      （事件来源链接）
  ├─ domain_insights    （Dreaming 投资输出 + FTS5）
  └─ scheduler_logs     （cron 运行记录）
```

两库独立 SQLite 文件 + 各自 WAL 模式,不共享连接池,不共享事务。

### 3.2 holdings + trades schema

```sql
CREATE TABLE holdings (
    symbol      TEXT NOT NULL,           -- '600519.SH'
    qty         REAL NOT NULL,
    avg_cost    REAL NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'CNY',
    status      TEXT NOT NULL DEFAULT 'active',
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (symbol, currency)
);

CREATE TABLE trades (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol      TEXT NOT NULL,
    side        TEXT NOT NULL,            -- 'buy' | 'sell'
    qty         REAL NOT NULL,
    price       REAL NOT NULL,
    amount      REAL NOT NULL,
    currency    TEXT NOT NULL,
    trade_date  TEXT NOT NULL,
    source      TEXT NOT NULL,            -- 'manual' | 'committee' | 'external'
    note        TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE cash (
    currency    TEXT PRIMARY KEY,
    amount      REAL NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX idx_trades_symbol ON trades(symbol);
CREATE INDEX idx_trades_date ON trades(trade_date);
```

### 3.3 `record_external_trade` 真原子事务

```rust
// src-tauri/src/storage/portfolio.rs

pub async fn record_external_trade(
    pool: &SqlitePool,
    trade: &TradeInput,
) -> Result<TradeRecord, StorageError> {
    let mut tx = pool.begin().await?;

    // 用 IMMEDIATE 立即取写锁,避免 SQLite 在 commit 时才发现冲突
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *tx).await?;

    match trade.side {
        Side::Buy => {
            // 1. 加权平均成本
            let cur = sqlx::query_as::<_, Holding>(
                "SELECT * FROM holdings WHERE symbol=? AND currency=?"
            )
            .bind(&trade.symbol).bind(&trade.currency)
            .fetch_optional(&mut *tx).await?;

            let (new_qty, new_avg) = match cur {
                Some(h) => {
                    let nq = h.qty + trade.qty;
                    let na = (h.avg_cost * h.qty + trade.price * trade.qty) / nq;
                    (nq, na)
                }
                None => (trade.qty, trade.price),
            };

            // 2. Upsert holding
            sqlx::query(
                "INSERT INTO holdings(symbol,qty,avg_cost,currency,status,updated_at)
                 VALUES(?,?,?,?,'active',?)
                 ON CONFLICT(symbol,currency) DO UPDATE SET
                   qty=excluded.qty, avg_cost=excluded.avg_cost, updated_at=excluded.updated_at"
            )
            .bind(&trade.symbol).bind(new_qty).bind(new_avg)
            .bind(&trade.currency).bind(now_iso8601())
            .execute(&mut *tx).await?;

            // 3. 扣 cash
            sqlx::query(
                "UPDATE cash SET amount = amount - ?, updated_at = ? WHERE currency = ?"
            )
            .bind(trade.qty * trade.price).bind(now_iso8601()).bind(&trade.currency)
            .execute(&mut *tx).await?;
        }
        Side::Sell => { /* 镜像逻辑 */ }
    }

    // 4. 写 trade 审计
    let trade_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO trades(symbol,side,qty,price,amount,currency,trade_date,source,note,created_at)
         VALUES(?,?,?,?,?,?,?,?,?,?) RETURNING id"
    )
    .bind(&trade.symbol).bind(trade.side.as_str())
    .bind(trade.qty).bind(trade.price).bind(trade.qty * trade.price)
    .bind(&trade.currency).bind(&trade.trade_date)
    .bind(trade.source.as_str()).bind(&trade.note).bind(now_iso8601())
    .fetch_one(&mut *tx).await?;

    tx.commit().await?;  // 真原子性

    Ok(TradeRecord { id: trade_id, /* ... */ })
}
```

### 3.4 PnL 快照读写锁

PnL 定时任务读全表,用户操作写入 — 用 SQLite WAL 模式即可天然支持并发读 + 单写。**不需要额外 RwLock**,只需保证写事务用 `IMMEDIATE`。

### 3.5 迁移：旧 JSON → SQLite

Phase 2 启动时运行一次性迁移：

```rust
pub async fn migrate_legacy_portfolio(pool: &SqlitePool) -> Result<()> {
    let portfolio_json = "~/.claw-go/invest/portfolio.json";
    let history_jsonl = "~/.claw-go/invest/history.jsonl";

    if !Path::new(portfolio_json).exists() {
        return Ok(());  // 新用户,无需迁移
    }

    let mut tx = pool.begin().await?;

    // 读 portfolio.json → INSERT holdings + cash
    let p: LegacyPortfolio = serde_json::from_reader(File::open(portfolio_json)?)?;
    for h in p.holdings { /* INSERT */ }
    for (ccy, amt) in p.cash { /* INSERT cash */ }

    // 读 history.jsonl 逐行 → INSERT trades
    for line in BufReader::new(File::open(history_jsonl)?).lines() {
        let t: LegacyTrade = serde_json::from_str(&line?)?;
        /* INSERT */
    }

    tx.commit().await?;

    // 备份旧文件
    fs::rename(portfolio_json, format!("{}.legacy", portfolio_json))?;
    fs::rename(history_jsonl, format!("{}.legacy", history_jsonl))?;
    Ok(())
}
```

迁移失败时不删除旧文件,保留原始数据供手动恢复。

---

## 四、A 股交易日历守卫

### 4.1 数据来源

Tushare MCP `trade_cal` 接口提供完整日历：

```python
# 调用示例
trade_cal(exchange='SSE', start_date='20260101', end_date='20271231')
# 返回字段: cal_date, is_open (0/1), pretrade_date
```

### 4.2 本地缓存 schema

```sql
-- invest.db
CREATE TABLE trade_calendar (
    cal_date    TEXT PRIMARY KEY,          -- 'YYYYMMDD'
    is_open     INTEGER NOT NULL,          -- 0 | 1
    pretrade    TEXT,
    fetched_at  TEXT NOT NULL
);
```

### 4.3 同步策略

```rust
pub async fn sync_trade_calendar(pool: &SqlitePool, mcp: &TushareMcp) -> Result<()> {
    // 每月初同步未来 24 个月
    let last = sqlx::query_scalar::<_, String>(
        "SELECT MAX(cal_date) FROM trade_calendar"
    ).fetch_one(pool).await?;

    let today = Utc::now().format("%Y%m%d").to_string();
    let need_sync = last < add_days(&today, 90);  // 距末尾不足 90 天就拉

    if need_sync {
        let end = add_days(&today, 730);  // 拉未来 2 年
        let cal = mcp.trade_cal("SSE", &today, &end).await?;
        // batch INSERT OR REPLACE
    }
    Ok(())
}
```

应用启动时执行一次,失败不阻塞启动（用现有缓存）。

### 4.4 `is_trading_day()` 守卫

```rust
pub async fn is_trading_day(pool: &SqlitePool, date: &str) -> Result<bool> {
    let r = sqlx::query_scalar::<_, i64>(
        "SELECT is_open FROM trade_calendar WHERE cal_date = ?"
    )
    .bind(date)
    .fetch_optional(pool).await?;

    match r {
        Some(1) => Ok(true),
        Some(0) => Ok(false),
        None => {
            log::warn!("交易日历缺失 {}, 退化为周一到周五判定", date);
            Ok(weekday_is_mon_fri(date))
        }
    }
}
```

### 4.5 所有 cron job 集成守卫

```rust
// src-tauri/src/invest/scheduler/runner.rs

pub async fn run_cron_tick(job: &CronJob, pool: &SqlitePool, mcp: &TushareMcp) {
    let today = Utc::now().format("%Y%m%d").to_string();

    if job.requires_trading_day && !is_trading_day(pool, &today).await.unwrap_or(true) {
        log::info!("[cron {}] 跳过非交易日 {}", job.name, today);
        append_log(pool, &job.name, "skipped_non_trading_day", None).await.ok();
        return;
    }

    // 执行任务...
}
```

`requires_trading_day` 配置：

| Job | requires_trading_day |
|-----|---------------------|
| PnL 快照 | true |
| Verdict Review | true |
| 每日报告 | true |
| Event Watch | false（节假日也有政策新闻） |
| Dreaming（用户记忆） | false |
| Dreaming（投资） | true（无新裁决无意义) |
| Payday Check | false（按自然日） |

### 4.6 优化后的 cron 表（替换原方案 §5b.2）

| 任务 | Cron | 守卫 |
|------|------|------|
| PnL 快照 | `30 9,11 * * 1-5` + `0 13,15 * * 1-5` | trading_day |
| Verdict Review | `0 17 * * 1-5` | trading_day |
| 每日报告 | `0 22 * * 1-5` | trading_day |
| Event Watch | `*/30 8-22 * * 1-5` + `0 9,18 * * 0,6` | none |
| Dreaming（用户) | 自动间隔 120min | none |
| Dreaming（投资） | `0 3 * * *` | trading_day（昨日是交易日才跑) |
| Payday Check | `0 9 25 * *` | none |

---

## 五、Phase 3 拆分（3a / 3b / 3c）

### Phase 3a — LLM 直调层 + 委员会核心（batch 模式跑通)

**前置依赖**：Phase 1 + Phase 2 已完成

**目标**：让 `run_committee` Tauri 命令对单个 symbol 跑出完整裁决,**先用 batch 模式**（虽然 D11 要 streaming,但先有正确的 batch 才能加 streaming),CLI 输出能看到结果即可,UI 暂用最简列表。

任务清单：
- [ ] `src-tauri/src/invest/llm/` 模块
  - [ ] `OpenAiCompatClient`（reqwest + JSON）
  - [ ] `LlmGovernor` (8 并发 Semaphore × 3 Provider)
  - [ ] 重试退避 (3 次, 500ms 起)
  - [ ] timeout 60s
  - [ ] proxy 复用现有配置
  - [ ] Provider 凭据从 `PlatformCredential` 读取
- [ ] `src-tauri/src/invest/committee/` 模块
  - [ ] `Role` enum + `RoundOutput` 类型
  - [ ] LLM 输出解析（SIGNAL / STRENGTH / CONCENTRATION_PCT / verdict / confidence）
  - [ ] 解析失败 fallback（保留原文 + warning,不阻塞）
  - [ ] SENTINEL 覆写（CONCENTRATION_PCT diff > 0.3% 强制覆写）
  - [ ] 收敛检测（Quant + Risk 最近 2 轮 SIGNAL 同 + STRENGTH diff < 1.0)
  - [ ] CIO Sanity Check 三道门
  - [ ] 输出长度硬截断兜底
- [ ] 5 角色 prompt 文件 + A 股本地化版
  - [ ] `~/.claw-go/invest/prompts/macro.md` (A 股版)
  - [ ] `quant.md` / `quant_rebuttal.md`
  - [ ] `risk.md` / `risk_rebuttal.md`
  - [ ] `wealth.md`
  - [ ] `cio.md`
- [ ] 白名单工具实现（5 个,仅 Macro 可调）
  - [ ] `get_history_data`（Tushare `daily`)
  - [ ] `analyze_multi_timeframe` (Rust 计算)
  - [ ] `get_macro_snapshot`（A 股本地化版,§2.3 schema)
  - [ ] `query_dreaming_insights`（FTS5 查询）
  - [ ] `get_recent_committee_verdicts`
- [ ] holdings/trades SQLite 迁移（§3）
- [ ] 交易日历同步 + `is_trading_day` 守卫（§4)
- [ ] Tauri 命令：`run_committee`(batch), `cancel_committee_run`
- [ ] 单元测试：parser、SENTINEL、Sanity Check、收敛检测

**验收标准**：
- `cargo test --manifest-path src-tauri/Cargo.toml invest::` 通过
- 手动调 `run_committee("600519.SH")` 能从 DeepSeek + MiMo 两个 Provider 各跑通一次
- holdings/trades 迁移脚本对历史 JSON 数据无损迁入 invest.db
- 节假日 cron 守卫生效（mock trade_calendar 测试）

### Phase 3b — Streaming + 7 Tab UI + PipelineFlow

**前置依赖**：Phase 3a

**目标**：Streaming 协议落地,完整委员会页面,PipelineFlow 动画。

任务清单：
- [ ] `InvestLlmClient::chat_stream` (SSE 解析)
- [ ] `CommitteeStreamEvent` Tauri event channel 推流
- [ ] 前端 store `invest-committee-store.svelte.ts`
  - [ ] 监听 `committee:stream`
  - [ ] 按 (symbol, role, round) 累积 delta
  - [ ] streaming token 增量渲染
- [ ] `/invest` 路由 + Tab 导航
- [ ] 委员会 7 子 Tab
  - [ ] 直播（StatusBadge + SseIndicator + 启动按钮)
  - [ ] 决议归档（左右双栏 + CommitteeDetail）
  - [ ] 决策回放（PipelineFlow)
  - [ ] 角色配置（5 角色 prompt 编辑）
  - [ ] 命中率（KPI 卡片 + 表格）
  - [ ] LLM 用量
  - [ ] Tool 调用日志
- [ ] PipelineFlow 组件（Svelte transitions + CSS）
  - [ ] 5 角色节点（48×48,角色色编码)
  - [ ] pending/active/done/error 状态
  - [ ] active 状态脉冲动画（CSS keyframes）
  - [ ] 流动光点（3 圆点循环）
  - [ ] 动态轮数 overflow-x-auto
  - [ ] 入场动画 `transition:fly`
- [ ] Provider 下拉选择 + 角色级覆盖 UI
- [ ] 辩论轮数下拉（D7：1/2/3/4 默认/6/8)
- [ ] i18n 更新

**验收标准**：
- 直播 Tab 启动委员会,能看到 PipelineFlow 节点状态变化 + 角色文本逐 token 出现
- 取消按钮能中断进行中的委员会运行
- 切换 6 个轮数选项均能正常运行,8 轮极限模式 token 用量符合预期

### Phase 3c — Event Watch + 触发链

**前置依赖**：Phase 3b

**目标**：新闻事件监控 + 触发委员会确认流。

任务清单：
- [ ] `src-tauri/src/invest/events/` 模块
  - [ ] Tushare `news` / `major_news` / `anns_d` 适配
  - [ ] RSS 抓取（feed-rs Rust crate)
  - [ ] LLM 归一化（severity / stance / entity match）
- [ ] `events` + `event_sources` 表
- [ ] FTS5 索引（按 claim、affected_symbols 搜索）
- [ ] 三重过滤逻辑
- [ ] Event Watch cron（`*/30 8-22 * * 1-5` + 周末早晚)
- [ ] 触发确认对话框 UI（高 severity 事件弹窗）
- [ ] 事件监控 Tab
  - [ ] 事件流列表
  - [ ] 配置区（频率、最低 severity、关注 symbol）

**验收标准**：
- 真实 Tushare 新闻能被抓取并归一化
- 高 severity 事件弹出确认对话框,确认后能链到 Phase 3b 的委员会运行
- RSS 中文源（财新/第一财经）编码无乱码

---

## 六、风险登记表

| ID | 风险 | 等级 | 缓解 |
|----|------|------|------|
| R1 | DeepSeek/MiMo OpenAI 兼容性细微差异（如 streaming finish_reason 格式） | M | Phase 3a 单元测试覆盖两 Provider |
| R2 | Tushare MCP 限流 200/min,委员会 + cron 并发可能超限 | M | 全局 RateLimiter（每分钟 180 上限）+ 缓存近期查询结果 |
| R3 | invest.db 迁移失败导致用户历史持仓丢失 | H | 不删除旧 JSON,保留 `.legacy` 后缀,文档化恢复流程 |
| R4 | 8 轮极限模式 LLM 用量爆炸（>50K tokens/symbol) | M | UI 提示「预计 token 消耗」+ 用量上限警告 |
| R5 | Streaming 长连接在弱网下断流 | M | SSE 心跳 + 断线检测 + 自动重新发起请求 |
| R6 | A 股 Crash regime 阈值需要回测验证 | M | Phase 4 用 Tushare 历史数据 backtest 阈值 |
| R7 | 节假日守卫的 trade_calendar 同步失败时退化为周一到周五会误触发调休补班日 | L | 应用启动检测同步状态,失败时 toast 警告 |

---

## 七、确认清单

- [ ] 主人确认 Phase 3 拆为 3a/3b/3c
- [ ] 主人确认 D1-D11 全部决策
- [ ] 主人确认 cron 优化表（§4.6）
- [ ] 主人确认 A 股宏观替代映射（§2.1）
- [ ] 主人确认风险登记表 R1-R7

确认后,主方案 `[wip] 2026-05-28-openinvest-investgui-port.md` 同步更新：
- §6.1 添加 invest.db 独立说明
- §7.3 替换为本 RFC §1
- §7.7 替换为本 RFC §2.3
- §8.3 添加路径 A 快照 + archived 视图
- §11 重新组织 Phase 3 (3a/3b/3c)
- 新增 §11.0 应用未运行时任务丢失策略
