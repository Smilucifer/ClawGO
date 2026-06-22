# 清仓延迟转换 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 清仓当日保持 Hold 状态（shares=0），第二天 5:00 后自动转为 Watch，确保当日卖出、当日盈亏、当日盈亏比计算正常。

**Architecture:** 在 `holdings` 表新增 `cleared_date` 字段。清仓时不再立即转 Watch，而是标记 `cleared_date = today`。trade replay 的写回步骤中，检查 `cleared_date < today` 的过期清仓持仓并转为 Watch。前端 dailyPnl 对清仓当日持仓使用开盘股数计算。

**Tech Stack:** Rust (rusqlite, chrono), Svelte 5 (runes), TypeScript

## Global Constraints

- Windows-first，不引入 Unix-only API
- 使用 `cargo check` 验证 Rust 编译（本机 runtime 有 STATUS_ENTRYPOINT_NOT_FOUND 问题）
- 遵循 Conventional Commits (`feat:`, `fix:`)
- `getInvestDate()` 和 `get_invest_date()` 使用 05:00 cutoff 对齐业务日
- 数据库 migration 使用 `has_column` + `ALTER TABLE ADD COLUMN` 模式

---

## File Structure

| 文件 | 职责 | 操作 |
|------|------|------|
| `src-tauri/src/storage/invest/mod.rs` | DB schema, migration | Modify: 新增 `cleared_date` 列 migration |
| `src-tauri/src/storage/invest/portfolio.rs` | 核心: Holding/MemHolding/process_sell/process_buy/write-back/list_holdings | Modify: 全部改动 |
| `src-tauri/src/invest/scheduler/mod.rs` | 定时任务定义 | Modify: 新增 `clearance_convert` job |
| `src-tauri/src/invest/scheduler/runner.rs` | 定时任务 dispatch | Modify: 新增 dispatch 分支 |
| `src/lib/types.ts` | TS 类型定义 | Modify: Holding 新增 `clearedDate` |
| `src/lib/stores/invest-store.svelte.ts` | 前端状态 + dailyPnl 计算 | Modify: dailyPnl/dailyPnlPct/holdingsMarketValue |
| `src/lib/components/invest/HoldingsTable.svelte` | 持仓表格显示 | Modify: 清仓标记 + 日内盈亏 |

---

### Task 1: 数据库 Schema — 新增 `cleared_date` 列

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs:434-449` (CREATE_TABLES_SQL)
- Modify: `src-tauri/src/storage/invest/mod.rs:279-283` (migration block, 在 asset_type migration 之后)

**Interfaces:**
- Produces: `holdings` 表新增 `cleared_date TEXT` 列（nullable）

- [ ] **Step 1: 在 CREATE_TABLES_SQL 中添加 cleared_date 列**

在 `src-tauri/src/storage/invest/mod.rs` 的 `CREATE_TABLES_SQL` 中，在 `asset_type` 行之后添加：

```sql
cleared_date TEXT,
```

完整 holdings 表定义变为：
```sql
CREATE TABLE IF NOT EXISTS holdings (
    symbol TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'CNY',
    kind TEXT NOT NULL CHECK (kind IN ('hold', 'watch', 'cash')),
    name TEXT,
    notional REAL NOT NULL DEFAULT 0,
    avg_cost REAL,
    shares REAL,
    entry_date TEXT,
    linked_verdict_id TEXT,
    notes TEXT,
    asset_type TEXT NOT NULL DEFAULT 'stock',
    cleared_date TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (symbol, currency, kind)
);
```

- [ ] **Step 2: 添加 migration 逻辑**

在 `src-tauri/src/storage/invest/mod.rs` 的 `init_db_inner` 函数中，在 asset_type migration 块（约 line 280-283）之后添加：

```rust
// Migration: add cleared_date column to holdings table if missing
if !has_column(&conn, "holdings", "cleared_date") {
    conn.execute_batch("ALTER TABLE holdings ADD COLUMN cleared_date TEXT;")
        .map_err(|e| format!("Failed to add cleared_date column: {}", e))?;
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过，无 error

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): add cleared_date column to holdings table"
```

---

### Task 2: Rust 结构体 — Holding 和 MemHolding 新增字段

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs:129-157` (Holding struct + impl)
- Modify: `src-tauri/src/storage/invest/portfolio.rs:559-585` (MemHolding struct + impl)

