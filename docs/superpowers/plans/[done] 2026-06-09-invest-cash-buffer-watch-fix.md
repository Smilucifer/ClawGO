# 修复 invest 模块三个 bug + 重构备用金逻辑

## Context

用户报告了 invest 模块的三个问题：
1. **交易操作导致现金变负数 (-100000)**：`set_cash_inner` 的 INSERT 分支在现金行不存在时会把 `initial_balance` 设置为当前 `available` 值，而非初始资本。
2. **User Profile 备用金逻辑不合理**：`emergency_buffer_cny` 是一个独立的固定金额（默认 10 万），应改为由策略配置的 `min_cash_pct`（最小现金比例）动态计算。用户档案仅作为风险偏好设置，风险偏好从已有的账户类型（零花钱、教育金、长线等）推导，注入 prompt。
3. **卖出后多出 watch 项目**：`recalculate_holdings_inner_body` 的 sell 分支在股票全部卖出后自动创建 watch 条目，用户不期望这种行为。

## 修改方案

### Bug 1：修复现金变负数

**文件**：`src-tauri/src/storage/invest/portfolio.rs`

**根因**：`set_cash_inner` (line 537) 的 INSERT SQL 把 `initial_balance` 设为和 `available` 相同的值。当现金行被清除后重建时，`initial_balance` 会被覆盖为错误值。

**修复**：`set_cash_inner` 的 INSERT 分支不设置 `initial_balance`，改为 NULL。新 SQL：
```sql
INSERT INTO cash (id, available, initial_balance, updated_at) VALUES (1, ?1, NULL, ?2)
ON CONFLICT(id) DO UPDATE SET available=?1, updated_at=?2
```
`get_initial_cash_inner` 已用 `COALESCE(initial_balance, 0.0)` 处理 NULL，无需改动。只有 `set_initial_cash` 才会设置 `initial_balance`。

### Bug 2：重构备用金 — 由策略 min_cash_pct 动态计算

**核心思路**：
- **删除** `emergency_buffer_cny` 作为独立配置（从 UserProfile 和 LLM config 两处都删除）
- Buffer 改为 `total_assets * min_cash_pct / 100` 动态计算
- 用户档案的风险偏好从已有的 `account_purpose` 推导，注入 prompt（不新增数值滑块）

**修改文件**：

#### 2a. 后端 — 删除 UserProfile 中的 emergency_buffer_cny

**`src-tauri/src/storage/invest/user_profile.rs`**：
- `UserProfile` 结构体：删除 `emergency_buffer_cny` 字段
- `get_profile()`：从 SELECT 中移除该列
- `save_profile()`：从 INSERT/UPDATE 中移除该列
- 默认值中删除 `emergency_buffer_cny: 100_000.0`

#### 2b. 后端 — 删除 LLM config 中的 emergency_buffer_cny

**`src-tauri/src/commands/invest.rs`**：
- `InvestLlmConfig` 结构体：删除 `emergency_buffer_cny` 字段
- 默认值中删除 `emergency_buffer_cny: 100_000.0`
- `META_KEYS` 数组中删除 `"emergency_buffer_cny"`
- 反序列化/序列化逻辑中删除该字段
- `CommitteeConfig` 构建时不再设置 `emergency_buffer_cny`

#### 2c. 后端 — Gate 3 改为动态计算 buffer

**`src-tauri/src/invest/committee/orchestrator.rs`**：
- `run_committee()` (line 1560-1565)：**删除** user profile 覆盖逻辑
- 在 portfolio_data 加载完成后，从策略计算 buffer：
  ```rust
  let effective_buffer = strategies.first()
      .and_then(|s| s.min_cash_pct)
      .map(|pct| portfolio_data.total_assets() * pct / 100.0)
      .unwrap_or(0.0);  // 无策略或无 min_cash_pct 时 buffer=0，不阻断交易
  ```
- 将 `effective_buffer` 传递给 `cio_sanity_check`

**`src-tauri/src/invest/committee/analysis.rs`**：
- `cio_sanity_check()` 签名不变，`emergency_buffer_cny` 参数改为传入动态计算值

#### 2d. 后端 — 风险偏好从 account_purpose 推导注入 prompt

