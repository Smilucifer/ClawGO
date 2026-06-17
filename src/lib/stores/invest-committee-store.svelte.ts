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
  | { type: 'error'; symbol: string; error: string };

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

export interface SymbolProgress {
  activeStep: number;       // stepIndex of currently running role (-1 if idle)
  completedSteps: number;   // how many roles finished
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
  regimeData: RegimeStepData | null;  // REGIME step output (populated during streaming)
  failedSteps?: Set<number>;  // explicit per-step failure from orchestrator
}

// ── Store ───────────────────────────────────────────────────────────────────

class InvestCommitteeStore {
  llmConfig = $state<InvestLlmConfig | null>(null);
  configLoading = $state(false);

  running = $state(false);
  results = $state<CommitteeResult[]>([]);
  runError = $state<string | null>(null);

  // Streaming state
  streaming = $state(false);
  activeSymbols = $state<string[]>([]);
  perSymbolProgress = $state<Map<string, SymbolProgress>>(new Map());
  toolCallHistory = $state<ToolCallRecord[]>([]);

  rolePrompts = $state<RolePrompts>({});
  showConfigPanel = $state(false);

  private _unlisten: (() => void) | null = null;

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

  // ── Committee Run (streaming) ──────────────────────────────────────────

  async runCommittee(symbols: string[], debateRounds?: number, dryRun?: boolean) {
    // Guard against concurrent calls — tear down previous listener first
    const prevUnlisten = this._unlisten;
    this._unlisten = null;
    prevUnlisten?.();

    this.streaming = true;
    this.running = true;
    this.runError = null;
    this.activeSymbols = symbols;

    const runSet = new Set(symbols);

    // Only remove results for symbols being re-run; preserve others
    this.results = this.results.filter((r) => !runSet.has(r.symbol));

    // Only remove tool call history for symbols being re-run; preserve others
    this.toolCallHistory = this.toolCallHistory.filter((e) => !runSet.has(e.symbol));

    // Only reset progress for symbols being re-run; preserve others
    for (const s of symbols) {
      this.perSymbolProgress.set(s, {
        activeStep: -1,
        completedSteps: 0,
        completedRounds: [],
        done: false,
        error: null,
        result: null,
        regimeData: null,
        failedSteps: new Set(),
      });
    }
    // Trigger reactivity without replacing the entire Map
    this.perSymbolProgress = new Map(this.perSymbolProgress);

    try {
      // Subscribe to streaming events
      this._unlisten = await getTransport().listen<CommitteeEventType>(
        'committee-event',
        (event) => this._handleCommitteeEvent(event),
      );

      // Invoke the streaming command — results arrive via events,
      // but the final return value is also captured.
      const batchResults = await invoke<CommitteeResult[]>('run_committee_stream', {
        symbols,
        debateRounds: debateRounds ?? null,
        dryRun: dryRun ?? false,
      });
      // Merge: replace results for this batch, preserve results from previous runs
      const preserved = this.results.filter((r) => !runSet.has(r.symbol));
      this.results = [...preserved, ...batchResults];
    } catch (e) {
      this.runError = String(e);
    } finally {
      this.streaming = false;
      this.running = false;
      this._unlisten?.();
      this._unlisten = null;
    }
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
