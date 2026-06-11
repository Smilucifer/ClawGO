# Invest 交易逻辑全面重构设计

> **审查状态**: 3 路审查通过（Claude Sonnet / MiMo Plan / DeepSeek），12 项发现已合并修正。
> **策略调整**: 拆分为 PR1（bug fix，快速上线）+ PR2（refactor，稳步推进）。

## Context

当前 invest 模块的买入/卖出、持仓/观望转换逻辑经过多次迭代积累了以下技术债：

1. **双路径操作不一致**：`addToWatch` 同时调用 `add_holding`(直接写 DB) + `record_trade`(日志+重算)，导致 holdings 表和 trade 日志短暂不一致。
2. **死代码**：`convert_hold_to_watch` action 后端已实现但前端从未调用（全清仓自动转观望走的是 `sell` 分支隐式逻辑）；`add_holding` 和 `update_holding` 命令完全重复。本次重构一并清除 `convert_hold_to_watch`。
3. **类型安全缺失**：`Trade.kind` 和 `Trade.action` 在前端是 `string` 而非 union type。
4. **`recalculate_holdings_inner_body` 过于庞大**：270+ 行函数处理 6 种 action、做T P&L 追踪、watch 删除防复活、notional 保留。
5. **`edit_holding` 绕过交易日志**：直接写 holdings 不留审计痕迹。
6. **前端 IPC 调用模式不统一**：有的方法调一次 IPC，有的调两次；有的自动 `loadAll()`，有的不调。
7. **Cash 管理逻辑分散**：delta-based 和 full-recalc 两种路径并存，理解成本高。
8. **`convertWatchToHold` 原子性缺陷**：两步 IPC 中第一步成功第二步失败会导致数据丢失（watch 被删但 hold 未创建）。

## 设计目标

- **单一真相源**：所有持仓变更必须通过 `record_trade` 一个入口，消除直接操作 holdings 表的 IPC。
- **原子性**：前端一次操作 = 后端一次事务，消除短暂不一致。
- **类型安全**：Rust 和 TypeScript 两侧使用强类型枚举。
- **可审计**：所有变更（包括 edit_holding）都有交易日志记录。
- **函数拆分**：`recalculate_holdings_inner_body` 按 action 类型拆分为独立函数。
- **向后兼容**：DB schema 不做 breaking change，旧数据正常迁移。

---

## PR1: Bug Fix（快速上线）

### 1.1 新增 `EditHolding` trade action

当用户编辑持仓元数据（成本价、份额、入场日期、备注）时，不再直接 `UPDATE holdings`，而是记录一条 `edit_holding` trade。

**后端** `portfolio.rs` 新增分支：

```rust
"edit_holding" => {
    if let Some(entry) = map.get_mut(&key) {
        // 注意：price/shares 在此 action 中是元数据覆盖值，非交易执行价/数量。
        // amount 必须为 null，cash_delta_for_trade 返回 0。
        if let Some(p) = t.price { entry.avg_cost = p; }
        if let Some(s) = t.shares { entry.shares = s; }
        if let Some(ref d) = t.trade_date { entry.entry_date = Some(d.clone()); }
        if let Some(ref n) = t.notes { entry.notes = Some(n.clone()); }
        entry.recompute_notional();
        // 清除该标的的 P&L 追踪器，避免修改成本基准后做T摊销计算使用过时数据。
        pnl_tracker_map.remove(&t.symbol);
    }
}
```

> **审查修正 [F5]**: 新增 `pnl_tracker_map.remove(&t.symbol)` 清除 P&L 追踪，防止修改成本基准后做T摊销使用过时的 realized_pnl。

> **审查修正 [F4]**: `edit_holding` action 中 `price` 字段语义为"目标成本价"而非"执行价"，`shares` 语义为"目标份额"而非"交易数量"。需在代码注释中明确标注，`amount` 必须为 null。

### 1.2 消除双路径：前端 `addToWatch` / `deleteWatch` / `updateHoldingMeta` 统一为单次 IPC

