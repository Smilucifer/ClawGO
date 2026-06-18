# invest Dashboard 优化 (Part B) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 优化 invest Dashboard:KPI 卡重构(总收益/当日收益 + 删除两卡)、持仓明细表扩展(16 字段含 T+1 冻结)、委员会评级预测列、归档文件名带股票名称。

**Architecture:** 持仓明细字段以前端计算为主(数据已在 store);T+1 冻结量由后端在 `list_holdings` 时从今日 trades 实时计算并随 `Holding` 返回(无需 DB 迁移);评级列复用已加载的 `investStore.verdicts`;归档文件名复用 orchestrator 已有的 `asset_name`。

**Tech Stack:** SvelteKit (Svelte 5 runes), Rust (Tauri), rusqlite。

**Spec:** `docs/superpowers/specs/2026-06-18-committee-live-and-dashboard-design.md` (Part B)

## 对 spec 的两处简化(已与用户确认)

1. **T+1 冻结(B3)**:spec 原写"Holding 加 `frozen_shares`+`frozen_date` 列 + 惰性解冻"。因 `record_trade` → `recalculate_holdings_inner` 本就是从 trades 全量回放重建持仓,改为**读取时从今日 buy trades 算冻结量**,随 `Holding.frozen_shares` 返回。零 DB 迁移、跨交易日天然解冻、多笔买入天然累加。仍满足"后端真字段"本意。
2. **评级列(B4)**:spec 提议新增 `get_latest_verdicts` 批量命令。因 `investStore.verdicts`(最近 50 条)已在 `loadAll` 加载,改为**前端从已有 verdicts 取每 symbol 最新一条**,无需新命令。

## Global Constraints

- 字段枚举值(verdict 等)保持英文原样显示。
- 前端用 Svelte 5 runes。
- 新增 UI 文案必须同步 `messages/en.json` 与 `messages/zh-CN.json`。
- Rust 验证用 `cargo check --manifest-path src-tauri/Cargo.toml`(本机单测因 VCRUNTIME 无法运行二进制,纯逻辑测试代码仍须写,本机以 `cargo check` 验证编译)。
- 前端验证:`npm run check` + `npm run build` + `npm run i18n:check`。
- A 股交易日 5AM 截止规则用现有前端日期工具(对齐 `getInvestDate`)。
- Conventional Commit 风格。每个 task 完成后:simplify 审查 → 修复 → commit → 验证。

---

## File Structure

| 文件 | 责任 | 改动 |
|------|------|------|
| `src-tauri/src/storage/invest/portfolio.rs` | 持仓持久化 | `Holding` 加 `frozen_shares` 字段;`list_holdings` 计算今日冻结 |
| `src/lib/types.ts` | 前端类型 | `Holding` 加 `frozenShares` |
| `src/lib/stores/invest-store.svelte.ts` | invest 状态 | 当日收益派生值;评级 map;今日买卖聚合 |
| `src/lib/components/invest/HoldingsTable.svelte` | 持仓明细表 | 扩展为 16 字段表 + 评级列 |
| `src/routes/invest/+page.svelte` | dashboard 页 | KPI 卡重构 + 删两卡 |
| `src-tauri/src/invest/committee/archive.rs` | 委员会归档 | 文件名带 name + 读取兼容 |
| `src-tauri/src/invest/committee/orchestrator.rs` | 编排 | 归档调用传 name |
| `messages/en.json` / `messages/zh-CN.json` | i18n | 新增列标题文案 |

实现顺序:Task 1(后端 T+1 字段)→ Task 2(前端 store 派生)→ Task 3(HoldingsTable 扩展)→ Task 4(KPI 卡重构)→ Task 5(评级列)→ Task 6(归档文件名)。Task 1 是 Task 3 冻结列的前置;Task 2 是 Task 3/4 的数据前置。

---

### Task 1: 后端 list_holdings 计算今日冻结量

**Files:**
- Modify: `src-tauri/src/storage/invest/portfolio.rs`(Holding 结构 129-145、list_holdings 198-228)
- Test: 同文件 `#[cfg(test)] mod tests`(若存在;否则新增冻结计算单元测试)

**Interfaces:**
- Produces: `Holding.frozen_shares: Option<f64>` — 今日(交易日)buy 动作累计股数,A 股 T+1 当日不可卖部分。

- [ ] **Step 1: Holding 结构新增 frozen_shares 字段**

将 `src-tauri/src/storage/invest/portfolio.rs:129-145` 的 `Holding` 结构,在 `shares` 字段(138)后新增:

