<script lang="ts">
  import type { MemoryNode } from "$lib/types";
  import { characterMemoryStore } from "$lib/stores/character-memory-store.svelte";
  import MemoryAddModal from "./MemoryAddModal.svelte";

  let {
    characterId,
    characterLabel = "Character",
    characterIcon = "",
    open = false,
    onclose,
  }: {
    characterId: string;
    characterLabel?: string;
    characterIcon?: string;
    open?: boolean;
    onclose: () => void;
  } = $props();

  const store = characterMemoryStore;
  let showAddModal = $state(false);
  let deletingId = $state<string | null>(null);
  let clearing = $state(false);

  $effect(() => {
    if (open) {
      store.load(characterId);
      function onKey(e: KeyboardEvent) {
        if (e.key === "Escape") onclose();
      }
      window.addEventListener("keydown", onKey);
      return () => window.removeEventListener("keydown", onKey);
    }
  });

  async function handleDelete(memoryId: string) {
    deletingId = memoryId;
    try {
      await store.deleteMemory(memoryId);
    } catch {
      // fail silent
    } finally {
      deletingId = null;
    }
  }

  async function handleClear() {
    if (!confirm("确认清空此角色的所有记忆？")) return;
    clearing = true;
    try {
      await Promise.all(store.memories.map((m) => store.deleteMemory(m.id)));
    } catch {
      // fail silent
    } finally {
      clearing = false;
    }
  }

  const typeLabels: Record<string, string> = {
    fact: "事实",
    experience: "经验",
    preference: "偏好",
    rule: "规则",
    relationship: "关系",
  };

  const typeColors: Record<string, string> = {
    fact: "bg-blue-500/10 text-blue-400 border-blue-500/20",
    experience: "bg-emerald-500/10 text-emerald-400 border-emerald-500/20",
    preference: "bg-purple-500/10 text-purple-400 border-purple-500/20",
    rule: "bg-amber-500/10 text-amber-400 border-amber-500/20",
    relationship: "bg-pink-500/10 text-pink-400 border-pink-500/20",
  };

  const sourceLabels: Record<string, string> = {
    chat: "对话",
    manual: "手动",
    inference: "推断",
  };

  function confidenceColor(c: number): string {
    if (c >= 90) return "bg-emerald-500";
    if (c >= 70) return "bg-emerald-400";
    return "bg-amber-400";
  }

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-50 flex items-center justify-center">
    <!-- Backdrop -->
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
      onclick={onclose}
      role="presentation"
    ></div>

    <!-- Panel -->
    <div
      class="relative z-50 flex h-[85vh] w-[90vw] max-w-6xl flex-col rounded-lg border border-[#1e1e2e] bg-[#0a0a0f] shadow-2xl"
    >
      <!-- ── Top Bar ── -->
      <div class="flex h-14 shrink-0 items-center gap-3 border-b border-[#1e1e2e] px-4">
        <!-- Avatar / initial -->
        <div
          class="flex h-8 w-8 items-center justify-center rounded-full bg-primary/10 text-sm font-bold text-primary"
        >
          {characterIcon || characterLabel.charAt(0).toUpperCase()}
        </div>

        <span class="text-sm font-semibold text-foreground">{characterLabel}</span>
        <span class="text-xs text-muted-foreground">{store.memories.length} 条记忆</span>

        <div class="flex-1"></div>

        <!-- Search -->
        <div class="relative w-48">
          <svg
            class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <circle cx="11" cy="11" r="8" />
            <path d="m21 21-4.3-4.3" />
          </svg>
          <input
            class="h-8 w-full rounded-md border border-input bg-background pl-8 pr-3 text-xs outline-none focus:ring-2 focus:ring-ring"
            placeholder="搜索记忆..."
            bind:value={store.searchQuery}
          />
        </div>

        <!-- Add button -->
        <button
          class="flex h-8 items-center gap-1 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors"
          onclick={() => (showAddModal = true)}
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M5 12h14" />
            <path d="M12 5v14" />
          </svg>
          手动添加
        </button>

        <!-- Close -->
        <button
          class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          onclick={onclose}
          aria-label="Close"
        >
          <svg
            class="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M18 6 6 18" />
            <path d="m6 6 12 12" />
          </svg>
        </button>
      </div>

      <!-- ── Tabs ── -->
      <div class="flex h-10 shrink-0 items-center gap-1 border-b border-[#1e1e2e] px-4">
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'memories'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'memories')}
        >全部记忆</button>
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'graph'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'graph')}
        >知识图谱</button>
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'gaps'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'gaps')}
        >知识缺口</button>
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'communities'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'communities')}
        >社区 Community</button>
      </div>

      <!-- ── Content ── -->
      {#if store.loading}
        <div class="flex flex-1 items-center justify-center">
          <div class="h-6 w-6 animate-spin rounded-full border-2 border-primary/30 border-t-primary"></div>
        </div>
      {:else}
        <div class="flex flex-1 overflow-hidden">
          <!-- Left pane: tab-specific -->
          <div class="flex-1 overflow-y-auto p-4">
            {#if store.activeTab === 'memories'}
              <div class="space-y-3">
                <h3 class="text-sm font-semibold text-foreground">记忆分布</h3>
                <div class="grid grid-cols-5 gap-2">
                  {#each Object.entries(typeLabels) as [key, label]}
                    <div class="rounded-lg border border-[#1e1e2e] bg-background p-3 text-center">
                      <div class="text-lg font-bold text-foreground">
                        {store.memories.filter((m) => m.type === key).length}
                      </div>
                      <div class="mt-1 text-[10px] text-muted-foreground">{label}</div>
                    </div>
                  {/each}
                </div>
                {#if store.memories.length === 0}
                  <div class="py-12 text-center text-sm text-muted-foreground">暂无记忆数据</div>
                {/if}
              </div>
            {:else if store.activeTab === 'graph'}
              <div class="flex h-full flex-col items-center justify-center gap-4">
                <svg
                  class="h-32 w-32 text-muted-foreground/30"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <circle cx="6" cy="6" r="2" />
                  <circle cx="18" cy="6" r="2" />
                  <circle cx="12" cy="18" r="2" />
                  <path d="M6 6l12 0" />
                  <path d="M6 6l4 10" />
                  <path d="M18 6l-4 10" />
                </svg>
                <p class="text-sm text-muted-foreground">知识图谱可视化</p>
                {#if store.graph}
                  <p class="text-xs text-muted-foreground">
                    {store.graph.nodes.length} 节点 &middot; {store.graph.edges.length} 关系边
                  </p>
                {:else}
                  <p class="text-xs text-muted-foreground">暂无图谱数据</p>
                {/if}
              </div>
            {:else if store.activeTab === 'gaps'}
              <div class="space-y-2">
                <h3 class="text-sm font-semibold text-foreground">知识缺口</h3>
                {#if store.gaps.length === 0}
                  <div class="py-12 text-center text-sm text-muted-foreground">暂无知识缺口</div>
                {:else}
                  {#each store.gaps as gap}
                    <div class="rounded-lg border border-[#1e1e2e] bg-background p-3">
                      <div class="flex items-center gap-2">
                        <span
                          class="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-400"
                        >
                          {gap.gap_type === 'isolated_node'
                            ? '孤立节点'
                            : gap.gap_type === 'sparse_community'
                              ? '稀疏社区'
                              : '桥节点'}
                        </span>
                      </div>
                      <p class="mt-2 text-xs text-foreground">{gap.description}</p>
                      <p class="mt-1 text-[11px] text-muted-foreground">
                        建议: {gap.suggestion}
                      </p>
                    </div>
                  {/each}
                {/if}
              </div>
            {:else if store.activeTab === 'communities'}
              <div class="space-y-2">
                <h3 class="text-sm font-semibold text-foreground">记忆社区</h3>
                {#if store.communities.length === 0}
                  <div class="py-12 text-center text-sm text-muted-foreground">暂无社区</div>
                {:else}
                  {#each store.communities as community}
                    <div class="rounded-lg border border-[#1e1e2e] bg-background p-3">
                      <div class="flex items-center justify-between">
                        <span class="text-sm font-medium text-foreground">{community.label}</span>
                        <span class="text-[11px] text-muted-foreground">{community.node_count} 节点</span>
                      </div>
                      <div class="mt-2 flex items-center gap-2">
                        <span class="text-[11px] text-muted-foreground">内聚度:</span>
                        <div class="h-1.5 flex-1 overflow-hidden rounded-full bg-border">
                          <div
                            class="h-full rounded-full bg-primary"
                            style="width: {community.cohesion * 100}%"
                          ></div>
                        </div>
                        <span class="text-[11px] text-muted-foreground">{(community.cohesion * 100).toFixed(0)}%</span>
                      </div>
                    </div>
                  {/each}
                {/if}
              </div>
            {/if}
          </div>

          <!-- Right pane: memory list (340px) -->
          <div class="flex w-[340px] shrink-0 flex-col border-l border-[#1e1e2e]">
            <!-- Sort -->
            <div class="flex items-center justify-between border-b border-[#1e1e2e] px-3 py-2">
              <span class="text-[11px] text-muted-foreground">排序:</span>
              <select
                class="h-7 rounded border border-input bg-background px-2 text-[11px] outline-none focus:ring-2 focus:ring-ring"
                bind:value={store.sortBy}
              >
                <option value="newest">最新</option>
                <option value="confidence">置信度</option>
              </select>
            </div>

            <!-- List -->
            <div class="flex-1 overflow-y-auto">
              {#each store.sortedMemories as memory (memory.id)}
                <div
                  class="border-b border-[#1e1e2e]/50 px-3 py-2.5 transition-colors hover:bg-accent/30"
                >
                  <div class="flex items-start justify-between gap-2">
                    <div class="min-w-0 flex-1">
                      <!-- Content -->
                      <p class="line-clamp-3 text-xs leading-5 text-foreground">{memory.content}</p>
                      <!-- Badges -->
                      <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                        <span
                          class="rounded border px-1.5 py-0.5 text-[10px] font-medium {typeColors[memory.type] ||
                            'bg-gray-500/10 text-gray-400 border-gray-500/20'}"
                        >
                          {typeLabels[memory.type] || memory.type}
                        </span>
                        <span
                          class="rounded bg-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground"
                        >
                          {sourceLabels[memory.source?.kind] || memory.source?.kind || '未知'}
                        </span>
                      </div>
                      <!-- Tags -->
                      {#if memory.tags.length > 0}
                        <div class="mt-1 flex flex-wrap gap-1">
                          {#each memory.tags as tag}
                            <span
                              class="rounded bg-primary/5 px-1 py-0.5 text-[9px] text-primary"
                            >{tag}</span>
                          {/each}
                        </div>
                      {/if}
                      <!-- Confidence bar -->
                      <div class="mt-1.5 flex items-center gap-2">
                        <div class="h-1 flex-1 overflow-hidden rounded-full bg-border">
                          <div
                            class="h-full rounded-full {confidenceColor(memory.confidence)}"
                            style="width: {memory.confidence}%"
                          ></div>
                        </div>
                        <span class="w-8 text-right text-[10px] text-muted-foreground"
                        >{memory.confidence}%</span
                        >
                      </div>
                      <!-- Date -->
                      <div class="mt-0.5 text-[10px] text-muted-foreground">
                        {formatDate(memory.created_at)}
                      </div>
                    </div>
                    <!-- Delete -->
                    <button
                      class="mt-0.5 shrink-0 rounded p-1 text-muted-foreground/50 transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-30"
                      onclick={() => handleDelete(memory.id)}
                      disabled={deletingId === memory.id}
                      title="删除"
                    >
                      {#if deletingId === memory.id}
                        <span
                          class="block h-3 w-3 animate-spin rounded-full border border-current/30 border-t-current"
                        ></span>
                      {:else}
                        <svg
                          class="h-3.5 w-3.5"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          stroke-width="2"
                          stroke-linecap="round"
                          stroke-linejoin="round"
                        >
                          <path d="M3 6h18" />
                          <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
                          <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                        </svg>
                      {/if}
                    </button>
                  </div>
                </div>
              {:else}
                <div class="px-3 py-12 text-center text-xs text-muted-foreground">
                  {store.searchQuery ? '无匹配记忆' : '暂无记忆'}
                </div>
              {/each}
            </div>
          </div>
        </div>
      {/if}

      <!-- ── Footer ── -->
      <div class="flex h-10 shrink-0 items-center justify-between border-t border-[#1e1e2e] px-4">
        <span class="text-[11px] text-muted-foreground">
          {#if store.graph}
            {store.graph.nodes.length} 节点 &middot; {store.graph.edges.length} 关系
          {/if}
          {#if store.communities.length > 0}
            &middot; {store.communities.length} 个社区
          {/if}
          {#if store.gaps.length > 0}
            &middot; {store.gaps.length} 个缺口
          {/if}
        </span>
        <button
          class="flex h-7 items-center gap-1 rounded-md px-2.5 text-[11px] text-destructive transition-colors hover:bg-destructive/10 disabled:opacity-50"
          onclick={handleClear}
          disabled={clearing || store.memories.length === 0}
        >
          {clearing ? '清空中...' : '清空记忆'}
        </button>
      </div>
    </div>
  </div>
{/if}

<MemoryAddModal bind:open={showAddModal} />