**Interfaces:**
- Produces: `Holding.cleared_date: Option<String>`, `MemHolding.cleared_date: Option<String>`

- [ ] **Step 1: Holding 结构体新增 cleared_date 字段**

在 `src-tauri/src/storage/invest/portfolio.rs` 的 `Holding` struct 中，在 `asset_type` 字段之后添加：

```rust
/// 清仓日期 (YYYY-MM-DD)。当日清仓的持仓保持 Hold 状态直到次日 05:00。
#[serde(default)]
pub cleared_date: Option<String>,
```

- [ ] **Step 2: MemHolding 结构体新增 cleared_date 字段**

在 `MemHolding` struct 中，在 `asset_type` 字段之后添加：

```rust
cleared_date: Option<String>,
```

- [ ] **Step 3: 更新 copy_core_fields_from 方法**

在 `MemHolding::copy_core_fields_from` 中添加 `cleared_date` 的复制：

```rust
fn copy_core_fields_from(&mut self, src: &MemHolding) {
    self.name = src.name.clone();
    self.avg_cost = src.avg_cost;
    self.notional = src.notional;
    self.entry_date = src.entry_date.clone();
    self.asset_type = src.asset_type.clone();
    // NOTE: 不复制 cleared_date — 转换时由调用方决定
}
```

注意：`copy_core_fields_from` 用于 hold↔watch 转换。转换到 Watch 时不需要保留 `cleared_date`，所以不复制。

- [ ] **Step 4: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过（可能有 unused field 警告，后续 task 会使用）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): add cleared_date field to Holding and MemHolding"
```

---

### Task 3: 修改 process_sell — 清仓时不转 Watch

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs:646-686` (process_sell function)

**Interfaces:**
- Consumes: `RecalcContext`, `Trade`
- Produces: 清仓后 MemHolding 保持 Hold + 设置 `cleared_date`

- [ ] **Step 1: 重写 process_sell 函数**

将 `process_sell` 函数（line 646-686）替换为：

```rust
fn process_sell(ctx: &mut RecalcContext, key: (String, String, String), t: &Trade) {
    let shares = t.shares.unwrap_or(0.0);
    let sell_price = t.price.unwrap_or(0.0);
    if let Some(entry) = ctx.map.get_mut(&key) {
        // Calculate realized P&L for this sell trade
        // realized_pnl = shares_sold * (sell_price - avg_cost)
        let is_cleared_before = entry.shares <= shares + 0.0001; // will be cleared after this sell
        if entry.shares > 0.0 && entry.avg_cost > 0.0 {
            let pnl = shares * (sell_price - entry.avg_cost);
            let tracker = ctx.pnl_tracker.entry(t.symbol.clone()).or_insert(PnlTracker {
                realized_pnl: 0.0,
                cleared_date: None,
            });
            tracker.realized_pnl += pnl;
            // 清仓时同步 cleared_date 到 pnl_tracker（用于做T成本调整）
            if is_cleared_before && !entry.is_watch && !ctx.watch_deleted.contains(&t.symbol) {
                let trade_date = t.created_at[..10].to_string();
                tracker.cleared_date = Some(trade_date);
            }
        }
        entry.shares = (entry.shares - shares).max(0.0);
        entry.recompute_notional();
        // 清仓：标记 cleared_date，但保持 Hold 状态
        // 第二天 05:00 后由 write-back 步骤或定时任务转换为 Watch
        let is_cleared = entry.shares <= 0.0001
            && !entry.is_watch
            && !ctx.watch_deleted.contains(&t.symbol);
        if is_cleared {
            let trade_date = t.created_at[..10].to_string();
            entry.cleared_date = Some(trade_date);
        }
    }
}
```

