# Plan: 卖出自动转 Watch + G3 子弹数据修复

## Context

两个独立的 invest 模块 bug：
1. **Dashboard 卖出后持仓消失**：用户卖出全部股份后，持仓直接从 holdings 表中删除。用户期望自动转为 watch 状态，方便后续手动删除或重新买入。
2. **G3 检查永远显示"子弹数据不可用"**：`cio_sanity_check` 从 LLM 输出中解析 `dry_powder_cny`，但 LLM 经常不输出该字段。系统已有真实的现金余额数据（`portfolio_data.cash`），应作为兜底使用。

---

## Task 1: 卖出全部持仓后自动转为 Watch

### 修改文件
- `src-tauri/src/storage/invest/portfolio.rs`

### 方案
在 `recalculate_holdings_inner_body` 的 sell 分支中，当卖出导致 `shares ≈ 0` 时，将 hold 条目转为 watch 条目（参照已有的 `convert_hold_to_watch` 逻辑）。

**具体改动**（sell 分支，约 line 313-318）：

```rust
"sell" => {
    let shares = t.shares.unwrap_or(0.0);
    if let Some(entry) = map.get_mut(&key) {
        entry.shares = (entry.shares - shares).max(0.0);
        entry.recompute_notional();
        // Auto-convert to watch when all shares are sold
        if entry.shares <= 0.0001 && !entry.is_watch {
            let watch_key = (t.symbol.clone(), t.currency.clone(), "watch".to_string());
            let name = entry.name.clone();
            let avg_cost = entry.avg_cost;
            let notional = entry.notional;
            let entry_date = entry.entry_date.clone();
            let asset_type = entry.asset_type.clone();
            map.remove(&key);
            let watch_entry = map.entry(watch_key).or_default();
            watch_entry.name = name;
            watch_entry.avg_cost = avg_cost;
            watch_entry.notional = notional;
            watch_entry.entry_date = entry_date;
            watch_entry.asset_type = asset_type;
            watch_entry.is_watch = true;
        }
    }
}
```

**效果**：
- 卖出部分股份 → 持仓减少但保留为 hold
- 卖出全部股份 → 持仓自动转为 watch，保留名称、成本价等信息
- 已有 watch 条目的情况 → `or_default()` 会保留已有 watch 条目（不会覆盖）
- 现金计算不受影响（sell 的 cash += amount 在 sell 分支内已完成）

---

## Task 2: G3 子弹数据兜底修复

### 修改文件
1. `src-tauri/src/invest/committee/analysis.rs` — `cio_sanity_check` 签名和 G3 逻辑
2. `src-tauri/src/invest/committee/orchestrator.rs` — 调用处传入 `portfolio_data.cash`

### 方案

**analysis.rs**：给 `cio_sanity_check` 新增 `actual_cash_cny: Option<f64>` 参数（`Option` 避免破坏 10+ 现有测试的调用签名，现有测试传 `None` 保持原行为）。

```rust
pub fn cio_sanity_check(
    cio_parsed: &ParsedFields,
    round_outputs: &[RoundOutput],
    macro_signal: &str,
    emergency_buffer_cny: f64,
    actual_cash_cny: Option<f64>,  // 新增：PortfolioData.cash，None 则保持旧行为
) -> SanityCheckResult {
    // ...
    // Gate 3 -- Dry powder check
    let dry_powder = cio_parsed.dry_powder_cny.or_else(|| {
        round_outputs
            .iter()
            .filter_map(|o| o.parsed.dry_powder_cny)
            .last()
    }).or(actual_cash_cny);  // 兜底：用真实现金余额
    // ... 后续逻辑不变（如果 dry_powder 仍是 None，走原有"数据不可用"分支）
}
```

当 `actual_cash_cny` 为 `Some(cash)` 时，`dry_powder` 永远是 `Some`，G3 检查永远能执行。"子弹数据不可用"的 note 不再出现。当为 `None` 时保持旧逻辑，现有测试无需修改。

**orchestrator.rs**：调用处传入 `Some(portfolio_data.cash)`：

```rust
let sanity = cio_sanity_check(
    &cio_parsed,
    &round_outputs,
    &macro_signal,
    config.emergency_buffer_cny,
    Some(portfolio_data.cash),  // 新增
);
```

### 现有测试影响
- 所有 ~10 个现有测试调用需添加第 5 个参数 `None`（Rust 无默认参数），行为不变
- 新增 1 个测试验证 `actual_cash_cny` 兜底逻辑（传 `Some(200000.0)` 且不设 `dry_powder_cny`，验证 G3 pass）

---

## 验证

1. `cargo check --manifest-path src-tauri/Cargo.toml` — 编译通过
2. `cargo test --manifest-path src-tauri/Cargo.toml` — 已有测试不回归（注意：Rust 单元测试因 STATUS_ENTRYPOINT_NOT_FOUND 可能无法运行，用 cargo check 替代）
3. 功能验证：
   - Task 1：在 dashboard 卖出某只股票的全部持仓，确认它出现在 watch 列表中
   - Task 2：运行委员会，确认 G3 不再显示"子弹数据不可用"