**前端** `invest-store.svelte.ts`：

```typescript
// addToWatch — 删除 add_holding IPC，只保留 record_trade
async addToWatch(symbol, name, price, assetType?) {
  await invoke("record_trade", {
    id: null, symbol, currency: "CNY", kind: "watch",
    action: "add_watch", shares: null, price, amount: 0,
    name, assetType: assetType ?? "stock",
  });
  await this.loadAll();
  if (price > 0) { /* 预填价格缓存 */ }
}

// deleteWatch — 删除 delete_holding IPC，只保留 record_trade
async deleteWatch(symbol) {
  await invoke("record_trade", {
    id: null, symbol, currency: "CNY", kind: "watch",
    action: "delete_watch", shares: null, price: null, amount: 0,
  });
  await this.loadAll();
}

// updateHoldingMeta — 改为走 record_trade(action="edit_holding")
async updateHoldingMeta(params) {
  await invoke("record_trade", {
    id: null, symbol: params.symbol, currency: params.currency,
    kind: params.kind, action: "edit_holding",
    shares: params.shares, price: params.avgCost,
    amount: null, notes: params.notes,
    tradeDate: params.entryDate, assetType: params.assetType,
  });
  await this.loadAll();
}
```

### 1.3 新增原子化 `convertWatchToHold` 后端命令

> **审查修正 [F1]**: 两步 IPC 存在原子性缺陷。新增后端命令在单个事务中完成。

**后端** `commands/invest.rs` 新增：

```rust
#[tauri::command]
pub fn convert_watch_to_hold(
    symbol: String,
    name: Option<String>,
    shares: f64,
    price: f64,
    asset_type: Option<String>,
) -> Result<(), String> {
    portfolio::convert_watch_to_hold(&symbol, name, shares, price, asset_type)
}
```

**后端** `portfolio.rs` 新增：

```rust
pub fn convert_watch_to_hold(
    symbol: &str,
    name: Option<String>,
    shares: f64,
    price: f64,
    asset_type: Option<String>,
) -> Result<(), String> {
    with_conn(|conn| {
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        // 在同一个 with_conn 事务中写入两条 trade
        let delete_trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            currency: "CNY".to_string(),
            kind: "watch".to_string(),
            action: "delete_watch".to_string(),
            shares: None, price: None, amount: Some(0.0),
            notes: None, created_at: now.clone(), name: None,
            trade_date: None, asset_type: None,
        };
        let buy_trade = Trade {
            id: uuid::Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            currency: "CNY".to_string(),
            kind: "hold".to_string(),
            action: "buy".to_string(),
            shares: Some(shares), price: Some(price),
            amount: Some(shares * price),
            notes: None, created_at: now, name,
            trade_date: None,
            asset_type: asset_type.or_else(|| Some("stock".to_string())),
        };
        // 两条 INSERT + 两次 cash delta + 一次 recalculate，全部在同一事务中
        conn.execute("INSERT INTO trades ...", ...);  // delete_trade
        apply_cash_delta_sql(conn, cash_delta_for_trade(&delete_trade, false))?;
        conn.execute("INSERT INTO trades ...", ...);  // buy_trade
        apply_cash_delta_sql(conn, cash_delta_for_trade(&buy_trade, false))?;
        recalculate_holdings_inner(conn, false)?;
        Ok(())
    })
}
```

**前端** `invest-store.svelte.ts` 简化为单次 IPC：

```typescript
async convertWatchToHold(symbol, name, qty, price) {
  const watchHolding = this.watchHoldings.find(h => h.symbol === symbol);
  await invoke("convert_watch_to_hold", {
    symbol, name: name || null, shares: qty, price,
    assetType: watchHolding?.assetType ?? "stock",
  });
  await this.loadAll();
}
```

### 1.4 DB Migration：复用现有 `migrate_trades_table`

> **审查修正 [F6]**: 不引入新的 migration 路径，复用 `mod.rs` 中已有的 `migrate_trades_table` 基础设施。

在 `migrate_trades_table` 函数中扩展 CHECK 约束：