```rust
    pub shares: Option<f64>,
    /// 今日(交易日)买入累计股数 — A 股 T+1 当日冻结不可卖。
    /// 由 list_holdings 从今日 buy trades 实时计算,不持久化。
    #[serde(default)]
    pub frozen_shares: Option<f64>,
```

- [ ] **Step 2: list_holdings 的 SELECT 映射补 frozen_shares 默认 None**

`list_holdings`(198-228)的 `query_map` 构造 `Holding` 时,DB 不含该列,补 `frozen_shares: None`。在 `shares: row.get(6)?,`(212)后新增:

```rust
                    shares: row.get(6)?,
                    frozen_shares: None,
```

注意后续字段的 row index 不变(frozen_shares 不来自 DB)。

- [ ] **Step 3: 新增今日冻结计算辅助函数**

在 `list_holdings`(228 行结束)后新增函数,从今日 buy trades 聚合每 symbol 冻结股数。今日交易日用现有 `date_utils`:

```rust
/// 计算每个 symbol 今日(交易日)买入累计股数(T+1 冻结量)。
/// 返回 symbol → frozen_shares 映射。今日交易日由 date_utils 决定(5AM 截止)。
fn today_frozen_shares(conn: &Connection) -> Result<std::collections::HashMap<String, f64>, String> {
    let today = crate::invest::date_utils::get_invest_date();
    let mut stmt = conn
        .prepare(
            "SELECT symbol, COALESCE(SUM(shares), 0.0) FROM trades \
             WHERE action = 'buy' AND COALESCE(trade_date, substr(created_at, 1, 10)) = ?1 \
             GROUP BY symbol",
        )
        .map_err(|e| format!("prepare frozen query: {}", e))?;
    let rows = stmt
        .query_map(params![today], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })
        .map_err(|e| format!("frozen query: {}", e))?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (sym, shares) = row.map_err(|e| format!("frozen row: {}", e))?;
        map.insert(sym, shares);
    }
    Ok(map)
}
```

- [ ] **Step 4: list_holdings 填充 frozen_shares**

将 `list_holdings`(198-228)的实现改为在收集 holdings 后填充冻结量。把函数体末尾的 `Ok(items)`(226)前改为:

```rust
        let frozen = today_frozen_shares(conn)?;
        for h in items.iter_mut() {
            if h.kind == HoldingKind::Hold {
                if let Some(&f) = frozen.get(&h.symbol) {
                    if f > 0.0 {
                        h.frozen_shares = Some(f);
                    }
                }
            }
        }
        Ok(items)
```

(需确保 `items` 声明为 `let mut items`;原代码已是 `let mut items`。)

- [ ] **Step 5: 新增冻结计算单元测试**

在 portfolio.rs 测试模块(若无则在文件末尾新增 `#[cfg(test)] mod frozen_tests`)加入:

```rust
#[cfg(test)]
mod frozen_tests {
    use super::*;

    #[test]
    fn frozen_shares_field_defaults_none() {
        let h = Holding {
            symbol: "600519".into(),
            currency: "CNY".into(),
            kind: HoldingKind::Hold,
            name: None,
            notional: 0.0,
            avg_cost: None,
            shares: Some(100.0),
            frozen_shares: None,
            entry_date: None,
            linked_verdict_id: None,
            notes: None,
            asset_type: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        // Serialize → camelCase frozenShares present, null when None.
        let json = serde_json::to_string(&h).unwrap();
        assert!(json.contains("\"frozenShares\""));
    }
}
```

- [ ] **Step 6: 验证编译 + 测试编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。所有构造 `Holding` 的地方(upsert/list/recalculate)需补 `frozen_shares` 字段;若编译报缺字段,逐处补 `frozen_shares: None`。

- [ ] **Step 7: simplify 审查 + commit**

```bash
git add src-tauri/src/storage/invest/portfolio.rs
git commit -m "feat(invest): compute T+1 frozen shares from today's buy trades in list_holdings"
```

---

### Task 2: 前端 store — 当日收益派生 + 今日买卖聚合 + 评级 map

**Files:**
- Modify: `src/lib/types.ts`(Holding 接口)
- Modify: `src/lib/stores/invest-store.svelte.ts`(派生值,约 120-141 区域后)

**Interfaces:**
- Consumes: `Holding.frozenShares`(Task 1)、`priceMap[sym].change`、`investStore.trades`、`investStore.verdicts`。
- Produces:
  - `investStore.dailyPnl: number`、`dailyPnlPct: number`(组合当日收益)
  - `investStore.todayTradedShares: Map<string, { buy: number; sell: number }>`
  - `investStore.latestVerdictMap: Map<string, Verdict>`

- [ ] **Step 1: types.ts 的 Holding 加 frozenShares**

在 `src/lib/types.ts` 的 `Holding` 接口,`shares` 字段后新增:

