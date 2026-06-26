# MacroSnapshot Implementation Plan

> **Status: ✅ DONE** — 后端 + 前端全部完成，已合并到 master。

**Goal:** 在投资委员会分析结果中注入 10 个宏观指标的精确数值，前端可直接展示。

**Architecture:** 新增 `MacroSnapshot` struct 持有 10 个 `Option<f64>` 字段，通过 `build_macro_snapshot()` 从 `macro_cache` 表读取。挂到 `CommitteeResult` 顶层字段，`run_committee` 尾部注入。

**Tech Stack:** Rust, serde, rusqlite (已有依赖)

**Post-implementation notes:**
- `MacroSnapshot` + `build_macro_snapshot()` 已从 `parser.rs` 移到 `macro_cache.rs`（/simplify review 建议）
- HashMap 中间层改为 linear scan（与 `tools.rs:format_macro_entries` 一致）
- 前端新增 `MacroSnapshotCard.svelte` 组件，5 列网格展示
- i18n keys 已添加到 `messages/en.json` 和 `messages/zh-CN.json`

## Global Constraints

- `ParsedFields` 不改动，保持 LLM 解析的纯粹性
- 无过期判定，有值就填，无值为 None
- 复用 `macro_cache::load_all_macro_cache()` 读取数据
- `serde(rename_all = "camelCase")` 与现有 struct 保持一致

---

### Task 1: 新增 MacroSnapshot struct 和 build_macro_snapshot()

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs:1-6` (imports)
- Modify: `src-tauri/src/invest/committee/parser.rs:109` (after ParsedFields)

**Interfaces:**
- Consumes: `crate::storage::invest::macro_cache::load_all_macro_cache() -> Result<Vec<MacroCacheEntry>, String>`
- Produces: `pub struct MacroSnapshot` (10 个 Option<f64> 字段)
- Produces: `pub fn build_macro_snapshot() -> Option<MacroSnapshot>`

- [x] **Step 1: Add MacroSnapshot struct after ParsedFields (line 109)**

在 `parser.rs` 的 `ParsedFields` struct 结束（line 109 `}`）之后、`// Parser functions` 注释之前，插入：

```rust
/// 宏观指标快照（从 macro_cache 直接注入，非 LLM 解析）。
/// 10 个核心指标，用于前端直接展示精确数值。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroSnapshot {
    /// 上证指数
    pub sh_composite_close: Option<f64>,
    /// 上证指数 20 日波动率 (%)
    pub sh_composite_vol20: Option<f64>,
    /// 北向资金净流入 (亿)
    pub northbound_net: Option<f64>,
    /// VIX 恐慌指数
    pub vix: Option<f64>,
    /// 国际金价 (USD)
    pub gold: Option<f64>,
    /// 上涨家数
    pub advance_count: Option<f64>,
    /// 下跌家数
    pub decline_count: Option<f64>,
    /// 两市成交额 (亿)
    pub two_market_volume: Option<f64>,
    /// 涨停家数
    pub limit_up_count: Option<f64>,
    /// 跌停家数
    pub limit_down_count: Option<f64>,
}
```

- [x] **Step 2: Add build_macro_snapshot() function**

在 `MacroSnapshot` struct 之后、`// Parser functions` 注释之前，插入：

```rust
/// 从 macro_cache 表读取 10 个宏观指标，填入 MacroSnapshot。
/// 无过期判定——macro_cache 的刷新策略已保证时效性。
pub fn build_macro_snapshot() -> Option<MacroSnapshot> {
    let entries = crate::storage::invest::macro_cache::load_all_macro_cache().ok()?;
    let map: std::collections::HashMap<String, Option<f64>> = entries
        .into_iter()
        .map(|e| (e.indicator, e.value))
        .collect();
    let get = |key: &str| map.get(key).copied().flatten();
    Some(MacroSnapshot {
        sh_composite_close: get("sh_composite_close"),
        sh_composite_vol20: get("sh_composite_vol20"),
        northbound_net: get("northbound_net"),
        vix: get("vix"),
        gold: get("gold"),
        advance_count: get("advance_count"),
        decline_count: get("decline_count"),
        two_market_volume: get("two_market_volume"),
        limit_up_count: get("limit_up_count"),
        limit_down_count: get("limit_down_count"),
    })
}
```

- [x] **Step 3: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles without errors

- [x] **Step 4: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: no warnings

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(invest): add MacroSnapshot struct and build_macro_snapshot()"
```

---

### Task 2: CommitteeResult 加字段 + run_committee 中注入

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs:176-191` (CommitteeResult struct)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs:1700-1713` (result construction)

**Interfaces:**
- Consumes: `super::parser::MacroSnapshot`, `super::parser::build_macro_snapshot()`
- Produces: `CommitteeResult.macro_snapshot: Option<MacroSnapshot>` — 序列化后前端可通过 `result.macroSnapshot` 访问

- [x] **Step 1: Add macro_snapshot field to CommitteeResult (line 190)**

在 `orchestrator.rs` 的 `CommitteeResult` struct 中，`sanity_check` 字段之后添加：

```rust
pub struct CommitteeResult {
    pub symbol: String,
    pub final_verdict: String,
    pub final_confidence: f64,
    pub macro_signal: String,
    pub macro_strength: Option<f64>,
    /// CIO raw reasoning text (preserved for archiving).
    pub reasoning: String,
    /// All role outputs (Macro, Quant(R1/R2), Risk(R1/R2), CIO).
    pub rounds: Vec<RoundOutputSummary>,
    pub total_tokens: u32,
    pub total_latency_ms: u64,
    pub converged: bool,
    pub sentinel_override: Option<SentinelOverride>,
    pub sanity_check: SanityCheckResult,
    /// 宏观指标快照（从 macro_cache 直接注入，非 LLM 解析）
    pub macro_snapshot: Option<super::parser::MacroSnapshot>,
}
```

- [x] **Step 2: Inject snapshot in run_committee result construction (line 1700)**

在 `run_committee` 中构建 `CommitteeResult` 的位置（约 line 1700），在 `sanity_check: sanity,` 之后添加 `macro_snapshot` 字段：

```rust
    let result = CommitteeResult {
        symbol: symbol.to_string(),
        final_verdict,
        final_confidence,
        macro_signal,
        macro_strength,
        reasoning,
        rounds: round_outputs.iter().map(RoundOutputSummary::from).collect(),
        total_tokens,
        total_latency_ms,
        converged,
        sentinel_override: sentinel,
        sanity_check: sanity,
        macro_snapshot: super::parser::build_macro_snapshot(),
    };
```

- [x] **Step 3: Verify compilation**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles without errors

- [x] **Step 4: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: no warnings

- [x] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): inject MacroSnapshot into CommitteeResult"
```
