/**
 * Shared utility constants and functions for memory panel components.
 * Used by both UserMemoryPanel and CharacterMemoryPanel.
 */

export const typeLabels: Record<string, string> = {
  fact: "事实",
  experience: "经验",
  preference: "偏好",
  feedback: "反馈",
  relationship: "关系",
  skill: "技能",
};

export const typeColors: Record<string, string> = {
  fact: "bg-blue-500/10 text-blue-400 border-blue-500/20",
  experience: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
  preference: "bg-purple-500/10 text-purple-400 border-purple-500/20",
  feedback: "bg-amber-500/10 text-amber-400 border-amber-500/20",
  relationship: "bg-pink-500/10 text-pink-400 border-pink-500/20",
  skill: "bg-cyan-500/10 text-cyan-400 border-cyan-500/20",
};

export const sourceLabels: Record<string, string> = {
  chat: "对话",
  manual: "手动",
  inference: "推断",
  auto_extract: "自动提取",
};

export function confidenceColor(c: number): string {
  if (c >= 90) return "bg-emerald-500";
  if (c >= 70) return "bg-emerald-400";
  return "bg-amber-400";
}

export function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString("zh-CN", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
