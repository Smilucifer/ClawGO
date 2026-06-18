import { getTransport } from '$lib/transport';
import { roleToBackendIdx } from '$lib/components/invest/pipeline-config';

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

// ── Types ───────────────────────────────────────────────────────────────────

export interface InvestLlmProviderConfig {
  providerId: string;
  apiKey: string;
  baseUrl: string;
  defaultModel: string;
}

export interface InvestLlmConfig {
  providers: InvestLlmProviderConfig[];
  selectedProvider: string;
  debateRounds: number;
  timeoutSecs: number;
  maxConcurrentSymbols?: number;
}

export interface RoundOutputSummary {
  role: string;
  round: number;
  label: string;
  parsed: {
    rawText: string;
    signal?: string;
    strength?: number;
    verdict?: string;
    confidence?: number;
    oneLiner?: string;
    reasoning?: string;
    truncated?: boolean;
    fallbackReason?: string;
    // Macro
    marketPhase?: string;
    emotionTemperature?: string;
    // Quant
    buyPointAssessment?: string;
    valuationAssessment?: string;
    moneyFlow?: string;
    // Risk
    concentrationPct?: number;
    dryPowderCny?: number;
    pnlPct?: number;
    stockRiskSummary?: string;
    // CIO
    catalystTier?: string;
    catalystSummary?: string;
    // Task 4 additions
    executionMode?: string;
    firstTrancheCny?: number;
    signalReason?: string;
    marketPhaseReason?: string;
  };
  latencyMs: number;
  tokensUsed: number;
}

export interface SanityCheckResult {
  gate1Pass: boolean;
  gate2Pass: boolean;
  finalVerdict: string;
  finalConfidence: number;
  notes: string[];
}

export interface CommitteeResult {
  symbol: string;
  finalVerdict: string;
  finalConfidence: number;
  macroSignal: string;
  macroStrength: number | null;
  reasoning: string;
  rounds: RoundOutputSummary[];
  sanityCheck: SanityCheckResult;
  sentinelOverride: {
    reason: string;
    forcedVerdict: string;
    forcedConfidence: number;
  } | null;
  converged: boolean;
  totalLatencyMs: number;
  totalTokens: number;
}

export type RolePrompts = Record<string, string>;

export interface RegimeMetrics {
  latest: number;
  ma20: number;
  ma60: number;
  rsi14: number;
  volatilityAnn: number;
  priceQuantile2y: number;
}

export interface RegimeStepData {
  regime: string;
  reason: string;
  strategyHint: string;
  metrics: RegimeMetrics;
}

export interface ArchivedDecision {
  date: string;
  symbol: string;
  content: string;
}

// ── Streaming event types ───────────────────────────────────────────────────

export type CommitteeEventType =
  | { type: 'committee_start'; symbols: string[]; total: number }
  | { type: 'role_start'; symbol: string; role: string; round: number; stepIndex: number }
  | { type: 'role_complete'; symbol: string; role: string; round: number; summary: RoundOutputSummary; stepIndex: number }
  | { type: 'regime_step'; symbol: string; success: boolean; contextPreview: string; stepIndex: number; regime?: string; reason?: string; strategyHint?: string; metrics?: RegimeMetrics }
  | { type: 'tool_call'; symbol: string; role: string; round: number; toolName: string; arguments: string; result?: string; success: boolean; latencyMs: number }
  | { type: 'symbol_complete'; symbol: string; result: CommitteeResult }
  | { type: 'done'; completed: number; total: number }
  | { type: 'error'; symbol: string; error: string }
  | { type: 'symbol_aborted'; symbol: string };

export interface ToolCallRecord {
  symbol: string;
  role: string;
  round: number;
  toolName: string;
  arguments: string;
  result?: string;
  success: boolean;
  latencyMs: number;
  timestamp: number;
}

export type QueueItemStatus = 'queued' | 'running' | 'done' | 'failed' | 'aborted';

