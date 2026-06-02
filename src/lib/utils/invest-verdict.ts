/**
 * 委员会判决（verdict）的颜色映射 — 唯一权威。
 * 在 CommitteeLiveTab / CommitteeReplayTab / CommitteeArchiveTab 之间共享，
 * 避免三份独立实现（HOLD 颜色历史上曾经不一致）。
 */
export type VerdictKind = 'BUY' | 'ACCUMULATE' | 'HOLD' | 'TRIM' | 'SELL' | 'WATCH';

/**
 * 返回内联 style 字符串（含 background 和 color），可直接给 <span style={...}> 使用。
 * 接受 null/undefined（归档解析未匹配时），返回中性灰底。
 */
export function getVerdictBadgeStyle(verdict: string | null | undefined): string {
  if (verdict === 'BUY') return 'background:rgba(138,154,118,0.2); color:#8a9a76;';
  if (verdict === 'ACCUMULATE') return 'background:rgba(59,130,246,0.2); color:#3b82f6;';
  if (verdict === 'HOLD' || verdict === 'WATCH')
    return 'background:var(--accent-muted); color:var(--accent);';
  if (verdict === 'TRIM') return 'background:rgba(245,158,11,0.2); color:#f59e0b;';
  if (verdict === 'SELL') return 'background:rgba(168,122,122,0.2); color:#a87a7a;';
  return 'background:var(--bg-input); color:var(--text-tertiary);';
}

/**
 * 解析归档 markdown 内容里的 verdict（最佳努力）。
 * 匹配 "CIO 判决: BUY"、"verdict: BUY"、"裁决: BUY" 等常见模式。
 */
const VERDICT_RE =
  /(?:CIO\s*[判裁]决|verdict|裁决|判决)\s*[:：]\s*\*{0,2}(BUY|ACCUMULATE|HOLD|TRIM|SELL|WATCH)\*{0,2}/i;

export function parseVerdictFromContent(content: string): VerdictKind | null {
  const m = content.match(VERDICT_RE);
  return m ? (m[1].toUpperCase() as VerdictKind) : null;
}
