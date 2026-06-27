import * as api from "$lib/api";
import type { MemoryNode } from "$lib/types";

export class UserMemoryStore {
  memories = $state<MemoryNode[]>([]);
  loading = $state(false);
  activeTab = $state<"memories" | "review">("memories");
  searchQuery = $state("");
  sortBy = $state<"newest" | "confidence">("newest");

  // Guards against out-of-order responses when load() is called rapidly: a stale
  // response from an earlier call must not overwrite a newer one (H-fe-race).
  #loadSeq = 0;

  async load() {
    const seq = ++this.#loadSeq;
    this.loading = true;
    try {
      const memories = await api.listCharacterMemories("");
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
    const node = await api.createCharacterMemory("", content, type, confidence, tags);
    this.memories = [node, ...this.memories];
  }

  async deleteMemory(memoryId: string) {
    await api.deleteCharacterMemory("", memoryId);
    this.memories = this.memories.filter((m) => m.id !== memoryId);
  }

  async updateMemory(
    memoryId: string,
    updates: { content?: string; memoryType?: string; confidence?: number; tags?: string[] },
  ) {
    const updated = await api.updateCharacterMemory("", memoryId, updates);
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

export const userMemoryStore = new UserMemoryStore();