```ts
  shares?: number | null;
  frozenShares?: number | null;
```

(若 `Holding` 接口中字段名/可选性不同,按现有风格对齐;字段名 camelCase `frozenShares` 与后端 serde 一致。)

- [ ] **Step 2: 新增当日收益派生值**

在 `src/lib/stores/invest-store.svelte.ts` 的 `totalReturnPct`(137-141)后新增:

```ts
  /** 组合当日收益金额 = Σ(当日价格变动 × 持仓股数),仅 hold。 */
  dailyPnl = $derived(
    this.holdHoldings.reduce((sum, h) => {
      const q = this.priceMap[h.symbol];
      if (q && h.shares) return sum + q.change * h.shares;
      return sum;
    }, 0),
  );

  /** 组合当日收益率 = 当日收益 / 昨收总市值。昨收 = close - change。 */
  dailyPnlPct = $derived.by(() => {
    let prevValue = 0;
    for (const h of this.holdHoldings) {
      const q = this.priceMap[h.symbol];
      if (q && h.shares) prevValue += (q.close - q.change) * h.shares;
    }
    return prevValue > 0 ? (this.dailyPnl / prevValue) * 100 : 0;
  });
```

- [ ] **Step 3: 新增今日买卖股数聚合**

在 `dailyPnlPct` 后新增(今日交易日复用现有前端日期工具;确认 `getInvestDate` 的 import 路径,与页面其它处一致):

```ts
  /** 每 symbol 今日买入/卖出股数聚合(从 trades)。 */
  todayTradedShares = $derived.by(() => {
    const today = getInvestDate(); // YYYY-MM-DD, 5AM cutoff
    const map = new Map<string, { buy: number; sell: number }>();
    for (const tr of this.trades) {
      const d = tr.tradeDate ?? tr.createdAt?.slice(0, 10);
      if (d !== today) continue;
      if (tr.action !== 'buy' && tr.action !== 'sell') continue;
      const cur = map.get(tr.symbol) ?? { buy: 0, sell: 0 };
      cur[tr.action] += tr.shares ?? 0;
      map.set(tr.symbol, cur);
    }
    return map;
  });
```

在文件顶部 import 区补 `getInvestDate`(确认其导出位置,如 `$lib/utils/invest-date` 或现有页面使用的路径)。

- [ ] **Step 4: 新增每 symbol 最新 verdict map**

在 `todayTradedShares` 后新增:

```ts
  /** 每 symbol 最新一条委员会 verdict(verdicts 已按 created_at desc 排序)。 */
  latestVerdictMap = $derived.by(() => {
    const map = new Map<string, Verdict>();
    for (const v of this.verdicts) {
      if (!map.has(v.symbol)) map.set(v.symbol, v); // first = newest due to DESC order
    }
    return map;
  });
```

(确认 `Verdict` 类型已在该 store 文件 import;`verdicts` 字段已存在。)

- [ ] **Step 5: 验证类型 + 构建**

Run: `npm run check && npm run build`
Expected: 通过。

- [ ] **Step 6: simplify 审查 + commit**

```bash
git add src/lib/types.ts src/lib/stores/invest-store.svelte.ts
git commit -m "feat(invest): add daily-pnl, today-traded-shares, latest-verdict derived state"
```

---

### Task 3: HoldingsTable 扩展为持仓明细表

**Files:**
- Modify: `src/lib/components/invest/HoldingsTable.svelte`(表头 79-90、行 92-141、脚本辅助 30-46)
- Modify: `messages/en.json` / `messages/zh-CN.json`(新列标题)

**Interfaces:**
- Consumes: Task 1 的 `h.frozenShares`、Task 2 的 `investStore.todayTradedShares` / `latestVerdictMap`、`priceMap`。

- [ ] **Step 1: 新增 i18n 列标题 key**

在 `messages/en.json` 与 `messages/zh-CN.json` 新增(成对):

| key | en | zh-CN |
|-----|----|----|
| `invest_available_shares` | Available | 可用 |
| `invest_frozen_shares` | Frozen | 冻结 |
| `invest_market_value` | Market Value | 市值 |
| `invest_pnl_amount` | P&L | 盈亏 |
| `invest_daily_pnl` | Today P&L | 当日盈亏 |
| `invest_position_pct` | Weight | 仓位 |
| `invest_today_buy` | Buy Today | 今买 |
| `invest_today_sell` | Sell Today | 今卖 |
| `invest_rating` | Rating | 评级 |

(已有 key 复用:`invest_quantity` 持仓数量、`invest_cost_price` 成本、`invest_current_price` 现价、`invest_pnl_pct` 盈亏比。)

