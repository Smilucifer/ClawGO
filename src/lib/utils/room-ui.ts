import type { RoomKind, RoomTurnMode } from "$lib/types";
import { getPhase7Provider, providerIdForRun } from "$lib/utils/provider-catalog";

export type RoomPlaceholderKey =
  | "room_roundtablePlaceholder"
  | "room_driverPlaceholder"
  | "room_researchPlaceholder";

export function roomRequiresThreeParticipants(kind: RoomKind): boolean {
  return kind === "roundtable";
}

export function canSendRoomMessage(
  kind: RoomKind,
  participantCount: number,
  message: string,
): boolean {
  if (!message.trim()) return false;
  if (roomRequiresThreeParticipants(kind)) return participantCount >= 3;
  return participantCount > 0;
}

export function roomMessagePlaceholderKey(kind: RoomKind): RoomPlaceholderKey {
  if (kind === "driver") return "room_driverPlaceholder";
  if (kind === "research") return "room_researchPlaceholder";
  return "room_roundtablePlaceholder";
}

export function roomParticipantBadge(kind: RoomKind, participantCount: number): string {
  if (roomRequiresThreeParticipants(kind)) return `${participantCount}/3`;
  return String(participantCount);
}

export function roomParticipantProviderLabel(agent: string, platformId?: string | null): string {
  return getPhase7Provider(providerIdForRun(agent, platformId)).label;
}

export function roomParticipantMetaLabel(
  agent: string,
  platformId?: string | null,
  model?: string | null,
): string {
  const providerLabel = roomParticipantProviderLabel(agent, platformId);
  const cleanModel = model?.trim();
  return cleanModel ? `${providerLabel} · ${cleanModel}` : providerLabel;
}

const TURN_MODE_LABELS: Record<RoomTurnMode, string> = {
  fanout: "Fanout",
  debate: "Debate",
  summary: "Summary",
  private: "Private",
  review: "Review",
  research: "Research",
  singletarget: "Single Target",
};

export function roomTurnModeLabel(mode: RoomTurnMode): string {
  return TURN_MODE_LABELS[mode] ?? mode;
}

const TURN_MODE_COLORS: Record<RoomTurnMode, string> = {
  fanout: "bg-blue-100 text-blue-700",
  debate: "bg-orange-100 text-orange-700",
  summary: "bg-purple-100 text-purple-700",
  private: "bg-gray-100 text-gray-500",
  review: "bg-green-100 text-green-700",
  research: "bg-cyan-100 text-cyan-700",
  singletarget: "bg-pink-100 text-pink-700",
};

export function roomTurnModeColor(mode: RoomTurnMode): string {
  return TURN_MODE_COLORS[mode] ?? "bg-gray-100 text-gray-500";
}
