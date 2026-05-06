import { describe, it, expect, vi, beforeEach } from "vitest";
import type { RoomDetail, RoomSummary } from "$lib/types";

vi.mock("$lib/api", () => ({
  listRooms: vi.fn(),
  getRoom: vi.fn(),
  createRoom: vi.fn(),
  attachRoomRun: vi.fn(),
  createRoomClaudeParticipant: vi.fn(),
  createRoomParticipant: vi.fn(),
  updateRoomMemo: vi.fn(),
  sendRoomMessage: vi.fn(),
  deleteRoom: vi.fn(),
}));

vi.mock("$lib/utils/debug", () => ({
  dbg: vi.fn(),
  dbgWarn: vi.fn(),
}));

import { RoomStore } from "./room-store.svelte";
import * as api from "$lib/api";
import { capabilitiesForAgent } from "$lib/utils/agent-capabilities";

function summary(id: string, name: string, kind: RoomSummary["kind"] = "roundtable"): RoomSummary {
  return {
    id,
    kind,
    name,
    description: "",
    cwd: undefined,
    participant_count: 0,
    memo_preview: undefined,
    updated_at: "2026-04-30T00:00:00Z",
  };
}

function detail(id: string, name: string, kind: RoomDetail["kind"] = "roundtable"): RoomDetail {
  return {
    id,
    kind,
    name,
    description: "",
    cwd: undefined,
    memo: "",
    participants: [],
    turns: [],
    research_artifact: null,
    created_at: "2026-04-30T00:00:00Z",
    updated_at: "2026-04-30T00:00:00Z",
  };
}

