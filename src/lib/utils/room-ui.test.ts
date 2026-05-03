import { describe, expect, it } from "vitest";
import {
  canSendRoomMessage,
  roomMessagePlaceholderKey,
  roomRequiresThreeParticipants,
} from "./room-ui";

describe("room UI helpers", () => {
  it("requires exactly the fixed three-seat guard only for roundtable rooms", () => {
    expect(roomRequiresThreeParticipants("roundtable")).toBe(true);
    expect(roomRequiresThreeParticipants("driver")).toBe(false);
    expect(roomRequiresThreeParticipants("research")).toBe(false);
  });

  it("allows Driver and Research rooms to send with existing non-three participant counts", () => {
    expect(canSendRoomMessage("roundtable", 2, "compare")).toBe(false);
    expect(canSendRoomMessage("roundtable", 3, "compare")).toBe(true);
    expect(canSendRoomMessage("driver", 1, "/review this")).toBe(true);
    expect(canSendRoomMessage("research", 1, "research topic")).toBe(true);
    expect(canSendRoomMessage("research", 0, "research topic")).toBe(false);
  });

  it("uses kind-specific composer placeholders", () => {
    expect(roomMessagePlaceholderKey("roundtable")).toBe("room_roundtablePlaceholder");
    expect(roomMessagePlaceholderKey("driver")).toBe("room_driverPlaceholder");
    expect(roomMessagePlaceholderKey("research")).toBe("room_researchPlaceholder");
  });
});
