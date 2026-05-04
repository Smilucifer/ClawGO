<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { MemoStore } from "$lib/stores/memo-store.svelte";
  import { shouldApplyMemoProjectCwdChange } from "$lib/utils/memo-page";
  import type { MemoItem, MemoScope } from "$lib/types";

  const store = new MemoStore();

  let projectCwd = $state(
    typeof window !== "undefined" ? (localStorage.getItem("ocv:project-cwd") ?? "") : "",
  );
  let selectedScope = $state<"global" | "project">("global");
  let draft = $state("");
  let selectedId = $state("");
  let editorText = $state("");
  let copiedId = $state("");
  let lastEditorSourceId = "";

  const hasProject = $derived(Boolean(projectCwd.trim()));
  const selectedMemo = $derived<MemoItem | null>(
    store.items.find((item) => item.id === selectedId) ?? null,
  );
  const isDirty = $derived(Boolean(selectedMemo && editorText !== selectedMemo.text));

  function currentScope(): MemoScope {
    if (selectedScope === "project" && hasProject) {
      return { kind: "project", cwd: projectCwd };
    }
    return { kind: "global" };
  }

  function selectFirst() {
    const first = store.items[0];
    selectedId = first?.id ?? "";
    editorText = first?.text ?? "";
    lastEditorSourceId = first?.id ?? "";
  }

  async function loadCurrentScope() {
    if (selectedScope === "project" && !hasProject) {
      selectedScope = "global";
    }
    await store.load(currentScope());
    selectFirst();
  }

  function confirmDiscard(): boolean {
    return !isDirty || window.confirm(t("memory_discardConfirm"));
  }

  async function switchScope(next: "global" | "project") {
    if (next === selectedScope) return;
    if (next === "project" && !hasProject) return;
    if (!confirmDiscard()) return;
    selectedScope = next;
    await loadCurrentScope();
  }

  function selectMemo(id: string) {
    if (id === selectedId) return;
    if (!confirmDiscard()) return;
    const item = store.items.find((memo) => memo.id === id);
    selectedId = item?.id ?? "";
    editorText = item?.text ?? "";
    lastEditorSourceId = item?.id ?? "";
  }

  async function addMemo() {
    const text = draft.trim() || t("memo_untitled");
    const item = await store.add(text);
    draft = "";
    selectedId = item.id;
    editorText = item.text;
    lastEditorSourceId = item.id;
  }

  async function saveMemo() {
    if (!selectedMemo) return;
    const text = editorText.trim();
    if (!text) return;
    const item = await store.update(selectedMemo.id, text);
    editorText = item.text;
    lastEditorSourceId = item.id;
  }

  async function deleteSelected() {
    if (!selectedMemo) return;
    await store.delete(selectedMemo.id);
    selectFirst();
  }

  async function clearAll() {
    if (store.items.length === 0) return;
    if (!window.confirm(t("memo_clearConfirm"))) return;
    await store.clear();
    selectedId = "";
    editorText = "";
    lastEditorSourceId = "";
  }

  async function copySelected() {
    if (!selectedMemo) return;
    await navigator.clipboard.writeText(selectedMemo.text);
    copiedId = selectedMemo.id;
    window.setTimeout(() => {
      if (copiedId === selectedMemo.id) copiedId = "";
    }, 1200);
  }

  $effect(() => {
    if (!selectedMemo) {
      if (lastEditorSourceId) {
        editorText = "";
        lastEditorSourceId = "";
      }
      return;
    }
    if (lastEditorSourceId !== selectedMemo.id) {
      editorText = selectedMemo.text;
      lastEditorSourceId = selectedMemo.id;
    }
  });

  onMount(() => {
    void loadCurrentScope();

    function onProjectChanged(e: Event) {
      if (!shouldApplyMemoProjectCwdChange(selectedScope, isDirty, confirmDiscard)) return;
      projectCwd = (e as CustomEvent).detail?.cwd ?? "";
      if (selectedScope === "project") {
        void loadCurrentScope();
      }
    }

    window.addEventListener("ocv:project-changed", onProjectChanged);
    return () => window.removeEventListener("ocv:project-changed", onProjectChanged);
  });
</script>

