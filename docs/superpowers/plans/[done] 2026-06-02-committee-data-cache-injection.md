# 委员会数据缓存 + 8 段注入优化

## Context

委员会运行时 `build_asset_context()` 每次直接调 5 个 tushare API（daily_basic、fina_indicator、report_rc、moneyflow_dc、stock_basic），没有任何缓存。同时 prompt 模板里写了 `{{pe_ttm}}`、`{{industry}}` 等占位符，但 `load_prompt_for_round()` 只替换 `{{asset_name}}` 和 `{{asset_symbol}}`，其他占位符原样残留。`run_role_phase()` 又在 user message 末尾追加同样的数据，造成双写。

目标：
1. 新建 `stock_data_cache` 表，永久存储个股数据
2. `build_asset_context()` 改为 cache-first + API fallback
3. 修复占位符替换，删掉重复注入
4. 扩展 AssetContext 补齐 Risk/CIO/L4 缺失字段
5. 工具层（exec_company_info、exec_moneyflow）也走 cache-first

## 修改文件清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/storage/invest/mod.rs` | 加 `pub mod stock_data_cache;` + CREATE TABLE migration |
| **新建** `src-tauri/src/storage/invest/stock_data_cache.rs` | 新表 CRUD |
| `src-tauri/src/invest/committee/orchestrator.rs` | AssetContext 扩展 + build_asset_context 改 cache-first + 删除 run_role_phase 重复注入 + load_prompt_for_round 传入 ctx |
| `src-tauri/src/invest/committee/roles.rs` | `load_prompt_for_round` 签名扩展，一次性替换全部占位符 |
| `src-tauri/src/invest/committee/tools.rs` | exec_company_info / exec_moneyflow 改 cache-first |

## Step 1: stock_data_cache 表

新建 `src-tauri/src/storage/invest/stock_data_cache.rs`，参照 `macro_cache.rs` 模式。

```sql
CREATE TABLE IF NOT EXISTS stock_data_cache (
    symbol      TEXT NOT NULL,
    data_type   TEXT NOT NULL,
    data_date   TEXT NOT NULL,
    value_json  TEXT NOT NULL,
    fetched_at  TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, data_type, data_date)
);
```

`data_type` 取值：
- `daily_basic` — PE/PB/换手率/总市值/流通市值
- `fina_indicator` — ROE/ROA/营收增速/净利增速/负债率
- `report_rc` — 机构评级明细
- `moneyflow_dc` — 近5日资金流向汇总
- `industry` — 行业/赛道

公共接口：
- `upsert_cache(symbol, data_type, data_date, value_json)` — INSERT OR REPLACE
- `load_latest(symbol, data_type)` → `Option<(data_date, value_json, fetched_at)>` — ORDER BY data_date DESC LIMIT 1
- `load_on_date(symbol, data_type, data_date)` → `Option<value_json>`

在 `storage/invest/mod.rs` 加 `pub mod stock_data_cache;`，在 `init_db()` 中调用 `stock_data_cache::create_table(&conn)`。

## Step 2: 扩展 AssetContext

在 `orchestrator.rs` 的 `AssetContext` struct 中新增字段：

```rust
pub struct AssetContext {
    // ── 已有 ──
    pub asset_type: String,
    pub industry: Option<String>,
    pub money_flow_summary: Option<String>,
    pub pe_ttm: Option<f64>,
    pub pb: Option<f64>,
    pub total_mv_yi: Option<f64>,
    pub roe: Option<f64>,
    pub or_yoy: Option<f64>,
    pub np_yoy: Option<f64>,
    pub rating_summary: Option<String>,
    pub risk_news: Option<String>,
    pub turnover_rate: Option<f64>,

    // ── 新增（8 段补充）──
    pub circ_mv_yi: Option<f64>,           // 流通市值（亿）
    pub roa: Option<f64>,                   // ROA
    pub debt_to_assets: Option<f64>,        // 资产负债率%
    pub latest_close: Option<f64>,          // 最新价
    pub pre_close: Option<f64>,             // 昨收
    pub data_quality: Vec<String>,          // 缺失字段清单 ["PE=N/A", "评级=N/A"]
}
```