```sql
CHECK (action IN ('buy', 'sell',
    'cost_edit', 'cash_adjust', 'add_watch', 'delete_watch', 'edit_holding'))
```

同时更新 `CREATE_TABLES_SQL`（mod.rs）中的 CHECK 约束，确保新建数据库包含 `edit_holding`。

**清除 `convert_hold_to_watch` 死代码**：
- `migrate_trades_table` 中已有 `convert_watch_to_hold → buy` 的转换逻辑（mod.rs:212）。
- 新增 `convert_hold_to_watch → cost_edit` 的转换（该 action 前端从未创建，但为安全起见做转换而非删除）。
- `recalculate_holdings_inner_body` 中删除 `convert_hold_to_watch` 分支（portfolio.rs:459-469）。
- 旧 DB 中若存在此类记录，migration 会将其转为 `cost_edit`，replay 时不会报错。

### 1.5 i18n 和 UI 传递

> **审查修正 [F7]**: 新增按钮需要 i18n key 和 `+page.svelte` prop 传递。

新增 i18n key：
- `invest_convert_to_watch` — "转为观望" / "Convert to Watch"
- `invest_delete_watch` — "删除观望" / "Remove from Watch"

HoldingsTable 新增 prop：
```typescript
let { onBuy, onSell, onAddWatch, onEdit, onConvertToWatch, onDeleteWatch, tushareToken } = $props();
```

`+page.svelte` 需传递 `onConvertToWatch` 和 `onDeleteWatch` 回调。

HoldingsTable 行动按钮增强：
- **HOLD 行**：Edit | Buy | Sell | 转观望
- **WATCH 行**：Edit | 买入建仓 | 删除

### 1.6 状态图补充

> **审查修正 [F10]**: 补充 WATCH 直接买入路径。

```
[WATCH] ──buy (direct)──→ [HOLD + WATCH 共存] (前端 watchHoldings 去重隐藏 watch)
```

---

## PR2: Refactor（稳步推进）

### 2.1 Trade Action 枚举化

> **审查修正 [F12]**: 枚举需包含 `ConvertHoldToWatch`。

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeAction {
    Buy,
    Sell,
    CostEdit,
    CashAdjust,
    AddWatch,
    DeleteWatch,
    EditHolding,
    // convert_hold_to_watch 已清除 — 前端从未使用，全清仓转观望走 sell 分支隐式逻辑
}

impl TradeAction {
    pub fn as_str(&self) -> &'static str { ... }
    pub fn from_str(s: &str) -> Result<Self, String> { ... }
    pub fn affects_cash(&self) -> bool {
        matches!(self, Self::Buy | Self::Sell | Self::CashAdjust)
    }
    pub fn affects_holdings(&self) -> bool {
        !matches!(self, Self::CashAdjust)
    }
}
```

同理 `HoldingKind`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingKind {
    Hold,
    Watch,
}
```

**TypeScript** `types.ts`：

```typescript
export type TradeAction =
  | 'buy' | 'sell' | 'cost_edit' | 'cash_adjust'
  | 'add_watch' | 'delete_watch' | 'edit_holding';
// convert_hold_to_watch 已清除

// Holding.kind 已经是 "hold" | "watch"，无需额外定义 HoldingKind 类型。
// Trade.kind 改为 "hold" | "watch" 联合类型。
```

> **审查修正 [Claude F1.3]**: TypeScript 侧 `Holding.kind` 已是 `"hold" | "watch"`，无需重复定义 `HoldingKind` 类型。只对 `Trade.kind` 做类型收紧。

### 2.2 `recalculate_holdings_inner_body` 函数拆分

> **审查修正 [Claude F2.2]**: 使用 `RecalcContext` 结构体封装共享可变状态。