- [ ] **Step 2: 脚本块新增计算辅助函数**

在 `src/lib/components/invest/HoldingsTable.svelte` 脚本块的 `getPnlPct`(34-38)后新增:

```ts
  function marketValue(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (price != null && h.shares) return price * h.shares;
    return h.notional ?? null;
  }

  function pnlAmount(h: Holding): number | null {
    const price = getPrice(h.symbol);
    if (price == null || h.avgCost == null || !h.shares) return null;
    return (price - h.avgCost) * h.shares;
  }

  function dailyPnlAmount(h: Holding): number | null {
    const q = investStore.priceMap[h.symbol];
    if (!q || !h.shares) return null;
    return q.change * h.shares;
  }

  function dailyPnlPct(h: Holding): number | null {
    return investStore.priceMap[h.symbol]?.pctChg ?? null;
  }

  function positionPct(h: Holding): number | null {
    const mv = marketValue(h);
    if (mv == null || investStore.totalAssets <= 0) return null;
    return (mv / investStore.totalAssets) * 100;
  }

  function availableShares(h: Holding): number | null {
    if (h.shares == null) return null;
    return h.shares - (h.frozenShares ?? 0);
  }

  function todayTraded(sym: string): { buy: number; sell: number } {
    return investStore.todayTradedShares.get(sym) ?? { buy: 0, sell: 0 };
  }
```

- [ ] **Step 3: 替换表头为完整列**

将表头(79-90 的 `<tr>`)替换为(列顺序:标的/状态/类型/持仓/可用/冻结/成本/现价/市值/盈亏/盈亏%/当日盈亏/当日%/仓位/今买/今卖/评级/操作)。表头较宽,统一用紧凑样式。把 `<thead>...</thead>` 整段替换:

```svelte
      <thead>
        <tr class="border-b border-border">
          {#each [
            t('invest_trade_stock'), t('invest_status'), t('invest_asset_type'),
            t('invest_quantity'), t('invest_available_shares'), t('invest_frozen_shares'),
            t('invest_cost_price'), t('invest_current_price'), t('invest_market_value'),
            t('invest_pnl_amount'), t('invest_pnl_pct'), t('invest_daily_pnl'),
            t('invest_position_pct'), t('invest_today_buy'), t('invest_today_sell'),
            t('invest_rating'), t('invest_actions')
          ] as col}
            <th class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-2)] text-left text-[11px] font-semibold uppercase tracking-wider text-[var(--text-tertiary)]">{col}</th>
          {/each}
        </tr>
      </thead>
```

- [ ] **Step 4: 替换数据行为完整列**

将 `<tbody>` 内的 `{#each filteredHoldings as h}` 行块(92-141)替换。新行计算各值并渲染,watch 类无 shares 的列显示 `—`:

