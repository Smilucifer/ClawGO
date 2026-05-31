import { getTransport } from "$lib/transport";
import type {
  Holding,
  Trade,
  PnlSnapshot,
  Verdict,
  Strategy,
  PriceQuote,
  InvestEvent,
  ScanStatus,
  EventFilter,
} from "$lib/types";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

class InvestStore {
  // ── State ────────────────────────────────────────────────────────────
  holdings = $state<Holding[]>([]);
  trades = $state<Trade[]>([]);
  pnlSnapshots = $state<PnlSnapshot[]>([]);
  verdicts = $state<Verdict[]>([]);
  cash = $state<number>(0);
  strategies = $state<Strategy[]>([]);

  loading = $state<boolean>(false);
  error = $state<string | null>(null);

  /** Live price cache: tsCode → PriceQuote */
  priceMap = $state<Record<string, PriceQuote>>({});

  // ── Event Watch State ───────────────────────────────────────────────
  events = $state<InvestEvent[]>([]);
  eventFilter = $state<EventFilter>({
    timeWindow: "24h",
    severity: "all",
    search: "",
  });
  scanStatus = $state<ScanStatus | null>(null);
  isScanning = $state<boolean>(false);

  // ── Derived ──────────────────────────────────────────────────────────
  holdHoldings = $derived(this.holdings.filter((h) => h.kind === "hold"));
  watchHoldings = $derived(this.holdings.filter((h) => h.kind === "watch"));
  holdCount = $derived(this.holdHoldings.length);
  watchCount = $derived(this.watchHoldings.length);

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
      ? ((this.totalAssets - this.totalCostBasis) / this.totalCostBasis) * 100
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
      const [holdings, trades, snapshots, verdicts, cash, strategies] =
        await Promise.all([
          invoke<Holding[]>("get_holdings"),
          invoke<Trade[]>("get_trades", { symbol: null, limit: 200 }),
          invoke<PnlSnapshot[]>("get_pnl_snapshots", { limit: 80 }),
          invoke<Verdict[]>("get_verdicts", { symbol: null, limit: 50 }),
          invoke<number>("get_cash"),
          invoke<Strategy[]>("list_strategies").catch(() => []),
        ]);
      this.holdings = holdings;
      this.trades = trades;
      this.pnlSnapshots = snapshots;
      this.verdicts = verdicts;
      this.cash = cash;
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

    // Merge new prices into existing map (preserve cached prices for failed fetches)
    const updated = { ...this.priceMap };
    for (const sym of syms) {
      try {
        const bars = await invoke<
          Array<{
            tsCode: string;
            close: number;
            change: number;
            pctChg: number;
            vol: number;
            amount: number;
          }>
        >("get_daily_bars", {
          tsCode: sym,
          startDate: "",
          endDate: "",
          token: tushareToken,
        });
        if (bars.length > 0) {
          const latest = bars[0];
          updated[sym] = {
            tsCode: latest.tsCode,
            name: "",
            close: latest.close,
            change: latest.change,
            pctChg: latest.pctChg,
            vol: latest.vol,
            amount: latest.amount,
          };
        }
      } catch {
        // Keep existing cached price for this symbol
      }
    }
    this.priceMap = updated;
  }

  async searchStocks(
    name: string,
    tushareToken: string,
  ): Promise<Array<{ tsCode: string; name: string; symbol: string; industry: string }>> {
    return invoke("search_stocks", { name, token: tushareToken });
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
  ): Promise<void> {
    const amount = qty * price;
    const now = new Date().toISOString();

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
    });

    const existing = this.holdHoldings.find((h) => h.symbol === symbol);
    if (existing && existing.shares != null && existing.shares > 0 && existing.avgCost != null) {
      const newShares = existing.shares + qty;
      const newAvgCost =
        (existing.avgCost * existing.shares + price * qty) / newShares;
      await invoke("update_holding", {
        symbol,
        currency: "CNY",
        kind: "hold",
        name: name || existing.name,
        notional: 0,
        avgCost: newAvgCost,
        shares: newShares,
        entryDate: existing.entryDate,
        linkedVerdictId: existing.linkedVerdictId,
        notes: existing.notes,
        assetType: assetType ?? existing.assetType ?? "stock",
      });
    } else {
      await invoke("add_holding", {
        symbol,
        currency: "CNY",
        kind: "hold",
        name,
        notional: 0,
        avgCost: price,
        shares: qty,
        entryDate: now.split("T")[0],
        linkedVerdictId: null,
        notes: null,
        assetType: assetType ?? "stock",
      });
    }

    await invoke("update_cash", { available: this.cash - amount });
    this.cash = this.cash - amount;

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
    });

    const remaining = currentShares - qty;
    if (remaining <= 0.0001) {
      await invoke("delete_holding", { symbol, currency: "CNY", kind: "hold" });
    } else {
      await invoke("update_holding", {
        symbol,
        currency: "CNY",
        kind: "hold",
        name: existing.name,
        notional: 0,
        avgCost: existing.avgCost,
        shares: remaining,
        entryDate: existing.entryDate,
        linkedVerdictId: existing.linkedVerdictId,
        notes: existing.notes,
        assetType: existing.assetType ?? "stock",
      });
    }

    await invoke("update_cash", { available: this.cash + amount });
    this.cash = this.cash + amount;

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
    this.trades = this.trades.filter((t) => t.id !== id);
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
    try {
      await invoke("scan_events", {
        normalizerPrompt: null,
      });
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
}

export const investStore = new InvestStore();