```rust
/// 持仓重算的共享上下文，避免函数签名过长。
struct RecalcContext<'a> {
    map: &'a mut HashMap<(String, String, String), MemHolding>,
    pnl_tracker: &'a mut HashMap<String, PnlTracker>,
    watch_deleted: &'a mut HashSet<String>,
}

fn process_buy(ctx: &mut RecalcContext, t: &Trade) { ... }
fn process_sell(ctx: &mut RecalcContext, t: &Trade) { ... }  // 返回是否 auto-convert
fn process_cost_edit(entry: &mut MemHolding, t: &Trade) { ... }
fn process_add_watch(entry: &mut MemHolding, t: &Trade) { ... }
fn process_delete_watch(ctx: &mut RecalcContext, key: &(String, String, String), t: &Trade) { ... }
fn process_edit_holding(ctx: &mut RecalcContext, t: &Trade) { ... }
```

主循环简化为：

```rust
for t in &trades {
    let key = (t.symbol.clone(), t.currency.clone(), t.kind.clone());
    match t.action.as_str() {
        "buy" => process_buy(&mut ctx, t),
        "sell" => process_sell(&mut ctx, t),
        "cost_edit" => process_cost_edit(ctx.map.entry(key).or_default(), t),
        "add_watch" => process_add_watch(ctx.map.entry(key).or_default(), t),
        "delete_watch" => process_delete_watch(&mut ctx, &key, t),
        "edit_holding" => process_edit_holding(&mut ctx, t),
        _ => {}  // convert_hold_to_watch 已清除，旧数据中的此类 trade 被忽略
    }
}
```

> **审查修正 [Claude F4.3]**: `PnlTracker` 和 `is_pnl_expired` 逻辑内聚到 `process_buy` 函数作用域内，不散落在主循环中。

### 2.3 Cash 管理调整

> **审查修正 [DeepSeek F9]**: 保留 `recalculate_cash_inner` 作为恢复工具，不删除。

```rust
fn recalculate_holdings_inner(conn: &Connection) -> Result<(), String> {
    // 正常路径不触碰 cash，cash 变更由 record_trade/delete_trade/update_trade 的 delta 处理
    // recalculate_cash_inner 保留为 pub(crate) 恢复工具
    ...
}

pub fn recalculate_holdings() -> Result<(), String> {
    // 保留公开接口，但内部只重算 holdings
    with_conn(|conn| recalculate_holdings_inner(conn))
}

/// 恢复工具：从 initial_balance + trade history 全量重算 cash。
/// 正常流程不调用此函数，仅用于数据修复场景。
pub(crate) fn recalculate_cash_full() -> Result<(), String> {
    with_conn(|conn| {
        let trades = /* load all trades */;
        recalculate_cash_inner(conn, &trades)
    })
}
```

> **审查修正 [MiMo F1]**: 公开 `recalculate_holdings()` 的调用者需审计确认。当前仅 `portfolio.rs` 内部定义，无外部调用者依赖其修复 cash。文档化此不变量。

### 2.4 后端 IPC 命令精简

> **审查修正 [MiMo F2, Claude F4.5]**: 需处理 `orchestrator.rs` 的 `upsert_holding` 调用和 `lib.rs` 注册。

**标记 deprecated（不删除）**：
- `add_holding` — 被 `record_trade(action="add_watch")` 替代
- `update_holding` — 被 `record_trade(action="edit_holding")` 替代

**orchestrator.rs 的 `upsert_holding` 调用**：
- `orchestrator.rs:234` 的 `upsert_holding` 仅更新 notional（显示用市值），不代表用户交易操作。
- 将其收窄为 `update_holding_notional(symbol, notional)` 函数，只能修改 notional 字段，不能误改其他字段。
- 这是"单一入口"原则的明确例外，需在代码注释中说明。

**migrate_legacy_portfolio**：
- 保留 `upsert_holding` 调用不变。Legacy migration 仅运行一次，不值得改为 record_trade。

**lib.rs**：
- `add_holding` 和 `update_holding` 的 Tauri handler 注册保留（因为命令标记 deprecated 而非删除）。