```svelte
        {#each filteredHoldings as h}
          {@const isHold = h.kind === 'hold'}
          {@const price = getPrice(h.symbol)}
          {@const mv = marketValue(h)}
          {@const pnl = isHold ? pnlAmount(h) : null}
          {@const pnlPct = isHold ? getPnlPct(h) : null}
          {@const dPnl = isHold ? dailyPnlAmount(h) : null}
          {@const dPct = isHold ? dailyPnlPct(h) : null}
          {@const posPct = isHold ? positionPct(h) : null}
          {@const avail = isHold ? availableShares(h) : null}
          {@const traded = todayTraded(h.symbol)}
          {@const verdict = investStore.latestVerdictMap.get(h.symbol)}
          {@const dec = priceDecimals(h.assetType)}
          <tr class="border-b border-border transition-colors last:border-b-0 hover:bg-[var(--bg-hover)]">
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              <span class="text-[13px] font-semibold text-[var(--text-primary)]" title={h.symbol}>{h.name || h.symbol}</span>
              {#if h.name}<span class="ml-[var(--space-2)] text-[11px] font-[var(--font-mono)] text-[var(--text-tertiary)]">{h.symbol}</span>{/if}
            </td>
            <td class="px-[var(--space-3)] py-[var(--space-3)]">
              {#if isHold}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(138,154,118,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8a9a76]">HOLD</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[var(--accent-muted)] px-2 py-0.5 text-[10px] font-semibold text-[var(--accent)]">WATCH</span>
              {/if}
            </td>
            <td class="px-[var(--space-3)] py-[var(--space-3)]">
              {#if h.assetType === 'etf'}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(139,92,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#8b5cf6]">etf</span>
              {:else}
                <span class="inline-block rounded-[var(--radius-sm)] bg-[rgba(59,130,246,0.15)] px-2 py-0.5 text-[10px] font-semibold text-[#3b82f6]">stock</span>
              {/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.shares ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{avail ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] {h.frozenShares ? 'text-[#b89a6a]' : 'text-[var(--text-tertiary)]'}">{h.frozenShares ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{h.avgCost?.toFixed(dec) ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{price?.toFixed(dec) ?? '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{mv != null ? '¥' + mv.toFixed(0) : '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if pnl !== null}<span class={pnl >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{pnl >= 0 ? '+' : ''}{pnl.toFixed(0)}</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if pnlPct !== null}<span class={pnlPct >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{pnlPct >= 0 ? '+' : ''}{pnlPct.toFixed(2)}%</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px]">
              {#if dPnl !== null}<span class={dPnl >= 0 ? 'text-[#8a9a76]' : 'text-[#a87a7a]'}>{dPnl >= 0 ? '+' : ''}{dPnl.toFixed(0)}{#if dPct !== null}<span class="ml-1 text-[10px] opacity-70">{dPct >= 0 ? '+' : ''}{dPct.toFixed(2)}%</span>{/if}</span>{:else}<span class="text-[var(--text-tertiary)]">—</span>{/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{posPct != null ? posPct.toFixed(1) + '%' : '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{traded.buy || '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)] font-[var(--font-mono)] text-[13px] text-[var(--text-secondary)]">{traded.sell || '—'}</td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              {#if verdict}
                <span class="inline-block rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold" style={getVerdictBadgeStyle(verdict.verdict)}>{verdict.verdict}</span>
              {:else}
                <span class="text-[var(--text-tertiary)]">—</span>
              {/if}
            </td>
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              <button class="{clsAction} border-border text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => onEdit(h)}>{t('invest_edit')}</button>
              {#if isHold}
                <button class={clsBuy} onclick={() => onBuy(h)}>{t('invest_buy')}</button>
                <button class={clsSell} onclick={() => onSell(h)}>{t('invest_sell')}</button>
                <button class={clsAccent} onclick={() => onConvertToWatch(h)}>{t('invest_convert_to_watch')}</button>
              {:else}
                <button class={clsBuy} onclick={() => onBuy(h)}>{t('invest_convert_to_hold')}</button>
                <button class={clsSell} onclick={() => onDeleteWatch(h)}>{t('invest_delete_watch')}</button>
              {/if}
            </td>
          </tr>
        {/each}
```

- [ ] **Step 5: import getVerdictBadgeStyle + 容器横向滚动**

在脚本块顶部 import 区(2-4 行附近)新增:

```ts
  import { getVerdictBadgeStyle } from '$lib/utils/invest-verdict';
```

将表格外层容器(49 行的 `<div class="overflow-hidden ...">`)的 `overflow-hidden` 改为 `overflow-x-auto`,避免宽表被裁切:

```svelte
<div class="overflow-x-auto rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)]">
```

并给 `<table>`(78 行)加 `min-w-max` 确保列不被压缩:

```svelte
    <table class="w-full min-w-max">
```

- [ ] **Step 6: 验证 i18n + 类型 + 构建 + 目测**

Run: `npm run i18n:check && npm run check && npm run build`
Expected: 全通过。

`npm run tauri dev` → dashboard。
Expected: 持仓表显示全部 16+ 列;hold 有冻结/可用/盈亏/当日/仓位/评级;watch 相关列为 `—`;表格可横向滚动。

- [ ] **Step 7: simplify 审查 + commit**

```bash
git add src/lib/components/invest/HoldingsTable.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(invest): expand holdings table to full detail columns with rating"
```

---

### Task 4: Dashboard KPI 卡重构

**Files:**
- Modify: `src/routes/invest/+page.svelte`(KPI grid 170-176、删两卡 178-182、import 区)

**Interfaces:**
- Consumes: Task 2 的 `investStore.dailyPnl` / `dailyPnlPct`;现有 `holdingsMarketValue` / `totalCostBasis` / `totalReturnPct`。

- [ ] **Step 1: 重构 KPI grid(总收益率→总收益,新增当日收益)**

将 `src/routes/invest/+page.svelte:170-176` 的 KPI grid 改为 6 卡(保留总资产/持仓市值/现金/持仓数,改总收益,加当日收益)。把该 grid 段替换:

