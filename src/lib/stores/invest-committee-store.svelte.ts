import { getTransport } from '$lib/transport';

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return getTransport().invoke<T>(cmd, args);
}

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

class InvestCommitteeStore {
  llmConfig = $state<InvestLlmConfig | null>(null);
  configLoading = $state(false);

  running = $state(false);
  results = $state<CommitteeResult[]>([]);
  runError = $state<string | null>(null);

  rolePrompts = $state<RolePrompts>({});
  showConfigPanel = $state(false);

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

  // ── Committee Run ──────────────────────────────────────────────────────

  async runCommittee(symbols: string[], debateRounds?: number) {
    this.running = true;
    this.runError = null;
    this.results = [];
    try {
      this.results = await invoke<CommitteeResult[]>('run_committee', {
        symbols,
        debateRounds: debateRounds ?? null,
      });
    } catch (e) {
      this.runError = String(e);
    } finally {
      this.running = false;
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
}

export const investCommitteeStore = new InvestCommitteeStore();