关键变化：
- 移除了 `should_convert` 标志和后续的 hold→watch 转换逻辑
- 清仓时设置 `entry.cleared_date = Some(trade_date)`
- 保持 entry 在 map 中作为 Hold（shares=0）
- 合并了 pnl_tracker 的两次 `entry()` 调用为一次（先计算 PnL，再设置 cleared_date）

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): keep cleared positions as Hold with cleared_date"
```

---

### Task 4: 修改 process_buy — 回购时清除 cleared_date

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs:596-644` (process_buy function)

**Interfaces:**
- Consumes: `RecalcContext`, `Trade`
- Produces: 回购后 MemHolding 的 `cleared_date` 被清除

- [ ] **Step 1: 在 process_buy 中清除 cleared_date**

在 `process_buy` 函数中，在 `entry.shares = new_shares;` 行（约 line 640）之后添加：

```rust
    entry.shares = new_shares;
    // 回购清除 cleared_date（重新建仓）
    if entry.shares > 0.0001 {
        entry.cleared_date = None;
    }
    // Compute notional as cost basis (avg_cost * shares).
    // Will be updated to current market value in PortfolioData::load.
    entry.recompute_notional();
```

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): clear cleared_date on buy-back"
```

---

### Task 5: 修改写回逻辑 — 过期清仓持仓转 Watch

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs:813-849` (write-back step in recalculate_holdings_inner_body)

**Interfaces:**
- Consumes: `MemHolding` map 中的 `cleared_date` 字段
- Produces: 过期的清仓持仓转为 Watch 写入 DB；当日清仓的保持 Hold

- [ ] **Step 1: 重写 write-back 逻辑**

将 `recalculate_holdings_inner_body` 的 step 5（line 813-849）替换为：

```rust
    // 5. Post-process: convert stale cleared positions to Watch,
    //    then write rebuilt holdings to database
    let today = crate::invest::date_utils::get_invest_date();
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Collect keys that need hold→watch conversion (cleared before today)
    let mut conversions: Vec<(String, String, String)> = Vec::new();
    for ((symbol, _currency, kind), h) in map.iter() {
        if kind == "hold" && h.shares <= 0.0001 {
            if let Some(ref cd) = h.cleared_date {
                if cd < &today && !watch_deleted.contains(symbol) {
                    conversions.push((symbol.clone(), _currency.clone(), kind.clone()));
                }
            }
        }
    }

    // Execute conversions: remove from hold key, insert at watch key
    // 注意：如果 watch key 已存在（用户手动 add_watch），保留原有 watch entry
    for (symbol, currency, _kind) in conversions {
        let hold_key = (symbol.clone(), currency.clone(), HoldingKind::Hold.to_string());
        let watch_key = (symbol.clone(), currency.clone(), HoldingKind::Watch.to_string());
        if let Some(hold_entry) = map.remove(&hold_key) {
            if !map.contains_key(&watch_key) {
                // 不存在已有 watch entry，创建新的
                let watch_entry = map.entry(watch_key).or_default();
                watch_entry.copy_core_fields_from(&hold_entry);
                watch_entry.is_watch = true;
                watch_entry.cleared_date = None;
            }
            // 如果已有 watch entry，仅移除 hold entry（不做覆盖）
        }
    }

    // Write all entries to DB
    for ((symbol, currency, kind), h) in &map {
        // Skip zero-share holdings (but preserve watch items and cleared-today holds)
        if h.shares <= 0.0001 && !h.is_watch {
            // Allow cleared-today holds (shares=0 but cleared_date == today)
            let is_cleared_today = h.cleared_date.as_ref().map_or(false, |cd| cd == &today);
            if !is_cleared_today {
                continue;
            }
        }
        // Watch items have no shares; write null to DB
        let shares_val: Option<f64> = if h.is_watch { None } else { Some(h.shares) };
        // Preserve original created_at if available, otherwise use now
        let created_at = created_at_map
            .get(&(symbol.clone(), currency.clone(), kind.clone()))
            .cloned()
            .unwrap_or_else(|| now.clone());
        conn.execute(
            "INSERT INTO holdings (symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, asset_type, cleared_date, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                symbol,
                currency,
                kind,
                h.name,
                h.notional,
                h.avg_cost,
                shares_val,
                h.entry_date,
                h.linked_verdict_id,
                h.notes,
                h.asset_type,
                h.cleared_date,
                created_at,
                now,
            ],
        )
        .map_err(|e| format!("insert rebuilt holding: {}", e))?;
    }

    Ok(())
```