```svelte
      <div class="mb-[var(--space-6)] grid grid-cols-2 gap-[var(--space-3)] sm:grid-cols-3 lg:grid-cols-6">
        <KpiCard label={t('invest_total_assets')} value={'¥' + investStore.totalAssets.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_holdings_value')} value={'¥' + investStore.holdingsMarketValue.toLocaleString(undefined, { minimumFractionDigits: 2 })} />
        <KpiCard label={t('invest_cash')} value={'¥' + investStore.cash.toLocaleString(undefined, { minimumFractionDigits: 2 })} sub="✎" />
        <KpiCard
          label={t('invest_total_return')}
          value={(investStore.holdingsMarketValue - investStore.totalCostBasis >= 0 ? '+' : '') + '¥' + (investStore.holdingsMarketValue - investStore.totalCostBasis).toLocaleString(undefined, { maximumFractionDigits: 0 })}
          sub={(investStore.totalReturnPct >= 0 ? '+' : '') + investStore.totalReturnPct.toFixed(2) + '%'}
          trend={investStore.totalReturnPct >= 0 ? 'up' : 'down'}
        />
        <KpiCard
          label={t('invest_daily_return')}
          value={(investStore.dailyPnl >= 0 ? '+' : '') + '¥' + investStore.dailyPnl.toLocaleString(undefined, { maximumFractionDigits: 0 })}
          sub={(investStore.dailyPnlPct >= 0 ? '+' : '') + investStore.dailyPnlPct.toFixed(2) + '%'}
          trend={investStore.dailyPnl >= 0 ? 'up' : 'down'}
        />
        <KpiCard label={t('invest_position_count')} value={t('invest_hold') + ' ' + investStore.holdCount + ' + ' + t('invest_watch') + ' ' + investStore.watchCount} />
      </div>
```

- [ ] **Step 2: 删除宏观快照卡 + 最新裁决卡**

删除 `src/routes/invest/+page.svelte:178-182` 整段:

```svelte
      <!-- Macro snapshot + Latest verdict -->
      <div class="mb-[var(--space-4)] grid gap-[var(--space-3)] sm:grid-cols-2">
        <MacroSnapshotCard />
        <LatestVerdictCard />
      </div>
```

- [ ] **Step 3: 移除两组件的 import 与未用引用**

在 `+page.svelte` 顶部 import 区删除 `MacroSnapshotCard`、`LatestVerdictCard` 的 import。
Run: `grep -rn "MacroSnapshotCard\|LatestVerdictCard" src/` 确认无其它引用。
- 若组件文件无其它引用 → 保留文件(避免误删被复用组件),仅移除 dashboard import。
- 若仅此处引用且确认废弃 → 可删文件(实现阶段按 grep 结果决定,commit message 注明)。

- [ ] **Step 4: 新增 i18n key invest_daily_return**

`messages/en.json`: `"invest_daily_return": "Today's Return"`
`messages/zh-CN.json`: `"invest_daily_return": "当日收益"`

(`invest_total_return` 已存在,语义从"总收益率"变为"总收益";如文案需要,zh-CN 可改为"总收益"。确认现有值后决定是否调整。)

- [ ] **Step 5: 验证 i18n + 类型 + 构建 + 目测**

Run: `npm run i18n:check && npm run check && npm run build`
Expected: 全通过。

`npm run tauri dev` → dashboard。
Expected: 6 个 KPI 卡;总收益/当日收益显示大字金额 + 小字百分比 + 涨跌配色;宏观/裁决两卡消失。

- [ ] **Step 6: simplify 审查 + commit**

```bash
git add src/routes/invest/+page.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(invest): rework dashboard KPI cards (total/daily return, drop macro/verdict cards)"
```

---

### Task 5: 评级列新鲜度过滤

**Files:**
- Modify: `src/lib/components/invest/HoldingsTable.svelte`(评级单元格逻辑)
- Modify: `src/lib/stores/invest-store.svelte.ts`(若新鲜度判定放 store)

**Interfaces:**
- Consumes: Task 2 的 `latestVerdictMap`;现有交易日工具。

> 说明:Task 3 已渲染 `latestVerdictMap` 的最新 verdict。本 task 加"仅近一个交易日内才显示,过期不显示"的新鲜度过滤。

- [ ] **Step 1: 新增新鲜度判定辅助**

在 `src/lib/components/invest/HoldingsTable.svelte` 脚本块新增(判定 verdict 是否在上一交易日起点之后)。"上一交易日"用现有交易日历/日期工具;若无现成"上一交易日"函数,用保守阈值:`created_at` 在最近 2 个自然日内(覆盖周末/隔日):

```ts
  import { getInvestDate } from '$lib/utils/invest-date'; // 与 store 同源

  function isVerdictFresh(createdAt: string): boolean {
    // 仅显示近一个交易日内的评级。保守判定:created_at 日期 >= 今日交易日前 1 个自然日。
    // (跨周末时,周五评级在周一仍算"最近一个交易日内";如需精确交易日历,后续可接 trade_calendar。)
    const today = getInvestDate(); // YYYY-MM-DD
    const created = createdAt.slice(0, 10);
    const todayMs = new Date(today + 'T00:00:00').getTime();
    const createdMs = new Date(created + 'T00:00:00').getTime();
    const dayMs = 86_400_000;
    return todayMs - createdMs <= 4 * dayMs; // 容忍周末+节假日的近一个交易日窗口
  }
```

