import type { RoomKind } from "$lib/types";
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
