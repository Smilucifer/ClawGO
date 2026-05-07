import * as api from "$lib/api";
import type { RoomDetail, RoomKind, RoomSummary, RoomTurnSnapshot } from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";
import {
  getPhase7Provider,
  type Phase7ProviderEntry,
  type Phase7ProviderId,
} from "$lib/utils/provider-catalog";

export interface RoundtableSeatDraft {
  agent: Phase7ProviderId;
  prompt: string;
  model?: string;
  platformId?: string;
  connectionProfileId?: string;
  label?: string;
  role?: string;
}

function launchModelForProvider(
  provider: Phase7ProviderEntry,
  explicitModel?: string,
): string | undefined {
  if (explicitModel) return explicitModel;
  return provider.mode === "claude_compatible_api" ? provider.defaultModel : undefined;
}

export class RoomStore {
  rooms = $state<RoomSummary[]>([]);
  selectedRoomId = $state("");
  room = $state<RoomDetail | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);
  activeSnapshot = $state<RoomTurnSnapshot | null>(null);

  private _loadSeq = 0;
  private _detailSeq = 0;

  async loadRooms(): Promise<void> {
    const seq = ++this._loadSeq;
    this.loading = true;
    this.error = null;
    try {
      const rooms = await api.listRooms();
      if (seq !== this._loadSeq) return;
      this.rooms = rooms;
      dbg("rooms", "loadRooms", { count: rooms.length });
    } catch (e) {
      if (seq !== this._loadSeq) return;
      this.rooms = [];
      this.error = errorMessage(e);
      dbgWarn("rooms", "loadRooms error", e);
    } finally {
      if (seq === this._loadSeq) this.loading = false;
    }
  }

  async selectRoom(id: string): Promise<void> {
    this.selectedRoomId = id;
    this.activeSnapshot = null;
    if (!id) {
      this.room = null;
      return;
    }
    const seq = ++this._detailSeq;
    this.loading = true;
    this.error = null;
    try {
      const room = await api.getRoom(id);
      if (seq !== this._detailSeq || this.selectedRoomId !== id) return;
      this.room = room;
    } catch (e) {
      if (seq !== this._detailSeq || this.selectedRoomId !== id) return;
      this.room = null;
      this.error = errorMessage(e);
      dbgWarn("rooms", "selectRoom error", e);
    } finally {
      if (seq === this._detailSeq) this.loading = false;
    }
  }

  async createRoom(name: string, description = "", cwd?: string, kind?: RoomKind): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      const room = await api.createRoom(name, description, cwd, kind);
      this.selectedRoomId = room.id;
      this.room = room;
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "createRoom error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async createRoundtableWithParticipants(
    name: string,
    description: string,
    cwd: string,
    seats: RoundtableSeatDraft[],
  ): Promise<void> {
    if (seats.length !== 3) {
      throw new Error("Roundtable requires exactly three participants");
    }
    this.saving = true;
    this.error = null;
    try {
      const room = await api.createRoom(name, description, cwd, "roundtable");
      this.selectedRoomId = room.id;
      this.room = room;
      for (const seat of seats) {
        const provider = getPhase7Provider(seat.agent);
        this.room = await api.createRoomParticipant(
          room.id,
          provider.executionAgent,
          seat.prompt,
          cwd,
          launchModelForProvider(provider, seat.model),
          seat.platformId || provider.platformId,
          seat.connectionProfileId,
          seat.label,
          seat.role,
        );
      }
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "createRoundtableWithParticipants error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async attachRun(runId: string, label?: string, role?: string): Promise<void> {
    if (!this.selectedRoomId) throw new Error("No room selected");
    this.saving = true;
    this.error = null;
    try {
      this.room = await api.attachRoomRun(this.selectedRoomId, runId, label, role);
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "attachRun error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async createClaudeParticipant(
    prompt: string,
    cwd: string,
    model?: string,
    platformId?: string,
    connectionProfileId?: string,
    label?: string,
    role?: string,
  ): Promise<void> {
    if (!this.selectedRoomId) throw new Error("No room selected");
    this.saving = true;
    this.error = null;
    try {
      this.room = await api.createRoomClaudeParticipant(
        this.selectedRoomId,
        prompt,
        cwd,
        model,
        platformId,
        connectionProfileId,
        label,
        role,
      );
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "createClaudeParticipant error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async createParticipant(
    agent: Phase7ProviderId,
    prompt: string,
    cwd: string,
    model?: string,
    platformId?: string,
    connectionProfileId?: string,
    label?: string,
    role?: string,
  ): Promise<void> {
    if (!this.selectedRoomId) throw new Error("No room selected");
    this.saving = true;
    this.error = null;
    try {
      const provider = getPhase7Provider(agent);
      this.room = await api.createRoomParticipant(
        this.selectedRoomId,
        provider.executionAgent,
        prompt,
        cwd,
        launchModelForProvider(provider, model),
        platformId || provider.platformId,
        connectionProfileId,
        label,
        role,
      );
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "createParticipant error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async updateMemo(memo: string): Promise<void> {
    if (!this.selectedRoomId) throw new Error("No room selected");
    this.saving = true;
    this.error = null;
    try {
      this.room = await api.updateRoomMemo(this.selectedRoomId, memo);
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "updateMemo error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async sendMessage(message: string): Promise<void> {
    if (!this.selectedRoomId) throw new Error("No room selected");
    const roomId = this.selectedRoomId;
    const trimmed = message.trim();
    if (!trimmed) return;
    this.activeSnapshot = null;
    const pollSeq = ++this._loadSeq;
    this.saving = true;
    this.error = null;

    const MAX_POLLS = 120; // 120 × 1500ms = 3 min max polling
    let pollCount = 0;

    const poll = (): Promise<void> =>
      api.getRoom(roomId).then((current) => {
        if (pollSeq !== this._loadSeq) return;
        if (current && this.selectedRoomId === roomId) {
          this.room = current;
        }
      });

    try {
      const sendPromise = api.sendRoomMessage(roomId, trimmed);

      // Poll for incremental updates while turn is in progress,
      // so each participant's response appears as soon as it completes.
      const pollTimer = setInterval(() => {
        if (++pollCount > MAX_POLLS) return;
        poll().catch(() => {
          // ignore poll errors
        });
      }, 1500);

      try {
        const updated = await sendPromise;
        if (pollSeq === this._loadSeq && this.selectedRoomId === roomId) {
          this.room = updated;
        }
        await this.loadRooms();
      } finally {
        clearInterval(pollTimer);
      }
    } catch (e) {
      if (pollSeq !== this._loadSeq) return;
      this.error = errorMessage(e);
      dbgWarn("rooms", "sendMessage error", e);
      throw e;
    } finally {
      if (pollSeq === this._loadSeq) this.saving = false;
    }
  }

  async sendDebate(focus = ""): Promise<void> {
    const trimmed = focus.trim();
    await this.sendMessage(trimmed ? `@debate ${trimmed}` : "@debate");
  }

  async sendSummary(target: string): Promise<void> {
    const trimmed = target.trim().replace(/^@+/, "");
    if (!trimmed) return;
    await this.sendMessage(`@summary @${trimmed}`);
  }

  async deleteRoom(id: string): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      await api.deleteRoom(id);
      this.rooms = this.rooms.filter((room) => room.id !== id);
      if (this.selectedRoomId === id) {
        this.activeSnapshot = null;
        this.selectedRoomId = "";
        this.room = null;
      }
      if (typeof window !== "undefined") {
        window.dispatchEvent(new Event("ocv:runs-changed"));
      }
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "deleteRoom error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }
}

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
