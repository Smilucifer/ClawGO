import { describe, expect, it } from "vitest";
import {
  canSendRoomMessage,
  roomParticipantMetaLabel,
  roomParticipantProviderLabel,
  roomParticipantBadge,
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

  it("shows three-seat capacity only for roundtable room badges", () => {
    expect(roomParticipantBadge("roundtable", 2)).toBe("2/3");
    expect(roomParticipantBadge("driver", 2)).toBe("2");
    expect(roomParticipantBadge("research", 1)).toBe("1");
  });

  it("shows visible provider identity instead of only execution identity", () => {
    expect(roomParticipantProviderLabel("claude", "deepseek")).toBe("DeepSeek");
    expect(roomParticipantProviderLabel("claude", "zhipu")).toBe("GLM");
    expect(roomParticipantProviderLabel("codex")).toBe("Codex");
    expect(roomParticipantProviderLabel("gemini")).toBe("Gemini");
  });

  it("includes the model after the visible provider label when present", () => {
    expect(roomParticipantMetaLabel("claude", "deepseek", "deepseek-chat")).toBe(
      "DeepSeek · deepseek-chat",
    );
    expect(roomParticipantMetaLabel("claude", null, "")).toBe("Claude");
  });
});