export interface SymbolProgress {
  activeStep: number;       // stepIndex of currently running role (-1 if idle)
  completedSteps: number;   // how many roles finished
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
  regimeData: RegimeStepData | null;  // REGIME step output (populated during streaming)
  failedSteps?: Set<number>;  // explicit per-step failure from orchestrator
  status: QueueItemStatus; // queue-level scheduling status
}

export interface QueueItem {
  symbol: string;
  status: QueueItemStatus;
  error?: string;
  progress?: PersistedProgress | null;
}

/** Serializable subset of SymbolProgress persisted to disk for cross-restart recovery. */
export interface PersistedProgress {
  completedSteps: number;
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
  regimeData: RegimeStepData | null;
  failedSteps: number[]; // Set serialized as array
}

export interface SnapshotHolding {
  symbol: string;
  name?: string | null;
  shares?: number | null;
  notional: number;
  kind: string;
}

export interface PortfolioSnapshot {
  holdings: SnapshotHolding[];
  cash: number;
  totalNotional: number;
  timestamp: string;
}

export interface CommitteeQueueState {
  items: QueueItem[];
  snapshot?: PortfolioSnapshot | null;
  maxConcurrent: number;
  updatedAt: string;
}

// ── Store ───────────────────────────────────────────────────────────────────

export class InvestCommitteeStore {
  llmConfig = $state<InvestLlmConfig | null>(null);
  configLoading = $state(false);

  running = $state(false);
  results = $state<CommitteeResult[]>([]);
  runError = $state<string | null>(null);

  // Streaming state
  streaming = $state(false);
  perSymbolProgress = $state<Map<string, SymbolProgress>>(new Map());
  toolCallHistory = $state<ToolCallRecord[]>([]);

  // Queue scheduler state
  queue = $state<QueueItem[]>([]);
  maxConcurrent = $state(5);
  portfolioSnapshot = $state<PortfolioSnapshot | null>(null);
  private _saveTimer: ReturnType<typeof setTimeout> | null = null;

  rolePrompts = $state<RolePrompts>({});
  showConfigPanel = $state(false);

  private _unlisten: (() => void) | null = null;

  // ── Derived counts ──────────────────────────────────────────────
  get queuedCount() {
    return this.queue.filter((q) => q.status === 'queued').length;
  }
  get runningCount() {
    return this.queue.filter((q) => q.status === 'running').length;
  }
  get doneCount() {
    return this.queue.filter((q) => q.status === 'done').length;
  }

  /** Enqueue symbols and start draining. Optional snapshot is captured once. */
  async addToQueue(symbols: string[], snapshot?: PortfolioSnapshot) {
    if (symbols.length === 0) return;
    await this._ensureListening();
    if (snapshot && !this.portfolioSnapshot) {
      this.portfolioSnapshot = snapshot;
    }
    for (const sym of symbols) {
      const existing = this.queue.find((q) => q.symbol === sym);
      if (existing && (existing.status === 'queued' || existing.status === 'running')) {
        continue; // already pending — dedup
      }
      // Re-enqueue at tail: drop any prior entry, then push fresh.
      this.queue = this.queue.filter((q) => q.symbol !== sym);
      this.queue.push({ symbol: sym, status: 'queued' });
      this.perSymbolProgress.set(sym, this._freshProgress('queued'));
      this.results = this.results.filter((r) => r.symbol !== sym);
      this.toolCallHistory = this.toolCallHistory.filter((e) => e.symbol !== sym);
    }
    this.queue = [...this.queue];
    this.perSymbolProgress = new Map(this.perSymbolProgress);
    this._recomputeRunning();
    this._persistQueue();
    this._drainQueue();
  }

  /** Re-run a finished/failed/aborted symbol — appended to the queue tail. */
  async retrySymbol(symbol: string) {
    await this.addToQueue([symbol]);
  }

