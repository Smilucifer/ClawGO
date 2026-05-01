import * as api from "$lib/api";
import type { RoomDetail, RoomSummary } from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";

export class RoomStore {
  rooms = $state<RoomSummary[]>([]);
  selectedRoomId = $state("");
  room = $state<RoomDetail | null>(null);
  loading = $state(false);
  saving = $state(false);
  error = $state<string | null>(null);

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

  async createRoom(name: string, description = "", cwd?: string): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      const room = await api.createRoom(name, description, cwd);
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
    this.saving = true;
    this.error = null;
    try {
      const updated = await api.sendRoomMessage(roomId, trimmed);
      if (this.selectedRoomId === roomId) {
        this.room = updated;
      }
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "sendMessage error", e);
      throw e;
    } finally {
      this.saving = false;
    }
  }

  async deleteRoom(id: string): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      await api.deleteRoom(id);
      this.rooms = this.rooms.filter((room) => room.id !== id);
      if (this.selectedRoomId === id) {
        this.selectedRoomId = "";
        this.room = null;
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