**保留的命令**：
- `get_holdings` — 查询
- `delete_holding` — 标记 `#[deprecated]`，仅用于数据修复
- `record_trade` — 唯一写入入口
- `convert_watch_to_hold` — PR1 新增的原子化命令
- `get_trades`, `delete_trade`, `update_trade` — 交易日志管理
- `get_cash`, `get_initial_cash`, `set_initial_cash`, `update_cash` — 现金管理

### 2.5 TradeDialog edit_holding 后端迁移

> **审查修正 [DeepSeek F2, Claude F2.5]**: 原"模式精简"标题不准确，改为准确描述。

TradeDialog 的 8 种 mode 数量不变。唯一变化：`edit_holding` 模式的提交处理器从 `invoke("update_holding", ...)` 改为 `invoke("record_trade", { action: "edit_holding", ... })`。

### 2.6 前端类型强化

**文件**: `src/lib/types.ts`

```typescript
export type TradeAction =
  | 'buy' | 'sell' | 'cost_edit' | 'cash_adjust'
  | 'add_watch' | 'delete_watch' | 'edit_holding';

export interface Trade {
  id: string;
  symbol: string;
  currency: string;
  kind: 'hold' | 'watch';     // ← 从 string 改为联合类型
  action: TradeAction;          // ← 从 string 改为联合类型
  shares: number | null;
  price: number | null;
  amount: number | null;
  notes: string | null;
  createdAt: string;
  name: string | null;
  tradeDate: string | null;
  assetType: string | null;
}
```

---

## 不受影响的模块

> **审查修正 [MiMo F6]**: 明确声明。

- **Committee orchestrator**: 调用 `portfolio::list_holdings()` 和 `portfolio::upsert_holding()` Rust 函数（非 IPC），不受 IPC 命令变更影响。`upsert_holding` 调用在 PR2 中收窄为 `update_holding_notional`。
- **Event scanner**: 调用 `portfolio::list_holdings()`，不受影响。
- **init_invest_data**: 仅清除表数据行，不涉及 schema 变更。`migrate_trades_table` 在启动时自动处理 CHECK 约束更新。
- **delete_trade / update_trade**: 通过现有 delta + replay 机制正确处理 `edit_holding` trade。`cash_delta_for_trade` 对 `edit_holding` 返回 0，replay 时跳过该 trade 即恢复到编辑前状态。

---

## 并发安全

> **审查修正 [MiMo F6]**: 文档化。

全局 `Mutex<Option<Connection>>` 序列化所有 `record_trade` 调用。Full-recalculate 从零重播所有 trade 是确定性的，因此并发安全。无需额外锁。

---

## edit_holding 对交易日志的影响

> **审查修正 [DeepSeek F5]**: 元数据编辑会生成 trade 日志条目。

`edit_holding` 条目在交易日志中会显示为系统操作（与 `cost_edit`、`add_watch` 等同级）。`TradeLogTab` 已有"显示系统操作"开关，`edit_holding` 条目默认隐藏在该开关下，不会干扰正常交易记录的查看。

---

## 实现步骤

### PR1: Bug Fix（~30 行后端 + ~15 行前端 + migration）
1. 后端: `recalculate_holdings_inner_body` 新增 `edit_holding` 分支（含 PnlTracker 清除）
2. 后端: `recalculate_holdings_inner_body` 删除 `convert_hold_to_watch` 死代码分支
3. 后端: 新增 `convert_watch_to_hold` 原子化命令
4. 后端: `migrate_trades_table` 扩展 CHECK 约束（移除 `convert_hold_to_watch`，新增 `edit_holding`）+ 旧记录转换 + `CREATE_TABLES_SQL` 更新
5. 前端: `addToWatch` 改为单次 `record_trade` IPC
6. 前端: `deleteWatch` 改为单次 `record_trade` IPC
7. 前端: `updateHoldingMeta` 改为 `record_trade(action="edit_holding")`
8. 前端: `convertWatchToHold` 改为单次 `convert_watch_to_hold` IPC
9. 前端: HoldingsTable 新增「转观望」「删除」按钮 + i18n key
10. 验证: `cargo check` + `npm run build` + `npm run i18n:check` + 手动测试

