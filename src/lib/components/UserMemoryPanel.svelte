<script lang="ts">
  import type { MemoryNode } from "$lib/types";
  import { userMemoryStore } from "$lib/stores/user-memory-store.svelte";
  import * as api from "$lib/api";
  import MemoryAddModal from "./MemoryAddModal.svelte";
  import { typeLabels, typeColors, sourceLabels, confidenceColor, formatDate } from "$lib/utils/memory-panel-helpers";

  let {
    open = false,
    onclose,
  }: {
    open?: boolean;
    onclose: () => void;
  } = $props();

  const store = userMemoryStore;
  let showAddModal = $state(false);
  let deletingId = $state<string | null>(null);
  let clearing = $state(false);

  // Review queue state
  let pendingMemories = $state<MemoryNode[]>([]);
  let pendingLoading = $state(false);
  let reviewingId = $state<string | null>(null);

  $effect(() => {
    if (open) {
      store.load();
      loadPending();
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
    if (!confirm("确认清空所有用户记忆？")) return;
    clearing = true;
    try {
      await Promise.all(store.memories.map((m) => store.deleteMemory(m.id)));
    } catch {
      // fail silent
    } finally {
      clearing = false;
    }
  }

  async function loadPending() {
    pendingLoading = true;
    try {
      pendingMemories = await api.listPendingMemories("");
    } catch {
      pendingMemories = [];
    } finally {
      pendingLoading = false;
    }
  }

  async function handleApprove(memoryId: string) {
    reviewingId = memoryId;
    try {
      await api.approveMemory("", memoryId);
      pendingMemories = pendingMemories.filter((m) => m.id !== memoryId);
      store.load();
    } catch {
      // fail silent
    } finally {
      reviewingId = null;
    }
  }

  async function handleReject(memoryId: string) {
    reviewingId = memoryId;
    try {
      await api.rejectMemory("", memoryId);
      pendingMemories = pendingMemories.filter((m) => m.id !== memoryId);
    } catch {
      // fail silent
    } finally {
      reviewingId = null;
    }
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
      <!-- Top Bar -->
      <div class="flex h-14 shrink-0 items-center gap-3 border-b border-[#1e1e2e] px-4">
        <div class="flex h-8 w-8 items-center justify-center rounded-full bg-primary/10 text-sm font-bold text-primary">
          U
        </div>

        <span class="text-sm font-semibold text-foreground">用户记忆</span>
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

      <!-- Tabs -->
      <div class="flex h-10 shrink-0 items-center gap-1 border-b border-[#1e1e2e] px-4">
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'memories'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'memories')}
        >全部记忆</button>
        <button
          class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors {store.activeTab === 'review'
            ? 'bg-accent text-foreground'
            : 'text-muted-foreground hover:text-foreground hover:bg-accent/50'}"
          onclick={() => (store.activeTab = 'review')}
        >
          待审核
          {#if pendingMemories.length > 0}
            <span class="ml-1 rounded-full bg-amber-500/20 px-1.5 py-0.5 text-[10px] text-amber-400">{pendingMemories.length}</span>
          {/if}
        </button>
      </div>

      <!-- Content -->
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
            {:else if store.activeTab === 'review'}
              <div class="space-y-2">
                <h3 class="text-sm font-semibold text-foreground">待审核记忆</h3>
                <p class="text-[11px] text-muted-foreground">自动提取的记忆需要审核后才会被注入到对话中。</p>
                {#if pendingLoading}
                  <div class="flex items-center justify-center py-8">
                    <div class="h-5 w-5 animate-spin rounded-full border-2 border-primary/30 border-t-primary"></div>
                  </div>
                {:else if pendingMemories.length === 0}
                  <div class="py-12 text-center text-sm text-muted-foreground">没有待审核的记忆</div>
                {:else}
                  {#each pendingMemories as memory (memory.id)}
                    <div class="rounded-lg border border-amber-500/20 bg-amber-500/5 p-3">
                      <p class="text-xs leading-5 text-foreground">{memory.content}</p>
                      <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                        <span class="rounded border px-1.5 py-0.5 text-[10px] font-medium {typeColors[memory.type] || 'bg-gray-500/10 text-gray-400 border-gray-500/20'}">
                          {typeLabels[memory.type] || memory.type}
                        </span>
                        <span class="rounded bg-border/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                          置信度 {memory.confidence}%
                        </span>
                        {#each memory.tags as tag}
                          <span class="rounded bg-primary/5 px-1 py-0.5 text-[9px] text-primary">{tag}</span>
                        {/each}
                      </div>
                      <div class="mt-2 flex items-center gap-2">
                        <button
                          class="flex h-7 items-center gap-1 rounded-md bg-emerald-500/10 px-2.5 text-[11px] font-medium text-emerald-400 transition-colors hover:bg-emerald-500/20 disabled:opacity-50"
                          onclick={() => handleApprove(memory.id)}
                          disabled={reviewingId === memory.id}
                        >
                          {#if reviewingId === memory.id}
                            <span class="block h-3 w-3 animate-spin rounded-full border border-current/30 border-t-current"></span>
                          {:else}
                            <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M20 6 9 17l-5-5"/></svg>
                          {/if}
                          通过
                        </button>
                        <button
                          class="flex h-7 items-center gap-1 rounded-md bg-destructive/10 px-2.5 text-[11px] font-medium text-destructive transition-colors hover:bg-destructive/20 disabled:opacity-50"
                          onclick={() => handleReject(memory.id)}
                          disabled={reviewingId === memory.id}
                        >
                          {#if reviewingId === memory.id}
                            <span class="block h-3 w-3 animate-spin rounded-full border border-current/30 border-t-current"></span>
                          {:else}
                            <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
                          {/if}
                          拒绝
                        </button>
                        <span class="ml-auto text-[10px] text-muted-foreground">{formatDate(memory.created_at)}</span>
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
                      <p class="line-clamp-3 text-xs leading-5 text-foreground">{memory.content}</p>
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
                      {#if memory.tags.length > 0}
                        <div class="mt-1 flex flex-wrap gap-1">
                          {#each memory.tags as tag}
                            <span
                              class="rounded bg-primary/5 px-1 py-0.5 text-[9px] text-primary"
                            >{tag}</span>
                          {/each}
                        </div>
                      {/if}
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
                      <div class="mt-0.5 text-[10px] text-muted-foreground">
                        {formatDate(memory.created_at)}
                      </div>
                    </div>
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

      <!-- Footer -->
      <div class="flex h-10 shrink-0 items-center justify-between border-t border-[#1e1e2e] px-4">
        <span class="text-[11px] text-muted-foreground">
          {store.memories.length} 条记忆
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