关键变化：
- 写回前检查 `cleared_date < today` 的 entries 并转为 Watch
- `cleared_date == today` 的 entries 保持 Hold，写入 DB（shares=0）
- INSERT 语句新增 `cleared_date` 参数

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): convert stale cleared positions to Watch in write-back"
```

---

### Task 6: 更新 list_holdings — SELECT 包含 cleared_date

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs:202-249` (list_holdings function)

**Interfaces:**
- Produces: `Holding.cleared_date` 从 DB 读取

- [ ] **Step 1: 更新 list_holdings 的 SELECT 和 row 映射**

将 `list_holdings` 函数中的 SELECT 语句和 row 映射更新：

```rust
pub fn list_holdings() -> Result<Vec<Holding>, String> {
    with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT symbol, currency, kind, name, notional, avg_cost, shares, entry_date, linked_verdict_id, notes, asset_type, cleared_date, created_at, updated_at FROM holdings ORDER BY symbol")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Holding {
                    symbol: row.get(0)?,
                    currency: row.get(1)?,
                    kind: row.get(2)?,
                    name: row.get(3)?,
                    notional: row.get(4)?,
                    avg_cost: row.get(5)?,
                    shares: row.get(6)?,
                    frozen_shares: None,
                    entry_date: row.get(7)?,
                    linked_verdict_id: row.get(8)?,
                    notes: row.get(9)?,
                    asset_type: row.get(10)?,
                    cleared_date: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|e| format!("query: {}", e))?;
        // ... rest of the function unchanged
```

**frozen_shares 交互说明**：清仓当日的 Hold entry（shares=0）也会进入 frozen_shares 计算分支。由于 `capped = f.min(h.shares.unwrap_or(0.0))` 会将 frozen 限制为 0，所以不会产生异常值。这是正确的行为，无需额外处理。

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): include cleared_date in list_holdings SELECT"
```

---

### Task 7: 添加 expire_cleared_positions 公开函数

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs` (在 `recalculate_holdings` 函数之后)

**Interfaces:**
- Produces: `pub fn expire_cleared_positions() -> Result<String, String>`

- [ ] **Step 1: 添加 expire_cleared_positions 函数**

在 `recalculate_holdings` 函数之后添加：

```rust
/// 将昨日清仓的持仓转为关注。
/// 由定时任务 "clearance_convert" 在每天 05:00 调用。
/// 内部调用 recalculate_holdings 触发完整的 trade replay，
/// 其 write-back 步骤会自动将 cleared_date < today 的 entries 转为 Watch。
pub fn expire_cleared_positions() -> Result<String, String> {
    recalculate_holdings()?;
    Ok("Cleared positions expired successfully".to_string())
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): add expire_cleared_positions function"
```

---

### Task 8: 添加 clearance_convert 定时任务

**Files:**
- Modify: `src-tauri/src/invest/scheduler/mod.rs:31-138` (default_jobs)
- Modify: `src-tauri/src/invest/scheduler/runner.rs:48-119` (dispatch_job)

**Interfaces:**
- Produces: 新 CronJob "clearance_convert"，每天 05:00 执行

