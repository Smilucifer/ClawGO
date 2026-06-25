import { getTransport } from "$lib/transport";
import { currentLocale } from "$lib/i18n/index.svelte";
import { getInvestDate } from "$lib/i18n/format";
import { computeDailyPnl, type TodayTraded } from "$lib/utils/invest-fees";
import type {
  Holding,
  Trade,
  PnlSnapshot,
  Verdict,
  Strategy,
  PriceQuote,
  RealtimeQuote,
  InvestEvent,
  ScanStatus,
  ScanResult,
  EventFilter,
} from "$lib/types";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

/** Parse event date strings, handling Tushare "20260609" format as safety net. */
function parseEventDate(dateStr: string): number {
  const d = new Date(dateStr);
  if (!isNaN(d.getTime())) return d.getTime();
  // Handle "20260609" 8-digit numeric format from Tushare
  if (/^\d{8}$/.test(dateStr)) {
    const parsed = new Date(
      `${dateStr.slice(0, 4)}-${dateStr.slice(4, 6)}-${dateStr.slice(6, 8)}T00:00:00`,
    );
    return parsed.getTime();
  }
  return NaN;
}

/** Check if A-share market is currently in trading hours (9:15-11:30 or 13:00-15:00 CST, weekdays) */
function isMarketOpen(): boolean {
  const now = new Date();
  // Convert to CST (UTC+8)
  const utcHour = now.getUTCHours();
  const utcMin = now.getUTCMinutes();
  const cstMinutes = ((utcHour + 8) % 24) * 60 + utcMin;
  const day = now.getUTCDay(); // 0=Sun, 6=Sat
  // Weekday check (Mon-Fri); day 0 and 6 are weekends
  if (day === 0 || day === 6) return false;
  // Morning session: 9:15 - 11:30 (555 - 690 minutes)
  if (cstMinutes >= 555 && cstMinutes <= 690) return true;
  // Afternoon session: 13:00 - 15:00 (780 - 900 minutes)
  return cstMinutes >= 780 && cstMinutes <= 900;
}

/** 判断持仓是否为当日清仓（shares 归零但仍在 Hold 保护期内）。 */
export function isClearedToday(h: Holding): boolean {
  return h.kind === 'hold' && (h.shares ?? 0) <= 0.0001 && !!h.clearedDate && h.clearedDate >= getInvestDate();
}

class InvestStore {
  // ── State ────────────────────────────────────────────────────────────
  holdings = $state<Holding[]>([]);
  trades = $state<Trade[]>([]);
  pnlSnapshots = $state<PnlSnapshot[]>([]);
  verdicts = $state<Verdict[]>([]);
  cash = $state<number>(0);
  initialCash = $state<number>(0);
  strategies = $state<Strategy[]>([]);

  loading = $state<boolean>(false);
  error = $state<string | null>(null);

  /** Live price cache: tsCode → PriceQuote */
  priceMap = $state<Record<string, PriceQuote>>({});
  /** Last successful price refresh timestamp (ms) */
  lastRefreshAt = $state<number>(0);

  // ── Event Watch State ───────────────────────────────────────────────
  events = $state<InvestEvent[]>([]);
  eventFilter = $state<EventFilter>({
    timeWindow: "24h",
    severity: "all",
    search: "",
  });
  scanStatus = $state<ScanStatus | null>(null);
  isScanning = $state<boolean>(false);
  lastScanResult = $state<ScanResult | null>(null);

  // ── Derived ──────────────────────────────────────────────────────────
  holdHoldings = $derived(this.holdings.filter((h) => h.kind === "hold"));
  /**
   * Watch holdings excluding symbols that already have a hold entry.
   * Prevents duplicates when a watched stock is bought directly (buyStock)
   * instead of using convertWatchToHold — the hold entry takes priority.
   */
  watchHoldings = $derived.by(() => {
    const held = new Set(this.holdHoldings.map((h) => h.symbol));
    return this.holdings.filter((h) => h.kind === "watch" && !held.has(h.symbol));
  });
  holdCount = $derived(this.holdHoldings.length);
  watchCount = $derived(this.watchHoldings.length);

