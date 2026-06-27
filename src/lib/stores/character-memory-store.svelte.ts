import * as api from "$lib/api";
import type { MemoryNode } from "$lib/types";

export class CharacterMemoryStore {
  characterId = $state<string | null>(null);
  memories = $state<MemoryNode[]>([]);
  loading = $state(false);
  activeTab = $state<"memories" | "review">("memories");
  searchQuery = $state("");
  sortBy = $state<"newest" | "confidence">("newest");

  // Guards against out-of-order responses when switching characters rapidly: a stale
  // response for a previous characterId must not overwrite the current one (H-fe-race).
  #loadSeq = 0;

  async load(characterId: string) {
    const seq = ++this.#loadSeq;
    this.characterId = characterId;
    this.loading = true;
    try {
      const memories = await api.listCharacterMemories(characterId);
      if (seq !== this.#loadSeq) return;
      this.memories = memories;
    } catch {
      if (seq !== this.#loadSeq) return;
      this.memories = [];
    } finally {
      if (seq === this.#loadSeq) {
        this.loading = false;
      }
    }
  }

  get sortedMemories(): MemoryNode[] {
    let filtered = this.memories;
    if (this.searchQuery) {
      const q = this.searchQuery.toLowerCase();
      filtered = filtered.filter(
        (m) =>
          m.content.toLowerCase().includes(q) ||
          m.tags.some((t) => t.toLowerCase().includes(q)),
      );
    }
    if (this.sortBy === "newest") {
      return [...filtered].sort((a, b) => b.created_at.localeCompare(a.created_at));
    }
    if (this.sortBy === "confidence") {
      return [...filtered].sort((a, b) => b.confidence - a.confidence);
    }
    return filtered;
  }

  async addMemory(content: string, type: string, confidence: number, tags: string[]) {
    if (!this.characterId) return;
    const node = await api.createCharacterMemory(this.characterId, content, type, confidence, tags);
    this.memories = [node, ...this.memories];
  }

  async deleteMemory(memoryId: string) {
    if (!this.characterId) return;
    await api.deleteCharacterMemory(this.characterId, memoryId);
    this.memories = this.memories.filter((m) => m.id !== memoryId);
  }

  async updateMemory(
    memoryId: string,
    updates: { content?: string; memoryType?: string; confidence?: number; tags?: string[] },
  ) {
    if (!this.characterId) return;
    const updated = await api.updateCharacterMemory(this.characterId, memoryId, updates);
    const idx = this.memories.findIndex((m) => m.id === memoryId);
    if (idx >= 0) {
      this.memories = [
        ...this.memories.slice(0, idx),
        updated,
        ...this.memories.slice(idx + 1),
      ];
    }
  }
}

export const characterMemoryStore = new CharacterMemoryStore();