  /** Cancel one in-flight symbol; backend also emits symbol_aborted. */
  async abortSymbol(symbol: string) {
    try {
      await invoke('abort_committee_symbol', { symbol });
    } catch (e) {
      console.error('abort_committee_symbol failed:', e);
    }
    this._settleQueue(symbol, 'aborted');
  }

  /** Cancel everything in flight and clear queued items. */
  async abortAll() {
    try {
      await invoke('abort_committee_all');
    } catch (e) {
      console.error('abort_committee_all failed:', e);
    }
    for (const item of this.queue) {
      if (item.status === 'running' || item.status === 'queued') {
        item.status = 'aborted';
      }
    }
    this.queue = [...this.queue];
    for (const [sym, p] of this.perSymbolProgress) {
      if (p.status === 'running' || p.status === 'queued') {
        this.perSymbolProgress.set(sym, { ...p, status: 'aborted', activeStep: -1 });
      }
    }
    this.perSymbolProgress = new Map(this.perSymbolProgress);
    this._recomputeRunning();
    this._persistQueue();
  }

  setMaxConcurrent(n: number) {
    this.maxConcurrent = n;
    this._persistQueue();
    this._drainQueue();
  }

  // ── Persistence ─────────────────────────────────────────────────
  async loadQueue() {
    try {
      const state = await invoke<CommitteeQueueState>('load_committee_queue');
      this.maxConcurrent = state.maxConcurrent && state.maxConcurrent > 0 ? state.maxConcurrent : 5;
      this.portfolioSnapshot = state.snapshot ?? null;
      // Restore queue for display; running items (interrupted by restart) → queued.
      this.queue = (state.items ?? []).map((it) => ({
        symbol: it.symbol,
        status: it.status === 'running' ? ('queued' as QueueItemStatus) : it.status,
        error: it.error,
      }));
      const progress = new Map<string, SymbolProgress>();
      const restoredResults: CommitteeResult[] = [];
      for (const item of state.items ?? []) {
        const status: QueueItemStatus = item.status === 'running' ? 'queued' : item.status;
        if (item.progress) {
          const sp = this._fromPersisted(item.progress, status);
          progress.set(item.symbol, sp);
          if (sp.result) restoredResults.push(sp.result);
        } else {
          progress.set(item.symbol, this._freshProgress(status));
        }
      }
      this.perSymbolProgress = progress;
      this.results = restoredResults;
      this._recomputeRunning();
    } catch (e) {
      console.error('load_committee_queue failed:', e);
    }
  }

  private _persistQueue() {
    if (this._saveTimer) clearTimeout(this._saveTimer);
    this._saveTimer = setTimeout(() => void this._flushQueue(), 300);
  }

  private async _flushQueue() {
    const state: CommitteeQueueState = {
      items: this.queue.map((q) => {
        const p = this.perSymbolProgress.get(q.symbol);
        return {
          symbol: q.symbol,
          status: q.status,
          error: q.error,
          progress: p ? this._toPersisted(p) : null,
        };
      }),
      snapshot: this.portfolioSnapshot,
      maxConcurrent: this.maxConcurrent,
      updatedAt: new Date().toISOString(),
    };
    try {
      await invoke('save_committee_queue', { state });
    } catch (e) {
      console.error('save_committee_queue failed:', e);
    }
  }

  // ── Internal scheduling ─────────────────────────────────────────
  private _freshProgress(status: QueueItemStatus): SymbolProgress {
    return {
      activeStep: -1,
      completedSteps: 0,
      completedRounds: [],
      done: false,
      error: null,
      result: null,
      regimeData: null,
      failedSteps: new Set(),
      status,
    };
  }

  /** Convert in-memory SymbolProgress → serializable PersistedProgress. */
  private _toPersisted(p: SymbolProgress): PersistedProgress {
    return {
      completedSteps: p.completedSteps,
      completedRounds: p.completedRounds,
      done: p.done,
      error: p.error,
      result: p.result,
      regimeData: p.regimeData,
      failedSteps: p.failedSteps ? Array.from(p.failedSteps) : [],
    };
  }

