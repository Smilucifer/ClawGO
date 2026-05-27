<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { PreviewState } from "$lib/stores/preview-store.svelte";
  import MonacoEditor from "./MonacoEditor.svelte";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import HtmlPreview from "./HtmlPreview.svelte";
  import ImagePreview from "./ImagePreview.svelte";
  import PdfPreview from "./PdfPreview.svelte";
  import OfficePreview from "./OfficePreview.svelte";

  let {
    state,
    cwd = "",
    onClose = () => {},
    onContentChange = (_value: string) => {},
    onSave = () => {},
  }: {
    state: PreviewState;
    cwd?: string;
    onClose?: () => void;
    onContentChange?: (value: string) => void;
    onSave?: () => void;
  } = $props();

  let panelEl: HTMLDivElement | undefined;

  function handleKeydown(e: KeyboardEvent) {
    if (!panelEl?.contains(e.target as Node)) return;
    if (e.key === "Escape") onClose();
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      onSave();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div bind:this={panelEl} class="flex flex-col h-full border-l border-border bg-background">
  <!-- Header -->
  <div class="flex items-center justify-between px-3 py-1.5 border-b shrink-0 bg-muted/20">
    <span class="text-[11px] font-mono text-muted-foreground truncate min-w-0" title={state.filepath ?? ""}>
      {state.filepath ?? ""}
    </span>
    <div class="flex items-center gap-1 shrink-0 ml-2">
      {#if state.dirty}
        <button
          class="text-[10px] px-1.5 py-0.5 rounded bg-primary/15 text-primary hover:bg-primary/25 transition-colors"
          onclick={onSave}
          title="Save (Ctrl+S)">
          {t("filesPanel_save")}
        </button>
      {/if}
      <button
        class="rounded p-0.5 hover:bg-accent transition-colors"
        onclick={onClose}
        title="Close (Esc)">
        <svg class="h-3.5 w-3.5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18" /><path d="m6 6 12 12" />
        </svg>
      </button>
    </div>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-hidden">
    {#if state.loading}
      <div class="flex items-center justify-center h-full">
        <div class="h-5 w-5 rounded-full border-2 border-muted-foreground/30 border-t-primary animate-spin"></div>
      </div>
    {:else if state.error}
      <div class="flex items-center justify-center h-full text-xs text-red-500 p-4 text-center">
        {state.error}
      </div>
    {:else if state.fileType === "image"}
        <ImagePreview filepath={state.filepath!} {cwd} />
      {:else if state.fileType === "pdf"}
        <PdfPreview filepath={state.filepath!} {cwd} />
      {:else if state.fileType === "word" || state.fileType === "excel"}
        <OfficePreview filepath={state.filepath!} fileType={state.fileType === "word" ? "word" : "excel"} {cwd} />
      {:else if state.content !== null}
        {#if state.fileType === "code" || state.fileType === "other"}
          <MonacoEditor content={state.content} language={state.language} onChange={onContentChange} />
        {:else if state.fileType === "markdown"}
          <MarkdownPreview content={state.content} />
        {:else if state.fileType === "html"}
          <HtmlPreview content={state.content} />
        {/if}
      {:else}
        <div class="flex items-center justify-center h-full text-xs text-muted-foreground">
          {t("filesPanel_noContent")}
        </div>
      {/if}
  </div>
</div>
