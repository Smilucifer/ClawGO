import { getTransport } from '$lib/transport';

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
  debateRounds: number;
  emergencyBufferCny: number;
  timeoutSecs: number;
}

export interface RoundOutputSummary {
  role: string;
  round: number;
  label: string;
  parsed: { rawText: string; signal?: string; strength?: number };
  latencyMs: number;
  tokensUsed: number;
}

export interface SanityCheckResult {
  gate1Pass: boolean;
  gate2Pass: boolean;
  gate3Pass: boolean;
  gate4Pass: boolean;
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
  | { type: 'symbol_complete'; symbol: string; result: CommitteeResult }
  | { type: 'done'; completed: number; total: number }
  | { type: 'error'; symbol: string; error: string };

export interface SymbolProgress {
  activeStep: number;       // stepIndex of currently running role (-1 if idle)
  completedSteps: number;   // how many roles finished
  completedRounds: RoundOutputSummary[];
  done: boolean;
  error: string | null;
  result: CommitteeResult | null;
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

  async runCommittee(symbols: string[], debateRounds?: number) {
    // Guard against concurrent calls — tear down previous listener first
    const prevUnlisten = this._unlisten;
    this._unlisten = null;
    prevUnlisten?.();

    this.streaming = true;
    this.running = true;
    this.runError = null;
    this.results = [];
    this.activeSymbols = symbols;

    const progress = new Map<string, SymbolProgress>();
    for (const s of symbols) {
      progress.set(s, {
        activeStep: -1,
        completedSteps: 0,
        completedRounds: [],
        done: false,
        error: null,
        result: null,
      });
    }
    this.perSymbolProgress = progress;

    try {
      // Subscribe to streaming events
      this._unlisten = await getTransport().listen<CommitteeEventType>(
        'committee-event',
        (event) => this._handleCommitteeEvent(event),
      );

      // Invoke the streaming command — results arrive via events,
      // but the final return value is also captured.
      const results = await invoke<CommitteeResult[]>('run_committee_stream', {
        symbols,
        debateRounds: debateRounds ?? null,
      });
      this.results = results;
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
    const progress = new Map(this.perSymbolProgress);

    switch (event.type) {
      case 'committee_start': {
        // Already initialized in runCommittee
        break;
      }

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
          progress.set(event.symbol, {
            ...p,
            activeStep: -1,
            completedSteps: p.completedSteps + 1,
            completedRounds: [...p.completedRounds, event.summary],
          });
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
        // Also append to results incrementally
        if (!this.results.find((r) => r.symbol === event.symbol)) {
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

      case 'done': {
        // Final event — streaming is effectively done
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