- [ ] **Step 1: 在 default_jobs 中添加 clearance_convert**

在 `src-tauri/src/invest/scheduler/mod.rs` 的 `default_jobs` 函数中，在 `macro_refresh` job 之后添加：

```rust
        CronJob {
            id: "clearance_convert".into(),
            name: "清仓过期转换".into(),
            cron_expr: "0 0 5 * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false, // 非交易日也需要执行，确保周末也能转换
            last_run: None,
            next_run: None,
            last_status: None,
            description: "将昨日清仓的持仓转为关注".into(),
            dedicated: false,
        },
```

**幂等性说明**：此任务是幂等的。如果 app 在 05:00 未运行，下次启动时 scheduler 会补执行。此时 `cleared_date < today` 仍然成立（因为日期只会前进），所以转换逻辑正确。如果用户在补执行之前又买了同一支股票（做T），`process_buy` 已经清除了 `cleared_date`，不会误转换。

- [ ] **Step 2: 在 dispatch_job 中添加 dispatch 分支**

在 `src-tauri/src/invest/scheduler/runner.rs` 的 `dispatch_job` 函数中，在 `"macro_refresh"` 分支之后添加：

```rust
        "clearance_convert" => {
            let result = crate::storage::invest::portfolio::expire_cleared_positions()?;
            Ok(result)
        }
```

- [ ] **Step 3: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/invest/scheduler/mod.rs src-tauri/src/invest/scheduler/runner.rs
git commit -m "feat(invest): add clearance_convert scheduled job at 05:00"
```

---

### Task 9: 前端 — Holding 类型新增 clearedDate

**Files:**
- Modify: `src/lib/types.ts:1741-1756` (Holding interface)

**Interfaces:**
- Produces: `Holding.clearedDate?: string | null`

- [ ] **Step 1: 更新 Holding interface**

在 `src/lib/types.ts` 的 `Holding` interface 中，在 `assetType` 字段之后添加：

```typescript
  clearedDate?: string | null;
```

完整 interface：
```typescript
export interface Holding {
  symbol: string;
  currency: string;
  kind: "hold" | "watch";
  name: string | null;
  notional: number;
  avgCost: number | null;
  shares: number | null;
  frozenShares?: number | null;
  entryDate: string | null;
  linkedVerdictId: string | null;
  notes: string | null;
  assetType: string | null;
  clearedDate?: string | null;
  createdAt: string;
  updatedAt: string;
}
```

- [ ] **Step 2: 验证 TypeScript**

Run: `npm run check`
Expected: 类型检查通过

- [ ] **Step 3: Commit**

```bash
git add src/lib/types.ts
git commit -m "feat(invest): add clearedDate to Holding interface"
```

---

### Task 10: 前端 — 更新 dailyPnl/dailyPnlPct 计算

**Files:**
- Modify: `src/lib/stores/invest-store.svelte.ts:144-161` (dailyPnl, dailyPnlPct)

**Interfaces:**
- Consumes: `Holding.clearedDate`, `todayTradedShares`
- Produces: 清仓当日持仓正确计入 dailyPnl

- [ ] **Step 1: 添加 isClearedToday 辅助函数**

在 `src/lib/stores/invest-store.svelte.ts` 的 `InvestStore` class 之前（约 line 51），添加辅助函数：

```typescript
/** 判断是否为当日清仓的持仓（shares≈0 且 clearedDate == 今天） */
function isClearedToday(h: Holding, today: string): boolean {
  return h.kind === 'hold'
    && !!h.clearedDate
    && h.clearedDate === today
    && (!h.shares || h.shares <= 0.0001);
}
```

- [ ] **Step 2: 更新 dailyPnl 计算**

将 `dailyPnl` derived（line 144-151）替换为：

```typescript
  /** 组合当日收益金额 = Σ(当日价格变动 × 持仓股数),仅 hold。
   *  清仓当日持仓使用开盘股数(今日卖出-今日买入)计算。 */
  dailyPnl = $derived.by(() => {
    const today = getInvestDate();
    return this.holdHoldings.reduce((sum, h) => {
      const q = this.priceMap[h.symbol];
      if (!q) return sum;
      // 清仓当日：用开盘股数计算日内盈亏
      if (isClearedToday(h, today)) {
        const traded = this.todayTradedShares.get(h.symbol);
        const openingShares = (traded?.sell ?? 0) - (traded?.buy ?? 0);
        if (openingShares > 0) return sum + q.change * openingShares;
        return sum;
      }
      if (h.shares) return sum + q.change * h.shares;
      return sum;
    }, 0);
  });