## Step 3: build_asset_context 改为 cache-first

重写 `build_asset_context()`：

```
async fn build_asset_context(client, symbol, asset_type) -> AssetContext:
    // 1. 尝试从 cache 读取
    let cached = stock_data_cache::load_latest(symbol, "daily_basic");
    let fina_cached = stock_data_cache::load_latest(symbol, "fina_indicator");
    let rc_cached = stock_data_cache::load_latest(symbol, "report_rc");
    let mf_cached = stock_data_cache::load_latest(symbol, "moneyflow_dc");
    let ind_cached = stock_data_cache::load_latest(symbol, "industry");

    // 2. 任何一个 cache miss → 触发全量刷新
    if all_cached_is_some && not_stale:
        return AssetContext::from_cache(cached, fina_cached, ...);

    // 3. Cache miss → 调 API + 写入 cache
    refresh_asset_data(client, symbol).await;  // 新函数，批量调所有 API
    // 4. 从 cache 重新读取并组装 AssetContext
    return AssetContext::from_cache(...);
```

新增 `refresh_asset_data(client, symbol)` 函数：
- tokio::join! 并行调 `daily_basic` + `fina_indicator` + `report_rc` + `moneyflow_dc`
- 条件性调 `stock_basic`（仅 stock 类型）
- 对 `rt_k` 单独调用获取最新价
- 每个结果写入 `stock_data_cache::upsert_cache()`
- `rt_k` 结果不缓存（实时数据），直接返回

新增 `AssetContext::from_cache()` 关联函数：
- 从各 cache entry 的 value_json 反序列化
- 填充 `data_quality`（哪些字段是 None 就加入清单）
- `latest_close` 从 `daily_basic.close` 或独立 `rt_k` 调用获取

## Step 4: 修复 load_prompt_for_round 占位符替换

**roles.rs** — 扩展函数签名：

```rust
pub fn load_prompt_for_round(
    role: CommitteeRole,
    round: u8,
    asset_name: &str,
    asset_symbol: &str,
    asset_context: &AssetContext,  // 新增
) -> String
```

在现有的 `asset_name/asset_symbol` 替换之后，追加所有 AssetContext 字段的替换：

```rust
raw.replace("{{asset_name}}", asset_name)
    .replace("{{asset_symbol}}", asset_symbol)
    .replace("{{asset_type}}", &asset_context.asset_type)
    .replace("{{industry}}", asset_context.industry.as_deref().unwrap_or("N/A"))
    .replace("{{pe_ttm}}", &format_opt(pe_ttm))
    .replace("{{pb}}", &format_opt(pb))
    .replace("{{roe}}", &format_opt(roe))
    .replace("{{turnover_rate}}", &format_opt(turnover_rate))
    .replace("{{money_flow_summary}}", money_flow_summary.as_deref().unwrap_or("N/A"))
    // 新增 8 段字段
    .replace("{{latest_close}}", &format_opt(latest_close))
    .replace("{{circ_mv_yi}}", &format_opt(circ_mv))
    .replace("{{roa}}", &format_opt(roa))
    .replace("{{debt_to_assets}}", &format_opt(debt))
    .replace("{{rating_summary}}", rating_summary.as_deref().unwrap_or("N/A"))
```

## Step 5: 更新 prompt 模板 — Risk 加入缺失占位符

**roles.rs — RISK_PROMPT**：在 `**资产上下文**（系统注入）` 部分新增：

```
**资产上下文**（系统注入）：
- 标的类型: {{asset_type}}
- 所属行业: {{industry}}
- 最新价: {{latest_close}}（可能为 N/A）
- 估值: PE={{pe_ttm}}, PB={{pb}}, ROE={{roe}}%, 换手率={{turnover_rate}}%
- 财务: ROA={{roa}}%, 营收增速={{or_yoy}}%, 净利增速={{np_yoy}}%, 负债率={{debt_to_assets}}%
- 机构评级: {{rating_summary}}
```

