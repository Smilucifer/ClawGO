<script lang="ts" module>
  let _markedOptionsReady = false;
</script>

<script lang="ts">
  import DOMPurify from "dompurify";

  let {
    content = "",
  }: {
    content: string;
  } = $props();

  let renderedHtml = $state("");
  let sourceMode = $state(false);
  let loading = $state(true);

  async function render() {
    loading = true;
    try {
      const [markedMod, hljsMod] = await Promise.all([
        import("marked"),
        import("highlight.js"),
      ]);
      const { marked } = markedMod;
      const hljs = hljsMod.default;
      if (!_markedOptionsReady) {
        marked.setOptions({
          // @ts-expect-error marked highlight callback
          highlight: (code: string, lang: string) => {
            if (lang && hljs.getLanguage(lang)) {
              return hljs.highlight(code, { language: lang }).value;
            }
            return hljs.highlightAuto(code).value;
          },
        });
        _markedOptionsReady = true;
      }
      const parsed = marked.parse(content);
      if (typeof parsed === "string") {
        renderedHtml = DOMPurify.sanitize(parsed, {
          ADD_ATTR: ["class", "target", "data-code-copy"],
        });
      } else {
        renderedHtml = "";
      }
    } catch {
      renderedHtml = `<pre>${escapeHtml(content)}</pre>`;
    } finally {
      loading = false;
    }
  }

  function escapeHtml(s: string): string {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  $effect(() => {
    void content;
    if (!sourceMode) render();
  });
</script>

<div class="flex flex-col h-full">
  <div class="flex items-center gap-1 px-2 py-1 border-b shrink-0">
    <button
      class="text-[10px] px-1.5 py-0.5 rounded {sourceMode ? 'bg-muted text-muted-foreground' : 'bg-primary/15 text-primary'} transition-colors"
      onclick={() => (sourceMode = false)}>
      Preview
    </button>
    <button
      class="text-[10px] px-1.5 py-0.5 rounded {sourceMode ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'} transition-colors"
      onclick={() => (sourceMode = true)}>
      Source
    </button>
  </div>
  <div class="flex-1 overflow-auto">
    {#if loading}
      <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
    {:else if sourceMode}
      <pre class="text-[11px] font-mono leading-relaxed whitespace-pre-wrap p-4">{content}</pre>
    {:else}
      <div class="p-4 prose prose-sm dark:prose-invert max-w-none">{@html renderedHtml}</div>
    {/if}
  </div>
</div>
