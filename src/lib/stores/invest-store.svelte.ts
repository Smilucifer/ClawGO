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
    let filtered = this.events;

    // Time window filter
    const now = Date.now();
    const windows: Record<string, number> = {
      "24h": 86_400_000,
      "48h": 172_800_000,
      "7d": 604_800_000,
    };
    const cutoff = now - (windows[this.eventFilter.timeWindow] ?? 86_400_000);
    filtered = filtered.filter(
      (e) => new Date(e.createdAt).getTime() > cutoff,
    );

    // Severity filter
    if (this.eventFilter.severity !== "all") {
      filtered = filtered.filter(
        (e) => e.severity === this.eventFilter.severity,
      );
    }

    // Search filter
    if (this.eventFilter.search) {
      const q = this.eventFilter.search.toLowerCase();
      filtered = filtered.filter(
        (e) =>
          e.title.toLowerCase().includes(q) ||
          (e.body && e.body.toLowerCase().includes(q)),
      );
    }

    // Sort: HIGH severity first, then by created_at desc
    filtered.sort((a, b) => {
      if (a.severity === "high" && b.severity !== "high") return -1;
      if (b.severity === "high" && a.severity !== "high") return 1;
      return (
        new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
      );
    });

    return filtered;
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
    const holdSyms = this.holdHoldings.map((h) => h.symbol);
    if (holdSyms.length === 0 || !tushareToken) return;

    // Merge new prices into existing map (preserve cached prices for failed fetches)
    const updated = { ...this.priceMap };
    for (const sym of holdSyms) {
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
    tushareToken: string,
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
    if (existing && existing.shares && existing.avgCost) {
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

  async convertWatchToHold(
    symbol: string,
    name: string,
    qty: number,
    price: number,
  ): Promise<void> {
    const amount = qty * price;

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
    const events = await invoke<InvestEvent[]>("get_events", {
      source: null,
      limit: 200,
    });
    this.events = events;
  }

  async fetchScanStatus(): Promise<void> {
    const status = await invoke<ScanStatus>("get_scan_status");
    this.scanStatus = status;
  }

  async triggerScan(): Promise<unknown> {
    this.isScanning = true;
    try {
      const result = await invoke("scan_events", {
        normalizerPrompt: null,
      });
      // Refresh events and status after scan
      await this.fetchEvents();
      await this.fetchScanStatus();
      return result;
    } finally {
      this.isScanning = false;
    }
  }

  setEventFilter(filter: Partial<EventFilter>): void {
    this.eventFilter = { ...this.eventFilter, ...filter };
  }
}

export const investStore = new InvestStore();