(注:精确"上一交易日"需交易日历;此处用 4 自然日窗口近似覆盖"最近一个交易日内",避免节假日误判为过期。实现阶段若 store 已有 `tradeCalendar` 可换精确判定。)

- [ ] **Step 2: 评级单元格加新鲜度过滤**

将 Task 3 Step 4 的评级单元格(`{@const verdict = ...}` 对应的 `<td>`)改为带新鲜度判断:

```svelte
            <td class="whitespace-nowrap px-[var(--space-3)] py-[var(--space-3)]">
              {#if verdict && isVerdictFresh(verdict.createdAt)}
                <span class="inline-block rounded-[var(--radius-sm)] px-2 py-0.5 text-[10px] font-semibold" style={getVerdictBadgeStyle(verdict.verdict)} title={'置信度 ' + ((verdict.confidence ?? 0) * 100).toFixed(0) + '% · ' + verdict.createdAt.slice(0, 10)}>{verdict.verdict}</span>
              {:else}
                <span class="text-[var(--text-tertiary)]">—</span>
              {/if}
            </td>
```

- [ ] **Step 3: 验证类型 + 构建 + 目测**

Run: `npm run check && npm run build`
Expected: 通过。

`npm run tauri dev` → dashboard。Expected: 近期跑过委员会的标的显示 verdict chip;久未评级的标的显示 `—`。

- [ ] **Step 4: simplify 审查 + commit**

```bash
git add src/lib/components/invest/HoldingsTable.svelte
git commit -m "feat(invest): show committee rating only when fresh (within last trading day)"
```

---

### Task 6: 归档文件名带股票名称

**Files:**
- Modify: `src-tauri/src/invest/committee/archive.rs`(archive_decision_full 65-141、load_archive 247-279、format_decision_markdown 148、新增 sanitize 函数)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(归档调用 1995)

**Interfaces:**
- `archive_decision_full(symbol: &str, name: Option<&str>, result: &CommitteeResult)` — 签名新增 name 参数。

- [ ] **Step 1: 写失败测试 — 文件名 sanitize**

在 `src-tauri/src/invest/committee/archive.rs` 测试模块(`test_load_archive_empty_dir` 后)新增:

```rust
    #[test]
    fn test_sanitize_name_for_filename() {
        assert_eq!(sanitize_name_for_filename("贵州茅台"), "贵州茅台");
        assert_eq!(sanitize_name_for_filename("A/B:C*D"), "ABCD");
        assert_eq!(sanitize_name_for_filename(""), "");
        assert_eq!(sanitize_name_for_filename("Foo<Bar>"), "FooBar");
    }

    #[test]
    fn test_archive_filename_with_name() {
        assert_eq!(archive_md_filename("600519.SH", Some("贵州茅台")), "600519.SH_贵州茅台.md");
        assert_eq!(archive_md_filename("600519.SH", None), "600519.SH.md");
        assert_eq!(archive_md_filename("600519.SH", Some("")), "600519.SH.md");
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::archive::tests::test_sanitize 2>&1 | head -15`
Expected: 编译失败(函数未定义)。本机无法运行则 `cargo check` 确认。

- [ ] **Step 3: 新增 sanitize + 文件名构造函数**

在 `src-tauri/src/invest/committee/archive.rs` 的 `validate_symbol`(46-60)后新增:

```rust
/// 去除文件名中的文件系统非法字符,保留中文。空名返回空串。
fn sanitize_name_for_filename(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string()
}

/// 构造归档 md 文件名:有名称用 `{symbol}_{name}.md`,否则 `{symbol}.md`。
fn archive_md_filename(symbol: &str, name: Option<&str>) -> String {
    match name.map(sanitize_name_for_filename).filter(|s| !s.is_empty()) {
        Some(safe) => format!("{symbol}_{safe}.md"),
        None => format!("{symbol}.md"),
    }
}
```

- [ ] **Step 4: archive_decision_full 签名加 name,用新文件名**

修改 `archive_decision_full`(65-68)签名:

```rust
pub fn archive_decision_full(
    symbol: &str,
    name: Option<&str>,
    result: &CommitteeResult,
) -> Result<(), String> {
```

将 md 路径构造(76)改为:

```rust
    let md_path = dir.join(archive_md_filename(symbol, name));
```

将 markdown 标题传入 name —— 把 `format_decision_markdown(symbol, result)`(75)改为 `format_decision_markdown(symbol, name, result)`,并更新该函数签名(148-151):

