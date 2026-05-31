<script lang="ts">
  import { onMount } from "svelte";
  import { beforeNavigate } from "$app/navigation";
  import { page } from "$app/stores";
  import * as api from "$lib/api";
  import Button from "$lib/components/Button.svelte";
  import MarkdownContent from "$lib/components/MarkdownContent.svelte";
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { dbgWarn } from "$lib/utils/debug";
  import { filterVisibleCandidates } from "$lib/utils/memory-helpers";
  import type { MemoryFileCandidate } from "$lib/types";

  // --- File browser state ---
  let candidates = $state<MemoryFileCandidate[]>([]);
  let showCreate = $state(false);

  // Selected file path -- set by browser click or initial auto-select
  // Declared early so derived values below can reference it.
  let selectedFile = $state("");

  // Custom file from ?file= query param (overrides selection)
  let customFile = $derived($page.url.searchParams.get("file") ?? "");

  let projectCwd = $state(
    typeof window !== "undefined" ? (localStorage.getItem("clawgo:project-cwd") ?? "") : "",
  );

  let scopeGlobal = $derived(candidates.filter((c) => c.scope === "global"));
  let scopeProject = $derived(candidates.filter((c) => c.scope === "project"));
  let scopeMemory = $derived(candidates.filter((c) => c.scope === "memory"));
  // Merge project + auto-memory for the project section
  let scopeFolder = $derived([...scopeProject, ...scopeMemory]);

  let visibleGlobal = $derived(filterVisibleCandidates(scopeGlobal, showCreate, selectedFile));
  let visibleFolder = $derived(filterVisibleCandidates(scopeFolder, showCreate, selectedFile));

  let hasGlobal = $derived(visibleGlobal.length > 0);
  let hasFolder = $derived(visibleFolder.length > 0);

  // Section expand/collapse state
  let sectionExpanded = $state<Record<string, boolean>>({
    global: true,
    project: true,
  });

  function toggleSection(key: string) {
    sectionExpanded[key] = !sectionExpanded[key];
  }

  // --- Editor state ---
  let viewMode = $state<"edit" | "preview">("edit");
  let content = $state("");
  let savedContent = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let toastVisible = $state(false);
  let toastFading = $state(false);
  let error = $state("");

  // The cwd that was active when the current content was loaded.
  let saveCwd = $state("");

  let currentPath = $derived(customFile || selectedFile);

  // Page title: show filename for custom file, otherwise file label
  let pageTitle = $derived.by(() => {
    if (customFile) return customFile.split(/[/\\]/).pop() ?? "File";
    const match = candidates.find((c) => c.path === selectedFile);
    if (match) return match.label;
    if (selectedFile) return selectedFile.split(/[/\\]/).pop() ?? "File";
    return "Memory";
  });

  // Only show preview toggle for markdown files
  let isMarkdown = $derived(currentPath.endsWith(".md"));

  // Dirty state
  let isDirty = $derived(content !== savedContent);

  // Notify layout sidebar of dirty state
  $effect(() => {
    window.dispatchEvent(
      new CustomEvent("clawgo:file-dirty", {
        detail: { path: currentPath, dirty: isDirty },
      }),
    );
  });

  // --- Sequence guards ---
  let loadSeq = 0;
  let candidateSeq = 0;
  let projectChangeSeq = 0;
  let autoSelectSeq = 0;

  /** Refresh the candidate list from the backend. */
  async function refreshCandidates(opts?: { soft?: boolean }) {
    const seq = ++candidateSeq;
    try {
      const result = await api.listMemoryFiles(projectCwd || undefined);
      if (seq !== candidateSeq) return;
      candidates = result;
    } catch (e) {
      if (seq !== candidateSeq) return;
      dbgWarn("memory", "candidates refresh failed", e);
    }
  }

  /** Load file content. Pass explicit path to avoid $derived timing issues. */
  async function loadContentForPath(explicitPath: string) {
    const seq = ++loadSeq;
    if (!explicitPath) {
      content = "";
      savedContent = "";
      loading = false;
      return;
    }
    loading = true;
    error = "";
    try {
      const cwdSnapshot = saveCwd || projectCwd;
      const text = await api.readTextFile(explicitPath, cwdSnapshot || undefined);
      if (seq !== loadSeq) return;
      content = text;
      savedContent = text;
      saveCwd = cwdSnapshot;
    } catch (e) {
      if (seq !== loadSeq) return;
      const msg = String(e);
      if (msg.includes("No such file") || msg.includes("not found")) {
        content = "";
        savedContent = "";
        saveCwd = projectCwd;
      } else {
        content = "";
        savedContent = "";
        saveCwd = projectCwd;
        error = msg;
      }
    } finally {
      if (seq === loadSeq) loading = false;
    }
  }

  /** Convenience wrapper: load content for the current path. */
  function loadContent() {
    loadContentForPath(currentPath);
  }

  /** Auto-select first existing file from candidates (initial load). */
  async function autoSelectFirst() {
    const seq = ++autoSelectSeq;
    try {
      const all = candidates.length > 0 ? candidates : await api.listMemoryFiles(projectCwd || undefined);
      if (seq !== autoSelectSeq) return;
      if (candidates.length === 0) candidates = all;
      // Prefer MEMORY.md, then first existing project file
      const memoryMd = all.find((f) => f.exists && f.path.replace(/\\/g, "/").endsWith("MEMORY.md"));
      const existing = all.find((f) => f.exists && f.scope === "project");
      const fallback = all.find((f) => f.exists) ?? all[0];
      const pick = memoryMd ?? existing ?? fallback;
      if (pick) {
        selectedFile = pick.path;
        if (!customFile) {
          window.dispatchEvent(
            new CustomEvent("clawgo:memory-file-selected", { detail: { path: pick.path } }),
          );
        }
      }
    } catch (e) {
      if (seq !== autoSelectSeq) return;
      dbgWarn("memory", "autoSelectFirst failed", e);
    }
  }

  /** Guard a file switch: confirm dirty state before switching. */
  function guardedFileSwitch(newPath: string, exists = true) {
    if (newPath === selectedFile) return;
    if (isDirty && !confirm(t("memory_discardConfirm"))) return;
    saveCwd = "";
    selectedFile = newPath;
    window.dispatchEvent(
      new CustomEvent("clawgo:memory-file-selected", { detail: { path: newPath } }),
    );
    if (exists) {
      loadContentForPath(newPath);
    } else {
      ++loadSeq;
      content = "";
      savedContent = "";
      loading = false;
      saveCwd = projectCwd;
    }
  }

  /** Async variant for project change: refresh candidates -> auto-select -> load. */
  async function guardedProjectChange(newCwd: string) {
    projectCwd = newCwd;
    if (isDirty && !confirm(t("memory_discardConfirm"))) return;
    saveCwd = "";
    const seq = ++projectChangeSeq;
    await refreshCandidates();
    await autoSelectFirst();
    if (seq !== projectChangeSeq) return;
    await loadContent();
  }

  // customFile (query param) changes
  let _customFileInit = false;
  let _prevCustomFile: string | undefined;
  $effect(() => {
    const f = customFile;
    if (!_customFileInit) {
      _customFileInit = true;
      _prevCustomFile = f;
      return;
    }
    if (f === _prevCustomFile) return;
    _prevCustomFile = f;
    ++projectChangeSeq;
    ++loadSeq;
    ++autoSelectSeq;
    if (f) {
      loadContentForPath(f);
    } else {
      autoSelectFirst().then(() => {
        loadContentForPath(selectedFile);
      });
    }
  });

  // Initial load
  onMount(async () => {
    await refreshCandidates();
    if (!customFile) {
      await autoSelectFirst();
    }
    await loadContent();
  });

  // Listen for sidebar file selection
  onMount(() => {
    function onMemorySelect(e: Event) {
      const detail = (e as CustomEvent).detail;
      const path = detail?.path ?? "";
      if (path) {
        guardedFileSwitch(path, detail?.exists ?? true);
      }
    }
    window.addEventListener("clawgo:memory-select", onMemorySelect);
    return () => window.removeEventListener("clawgo:memory-select", onMemorySelect);
  });

  // Listen for sidebar refresh signal (after save)
  onMount(() => {
    function onMemoryFileSaved() {
      refreshCandidates({ soft: true });
    }
    window.addEventListener("clawgo:memory-file-saved", onMemoryFileSaved);
    return () => window.removeEventListener("clawgo:memory-file-saved", onMemoryFileSaved);
  });

  // Sync projectCwd when layout changes it
  onMount(() => {
    function onProjectChanged(e: Event) {
      const cwd = (e as CustomEvent).detail?.cwd ?? "";
      if (cwd === projectCwd) return;
      if (customFile) {
        projectCwd = cwd;
        autoSelectFirst();
        return;
      }
      guardedProjectChange(cwd);
    }
    window.addEventListener("clawgo:project-changed", onProjectChanged);
    return () => window.removeEventListener("clawgo:project-changed", onProjectChanged);
  });

  // Warn before navigating away with unsaved changes
  beforeNavigate(({ cancel }) => {
    if (isDirty && !confirm(t("memory_discardConfirm"))) {
      cancel();
    }
  });

  onMount(() => {
    function onBeforeUnload(e: BeforeUnloadEvent) {
      if (content !== savedContent) {
        e.preventDefault();
      }
    }
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  });

  async function save() {
    const path = currentPath;
    if (!path) return;
    saving = true;
    error = "";
    try {
      await api.writeTextFile(path, content, saveCwd || undefined);
      savedContent = content;
      // Notify layout + self to refresh candidates
      window.dispatchEvent(new Event("clawgo:memory-file-saved"));
      toastFading = false;
      toastVisible = true;
      setTimeout(() => {
        toastFading = true;
        setTimeout(() => (toastVisible = false), 250);
      }, 2500);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  /** Get a short display name from a file path. */
  function shortName(path: string): string {
    const parts = path.replace(/\\/g, "/").split("/");
    return parts[parts.length - 1] ?? path;
  }
</script>

<!-- Toast notification -->
{#if toastVisible}
  <div
    class="fixed top-4 left-1/2 -translate-x-1/2 z-50 {toastFading
      ? 'animate-toast-out'
      : 'animate-toast-in'}"
  >
    <div
      class="flex items-center gap-2 rounded-lg bg-emerald-600 px-4 py-2.5 text-sm font-medium text-white shadow-lg"
    >
      <svg
        class="h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
      >
      {t("memory_saved")}
    </div>
  </div>
{/if}

<div class="flex h-full">
  <!-- Left: File browser panel -->
  <div class="flex w-60 shrink-0 flex-col border-r bg-background">
    <!-- Browser header -->
    <div class="flex items-center justify-between border-b px-3 py-2">
      <span class="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        {t("memory_tabMemory")}
      </span>
    </div>

    <!-- File sections -->
    <div class="flex-1 overflow-y-auto">
      <!-- Global Memory Section -->
      {#if hasGlobal}
        <div class="border-b">
          <button
            class="flex w-full items-center gap-1.5 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground hover:bg-muted/50 transition-colors"
            onclick={() => toggleSection("global")}
          >
            <svg
              class="h-3 w-3 shrink-0 transition-transform {sectionExpanded.global
                ? 'rotate-90'
                : ''}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
            >
            <!-- Globe icon -->
            <svg
              class="h-3 w-3 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><circle cx="12" cy="12" r="10" /><path
                d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"
              /><path d="M2 12h20" /></svg
            >
            <span>{t("memory_tabGlobal")}</span>
            <span class="ml-auto text-[10px] font-normal text-muted-foreground/60">
              {scopeGlobal.filter((f) => f.exists).length}
            </span>
          </button>
          {#if sectionExpanded.global}
            <div class="pb-1">
              {#each visibleGlobal as file (file.path)}
                <button
                  class="flex w-full items-center gap-2 px-4 py-1 text-left text-sm transition-colors
                    {currentPath === file.path
                    ? 'bg-primary/10 text-primary font-medium'
                    : 'text-foreground hover:bg-muted/50'}
                    {!file.exists ? 'opacity-50 italic' : ''}"
                  title={file.path}
                  onclick={() => guardedFileSwitch(file.path, file.exists)}
                >
                  <!-- File icon -->
                  <svg
                    class="h-3.5 w-3.5 shrink-0"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    ><path
                      d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"
                    /><path d="M14 2v4a2 2 0 0 0 2 2h4" /></svg
                  >
                  <span class="truncate">{shortName(file.path)}</span>
                  {#if !file.exists}
                    <span class="ml-auto text-[10px] text-muted-foreground">{t("memory_new")}</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- Project Memory Section -->
      {#if hasFolder}
        <div class="border-b">
          <button
            class="flex w-full items-center gap-1.5 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground hover:bg-muted/50 transition-colors"
            onclick={() => toggleSection("project")}
          >
            <svg
              class="h-3 w-3 shrink-0 transition-transform {sectionExpanded.project
                ? 'rotate-90'
                : ''}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
            >
            <!-- Folder icon -->
            <svg
              class="h-3 w-3 shrink-0"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              ><path
                d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
              /></svg
            >
            <span>{t("memory_tabProject")}</span>
            <span class="ml-auto text-[10px] font-normal text-muted-foreground/60">
              {scopeFolder.filter((f) => f.exists).length}
            </span>
          </button>
          {#if sectionExpanded.project}
            <div class="pb-1">
              {#each visibleFolder as file (file.path)}
                <button
                  class="flex w-full items-center gap-2 px-4 py-1 text-left text-sm transition-colors
                    {currentPath === file.path
                    ? 'bg-primary/10 text-primary font-medium'
                    : 'text-foreground hover:bg-muted/50'}
                    {!file.exists ? 'opacity-50 italic' : ''}"
                  title={file.path}
                  onclick={() => guardedFileSwitch(file.path, file.exists)}
                >
                  <!-- File icon -->
                  <svg
                    class="h-3.5 w-3.5 shrink-0"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    ><path
                      d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"
                    /><path d="M14 2v4a2 2 0 0 0 2 2h4" /></svg
                  >
                  <span class="truncate">
                    {#if file.scope === "memory"}
                      <span class="text-muted-foreground">{file.label}</span>
                    {:else}
                      {shortName(file.path)}
                    {/if}
                  </span>
                  {#if !file.exists}
                    <span class="ml-auto text-[10px] text-muted-foreground">{t("memory_new")}</span>
                  {/if}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      <!-- Empty state -->
      {#if !hasGlobal && !hasFolder}
        <div class="flex flex-col items-center justify-center gap-2 px-4 py-8 text-center">
          <svg
            class="h-8 w-8 text-muted-foreground/30"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            ><path
              d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
            /></svg
          >
          <p class="text-xs text-muted-foreground">{t("memory_setProjectFirst")}</p>
        </div>
      {/if}
    </div>
  </div>

  <!-- Right: Editor area -->
  <div class="flex flex-1 flex-col min-w-0">
    <!-- Header bar -->
    <div class="flex items-center justify-between border-b px-4 py-2 shrink-0">
      <div class="flex items-center gap-3 min-w-0">
        <span class="text-sm font-medium truncate">{pageTitle}</span>
        {#if isDirty}
          <span class="h-2 w-2 rounded-full bg-primary shrink-0" title={t("memory_unsavedChanges")}
          ></span>
        {/if}
        {#if currentPath}
          <span
            class="text-[11px] text-muted-foreground truncate hidden sm:inline"
            title={currentPath}>{currentPath}</span
          >
        {/if}
      </div>
      <div class="flex items-center gap-2 shrink-0">
        {#if isMarkdown}
          <div class="flex rounded-md border bg-background p-0.5">
            <button
              class="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium transition-colors
                {viewMode === 'edit'
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (viewMode = "edit")}
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" /><path
                  d="m15 5 4 4"
                /></svg
              >
              {t("common_edit")}
            </button>
            <button
              class="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-medium transition-colors
                {viewMode === 'preview'
                ? 'bg-muted text-foreground'
                : 'text-muted-foreground hover:text-foreground'}"
              onclick={() => (viewMode = "preview")}
            >
              <svg
                class="h-3 w-3"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
                ><path d="M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z" /><circle
                  cx="12"
                  cy="12"
                  r="3"
                /></svg
              >
              {t("common_preview")}
            </button>
          </div>
        {/if}
      </div>
    </div>

    <!-- Content area -->
    {#if !currentPath}
      <div class="flex flex-1 flex-col items-center justify-center gap-3">
        <svg
          class="h-10 w-10 text-muted-foreground/30"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          ><path
            d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
          /></svg
        >
        <p class="text-sm text-muted-foreground">{t("memory_setProjectFirst")}</p>
      </div>
    {:else if loading}
      <div class="flex flex-1 items-center justify-center">
        <div
          class="h-6 w-6 border-2 border-primary/30 border-t-primary rounded-full animate-spin"
        ></div>
      </div>
    {:else if viewMode === "preview" && isMarkdown}
      <div class="flex-1 overflow-y-auto p-4">
        {#if content}
          <MarkdownContent text={content} />
        {:else}
          <p class="text-sm text-muted-foreground italic">{t("memory_noContent")}</p>
        {/if}
      </div>
    {:else}
      <CodeEditor bind:content filePath={currentPath} onsave={save} class="flex-1" />
    {/if}

    <!-- Error -->
    {#if error}
      <div
        class="shrink-0 border-t border-destructive/30 bg-destructive/10 px-4 py-2 text-sm text-destructive"
      >
        {error}
      </div>
    {/if}

    <!-- Bottom action bar -->
    {#if currentPath && !loading}
      <div class="flex items-center gap-3 border-t px-4 py-2 shrink-0">
        <Button onclick={save} loading={saving}>
          {#snippet children()}
            {t("common_save")}
          {/snippet}
        </Button>
        <Button variant="outline" onclick={loadContent}>
          {#snippet children()}
            {t("memory_reload")}
          {/snippet}
        </Button>
      </div>
    {/if}
  </div>
</div>
