import { describe, it, expect, vi, beforeEach } from "vitest";
import type { RoomDetail, RoomSummary } from "$lib/types";

vi.mock("$lib/api", () => ({
  listRooms: vi.fn(),
  getRoom: vi.fn(),
  createRoom: vi.fn(),
  attachRoomRun: vi.fn(),
  createRoomClaudeParticipant: vi.fn(),
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

function summary(id: string, name: string): RoomSummary {
  return {
    id,
    kind: "roundtable",
    name,
    description: "",
    cwd: undefined,
    participant_count: 0,
    memo_preview: undefined,
    updated_at: "2026-04-30T00:00:00Z",
  };
}

function detail(id: string, name: string): RoomDetail {
  return {
    id,
    kind: "roundtable",
    name,
    description: "",
    cwd: undefined,
    memo: "",
    participants: [],
    turns: [],
    created_at: "2026-04-30T00:00:00Z",
    updated_at: "2026-04-30T00:00:00Z",
  };
}

describe("RoomStore", () => {
  let store: RoomStore;

  beforeEach(() => {
    store = new RoomStore();
    vi.clearAllMocks();
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

    expect(api.createRoom).toHaveBeenCalledWith("New Room", "", "D:/work");
    expect(store.selectedRoomId).toBe("r1");
    expect(store.room?.name).toBe("New Room");
    expect(store.rooms).toEqual([summary("r1", "New Room")]);
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

  it("sends a roundtable message and updates the selected room timeline", async () => {
    const updated = detail("r1", "Room");
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
    await store.sendMessage("Compare options");

    expect(api.sendRoomMessage).toHaveBeenCalledWith("r1", "Compare options");
    expect(store.room?.turns).toHaveLength(1);
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
