/**
 * 委员会 Pipeline 步骤定义
 * 在 CommitteeLiveTab 和 CommitteeReplayTab 之间共享
 */
export const STEP_DEFS = [
  { key: 'macro', labelKey: 'invest_pipeline_macro' as const, color: '#8b5cf6', backendIdx: 0 },
  { key: 'regime', labelKey: 'invest_pipeline_regime' as const, color: '#a78bfa', backendIdx: 1 },
  { key: 'quant_r1', labelKey: 'invest_pipeline_quant_r1' as const, color: '#3b82f6', backendIdx: 2 },
  { key: 'risk_r1', labelKey: 'invest_pipeline_risk_r1' as const, color: '#f97316', backendIdx: 3 },
  { key: 'quant_r2', labelKey: 'invest_pipeline_quant_r2' as const, color: '#3b82f6', backendIdx: 4 },
  { key: 'risk_r2', labelKey: 'invest_pipeline_risk_r2' as const, color: '#f97316', backendIdx: 5 },
  { key: 'l4_officer', labelKey: 'invest_pipeline_l4_officer' as const, color: '#ef4444', backendIdx: 6 },
  { key: 'cio', labelKey: 'invest_pipeline_cio' as const, color: '#eab308', backendIdx: 7 },
] as const;

/**
 * 角色名称到后端 step index 的映射
 */
export function roleToBackendIdx(role: string, round: number): number {
  if (role === 'macro') return 0;
  if (role === 'quant' && round === 1) return 2;
  if (role === 'risk' && round === 1) return 3;
  if (role === 'quant') return 4;
  if (role === 'risk') return 5;
  if (role === 'l4_officer') return 6;
  if (role === 'cio') return 7;
  return -1;
}