describe("RoomStore", () => {
  let store: RoomStore;

  beforeEach(() => {
    store = new RoomStore();
    vi.resetAllMocks();
  });

  it("loads room summaries", async () => {
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Room")]);

    await store.loadRooms();

    expect(api.listRooms).toHaveBeenCalledOnce();
    expect(store.rooms).toEqual([summary("r1", "Room")]);
    expect(store.loading).toBe(false);
    expect(store.error).toBeNull();
  });

  it("selects a room detail", async () => {
    vi.mocked(api.getRoom).mockResolvedValue(detail("r1", "Room"));

    await store.selectRoom("r1");

    expect(api.getRoom).toHaveBeenCalledWith("r1");
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.id).toBe("r1");
  });

  it("creates a room and selects it", async () => {
    vi.mocked(api.createRoom).mockResolvedValue(detail("r1", "New Room"));
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "New Room")]);

    await store.createRoom("New Room", "", "D:/work");

    expect(api.createRoom).toHaveBeenCalledWith("New Room", "", "D:/work", undefined);
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.name).toBe("New Room");
    expect(store.rooms).toEqual([summary("r1", "New Room")]);
  });

  it("creates a driver room and selects it", async () => {
    vi.mocked(api.createRoom).mockResolvedValue(detail("r1", "Driver Room", "driver"));
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Driver Room", "driver")]);

    await store.createRoom("Driver Room", "", "D:/work", "driver");

    expect(api.createRoom).toHaveBeenCalledWith("Driver Room", "", "D:/work", "driver");
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.kind).toBe("driver");
  });

  it("creates a fixed three-seat roundtable and starts all participants", async () => {
    const created = detail("r1", "Roundtable");
    created.cwd = "D:/work";
    const withClaude = detail("r1", "Roundtable");
    withClaude.participants = [
      {
        participant: {
          id: "p1",
          run_id: "run-claude",
          agent: "claude",
          label: "Claude",
          role: "participant",
          joined_at: "2026-04-30T00:00:00Z",
        },
        run: undefined,
        capabilities: capabilitiesForAgent("claude"),
      },
    ];
    const withCodex = detail("r1", "Roundtable");
    withCodex.participants = [
      ...withClaude.participants,
      {
        participant: {
          id: "p2",
          run_id: "run-codex",
          agent: "codex",
          label: "Codex",
          role: "participant",
          joined_at: "2026-04-30T00:00:00Z",
        },
        run: undefined,
        capabilities: capabilitiesForAgent("codex"),
      },
    ];
    const withGemini = detail("r1", "Roundtable");
    withGemini.participants = [
      ...withCodex.participants,
      {
        participant: {
          id: "p3",
          run_id: "run-gemini",
          agent: "gemini",
          label: "Gemini",
          role: "participant",
          joined_at: "2026-04-30T00:00:00Z",
        },
        run: undefined,
        capabilities: capabilitiesForAgent("gemini"),
      },
    ];
    vi.mocked(api.createRoom).mockResolvedValue(created);
    vi.mocked(api.createRoomParticipant)
      .mockResolvedValueOnce(withClaude)
      .mockResolvedValueOnce(withCodex)
      .mockResolvedValueOnce(withGemini);
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Roundtable")]);

    await store.createRoundtableWithParticipants("Roundtable", "", "D:/work", [
      {
        agent: "claude",
        prompt: "You are Claude.",
        model: "sonnet",
        platformId: "anthropic",
        label: "Claude",
        role: "participant",
      },
      {
        agent: "codex",
        prompt: "You are Codex.",
        model: "gpt-5.5",
        label: "Codex",
        role: "participant",
      },
      {
        agent: "gemini",
        prompt: "You are Gemini.",
        model: "gemini-2.5-pro",
        label: "Gemini",
        role: "participant",
      },
    ]);

    expect(api.createRoom).toHaveBeenCalledWith("Roundtable", "", "D:/work", "roundtable");
    expect(api.createRoomParticipant).toHaveBeenCalledTimes(3);
    expect(api.createRoomParticipant).toHaveBeenNthCalledWith(
      1,
      "r1",
      "claude",
      "You are Claude.",
      "D:/work",
      "sonnet",
      "anthropic",
      undefined,
      "Claude",
      "participant",
    );
    expect(api.createRoomParticipant).toHaveBeenNthCalledWith(
      2,
      "r1",
      "codex",
      "You are Codex.",
      "D:/work",
      "gpt-5.5",
      undefined,
      undefined,
      "Codex",
      "participant",
    );
    expect(api.createRoomParticipant).toHaveBeenNthCalledWith(
      3,
      "r1",
      "gemini",
      "You are Gemini.",
      "D:/work",
      "gemini-2.5-pro",
      undefined,
      undefined,
      "Gemini",
      "participant",
    );
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.participants).toHaveLength(3);
    expect(store.room?.participants.map((item) => item.participant.agent)).toEqual([
      "claude",
      "codex",
      "gemini",
    ]);
    expect(store.saving).toBe(false);
  });

  it("rejects roundtable creation unless exactly three seats are provided", async () => {
    await expect(
      store.createRoundtableWithParticipants("Roundtable", "", "D:/work", [
        { agent: "claude", prompt: "One", label: "One" },
        { agent: "codex", prompt: "Two", label: "Two" },
      ]),
    ).rejects.toThrow("Roundtable requires exactly three participants");

    expect(api.createRoom).not.toHaveBeenCalled();
    expect(api.createRoomParticipant).not.toHaveBeenCalled();
    expect(store.saving).toBe(false);
  });

  it("creates a research room and selects it", async () => {
    vi.mocked(api.createRoom).mockResolvedValue(detail("r1", "Research Room", "research"));
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Research Room", "research")]);

    await store.createRoom("Research Room", "", "D:/work", "research");

    expect(api.createRoom).toHaveBeenCalledWith("Research Room", "", "D:/work", "research");
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.kind).toBe("research");
  });

  it("updates selected room after attaching a run", async () => {
    const updated = detail("r1", "Room");
    updated.participants = [
      {
        participant: {
          id: "p1",
          run_id: "run-1",
          agent: "claude",
          label: "Reviewer",
          role: "reviewer",
          joined_at: "2026-04-30T00:00:00Z",
        },
        run: undefined,
        capabilities: capabilitiesForAgent("claude"),
      },
    ];
    vi.mocked(api.attachRoomRun).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    await store.attachRun("run-1", "Reviewer", "reviewer");

    expect(api.attachRoomRun).toHaveBeenCalledWith("r1", "run-1", "Reviewer", "reviewer");
    expect(store.room?.participants).toHaveLength(1);
  });

  it("updates memo on the selected room", async () => {
    const updated = detail("r1", "Room");
    updated.memo = "remember";
    vi.mocked(api.updateRoomMemo).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    await store.updateMemo("remember");

    expect(api.updateRoomMemo).toHaveBeenCalledWith("r1", "remember");
    expect(store.room?.memo).toBe("remember");
  });

  it("creates native CLI participants", async () => {
    const updated = detail("r1", "Room");
    updated.participants = [
      {
        participant: {
          id: "p1",
          run_id: "run-codex",
          agent: "codex",
          label: "Codex",
          role: "participant",
          joined_at: "2026-04-30T00:00:00Z",
        },
        run: undefined,
        capabilities: capabilitiesForAgent("codex"),
      },
    ];
    vi.mocked(api.createRoomParticipant).mockResolvedValue(updated);
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Room")]);

    store.selectedRoomId = "r1";
    await store.createParticipant("codex", "Review this", "D:/work", "gpt-5.5");

    expect(api.createRoomParticipant).toHaveBeenCalledWith(
      "r1",
      "codex",
      "Review this",
      "D:/work",
      "gpt-5.5",
      undefined,
      undefined,
      undefined,
      undefined,
    );
    expect(store.room?.participants[0].participant.agent).toBe("codex");
  });

  it("does not inject catalog display defaults as models for official CLI participants", async () => {
    const updated = detail("r1", "Room");
    vi.mocked(api.createRoomParticipant).mockResolvedValue(updated);
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Room")]);

    store.selectedRoomId = "r1";
    await store.createParticipant("claude", "Review this", "D:/work");

    expect(api.createRoomParticipant).toHaveBeenCalledWith(
      "r1",
      "claude",
      "Review this",
      "D:/work",
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    );
  });

  it("uses provider default model only for Claude-compatible API participants", async () => {
    const updated = detail("r1", "Room");
    vi.mocked(api.createRoomParticipant).mockResolvedValue(updated);
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Room")]);

    store.selectedRoomId = "r1";
    await store.createParticipant("glm", "Review this", "D:/work");

    expect(api.createRoomParticipant).toHaveBeenCalledWith(
      "r1",
      "claude",
      "Review this",
      "D:/work",
      "glm-5",
      "zhipu",
      undefined,
      undefined,
      undefined,
    );
  });

  it("uses parameterized provider defaults for QWEN and KIMI participants", async () => {
    const updated = detail("r1", "Room");
    vi.mocked(api.createRoomParticipant).mockResolvedValue(updated);
    vi.mocked(api.listRooms).mockResolvedValue([summary("r1", "Room")]);

    store.selectedRoomId = "r1";
    await store.createParticipant("qwen", "Review this", "D:/work");
    expect(api.createRoomParticipant).toHaveBeenLastCalledWith(
      "r1",
      "claude",
      "Review this",
      "D:/work",
      "qwen3.5-plus",
      "bailian",
      undefined,
      undefined,
      undefined,
    );

    await store.createParticipant("kimi", "Review this", "D:/work");
    expect(api.createRoomParticipant).toHaveBeenLastCalledWith(
      "r1",
      "claude",
      "Review this",
      "D:/work",
      "kimi-k2.5",
      "kimi",
      undefined,
      undefined,
      undefined,
    );
  });

  it("sends a roundtable message and updates the selected room timeline", async () => {
    const updated = detail("r1", "Room");
    updated.turns = [
      {
        id: "turn-1",
        idx: 1,
        mode: "research",
        user_input: "Compare options",
        target_participant_ids: ["p1"],
        responses: [],
        started_at: "2026-04-30T00:00:00Z",
        completed_at: "2026-04-30T00:00:01Z",
      },
    ];
    vi.mocked(api.sendRoomMessage).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    await store.sendMessage("Compare options");

    expect(api.sendRoomMessage).toHaveBeenCalledWith("r1", "Compare options");
    expect(store.room?.turns).toHaveLength(1);
  });

  it("sends a one-click debate command with optional focus", async () => {
    const updated = detail("r1", "Room");
    vi.mocked(api.sendRoomMessage).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    await store.sendDebate("focus on risks");

    expect(api.sendRoomMessage).toHaveBeenCalledWith("r1", "@debate focus on risks");
  });

  it("sends a one-click summary command for the selected participant", async () => {
    const updated = detail("r1", "Room");
    vi.mocked(api.sendRoomMessage).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    await store.sendSummary("Claude");

    expect(api.sendRoomMessage).toHaveBeenCalledWith("r1", "@summary @Claude");
  });

  it("ignores stale roundtable send responses after switching rooms", async () => {
    const updated = detail("r1", "Room 1");
    updated.turns = [
      {
        id: "turn-1",
        idx: 1,
        mode: "fanout",
        user_input: "Compare options",
        target_participant_ids: ["p1"],
        responses: [],
        started_at: "2026-04-30T00:00:00Z",
        completed_at: "2026-04-30T00:00:01Z",
      },
    ];
    vi.mocked(api.sendRoomMessage).mockResolvedValue(updated);

    store.selectedRoomId = "r1";
    const send = store.sendMessage("Compare options");
    store.selectedRoomId = "r2";
    store.room = detail("r2", "Room 2");

    await send;

    expect(store.selectedRoomId).toBe("r2");
    expect(store.room?.id).toBe("r2");
    expect(store.room?.turns).toHaveLength(0);
  });

  it("clears selection after deleting the selected room", async () => {
    vi.mocked(api.deleteRoom).mockResolvedValue(undefined);
    store.selectedRoomId = "r1";
    store.room = detail("r1", "Room");
    store.rooms = [summary("r1", "Room")];

    await store.deleteRoom("r1");

    expect(api.deleteRoom).toHaveBeenCalledWith("r1");
    expect(store.selectedRoomId).toBe("");
    expect(store.room).toBeNull();
    expect(store.rooms).toEqual([]);
  });
});