<div class="flex h-full overflow-hidden bg-background">
  <aside class="flex w-72 shrink-0 flex-col border-r border-border bg-muted/20">
    <div class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-semibold">{t("memo_title")}</h1>
      <div class="mt-3 inline-flex rounded-md border border-border bg-background p-0.5">
        <button
          type="button"
          class="h-8 px-3 text-xs font-medium rounded-sm transition-colors {selectedScope ===
          'global'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:bg-muted'}"
          onclick={() => void switchScope("global")}
        >
          {t("memo_scopeGlobal")}
        </button>
        <button
          type="button"
          class="h-8 px-3 text-xs font-medium rounded-sm transition-colors disabled:opacity-40 {selectedScope ===
          'project'
            ? 'bg-primary text-primary-foreground'
            : 'text-muted-foreground hover:bg-muted'}"
          disabled={!hasProject}
          onclick={() => void switchScope("project")}
        >
          {t("memo_scopeProject")}
        </button>
      </div>
    </div>

    <form
      class="border-b border-border p-3"
      onsubmit={(event) => {
        event.preventDefault();
        void addMemo();
      }}
    >
      <div class="flex gap-2">
        <input
          class="h-9 min-w-0 flex-1 rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring"
          bind:value={draft}
          placeholder={t("memo_addPlaceholder")}
        />
        <button
          type="submit"
          class="h-9 rounded-md bg-primary px-3 text-sm font-medium text-primary-foreground"
        >
          {t("memo_add")}
        </button>
      </div>
    </form>

    <div class="flex-1 overflow-y-auto p-2">
      {#if store.loading}
        <div class="py-8 text-center text-sm text-muted-foreground">{t("common_loading")}</div>
      {:else if store.items.length === 0}
        <div class="py-8 text-center text-sm text-muted-foreground">{t("memo_empty")}</div>
      {:else}
        <div class="space-y-1">
          {#each store.items as item (item.id)}
            <button
              type="button"
              class="w-full rounded-md px-3 py-2 text-left transition-colors {selectedId === item.id
                ? 'bg-primary/10 text-foreground'
                : 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
              onclick={() => selectMemo(item.id)}
            >
              <div class="truncate text-sm font-medium">
                {item.text.split("\n")[0] || t("memo_untitled")}
              </div>
              <div class="mt-1 truncate text-[11px] text-muted-foreground/80">
                {new Date(item.updatedAt).toLocaleString()}
              </div>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="border-t border-border p-3">
      <button
        type="button"
        class="h-8 w-full rounded-md border border-border text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-40"
        disabled={store.items.length === 0}
        onclick={clearAll}
      >
        {t("memo_clear")}
      </button>
    </div>
  </aside>

  <main class="flex min-w-0 flex-1 flex-col">
    <div class="flex h-12 shrink-0 items-center justify-between border-b border-border px-4">
      <div class="min-w-0">
        <div class="truncate text-sm font-medium">
          {selectedMemo ? selectedMemo.text.split("\n")[0] || t("memo_untitled") : t("memo_title")}
        </div>
        {#if selectedMemo}
          <div class="text-[11px] text-muted-foreground">
            {new Date(selectedMemo.updatedAt).toLocaleString()}
            {#if isDirty}
              <span class="ml-2 text-primary">{t("memo_unsaved")}</span>
            {/if}
          </div>
        {/if}
      </div>

      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="h-8 px-3 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-40"
          disabled={!selectedMemo}
          onclick={copySelected}
        >
          {copiedId === selectedMemo?.id ? t("common_copied") : t("common_copy")}
        </button>
        <button
          type="button"
          class="h-8 px-3 text-xs text-destructive transition-colors hover:text-destructive/80 disabled:opacity-40"
          disabled={!selectedMemo}
          onclick={deleteSelected}
        >
          {t("memo_delete")}
        </button>
        <button
          type="button"
          class="h-8 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground disabled:opacity-40"
          disabled={!selectedMemo || !editorText.trim() || !isDirty}
          onclick={saveMemo}
        >
          {t("common_save")}
        </button>
      </div>
    </div>

    {#if store.error}
      <div
        class="border-b border-destructive/30 bg-destructive/10 px-4 py-2 text-xs text-destructive"
      >
        {store.error}
      </div>
    {/if}

    {#if selectedMemo}
      <textarea
        class="flex-1 resize-none bg-background p-5 text-sm leading-6 outline-none"
        bind:value={editorText}
        spellcheck="false"
      ></textarea>
    {:else}
      <div class="flex flex-1 items-center justify-center text-sm text-muted-foreground">
        {t("memo_empty")}
      </div>
    {/if}
  </main>
</div>
