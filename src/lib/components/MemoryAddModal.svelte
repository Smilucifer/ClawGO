<script lang="ts">
  import type { MemoryNode } from "$lib/types";
  import { characterMemoryStore } from "$lib/stores/character-memory-store.svelte";

  let {
    open = $bindable(false),
  }: {
    open?: boolean;
  } = $props();

  let content = $state("");
  let memoryType: MemoryNode["type"] = $state("fact");
  let confidence = $state(75);
  let tagsText = $state("");
  let saving = $state(false);
  let error = $state("");

  const memoryTypes: { value: MemoryNode["type"]; label: string }[] = [
    { value: "fact", label: "事实 Fact" },
    { value: "experience", label: "经验 Experience" },
    { value: "preference", label: "偏好 Preference" },
    { value: "rule", label: "规则 Rule" },
    { value: "relationship", label: "关系 Relationship" },
  ];

  function reset() {
    content = "";
    memoryType = "fact";
    confidence = 75;
    tagsText = "";
    error = "";
  }

  async function handleSubmit() {
    const trimmed = content.trim();
    if (!trimmed) return;
    saving = true;
    error = "";
    try {
      const tags = tagsText
        .split(/[,，]/)
        .map((t) => t.trim())
        .filter(Boolean);
      await characterMemoryStore.addMemory(trimmed, memoryType, confidence, tags);
      reset();
      open = false;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !saving) {
      open = false;
    }
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onkeydown={handleKeydown}
  >
    <div
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
      onclick={() => !saving && (open = false)}
      role="presentation"
    ></div>

    <div class="relative z-50 w-full max-w-lg rounded-lg border border-border bg-background p-5 shadow-lg">
      <div class="mb-4 flex items-center justify-between">
        <h2 class="text-base font-semibold">手动添加记忆</h2>
        <button
          type="button"
          class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
          onclick={() => (open = false)}
          disabled={saving}
          aria-label="Close"
        >
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 6 6 18" /><path d="m6 6 12 12" />
          </svg>
        </button>
      </div>

      {#if error}
        <div class="mb-3 rounded border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {error}
        </div>
      {/if}

      <form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
        <div class="space-y-4">
          <!-- Content -->
          <div>
            <label class="mb-1.5 block text-xs font-medium text-foreground" for="mem-content">内容 Content</label>
            <textarea
              id="mem-content"
              class="h-24 w-full resize-none rounded-md border border-input bg-background px-3 py-2 text-sm outline-none focus:ring-2 focus:ring-ring"
              bind:value={content}
              placeholder="输入记忆内容..."
            ></textarea>
          </div>

          <!-- Type -->
          <div>
            <label class="mb-1.5 block text-xs font-medium text-foreground" for="mem-type">类型 Type</label>
            <select
              id="mem-type"
              class="h-9 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
              bind:value={memoryType}
            >
              {#each memoryTypes as mt}
                <option value={mt.value}>{mt.label}</option>
              {/each}
            </select>
          </div>

          <!-- Confidence -->
          <div>
            <label class="mb-1.5 block text-xs font-medium text-foreground" for="mem-confidence">
              置信度 Confidence: {confidence}%
            </label>
            <input
              id="mem-confidence"
              type="range"
              min="50"
              max="100"
              class="h-2 w-full cursor-pointer appearance-none rounded-full bg-border accent-primary"
              bind:value={confidence}
            />
            <div class="mt-1 flex justify-between text-[10px] text-muted-foreground">
              <span>50%</span>
              <span>100%</span>
            </div>
          </div>

          <!-- Tags -->
          <div>
            <label class="mb-1.5 block text-xs font-medium text-foreground" for="mem-tags">标签 Tags</label>
            <input
              id="mem-tags"
              class="h-9 w-full rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
              bind:value={tagsText}
              placeholder="用逗号分隔，如 编程, Rust, Tauri"
            />
          </div>
        </div>

        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="h-9 rounded-md border border-border px-4 text-sm text-muted-foreground hover:bg-accent hover:text-foreground transition-colors disabled:opacity-50"
            onclick={() => (open = false)}
            disabled={saving}
          >
            取消
          </button>
          <button
            type="submit"
            class="h-9 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground disabled:opacity-50"
            disabled={saving || !content.trim()}
          >
            {saving ? "保存中..." : "保存"}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