```rust
pub fn format_decision_markdown(
    symbol: &str,
    name: Option<&str>,
    result: &CommitteeResult,
) -> String {
```

标题行(156)改为带名称:

```rust
    let title_sym = match name.map(sanitize_name_for_filename).filter(|s| !s.is_empty()) {
        Some(n) => format!("{symbol} {n}"),
        None => symbol.to_string(),
    };
    md.push_str(&format!("# {} 委员会决策报告\n\n", title_sym));
```

- [ ] **Step 5: load_archive 兼容新旧文件名**

将 `load_archive`(260-276)的精确查找改为扫描目录匹配 `{symbol}.md` 或 `{symbol}_*.md`。替换 for 循环体内的查找段:

```rust
    for offset in 0..days {
        let date = (today - chrono::Duration::days(offset))
            .format("%Y-%m-%d")
            .to_string();
        let dir = root.join(&date);
        if !dir.exists() {
            continue;
        }
        // Match `{symbol}.md` (legacy) or `{symbol}_{name}.md` (new).
        let prefix = format!("{symbol}_");
        let legacy = format!("{symbol}.md");
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name().to_string_lossy().to_string();
                if !fname.ends_with(".md") {
                    continue;
                }
                if fname == legacy || fname.starts_with(&prefix) {
                    let content = fs::read_to_string(entry.path())
                        .map_err(|e| format!("read {}: {e}", entry.path().display()))?;
                    results.push(ArchivedDecision {
                        date: date.clone(),
                        symbol: symbol.to_string(),
                        content,
                    });
                    break; // one archive per symbol per day
                }
            }
        }
    }
```

- [ ] **Step 6: orchestrator 归档调用传 name**

在 `src-tauri/src/invest/committee/orchestrator.rs:1995` 的调用处,`asset_name` 已在 1958 行算出(`get_asset_name(symbol)`)。将调用改为传 name。把 1995 行附近改为:

```rust
        if let Err(e) = archive_decision_full(symbol, asset_name.as_deref(), &result) {
            log::warn!("archive_decision_full failed for {}: {}", symbol, e);
        }
```

(确认 `asset_name` 变量在该作用域可见;1958 行 `let asset_name = get_asset_name(symbol);` 已是 `Option<String>`。若作用域不同,在调用前补 `let asset_name = get_asset_name(symbol);`。)

- [ ] **Step 7: 更新现有测试调用签名**

archive.rs 测试中 `format_decision_markdown("TEST", &result)`(337、362、373 行)改为 `format_decision_markdown("TEST", None, &result)`。

- [ ] **Step 8: 运行测试 + 编译验证**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::archive::tests 2>&1 | tail -20`
Expected: 通过(本机无法运行则 `cargo check` 通过)。

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 9: simplify 审查 + commit**

```bash
git add src-tauri/src/invest/committee/archive.rs src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(invest): include stock name in committee archive filename and title"
```

---

## Self-Review

**Spec 覆盖检查(Part B)**:
- B1 KPI 卡重构(总收益/当日收益 + 删两卡)→ Task 4 ✓
- B2 持仓明细表扩展(16 字段)→ Task 3 ✓
- B3 T+1 冻结 → Task 1(后端算)+ Task 3(展示)✓ — 简化为读取时算,已与用户确认
- B4 评级预测列 → Task 2(map)+ Task 3(渲染)+ Task 5(新鲜度)✓ — 复用已加载 verdicts,无新命令
- B5 归档文件名带名称 → Task 6 ✓

**依赖顺序**:Task 1(frozen_shares 字段)→ Task 3(展示冻结列)✓;Task 2(派生值)→ Task 3/4 ✓;Task 3(渲染 verdict)→ Task 5(加新鲜度过滤)✓。

**类型一致性**:
- `frozen_shares`(Rust serde camelCase)↔ `frozenShares`(TS)一致 ✓
- `archive_decision_full(symbol, name, result)` 与 `format_decision_markdown(symbol, name, result)` 签名一致,调用点(orchestrator + 测试)均更新 ✓
- `dailyPnl` / `dailyPnlPct` / `todayTradedShares` / `latestVerdictMap` 在 Task 2 定义,Task 3/4 消费,命名一致 ✓

**待实现阶段确认的小项**(已在步骤中标注,非阻塞):
- `getInvestDate` 的确切 import 路径(Task 2 Step 3)
- `MacroSnapshotCard` / `LatestVerdictCard` 是否被其它处引用(Task 4 Step 3,grep 决定删文件与否)
- 精确"上一交易日"判定(Task 5 用 4 自然日窗口近似;若 store 有交易日历可换精确)