  /** Rebuild SymbolProgress from persisted snapshot, restoring transient fields. */
  private _fromPersisted(pp: PersistedProgress, status: QueueItemStatus): SymbolProgress {
    return {
      activeStep: -1,
      completedSteps: pp.completedSteps,
      completedRounds: pp.completedRounds ?? [],
      done: pp.done,
      error: pp.error,
      result: pp.result,
      regimeData: pp.regimeData,
      failedSteps: new Set(pp.failedSteps ?? []),
      status,
    };
  }

  private async _ensureListening() {
    if (this._unlisten) return;
    this._unlisten = await getTransport().listen<CommitteeEventType>(
      'committee-event',
      (event) => this._handleCommitteeEvent(event),
    );
  }

  private _recomputeRunning() {
    const active = this.queue.some((q) => q.status === 'queued' || q.status === 'running');
    this.running = active;
    this.streaming = active;
  }

  private _drainQueue() {
    const running = this.runningCount;
    let slots = this.maxConcurrent - running;
    if (slots <= 0) return;
    const toStart: string[] = [];
    for (const item of this.queue) {
      if (slots <= 0) break;
      if (item.status !== 'queued') continue;
      toStart.push(item.symbol);
      slots -= 1;
    }
    for (const sym of toStart) this._startSymbol(sym);
  }

  private _startSymbol(symbol: string) {
    this._markRunning(symbol);
    invoke<CommitteeResult[]>('run_committee_stream', {
      symbols: [symbol],
      debateRounds: null,
      dryRun: false,
    }).catch((e) => {
      // Whole invoke rejected without emitting symbol_complete/error.
      const item = this.queue.find((q) => q.symbol === symbol);
      if (item && item.status === 'running') {
        this._settleQueue(symbol, 'failed', String(e));
      }
    });
  }

  private _markRunning(symbol: string) {
    const item = this.queue.find((q) => q.symbol === symbol);
    if (item) {
      item.status = 'running';
      item.error = undefined;
      this.queue = [...this.queue];
    }
    const p = this.perSymbolProgress.get(symbol);
    if (p) {
      this.perSymbolProgress.set(symbol, { ...p, status: 'running' });
      this.perSymbolProgress = new Map(this.perSymbolProgress);
    }
    this._recomputeRunning();
  }

  /** Move a symbol to a terminal/aborted status, then persist + drain. */
  private _settleQueue(symbol: string, status: QueueItemStatus, error?: string) {
    const item = this.queue.find((q) => q.symbol === symbol);
    if (item && item.status !== status) {
      item.status = status;
      item.error = error;
      this.queue = [...this.queue];
    }
    const p = this.perSymbolProgress.get(symbol);
    if (p && p.status !== status) {
      const next: SymbolProgress = { ...p, status };
      if (status === 'aborted') next.activeStep = -1;
      this.perSymbolProgress.set(symbol, next);
      this.perSymbolProgress = new Map(this.perSymbolProgress);
    }
    this._recomputeRunning();
    this._persistQueue();
    this._drainQueue();
  }

  // ── Config ─────────────────────────────────────────────────────────────

  async loadConfig() {
    this.configLoading = true;
    try {
      this.llmConfig = await invoke<InvestLlmConfig>('get_llm_config');
    } catch (e) {
      console.error('Failed to load LLM config:', e);
    } finally {
      this.configLoading = false;
    }
  }

  async saveConfig(config: InvestLlmConfig) {
    await invoke('save_llm_config', { config });
    this.llmConfig = config;
  }

  // ── Event handler ──────────────────────────────────────────────────────

