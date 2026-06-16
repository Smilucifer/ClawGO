# Plan: PnL 快照收益计算排除转入转出

## Context

PnL 快照的 `daily_pnl` 计算公式为 `total_value_today - total_value_prev`，其中 `total_value = cash + holdings_value`。当用户执行转入/转出操作时，cash 变化会导致 `daily_pnl` 产生虚假的盈亏——转入 10000 元会显示为 +10000 收益，转出则显示为亏损。

`totalReturnPct`（前端 KPI 卡片）已经是纯持仓收益率，不受影响。问题仅在 PnL 快照的 `daily_pnl` / `daily_pnl_pct`。

## 修改方案

### 1. 后端：查询区间净转入金额

**文件:** `src-tauri/src/storage/invest/portfolio.rs`

新增函数 `get_net_transfer_between(from_date, to_date) -> f64`：
- 查询 `trades` 表中 `action IN ('transfer_in', 'transfer_out')` 且 `trade_date` 在两个日期之间的记录
- 返回 `sum(transfer_in amounts) - sum(transfer_out amounts)`

### 2. 后端：PnL 快照扣除净转入

**文件:** `src-tauri/src/lib.rs` (`run_pnl_snapshot` 函数，行 108-122)

修改 `daily_pnl` 计算：
```rust
let net_transfer = portfolio::get_net_transfer_between(&last.snapshot_date, &today)?;
let pnl = (total_value - last.total_value) - net_transfer;
```

`daily_pnl_pct` 同步调整：`pnl / last.total_value * 100`（分母不变，仍基于前日总资产）。

### 3. 无需前端改动

前端已从快照读取 `dailyPnl` / `dailyPnlPct`，后端修正后前端自动生效。PnL 图表的 `totalValue` 线表示总资产趋势（含现金），这是正确的——转入钱确实增加了总资产，但不会被错误地标记为收益。

## 影响范围

| 组件 | 变化 |
|------|------|
| `run_pnl_snapshot` (lib.rs) | daily_pnl 扣除净转入 |
| `portfolio.rs` | 新增 `get_net_transfer_between` |
| PnL 图表 | 自动修正 |
| PnL 历史表 | 自动修正 |
| totalReturnPct KPI | 不变（已是纯持仓） |

## 验证

```bash
cargo check --manifest-path src-tauri/Cargo.toml
npm run build
```
