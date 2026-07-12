import * as api from "$lib/api";
import type { AiCharacter, GroupChatDetail, GroupChatSummary, GroupChatTurnSnapshot } from "$lib/types";
import { dbg, dbgWarn } from "$lib/utils/debug";
import {
  getPhase7Provider,
  type Phase7ProviderEntry,
  type Phase7ProviderId,
} from "$lib/utils/provider-catalog";

export interface GroupChatSeatDraft {
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

export class GroupChatStore {
  rooms = $state<GroupChatSummary[]>([]);
  selectedRoomId = $state("");
  room = $state<GroupChatDetail | null>(null);
  loading = $state(false);
  saving = $state(false);
  cancelling = $state(false);
  error = $state<string | null>(null);
  activeSnapshot = $state<GroupChatTurnSnapshot | null>(null);

  private _loadSeq = 0;
  private _detailSeq = 0;
  private _sendGeneration = 0;

  async loadRooms(): Promise<void> {
    const seq = ++this._loadSeq;
    this.loading = true;
    this.error = null;
    try {
      const rooms = await api.listGroupChats();
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
      const room = await api.getGroupChat(id);
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

  async createRoom(name: string, cwd?: string): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      const room = await api.createGroupChat(name, cwd);
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
    cwd: string,
    seats: GroupChatSeatDraft[],
  ): Promise<void> {
    if (seats.length !== 3) {
      throw new Error("Roundtable requires exactly three participants");
    }
    this.saving = true;
    this.error = null;
    try {
      const room = await api.createGroupChat(name, cwd);
      this.selectedRoomId = room.id;
      this.room = room;
      for (const seat of seats) {
        const provider = getPhase7Provider(seat.agent);
        this.room = await api.createGroupChatParticipant(
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
      this.room = await api.attachGroupChatRun(this.selectedRoomId, runId, label, role);
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
      this.room = await api.createGroupChatClaudeParticipant(
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
      this.room = await api.createGroupChatParticipant(
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
      this.room = await api.updateGroupChatMemo(this.selectedRoomId, memo);
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
    ++this._sendGeneration;
    this.saving = true;
    this.error = null;

    const MAX_POLLS = 1200; // 1200 × 1500ms = 30 min (matches backend hard deadline)
    let pollCount = 0;

    const poll = (): Promise<void> =>
      api.getGroupChat(roomId).then((current) => {
        if (pollSeq !== this._loadSeq) return;
        if (current && this.selectedRoomId === roomId) {
          this.room = current;
        }
      });

    try {
      const sendPromise = api.sendGroupChatMessage(roomId, trimmed);

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

  async cancelTurn(): Promise<void> {
    if (!this.selectedRoomId || this.cancelling) return;
    const genAtCancel = this._sendGeneration;
    this.cancelling = true;
    this.error = null;
    try {
      await api.cancelGroupChatTurn(this.selectedRoomId);
      const current = await api.getGroupChat(this.selectedRoomId);
      if (this.selectedRoomId === current.id) {
        this.room = current;
      }
      await this.loadRooms();
    } catch (e) {
      this.error = errorMessage(e);
      dbgWarn("rooms", "cancelTurn error", e);
    } finally {
      this.cancelling = false;
      // Only reset saving if no new sendMessage started after this cancel
      if (this._sendGeneration === genAtCancel) {
        this.saving = false;
      }
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

  /**
   * Check if onboarding is needed: no AI characters exist yet.
   * Returns the list of existing characters for the caller to use.
   */
  async checkOnboardingNeeded(): Promise<{ needed: boolean; characters: AiCharacter[] }> {
    try {
      const characters = await api.listCharacters();
      return { needed: characters.length === 0, characters };
    } catch (e) {
      dbgWarn("group-chat", "checkOnboardingNeeded error", e);
      // On error, assume onboarding not needed to avoid blocking the user
      return { needed: false, characters: [] };
    }
  }

  /**
   * Create a Planner character with sensible defaults.
   * Returns the newly created character.
   */
  async createPlannerCharacter(): Promise<AiCharacter> {
    return api.createCharacter(
      "Planner",
      "planner",
      "You are a strategic planner. Break down tasks, coordinate participants, and ensure quality outcomes.",
      "claude",
      null,
      null,
    );
  }

  async deleteRoom(id: string): Promise<void> {
    this.saving = true;
    this.error = null;
    try {
      await api.deleteGroupChat(id);
      this.rooms = this.rooms.filter((room) => room.id !== id);
      if (this.selectedRoomId === id) {
        this.activeSnapshot = null;
        this.selectedRoomId = "";
        this.room = null;
      }
      if (typeof window !== "undefined") {
        window.dispatchEvent(new Event("clawgo:runs-changed"));
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
