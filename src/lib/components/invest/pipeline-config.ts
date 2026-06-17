/**
 * 委员会 Pipeline 步骤定义
 * 在 CommitteeLiveTab / CommitteeReplayTab / CommitteeToolsTab 之间共享
 */
import type { RoundOutputSummary, SymbolProgress } from '$lib/stores/invest-committee-store.svelte';

export const STEP_DEFS = [
  { key: 'macro', labelKey: 'invest_pipeline_macro' as const, color: '#8b5cf6', backendIdx: 0 },
  { key: 'regime', labelKey: 'invest_pipeline_regime' as const, color: '#a78bfa', backendIdx: 1 },
  { key: 'quant_r1', labelKey: 'invest_pipeline_quant_r1' as const, color: '#3b82f6', backendIdx: 2 },
  { key: 'risk_r1', labelKey: 'invest_pipeline_risk_r1' as const, color: '#f97316', backendIdx: 3 },
  { key: 'quant_r2', labelKey: 'invest_pipeline_quant_r2' as const, color: '#3b82f6', backendIdx: 4 },
  { key: 'risk_r2', labelKey: 'invest_pipeline_risk_r2' as const, color: '#f97316', backendIdx: 5 },
  { key: 'cio', labelKey: 'invest_pipeline_cio' as const, color: '#eab308', backendIdx: 6 },
] as const;

/**
 * 4 个角色的颜色（与 STEP_DEFS 颜色保持一致；用于 Tools 表格、徽章等）
 */
export const ROLE_COLORS: Record<'macro' | 'quant' | 'risk' | 'cio', string> = {
  macro: '#8b5cf6',
  quant: '#3b82f6',
  risk: '#f97316',
  cio: '#eab308',
};

/**
 * 角色名称到后端 step index 的映射
 */
export function roleToBackendIdx(role: string, round: number): number {
  if (role === 'macro') return 0;
  if (role === 'quant' && round === 1) return 2;
  if (role === 'risk' && round === 1) return 3;
  if (role === 'quant') return 4;
  if (role === 'risk') return 5;
  if (role === 'cio') return 6;
  return -1;
}

/**
 * 计算某个 step 的渲染状态：pending / active / done / error / failed
 * 在 Live 和 Replay (simulate 模式) 之间共享
 */
export function getStepState(
  symProgress: SymbolProgress | undefined,
  backendIdx: number,
  pipelineStarted?: boolean,
): 'pending' | 'active' | 'done' | 'error' | 'failed' | 'aborted' {
  if (!symProgress) return 'pending';
  if (backendIdx === -1) return pipelineStarted ? 'done' : 'pending';

  // Completed steps stay done regardless of later abort.
  for (const round of symProgress.completedRounds) {
    if (roleToBackendIdx(round.role, round.round) === backendIdx) return 'done';
  }

  // Check failed steps (explicit failure from orchestrator).
  if (symProgress.failedSteps?.has(backendIdx)) return 'failed';

  // Aborted symbol: any not-yet-completed step is aborted.
  if (symProgress.status === 'aborted') return 'aborted';

  if (symProgress.activeStep === backendIdx) return 'active';
  if (symProgress.done && !symProgress.error) return 'done';
  if (symProgress.error && backendIdx >= symProgress.completedSteps) return 'error';
  return 'pending';
}

/**
 * 找到指定 backend step 对应的 round 输出
 */
export function getRoundForStep(
  symProgress: SymbolProgress | undefined,
  backendIdx: number,
): RoundOutputSummary | undefined {
  if (!symProgress) return undefined;
  return symProgress.completedRounds.find(
    (r) => roleToBackendIdx(r.role, r.round) === backendIdx,
  );
}
