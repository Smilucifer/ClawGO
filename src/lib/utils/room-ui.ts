import type { RoomKind } from "$lib/types";

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