  /** Symbol → Chinese name lookup from holdings + price cache + trades */
  nameMap = $derived.by(() => {
    const map = new Map<string, string>();
    for (const h of this.holdings) {
      if (h.name) map.set(h.symbol, h.name);
    }
    // Enrich from price cache (rt_k returns name)
    for (const [code, q] of Object.entries(this.priceMap)) {
      if (q.name && !map.has(code)) map.set(code, q.name);
    }
    // Enrich from trades (persists names for sold positions)
    for (const tr of this.trades) {
      if (tr.name && !map.has(tr.symbol)) map.set(tr.symbol, tr.name);
    }
    return map;
  });

  /** All holdings merged into one list: HOLD first, then WATCH, sorted by name */
  mergedHoldings = $derived(
    [...this.holdings].sort((a, b) => {
      if (a.kind === "hold" && b.kind !== "hold") return -1;
      if (b.kind === "hold" && a.kind !== "hold") return 1;
      return (a.name ?? a.symbol).localeCompare(b.name ?? b.symbol);
    }),
  );

  holdingsMarketValue = $derived(
    this.holdHoldings.reduce((sum, h) => {
      if (isClearedToday(h)) return sum; // 清仓当日无市值
      const price = this.priceMap[h.symbol]?.close;
      if (price && h.shares) return sum + price * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );

  totalAssets = $derived(this.cash + this.holdingsMarketValue);

  totalCostBasis = $derived(
    this.holdHoldings.reduce((sum, h) => {
      if (isClearedToday(h)) return sum; // 清仓当日无成本
      if (h.avgCost && h.shares) return sum + h.avgCost * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );

  totalReturnPct = $derived(
    this.totalCostBasis > 0
      ? ((this.holdingsMarketValue - this.totalCostBasis) / this.totalCostBasis) * 100
      : 0,
  );

  /**
   * 组合当日盈亏汇总：单次遍历 holdHoldings,累加各 symbol 盯市当日盈亏(amount)
   * 与收益率分母(denom)。dailyPnl/dailyPnlPct 从此派生,避免重复遍历与重算分母。
   */
  dailyPnlSummary = $derived.by(() => {
    let amount = 0;
    let denom = 0;
    for (const h of this.holdHoldings) {
      const r = computeDailyPnl(h, this.priceMap[h.symbol], this.todayTraded.get(h.symbol));
      if (!r) continue;
      amount += r.amount;
      denom += r.denom;
    }
    return { amount, denom };
  });

  /** 组合当日收益金额,仅 hold。 */
  dailyPnl = $derived(this.dailyPnlSummary.amount);

  /** 组合当日收益率 = 当日收益 / 昨收开盘总市值(各 symbol denom 之和)。 */
  dailyPnlPct = $derived(
    this.dailyPnlSummary.denom > 0
      ? (this.dailyPnlSummary.amount / this.dailyPnlSummary.denom) * 100
      : 0,
  );

  /**
   * 每 symbol 今日成交聚合(股数 + 金额),供盯市当日盈亏计算。
   * 买入支出含佣金;卖出回款扣佣金、不扣印花税。"今日"为 5AM 截止的交易日。
   */
  todayTraded = $derived.by(() => {
    const today = getInvestDate(); // YYYY-MM-DD, 5AM cutoff
    const map = new Map<string, TodayTraded>();
    for (const tr of this.trades) {
      const d = tr.tradeDate ?? tr.createdAt?.slice(0, 10);
      if (d !== today) continue;
      if (tr.action !== 'buy' && tr.action !== 'sell') continue;
      const cur = map.get(tr.symbol) ?? { buyShares: 0, buyCost: 0, sellShares: 0, sellProceeds: 0 };
      const shares = tr.shares ?? 0;
      const notional = (tr.price ?? 0) * shares;
      const commission = tr.commission ?? 0;
      if (tr.action === 'buy') {
        cur.buyShares += shares;
        cur.buyCost += notional + commission;
      } else {
        cur.sellShares += shares;
        cur.sellProceeds += notional - commission;
      }
      map.set(tr.symbol, cur);
    }
    return map;
  });

  /** 每 symbol 今日买入/卖出股数聚合(从 trades),供 UI 显示与冻结量。 */
  todayTradedShares = $derived.by(() => {
    const map = new Map<string, { buy: number; sell: number }>();
    for (const [sym, t] of this.todayTraded) {
      map.set(sym, { buy: t.buyShares, sell: t.sellShares });
    }
    return map;
  });

  /** 每 symbol 最新一条委员会 verdict(verdicts 已按 created_at desc 排序)。 */
  latestVerdictMap = $derived.by(() => {
    const map = new Map<string, Verdict>();
    for (const v of this.verdicts) {
      if (!map.has(v.symbol)) map.set(v.symbol, v); // first = newest due to DESC order
    }
    return map;
  });

  // ── Event Watch Derived ─────────────────────────────────────────────

  filteredEvents = $derived.by(() => {
    const filtered = this.events;

    // Time window filter
    // Note: Date.now() is not reactive — cutoff is refreshed when events or filter change (e.g. after scan).
    const now = Date.now();
    const windows = {
      "all": 0,
      "24h": 86_400_000,
      "48h": 172_800_000,
      "7d": 604_800_000,
    } as const;
    const windowMs = windows[this.eventFilter.timeWindow];
    const cutoff = now - windowMs;

    // Pre-compute timestamps to avoid redundant Date allocations in filter + sort
    const withTs = filtered.map((e) => ({
      e,
      ts: parseEventDate(e.createdAt),
    }));

    // For "all" window, skip time filter; for others, filter by cutoff.
    // Keep events with unparseable timestamps (NaN) — they may have valid data.
    let result = windowMs === 0
      ? withTs
      : withTs.filter((item) => isNaN(item.ts) || item.ts > cutoff);

    // Severity filter
    if (this.eventFilter.severity !== "all") {
      result = result.filter(
        (item) => item.e.severity === this.eventFilter.severity,
      );
    }

    // Search filter
    if (this.eventFilter.search) {
      const q = this.eventFilter.search.toLowerCase();
      result = result.filter(
        (item) =>
          item.e.title.toLowerCase().includes(q) ||
          (item.e.body && item.e.body.toLowerCase().includes(q)),
      );
    }

    // Sort by created_at desc (newest first), all severities treated equally
    result.sort((a, b) => b.ts - a.ts);

    return result.map((item) => item.e);
  });

  // ── Actions ──────────────────────────────────────────────────────────

  async loadAll(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const [holdings, trades, snapshots, verdicts, cash, initialCash, strategies] =
        await Promise.all([
          invoke<Holding[]>("get_holdings"),
          invoke<Trade[]>("get_trades", { symbol: null, limit: 200 }),
          invoke<PnlSnapshot[]>("get_pnl_snapshots", { limit: 80 }),
          invoke<Verdict[]>("get_verdicts", { symbol: null, limit: 50 }),
          invoke<number>("get_cash"),
          invoke<number>("get_initial_cash").catch((e) => { console.warn('[invest] get_initial_cash:', e); return 0; }),
          invoke<Strategy[]>("list_strategies").catch((e) => { console.warn('[invest] list_strategies:', e); return []; }),
        ]);
      this.holdings = holdings;
      this.trades = trades;
      this.pnlSnapshots = snapshots;
      this.verdicts = verdicts;
      this.cash = cash;
      this.initialCash = initialCash;
      this.strategies = strategies;
    } catch (e) {
      this.error = String(e);
    } finally {
      this.loading = false;
    }
  }

  async refreshPrices(tushareToken: string): Promise<void> {
    // Include both hold and watch holdings for price refresh
    const syms = this.holdings.map((h) => h.symbol);
    if (syms.length === 0 || !tushareToken) return;

    // After hours, skip only if every current holding already has a cached price.
    // A newly added watch item without a price entry must still be fetched.
    if (!isMarketOpen() && this.lastRefreshAt > 0 && syms.every((s) => s in this.priceMap)) return;

    try {
      const quotes = await invoke<RealtimeQuote[]>("get_realtime_quotes", {
        tsCodes: syms,
        token: tushareToken,
      });
      // Merge new prices into existing map (preserve cached prices for failed fetches)
      const updated = { ...this.priceMap };
      for (const q of quotes) {
        if (q.close > 0) {
          updated[q.tsCode] = {
            tsCode: q.tsCode,
            name: q.name,
            close: q.close,
            change: q.preClose > 0 ? q.close - q.preClose : 0,
            pctChg: q.preClose > 0 ? ((q.close - q.preClose) / q.preClose) * 100 : 0,
            vol: q.vol,
            amount: q.amount,
          };
        }
      }
      this.priceMap = updated;
      this.lastRefreshAt = Date.now();
    } catch (e) {
      console.warn('[invest] refreshPrices failed:', e);
    }
  }

  async searchStocks(
    name: string,
    tushareToken: string,
  ): Promise<Array<{ tsCode: string; name: string; symbol: string; industry: string }>> {
    return invoke("search_stocks", { name, token: tushareToken });
  }

  async searchEtfs(
    name: string,
    tushareToken: string,
  ): Promise<Array<{ tsCode: string; name: string }>> {
    return invoke("search_etfs", { name, token: tushareToken });
  }

  async getLatestPrice(tsCode: string, tushareToken: string): Promise<number> {
    return invoke("get_latest_price", { tsCode, token: tushareToken });
  }

  // ── Portfolio Operations ─────────────────────────────────────────────

  async buyStock(
    symbol: string,
    name: string,
    qty: number,
    price: number,
    _tushareToken: string,
    assetType?: string,
    tradeDate?: string,
    commission?: number,
    stampDuty?: number,
  ): Promise<void> {
    const amount = qty * price;

    await invoke("record_trade", {
      id: null,
      symbol,
      currency: "CNY",
      kind: "hold",
      action: "buy",
      shares: qty,
      price,
      amount,
      notes: null,
      name: name || null,
      tradeDate: tradeDate || null,
      assetType: assetType || null,
      commission: commission ?? null,
      stampDuty: stampDuty ?? null,
    });

    // record_trade triggers recalculate_holdings_inner which rebuilds holdings
    // and recalculate_cash_inner which auto-updates the cash balance.

    await this.loadAll();
  }

  async sellStock(
    symbol: string,
    qty: number,
    price: number,
    commission?: number,
    stampDuty?: number,
  ): Promise<void> {
    const existing = this.holdHoldings.find((h) => h.symbol === symbol);
    if (!existing) throw new Error("Holding not found");

    const currentShares = existing.shares || 0;
    if (qty > currentShares) {
      throw new Error(`Cannot sell ${qty} shares, only hold ${currentShares}`);
    }

    const amount = qty * price;

    await invoke("record_trade", {
      id: null,
      symbol,
      currency: "CNY",
      kind: "hold",
      action: "sell",
      shares: qty,
      price,
      amount,
      notes: null,
      name: existing.name || null,
      tradeDate: null,
      assetType: existing.assetType || null,
      commission: commission ?? null,
      stampDuty: stampDuty ?? null,
    });

    // record_trade triggers recalculate_holdings_inner which rebuilds holdings
    // and recalculate_cash_inner which auto-updates the cash balance.

    await this.loadAll();
  }

  async addToWatch(
    symbol: string,
    name: string,
    price: number,
    assetType?: string,
  ): Promise<void> {
    const inWatch = this.watchHoldings.find((h) => h.symbol === symbol);
    if (inWatch) throw new Error(`${symbol} already in watchlist`);
    const inHold = this.holdHoldings.find((h) => h.symbol === symbol);
    if (inHold) throw new Error(`${symbol} already in holdings`);

    await invoke("record_trade", {
      id: null,
      symbol,
      currency: "CNY",
      kind: "watch",
      action: "add_watch",
      shares: null,
      price,
      amount: 0,
      notes: null,
      name: name || null,
      tradeDate: null,
      assetType: assetType ?? "stock",
    });

    await this.loadAll();
    // Pre-populate price cache so the watch item shows a price immediately
    // without waiting for the next refreshPrices cycle (especially important
    // when the market is closed and the isMarketOpen guard would skip).
    if (price > 0) {
      const updated = { ...this.priceMap };
      updated[symbol] = {
        tsCode: symbol,
        name,
        close: price,
        change: 0,
        pctChg: 0,
        vol: 0,
        amount: 0,
      };
      this.priceMap = updated;
    }
  }

  async deleteWatch(symbol: string): Promise<void> {
    await invoke("record_trade", {
      id: null,
      symbol,
      currency: "CNY",
      kind: "watch",
      action: "delete_watch",
      shares: null,
      price: null,
      amount: 0,
      notes: null,
      name: null,
      tradeDate: null,
    });
    await this.loadAll();
  }

  async convertWatchToHold(
    symbol: string,
    name: string,
    qty: number,
    price: number,
  ): Promise<void> {
    const watchHolding = this.watchHoldings.find((h) => h.symbol === symbol);
    await invoke("convert_watch_to_hold", {
      symbol,
      currency: watchHolding?.currency ?? "CNY",
      name: name || null,
      shares: qty,
      price,
      assetType: watchHolding?.assetType ?? "stock",
    });
    await this.loadAll();
  }

  // ── Trade Edit/Delete ────────────────────────────────────────────────

  async deleteTrade(id: string): Promise<void> {
    await invoke("delete_trade", { id });
    await this.loadAll();
  }

  async deletePnlSnapshot(id: number): Promise<void> {
    await invoke("delete_pnl_snapshot", { id });
    this.pnlSnapshots = this.pnlSnapshots.filter((s) => s.id !== id);
  }

  /** Lightweight refresh — fetch only pnl_snapshots without full loadAll(). */
  async refreshPnlSnapshots(): Promise<void> {
    this.pnlSnapshots = await invoke<PnlSnapshot[]>("get_pnl_snapshots", { limit: 80 });
  }

  async updateTrade(trade: {
    id: string;
    symbol: string;
    currency: string;
    kind: string;
    action: string;
    shares: number | null;
    price: number | null;
    amount: number | null;
    notes: string | null;
    name?: string | null;
    tradeDate?: string | null;
    assetType?: string | null;
    commission?: number | null;
    stampDuty?: number | null;
  }): Promise<void> {
    await invoke("update_trade", {
      id: trade.id,
      symbol: trade.symbol,
      currency: trade.currency,
      kind: trade.kind,
      action: trade.action,
      shares: trade.shares,
      price: trade.price,
      amount: trade.amount,
      notes: trade.notes,
      name: trade.name ?? null,
      tradeDate: trade.tradeDate ?? null,
      assetType: trade.assetType ?? null,
      commission: trade.commission ?? null,
      stampDuty: trade.stampDuty ?? null,
    });
    await this.loadAll();
  }

  // ── Strategy ─────────────────────────────────────────────────────────

  async saveStrategy(
    id: string | null,
    name: string,
    targets: Array<{ symbol: string; name: string; targetPct: number }>,
    maxSinglePct: number | null,
    minCashPct: number | null,
  ): Promise<void> {
    await invoke("save_strategy", {
      id,
      name,
      targets: JSON.stringify(targets),
      maxSinglePct,
      minCashPct,
    });
    await this.loadAll();
  }

  async deleteStrategy(id: string): Promise<void> {
    await invoke("delete_strategy", { id });
    await this.loadAll();
  }

  // ── Event Watch Actions ─────────────────────────────────────────────

  async fetchEvents(): Promise<void> {
    try {
      const events = await invoke<InvestEvent[]>("get_events", {
        source: null,
        limit: 200,
      });
      this.events = events;
    } catch (e) {
      console.error("Failed to fetch events:", e);
    }
  }

  async fetchScanStatus(): Promise<void> {
    try {
      const status = await invoke<ScanStatus>("get_scan_status");
      this.scanStatus = status;
    } catch (e) {
      console.error("Failed to fetch scan status:", e);
    }
  }

  async triggerScan(): Promise<void> {
    this.error = null;
    this.isScanning = true;
    this.lastScanResult = null;
    try {
      const result = await invoke<ScanResult>("scan_events", {
        normalizerPrompt: null,
        language: currentLocale(),
      });
      this.lastScanResult = result;
      if (result.errors && result.errors.length > 0) {
        console.debug("scan warnings:", result.errors);
      }
      // Refresh events and status after scan (parallel)
      await Promise.all([this.fetchEvents(), this.fetchScanStatus()]);
    } catch (e) {
      this.error = String(e);
      console.error("scan failed:", e);
    } finally {
      this.isScanning = false;
    }
  }

  async triggerCommittee(eventId: string, verdictId: string | null): Promise<boolean> {
    this.error = null;
    try {
      await invoke("mark_event_triggered", { id: eventId, verdictId });
      // Update local state
      this.events = this.events.map((e) =>
        e.id === eventId
          ? { ...e, triggered: true, triggerVerdictId: verdictId }
          : e,
      );
      // Refresh scan status
      await this.fetchScanStatus();
      return true;
    } catch (e) {
      console.error("Failed to mark event triggered:", e);
      this.error = String(e);
      return false;
    }
  }

  setEventFilter(filter: Partial<EventFilter>): void {
    this.eventFilter = { ...this.eventFilter, ...filter };
  }

  // ── Data Initialization ──────────────────────────────────────────────

  async initInvestData(tushareToken: string, initialBalance?: number): Promise<string> {
    const result = await invoke<string>("init_invest_data", {
      token: tushareToken,
      initialBalance: initialBalance ?? null,
    });
    await this.loadAll();
    return result;
  }

  // ── Generic Trade/Holding Operations ─────────────────────────────────

  async recordTrade(trade: {
    symbol: string;
    kind: string;
    action: string;
    shares?: number | null;
    price?: number | null;
    amount?: number | null;
    notes?: string | null;
    name?: string | null;
    tradeDate?: string | null;
    assetType?: string | null;
    commission?: number | null;
    stampDuty?: number | null;
  }): Promise<void> {
    await invoke("record_trade", {
      id: null,
      symbol: trade.symbol,
      currency: "CNY",
      kind: trade.kind,
      action: trade.action,
      shares: trade.shares ?? null,
      price: trade.price ?? null,
      amount: trade.amount ?? null,
      notes: trade.notes ?? null,
      name: trade.name ?? null,
      tradeDate: trade.tradeDate ?? null,
      assetType: trade.assetType ?? null,
      commission: trade.commission ?? null,
      stampDuty: trade.stampDuty ?? null,
    });
    await this.loadAll();
  }

  async updateHoldingMeta(params: {
    symbol: string;
    currency: string;
    kind: string;
    name: string | null;
    notional: number;
    avgCost: number | null;
    shares: number | null;
    entryDate: string | null;
    linkedVerdictId: string | null;
    notes: string | null;
    assetType: string | null;
  }): Promise<void> {
    await invoke("record_trade", {
      id: null,
      symbol: params.symbol,
      currency: params.currency,
      kind: params.kind,
      action: "edit_holding",
      shares: params.shares,
      price: params.avgCost,
      amount: null,
      notes: params.notes,
      name: params.name,
      tradeDate: params.entryDate,
      assetType: params.assetType,
    });
    await this.loadAll();
  }
}

export const investStore = new InvestStore();