### PR2: Refactor（~200 行后端 + ~50 行前端）
1. Rust: 添加 `TradeAction` 和 `HoldingKind` 枚举
2. Rust: `Trade` 和 `Holding` 结构体改用枚举类型
3. Rust: `cash_delta_for_trade` 改用枚举 match
4. Rust: `recalculate_holdings_inner_body` 拆分为 `process_*` 函数（`RecalcContext` 模式）
5. Rust: `recalculate_cash_inner` 保留为 `pub(crate)` 恢复工具
6. Rust: orchestrator `upsert_holding` 收窄为 `update_holding_notional`
7. Rust: `add_holding`/`update_holding` 标记 `#[deprecated]`
8. TypeScript: `types.ts` 添加 `TradeAction` 联合类型，收紧 `Trade.kind`
9. 验证: `cargo check` + `npm run check` + `npm run verify`

---

## 关键文件清单

| 文件 | PR | 变更类型 | 说明 |
|------|-----|---------|------|
| `src-tauri/src/storage/invest/portfolio.rs` | 1+2 | 重构 | edit_holding 分支 + convert_watch_to_hold + 清除 convert_hold_to_watch 死代码 + 枚举化 + 函数拆分 |
| `src-tauri/src/commands/invest.rs` | 1+2 | 精简 | 新增 convert_watch_to_hold 命令 + deprecated 标记 |
| `src-tauri/src/storage/invest/mod.rs` | 1 | 迁移 | CHECK 约束扩展（-convert_hold_to_watch +edit_holding）+ 旧记录转换 + CREATE_TABLES_SQL 更新 |
| `src-tauri/src/group_chat/orchestrator.rs` | 2 | 收窄 | upsert_holding → update_holding_notional |
| `src/lib/types.ts` | 2 | 增强 | TradeAction 联合类型 + Trade.kind 收紧 |
| `src/lib/stores/invest-store.svelte.ts` | 1 | 重构 | 统一 IPC 模式，消除双路径 |
| `src/lib/components/invest/TradeDialog.svelte` | 1 | 适配 | edit_holding 走 record_trade |
| `src/lib/components/invest/HoldingsTable.svelte` | 1 | 增强 | 转观望/删除按钮 |
| `src/routes/invest/+page.svelte` | 1 | 适配 | 传递新 prop |
| `messages/en.json` | 1 | i18n | 新增 2 key |
| `messages/zh-CN.json` | 1 | i18n | 新增 2 key |

---

## 风险评估与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| 旧 DB 数据不兼容 | 高 | `migrate_trades_table` 保留旧 action 值，CHECK 约束只增不删 |
| `recalculate_holdings_inner` 重构引入回归 | 高 | PR1 先上线验证，PR2 再拆分函数 |
| 前端 `add_holding` IPC 残留调用 | 中 | deprecated 标记 + 编译时类型检查 |
| `edit_holding` 新 action 旧版本不识别 | 低 | `_ => {}` 分支兜底 |
| `convertWatchToHold` 原子化命令 | 低 | 单事务，失败自动回滚 |
| P&L 追踪器与 edit_holding 交互 | 中 | edit_holding 时清除该标的的 pnl_tracker |
| orchestrator 的 upsert_holding 例外 | 低 | 收窄为 update_holding_notional，注释说明 |

---

## 验证方案

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译通过
2. `npm run check` — Svelte 类型检查通过
3. `npm run build` — 前端构建通过
4. `npm run i18n:check` — i18n 检查通过
5. 手动测试流程：
   - 添加观望 → 买入建仓 → 加仓 → 减仓 → 全部卖出(自动转观望) → 删除观望
   - 观望转持仓（convert 模式）→ 验证原子性
   - 编辑持仓元数据 → 检查 trade 日志有 edit_holding 记录
   - 手动补录交易 → 检查 cash 和 holdings 正确
   - 删除交易 → 检查 cash 和 holdings 回滚正确
   - 直接买入已观望标的 → 验证 hold/watch 共存 + 前端去重