注意：Risk prompt 里的 {{or_yoy}} 和 {{np_yoy}} 目前是通过 run_role_phase 追加的，改为占位符后就统一了。

**roles.rs — CIO_PROMPT**：不需要改模板，CIO 无工具，所有数据从 round_outputs 获得。但可在末尾追加数据质量注入。

## Step 6: 删除 run_role_phase 重复注入

**orchestrator.rs — run_role_phase**：删除整个 `// 根据角色注入 AssetContext 特定数据` 块（lines 1144-1242）。

因为：
- Macro 的 `{{industry}}` 已通过占位符替换
- Quant 的 `{{pe_ttm}}`/`{{pb}}`/`{{roe}}`/`{{turnover_rate}}`/`{{money_flow_summary}}` 已通过占位符替换
- Risk 的全部字段已通过占位符替换（Step 5）
- CIO 的摘要数据 → 在 system prompt 末尾追加 `build_cio_asset_summary(ctx)` 代替
- L4 的集中度/子弹 → 从 round_outputs（Risk 输出）获取，不需要重复注入；`latest_close` 通过占位符

保留的：
- `build_risk_metrics_context()` — 这是 portfolio 层面的预计算风险指标（集中度/浮盈/回撤），不同于 AssetContext，在 Risk R1 的 user message 中注入，保留。
- CIO 需要一个 `build_cio_asset_summary()` 在 system prompt 末尾追加（占位符不适用于 CIO 因为 CIO prompt 没有占位符定义）。

## Step 7: 工具层 cache-first

**tools.rs — exec_company_info()**：

```rust
async fn exec_company_info(symbol: &str) -> Result<String, String> {
    // Cache-first: 从 stock_data_cache 读取 daily_basic
    if let Ok(Some((_, json, _))) = stock_data_cache::load_latest(symbol, "daily_basic") {
        if let Ok(data) = serde_json::from_str::<DailyBasic>(&json) {
            return Ok(format!("【{} 估值数据】\nPE: {:.2}, PB: {:.2}, ...", ...));
        }
    }
    // Fallback: 调 API
    let client = TushareClient::from_settings()?;
    // ... 原逻辑
}
```

**tools.rs — exec_moneyflow()**：

```rust
async fn exec_moneyflow(symbol: &str) -> Result<String, String> {
    // Cache-first
    if let Ok(Some((date, json, _))) = stock_data_cache::load_latest(symbol, "moneyflow_dc") {
        if date == today {  // 只用当天的缓存
            if let Ok(summary) = serde_json::from_str::<String>(&json) {
                return Ok(format!("【{} 资金流向】\n{}", symbol, summary));
            }
        }
    }
    // Fallback: 调 API
    // ...
}
```

`get_history_data` 和 `analyze_multi_timeframe` 不改 — 它们需要完整日线数据做 MA 计算，不适合用 cache。

## Step 8: 更新所有 load_prompt_for_round 调用点

3 个调用点需要传入 `&AssetContext`：
1. `run_macro_phase` (line 914) — 传 `&AssetContext::default()` 或从 `build_asset_context` 结果传入
2. `run_role_phase` (line 1117) — 已有 `asset_context` 参数，直接传
3. `run_macro_phase` 需要先构建 asset_context，或者 Macro 不需要完整的 asset_context（Macro 只用 industry）

优化：`build_asset_context()` 在 `run_committee` 里提前调用（现在已经是），Macro phase 也能用。

## 验证

```bash
cargo check --manifest-path src-tauri/Cargo.toml
npm run lint
npm run build
```

回归验证：
- 委员会运行 Macro → system prompt 里 `{{industry}}` 已替换，user message 无重复追加
- 委员会运行 Quant → system prompt 里 `{{pe_ttm}}` 等已替换，user message 无重复追加
- 委员会运行 Risk → system prompt 包含 PE/PB/ROA/负债率/评级
- 委员会运行 CIO → system prompt 末尾有数据质量清单
- 工具 `get_company_info` 第二次调用走 cache（观察日志）
- `stock_data_cache` 表有数据写入
