<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { MemoStore } from "$lib/stores/memo-store.svelte";
  import { copyToClipboard } from "$lib/utils/tool-rendering";

  interface Props {
    open: boolean;
    onclose: () => void;
  }

  let { open, onclose }: Props = $props();

  const store = new MemoStore();

  let draft = $state("");
  let copiedId = $state("");

  onMount(() => {
    store.load({ kind: "global" });
  });

  $effect(() => {
    if (open) {
      store.load({ kind: "global" });
      function onKey(e: KeyboardEvent) {
        if (e.key === "Escape") onclose();
      }
      window.addEventListener("keydown", onKey);
      return () => window.removeEventListener("keydown", onKey);
    }
  });

  async function addMemo() {
    const text = draft.trim();
    if (!text) return;
    await store.add(text);
    draft = "";
  }

  async function deleteMemo(id: string) {
    await store.delete(id);
  }

  async function copyMemo(text: string, id: string) {
    await copyToClipboard(text);
    copiedId = id;
    setTimeout(() => {
      if (copiedId === id) copiedId = "";
    }, 1500);
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="fixed inset-0 z-40 bg-black/20"
    onclick={onclose}
    onkeydown={(e) => e.key === "Escape" && onclose()}
  ></div>

  <div class="fixed right-0 top-0 bottom-0 z-50 flex w-80 flex-col border-l border-border bg-background shadow-xl">
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <h2 class="text-sm font-semibold">{t("memo_panelTitle")}</h2>
      <button
        type="button"
        class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
        onclick={onclose}
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
          <path d="M18 6 6 18" /><path d="m6 6 12 12" />
        </svg>
      </button>
    </div>

    <form
      class="shrink-0 border-b border-border p-3"
      onsubmit={(e) => {
        e.preventDefault();
        addMemo();
      }}
    >
      <div class="flex gap-2">
        <input
          class="h-9 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
          bind:value={draft}
          placeholder={t("memo_panelPlaceholder")}
        />
        <button
          type="submit"
          class="h-9 shrink-0 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground disabled:opacity-50"
          disabled={!draft.trim()}
        >
          {t("memo_add")}
        </button>
      </div>
    </form>

    {#if store.error}
      <div class="shrink-0 border-b border-destructive/30 bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
        {store.error}
      </div>
    {/if}

    <div class="flex-1 overflow-y-auto">
      {#if store.loading}
        <div class="flex justify-center py-12">
          <div class="h-4 w-4 animate-spin rounded-full border-2 border-primary/30 border-t-primary"></div>
        </div>
      {:else if store.items.length === 0}
        <div class="py-12 text-center text-sm text-muted-foreground">
          {t("memo_empty")}
        </div>
      {:else}
        <div class="divide-y divide-border">
          {#each store.items as item (item.id)}
            <div class="px-4 py-3">
              <p class="whitespace-pre-wrap break-words text-sm leading-6">{item.text}</p>
              <div class="mt-2 flex items-center justify-between gap-2">
                <span class="text-[11px] text-muted-foreground">
                  {new Date(item.updatedAt).toLocaleString()}
                </span>
                <div class="flex items-center gap-1">
                  <button
                    type="button"
                    class="h-7 rounded-md px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground transition-colors"
                    onclick={() => copyMemo(item.text, item.id)}
                  >
                    {copiedId === item.id ? t("common_copied") : t("common_copy")}
                  </button>
                  <button
                    type="button"
                    class="h-7 rounded-md px-2 text-[11px] text-destructive hover:bg-destructive/10 transition-colors"
                    onclick={() => deleteMemo(item.id)}
                  >
                    {t("memo_delete")}
                  </button>
                </div>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}