```

- [ ] **Step 3: 更新 dailyPnlPct 计算**

将 `dailyPnlPct` derived（line 153-161）替换为：

```typescript
  /** 组合当日收益率 = 当日收益 / 昨收总市值。
   *  清仓当日持仓的昨收使用开盘股数计算。 */
  dailyPnlPct = $derived.by(() => {
    const today = getInvestDate();
    let prevValue = 0;
    for (const h of this.holdHoldings) {
      const q = this.priceMap[h.symbol];
      if (!q) continue;
      if (isClearedToday(h, today)) {
        const traded = this.todayTradedShares.get(h.symbol);
        const openingShares = (traded?.sell ?? 0) - (traded?.buy ?? 0);
        if (openingShares > 0) prevValue += (q.close - q.change) * openingShares;
      } else if (h.shares) {
        prevValue += (q.close - q.change) * h.shares;
      }
    }
    return prevValue > 0 ? (this.dailyPnl / prevValue) * 100 : 0;
  });
```

- [ ] **Step 4: 更新 holdingsMarketValue 计算**

将 `holdingsMarketValue` derived（line 121-127）替换为：

```typescript
  /** 持仓市值 = Σ(现价 × 股数)，不含清仓当日的 0 股持仓 */
  holdingsMarketValue = $derived(
    this.holdHoldings.reduce((sum, h) => {
      // 清仓当日持仓（shares≈0）不计入市值
      if (!h.shares || h.shares <= 0.0001) return sum;
      const price = this.priceMap[h.symbol]?.close;
      if (price) return sum + price * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );
```

- [ ] **Step 5: 更新 totalCostBasis 计算**

将 `totalCostBasis` derived（line 131-136）替换为：

```typescript
  /** 持仓成本 = Σ(成本价 × 股数)，不含清仓当日的 0 股持仓 */
  totalCostBasis = $derived(
    this.holdHoldings.reduce((sum, h) => {
      if (!h.shares || h.shares <= 0.0001) return sum;
      if (h.avgCost) return sum + h.avgCost * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );
```

- [ ] **Step 6: 验证 TypeScript**

Run: `npm run check`
Expected: 类型检查通过

- [ ] **Step 7: Commit**

```bash
git add src/lib/stores/invest-store.svelte.ts
git commit -m "feat(invest): update dailyPnl calculation for cleared-today positions"
```

---

### Task 11: 前端 — HoldingsTable 显示清仓标记

**Files:**
- Modify: `src/lib/components/invest/HoldingsTable.svelte:134-201` (table body)

**Interfaces:**
- Consumes: `Holding.clearedDate`, `getInvestDate()`
- Produces: 清仓当日持仓显示 "CLEARED" badge + 正确的日内盈亏

- [ ] **Step 1: 添加 isClearedToday 判断**

在 `HoldingsTable.svelte` 的 script 部分，在 `todayTraded` 函数之后添加：

```typescript
  function isClearedToday(h: Holding): boolean {
    const today = getInvestDate();
    return h.kind === 'hold'
      && !!h.clearedDate
      && h.clearedDate === today
      && (!h.shares || h.shares <= 0.0001);
  }
```

- [ ] **Step 2: 更新 table body 中的状态显示**

在 `{#each filteredHoldings as h}` 块中，在 `{@const isHold = h.kind === 'hold'}` 之后添加：

```svelte
          {@const isCleared = isHold && isClearedToday(h)}
```

然后将状态列（约 line 151-157）更新为：

```svelte
            <td class="px-[var(--space-3)] py-[var(--space-3)]">
              {#if isCleared}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(186,139,76,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#ba8b4c]">CLEARED</span>
              {:else if isHold}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(138,154,118,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8a9a76]">HOLD</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[var(--accent-muted)] px-2 py-0.5 text-[10px] font-semibold text-[var(--accent)]">WATCH</span>
              {/if}
            </td>
```

- [ ] **Step 3: 更新 dailyPnlAmount 函数支持清仓持仓**

将 `dailyPnlAmount` 函数更新为：

```typescript
  function dailyPnlAmount(h: Holding): number | null {
    const q = investStore.priceMap[h.symbol];
    if (!q) return null;
    // 清仓当日：用开盘股数计算
    if (isClearedToday(h)) {
      const traded = todayTraded(h.symbol);
      const openingShares = traded.sell - traded.buy;
      if (openingShares > 0) return q.change * openingShares;
      return null;
    }
    if (!h.shares) return null;
    return q.change * h.shares;
  }
```

- [ ] **Step 4: 验证 TypeScript**

Run: `npm run check`
Expected: 类型检查通过

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/invest/HoldingsTable.svelte
git commit -m "feat(invest): show CLEARED badge and daily PnL for cleared-today positions"
```

---

### Task 12: 前端 — CommitteeLiveTab 处理清仓持仓

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte:98-116` (allAssets)

**Interfaces:**
- Consumes: `Holding.clearedDate`, `investStore.holdHoldings`（已包含清仓当日持仓）
- Produces: 委员会 dashboard 的 allAssets 正确包含清仓当日持仓

- [ ] **Step 1: 检查 allAssets 是否需要修改**

`allAssets` 使用 `invest.holdHoldings`，而 `holdHoldings` 是 `holdings.filter(h => h.kind === 'hold')`。由于清仓当日的持仓 `kind` 仍然是 `'hold'`，它们会自动包含在 `holdHoldings` 中，因此 `allAssets` 不需要修改。

但 `portfolioStats` 中的 `maxHolding` 计算需要排除 0 股持仓：

```typescript
  const portfolioStats = $derived.by(() => {
    const hv = invest.holdingsMarketValue;
    const cashVal = invest.cash;
    const total = invest.totalAssets;
    const ret = invest.totalReturnPct;

    let maxHolding = { name: '', pct: 0 };
    if (total > 0) {
      for (const h of invest.holdHoldings) {
        // 清仓当日持仓不计入集中度
        if (!h.shares || h.shares <= 0.0001) continue;
        const price = invest.priceMap[h.symbol]?.close;
        const val = price && h.shares ? price * h.shares : h.notional || 0;
        const pct = (val / total) * 100;
        if (pct > maxHolding.pct) {
          maxHolding = { name: h.name || h.symbol, pct };
        }
      }
    }

    return { hv, cash: cashVal, total, ret, concentration: maxHolding };
  });
```

- [ ] **Step 2: 验证 TypeScript**

Run: `npm run check`
Expected: 类型检查通过

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte
git commit -m "feat(invest): exclude cleared-today from portfolio concentration"
```

---

### Task 13: 全面验证

**Files:**
- None (验证步骤)

- [ ] **Step 1: Rust 编译检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过，无 error

- [ ] **Step 2: TypeScript 类型检查**

Run: `npm run check`
Expected: 类型检查通过

- [ ] **Step 3: Lint 检查**

Run: `npm run lint`
Expected: 无 lint error

- [ ] **Step 4: 前端测试**

Run: `npm run test`
Expected: 所有测试通过

- [ ] **Step 5: Commit (如有修复)**

```bash
git add -A
git commit -m "fix(invest): address verification findings for clearance delayed conversion"
```
