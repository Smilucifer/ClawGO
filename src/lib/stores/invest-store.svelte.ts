import { getTransport } from "$lib/transport";
import { currentLocale } from "$lib/i18n/index.svelte";
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
  watchHoldings = $derived(this.holdings.filter((h) => h.kind === "watch"));
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
      const price = this.priceMap[h.symbol]?.close;
      if (price && h.shares) return sum + price * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );

  totalAssets = $derived(this.cash + this.holdingsMarketValue);

  totalCostBasis = $derived(
    this.holdHoldings.reduce((sum, h) => {
      if (h.avgCost && h.shares) return sum + h.avgCost * h.shares;
      return sum + (h.notional || 0);
    }, 0),
  );

  totalReturnPct = $derived(
    this.totalCostBasis > 0
      ? ((this.holdingsMarketValue - this.totalCostBasis) / this.totalCostBasis) * 100
      : 0,
  );

  // ── Event Watch Derived ─────────────────────────────────────────────

  filteredEvents = $derived.by(() => {
    const filtered = this.events;

    // Time window filter
    // Note: Date.now() is not reactive — cutoff is refreshed when events or filter change (e.g. after scan).
    const now = Date.now();
    const windows = {
      "24h": 86_400_000,
      "48h": 172_800_000,
      "7d": 604_800_000,
    } as const;
    const cutoff = now - windows[this.eventFilter.timeWindow];

    // Pre-compute timestamps to avoid redundant Date allocations in filter + sort
    const withTs = filtered.map((e) => ({
      e,
      ts: new Date(e.createdAt).getTime(),
    }));

    let result = withTs.filter((item) => item.ts > cutoff);

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

    // Sort: HIGH severity first, then by created_at desc
    result.sort((a, b) => {
      if (a.e.severity === "high" && b.e.severity !== "high") return -1;
      if (b.e.severity === "high" && a.e.severity !== "high") return 1;
      return b.ts - a.ts;
    });

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
    });

    // record_trade triggers recalculate_holdings_inner which fully rebuilds
    // the holdings table — no need for a separate add_holding/update_holding call.

    await invoke("update_cash", { available: this.cash - amount });

    await this.loadAll();
  }

  async sellStock(symbol: string, qty: number, price: number): Promise<void> {
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
    });

    // record_trade triggers recalculate_holdings_inner which fully rebuilds
    // the holdings table — no need for a separate delete_holding/update_holding call.

    await invoke("update_cash", { available: this.cash + amount });

    await this.loadAll();
  }

  async updateCash(newBalance: number, reason?: string): Promise<void> {
    await invoke("update_cash", { available: newBalance });
    await invoke("record_trade", {
      id: null,
      symbol: "CASH",
      currency: "CNY",
      kind: "hold",
      action: "cash_adjust",
      shares: null,
      price: null,
      amount: newBalance,
      notes: reason || null,
      name: null,
      tradeDate: null,
    });
    this.cash = newBalance;
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

    try {
      await invoke("add_holding", {
        symbol,
        currency: "CNY",
        kind: "watch",
        name,
        notional: 0,
        avgCost: price,
        shares: null,
        entryDate: new Date().toISOString().split("T")[0],
        linkedVerdictId: null,
        notes: null,
        assetType: assetType ?? "stock",
      });

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
      });
    } finally {
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
  }

  async deleteWatch(symbol: string): Promise<void> {
    try {
      await invoke("delete_holding", { symbol, currency: "CNY", kind: "watch" });

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
    } finally {
      await this.loadAll();
    }
  }

  async convertWatchToHold(
    symbol: string,
    name: string,
    qty: number,
    price: number,
  ): Promise<void> {
    const amount = qty * price;
    const prevCash = this.cash;

    // Save watch holding data for rollback
    const watchHolding = this.watchHoldings.find((h) => h.symbol === symbol);

    try {
      await invoke("delete_holding", { symbol, currency: "CNY", kind: "watch" });

      await invoke("add_holding", {
        symbol,
        currency: "CNY",
        kind: "hold",
        name,
        notional: 0,
        avgCost: price,
        shares: qty,
        entryDate: new Date().toISOString().split("T")[0],
        linkedVerdictId: null,
        notes: "converted from watchlist",
        assetType: watchHolding?.assetType ?? "stock",
      });

      await invoke("record_trade", {
        id: null,
        symbol,
        currency: "CNY",
        kind: "hold",
        action: "convert_watch_to_hold",
        shares: qty,
        price,
        amount,
        notes: null,
        name: name || null,
        tradeDate: null,
      });

      await invoke("update_cash", { available: this.cash - amount });
      this.cash = this.cash - amount;
    } catch (e) {
      // Rollback: restore watch holding and cash
      this.cash = prevCash;
      if (watchHolding) {
        await invoke("add_holding", {
          symbol,
          currency: "CNY",
          kind: "watch",
          name: watchHolding.name,
          notional: watchHolding.notional ?? 0,
          avgCost: watchHolding.avgCost ?? null,
          shares: watchHolding.shares ?? null,
          entryDate: watchHolding.entryDate ?? null,
          linkedVerdictId: watchHolding.linkedVerdictId ?? null,
          notes: watchHolding.notes ?? null,
          assetType: watchHolding.assetType ?? "stock",
        }).catch(() => {}); // Best-effort rollback
      }
      throw e;
    }

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
    await invoke("update_holding", params);
    await this.loadAll();
  }
}

export const investStore = new InvestStore();