**`src-tauri/src/invest/committee/orchestrator.rs`**：
- `build_user_profile_context()` 函数：根据 `account_purpose` 推导风险偏好描述，注入 prompt
  - `pocket_money` → "激进型：零花钱账户，可承受高波动，追求短期高收益"
  - `long_term` → "稳健型：长线投资，注重价值和分红"
  - `retirement` → "保守型：养老资金，优先保本"
  - `education` → "保守型：教育金，安全性优先"
  - `default` / `other` → "中性型：默认风险偏好"

#### 2e. 前端 — 删除 Emergency Buffer 输入

**`src/lib/components/invest/UserProfileSection.svelte`**：
- 删除 Emergency Buffer 输入框及相关验证
- 保留：account_purpose, family_support, lifestyle_notes, display_name
- 删除 `emergencyBufferCny` 相关状态

**`src/lib/components/invest/ProviderConfigPanel.svelte`**：
- 删除 Emergency Buffer 输入框

**`src/lib/components/invest/CommitteeLiveTab.svelte`**：
- 删除 `emergencyBuffer` 展示

#### 2f. i18n

**`messages/en.json`** + **`messages/zh-CN.json`**：
- 删除 `settings_profile_emergency_buffer`, `settings_profile_emergency_buffer_desc`, `settings_profile_invalid_buffer`
- 保留 `invest_committee_emergency_buffer`（Provider 配置 fallback 仍可能用到）

### Bug 3：修复卖出后已删除的 watch 复活

**文件**：`src-tauri/src/storage/invest/portfolio.rs`

**根因**：`recalculate_holdings_inner_body` 从头重放所有交易。sell 全部卖出时，自动转 watch 的逻辑**不检查用户是否曾通过 `delete_watch` 明确删除过该票的 watch**，导致已删除的 watch 条目被"复活"。

**修复**：在重放过程中维护一个 `watch_deleted: HashSet<String>` 集合。`delete_watch` 分支将 symbol 加入该集合。sell 分支在自动转 watch 前检查 symbol 是否在 `watch_deleted` 中，如果是则跳过转换。

```rust
// import 补充 HashSet
use std::collections::{HashMap, HashSet};

// 在 map 声明后添加
let mut watch_deleted: HashSet<String> = HashSet::new();

// delete_watch 分支修改为：
"delete_watch" => {
    watch_deleted.insert(t.symbol.clone());  // 记录用户明确删除
    map.remove(&key);
}

// sell 分支的 should_convert 条件增加检查：
let should_convert = if let Some(entry) = map.get_mut(&key) {
    entry.shares = (entry.shares - shares).max(0.0);
    entry.recompute_notional();
    entry.shares <= 0.0001 && !entry.is_watch && !watch_deleted.contains(&t.symbol)
} else {
    false
};
```

## 涉及文件清单

| 文件 | 修改类型 |
|------|----------|
| `src-tauri/src/storage/invest/portfolio.rs` | Bug 1 (set_cash_inner) + Bug 3 (sell 分支) |
| `src-tauri/src/storage/invest/user_profile.rs` | Bug 2a (删除 emergency_buffer_cny) |
| `src-tauri/src/commands/invest.rs` | Bug 2b (删除 LLM config 的 emergency_buffer_cny) |
| `src-tauri/src/invest/committee/orchestrator.rs` | Bug 2c + 2d (动态 buffer + 风险偏好注入) |
| `src-tauri/src/invest/committee/analysis.rs` | 无签名改动，传参变化 |
| `src/lib/components/invest/UserProfileSection.svelte` | Bug 2e (删除 buffer 输入) |
| `src/lib/components/invest/ProviderConfigPanel.svelte` | Bug 2e (删除 buffer 输入) |
| `src/lib/components/invest/CommitteeLiveTab.svelte` | Bug 2e (删除 buffer 展示) |
| `messages/en.json` | Bug 2f (删除 i18n keys) |
| `messages/zh-CN.json` | Bug 2f (删除 i18n keys) |

## 验证

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译通过
2. `npm run check` — Svelte 类型检查通过
3. `npm run lint` — ESLint 通过
4. `npm run build` — 前端构建通过
