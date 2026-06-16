# 现金管理增强 — 转入/转出/微调

## Context

当前 invest 模块的现金管理只有一个"编辑现金"按钮，直接覆写余额（无交易记录、无审计轨迹）。用户需要按照股票账户的操作逻辑，支持：
- **银证转入**：银行 → 证券账户（增加现金）
- **银证转出**：证券账户 → 银行（减少现金）
- **微调修正**：手续费等小额调整（可正可负）

所有操作均记录为交易，保留完整审计轨迹。

---

## 实现方案

### 1. 后端 — TradeAction 枚举扩展

**文件**: `src-tauri/src/storage/invest/portfolio.rs`

- 在 `TradeAction` 枚举中添加 `TransferIn => "transfer_in"` 和 `TransferOut => "transfer_out"`（放在 `CashAdjust` 之后）
- 在 `FromStr` 实现中添加对应 match arm
- 在 `HoldingKind` 枚举中添加 `Cash => "cash"` 及对应 `FromStr`

### 2. 后端 — 现金增量计算

**文件**: `src-tauri/src/storage/invest/portfolio.rs`

更新 `cash_delta_for_trade` 函数：
```rust
TradeAction::TransferIn => amount * sign,   // 转入增加现金
TradeAction::TransferOut => -amount * sign,  // 转出减少现金
```

更新 `recalculate_holdings_inner_body` match 块，将 TransferIn/TransferOut 与 CashAdjust 合并为 no-op：
```rust
TradeAction::CashAdjust | TradeAction::TransferIn | TradeAction::TransferOut => {}
```

### 3. 后端 — DB Schema 迁移

**文件**: `src-tauri/src/storage/invest/mod.rs`

- `CREATE_TABLES_SQL` 中 holdings 和 trades 表的 CHECK 约束添加 `'cash'` kind 和 `'transfer_in'`/`'transfer_out'` action
- `migrate_trades_table` 中 `trades_new` 表同步更新 CHECK 约束
- 更新 `check_is_current` 探测字符串触发重建

### 4. 前端 — 类型定义

**文件**: `src/lib/types.ts`

```typescript
export type TradeAction =
  | 'buy' | 'sell' | 'cost_edit' | 'cash_adjust'
  | 'add_watch' | 'delete_watch' | 'edit_holding'
  | 'transfer_in' | 'transfer_out'
  | 'unknown';
```

### 5. 前端 — TradeDialog 重设计

**文件**: `src/lib/components/invest/TradeDialog.svelte`

- 新增 `cashSubMode` 状态：`'transfer_in' | 'transfer_out' | 'fine_tune'`
- 新增 `cashAmount` 状态（金额输入）
- cash 模式模板改为三子模式选择器 + 金额输入 + 备注
- `handleSubmit` 中 cash 分支改用 `investStore.recordTrade()` 调用
- `canSubmit` 条件改为 `cashAmount !== 0`

### 6. 前端 — TradeLogTab 更新

**文件**: `src/lib/components/invest/TradeLogTab.svelte`

- `SYSTEM_ACTIONS` 添加 `'transfer_in'`, `'transfer_out'`
- 方向下拉框添加转入/转出选项
- 徽章颜色：转入=绿色，转出=红色，微调=中性色
- symbol 为 "CASH" 时显示本地化标签

### 7. 前端 — Store 方法

**文件**: `src/lib/stores/invest-store.svelte.ts`

- `recordTrade()` 已支持新参数，无需修改签名
- `updateCash()` 保留但不再被 TradeDialog 调用
- `nameMap` 添加 "CASH" 符号的本地化名称

### 8. 前端 — Dashboard 按钮

**文件**: `src/routes/invest/+page.svelte`

- 按钮文案改为 `t('invest_cash_management')`

### 9. i18n 国际化

**文件**: `messages/en.json`, `messages/zh-CN.json`

新增 key：
| Key | en | zh-CN |
|-----|-----|-------|
| `invest_cash_management` | Cash Management | 现金管理 |
| `invest_transfer_in` | Transfer In | 银证转入 |
| `invest_transfer_out` | Transfer Out | 银证转出 |
| `invest_fine_tune` | Fine-tune | 微调修正 |
| `invest_transfer_amount` | Transfer Amount | 转账金额 |
| `invest_fine_tune_amount` | Adjustment Amount (+/-) | 调整金额（正为增，负为减） |
| `invest_cash_account` | Cash Account | 现金账户 |
| `invest_transfer_in_desc` | Bank → Stock Account | 银行 → 证券账户 |
| `invest_transfer_out_desc` | Stock Account → Bank | 证券账户 → 银行 |
| `invest_fine_tune_desc` | Fee corrections & adjustments | 费用修正与微调 |
| `invest_trade_filter_transfer_in` | Transfer In | 转入 |
| `invest_trade_filter_transfer_out` | Transfer Out | 转出 |

---

## 实现顺序

1. 后端枚举扩展（TradeAction + HoldingKind）
2. DB Schema 迁移
3. 后端现金增量计算 + 持仓重算
4. 前端类型定义
5. i18n keys
6. TradeDialog 重设计
7. TradeLogTab 更新
8. Store 方法更新
9. Dashboard 按钮更新

---

## 验证方式

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译通过
2. `npm run check` — Svelte 类型检查通过
3. `npm run lint` — ESLint 通过
4. `npm run i18n:check` — i18n key 完整性
5. `npm run build` — 前端构建通过
6. 手动验证：打开现金管理对话框，测试转入/转出/微调三种操作，确认交易记录正确显示