  private _handleCommitteeEvent(event: CommitteeEventType) {
    // Events that don't mutate perSymbolProgress — handle without copying the Map
    switch (event.type) {
      case 'committee_start':
      case 'done':
        return;
      case 'tool_call':
        this.toolCallHistory = [
          ...this.toolCallHistory,
          {
            symbol: event.symbol,
            role: event.role,
            round: event.round,
            toolName: event.toolName,
            arguments: event.arguments,
            result: event.result,
            success: event.success,
            latencyMs: event.latencyMs,
            timestamp: Date.now(),
          },
        ];
        return;
    }

    // Mutating events — copy the Map for reactivity
    const progress = new Map(this.perSymbolProgress);

    switch (event.type) {
      case 'role_start': {
        const p = progress.get(event.symbol);
        if (p) {
          progress.set(event.symbol, { ...p, activeStep: event.stepIndex });
        }
        break;
      }

      case 'role_complete': {
        const p = progress.get(event.symbol);
        if (p) {
          const failedSteps = p.failedSteps ? new Set(p.failedSteps) : new Set<number>();
          if (event.summary.parsed?.fallbackReason) {
            const stepIdx = roleToBackendIdx(event.role, event.round);
            if (stepIdx !== -1) {
              failedSteps.add(stepIdx);
            }
          }
          progress.set(event.symbol, {
            ...p,
            activeStep: -1,
            completedSteps: p.completedSteps + 1,
            completedRounds: [...p.completedRounds, event.summary],
            failedSteps,
          });
        }
        break;
      }

      case 'regime_step': {
        const p = progress.get(event.symbol);
        if (p) {
          if (event.success && event.regime && event.reason && event.strategyHint && event.metrics) {
            progress.set(event.symbol, {
              ...p,
              activeStep: -1,
              completedSteps: Math.max(p.completedSteps, 2),
              regimeData: {
                regime: event.regime,
                reason: event.reason,
                strategyHint: event.strategyHint,
                metrics: event.metrics,
              },
            });
          } else if (event.success) {
            progress.set(event.symbol, {
              ...p,
              activeStep: -1,
              completedSteps: Math.max(p.completedSteps, 2),
            });
          } else {
            progress.set(event.symbol, {
              ...p,
              activeStep: -1,
              error: 'Regime computation failed',
            });
          }
        }
        break;
      }

      case 'symbol_complete': {
        const p = progress.get(event.symbol);
        if (p) {
          progress.set(event.symbol, {
            ...p,
            activeStep: -1,
            done: true,
            result: event.result,
          });
        }
        // Replace existing result for this symbol, or append if new
        const existingIdx = this.results.findIndex((r) => r.symbol === event.symbol);
        if (existingIdx >= 0) {
          this.results = this.results.map((r, i) => (i === existingIdx ? event.result : r));
        } else {
          this.results = [...this.results, event.result];
        }
        break;
      }

      case 'error': {
        const p = progress.get(event.symbol);
        if (p) {
          progress.set(event.symbol, { ...p, error: event.error, done: true, activeStep: -1 });
        }
        break;
      }

    }

    this.perSymbolProgress = progress;

    // Queue-level transitions after progress is committed.
    if (event.type === 'symbol_complete') {
      this._settleQueue(event.symbol, 'done');
    } else if (event.type === 'error') {
      this._settleQueue(event.symbol, 'failed', event.error);
    } else if (event.type === 'symbol_aborted') {
      this._settleQueue(event.symbol, 'aborted');
    }
  }

  // ── Role Prompts ───────────────────────────────────────────────────────

  async loadRolePrompts() {
    try {
      this.rolePrompts = await invoke<RolePrompts>('get_role_prompts');
    } catch (e) {
      console.error('Failed to load role prompts:', e);
    }
  }

  async saveRolePrompt(role: string, content: string) {
    await invoke('save_role_prompt', { role, content });
    this.rolePrompts[role] = content;
  }

  // ── Archive loading ────────────────────────────────────────────────────

  async loadArchive(symbol: string, days?: number): Promise<ArchivedDecision[]> {
    try {
      return await invoke<ArchivedDecision[]>('load_committee_archive', {
        symbol,
        days: days ?? 7,
      });
    } catch (e) {
      console.error('Failed to load archive:', e);
      return [];
    }
  }
}

export const investCommitteeStore = new InvestCommitteeStore();
