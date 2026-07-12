<script lang="ts">
  import { onMount } from "svelte";
  import { interopDefault } from "$lib/utils/interop-default";

  let {
    filepath = "",
    fileType = "word",
    cwd = "",
  }: {
    filepath: string;
    fileType: "word" | "excel";
    cwd: string;
  } = $props();

  let htmlContent = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      const { readFileAsBuffer } = await import("$lib/api");
      const buffer = await readFileAsBuffer(filepath, cwd);

      if (fileType === "word") {
        const mammoth = interopDefault(await import("mammoth"));
        const result = await mammoth.convertToHtml({ arrayBuffer: buffer });
        htmlContent = result.value;
      } else {
        const XLSX = interopDefault(await import("xlsx"));
        const workbook = XLSX.read(buffer, { type: "array" });
        const firstSheet = workbook.SheetNames[0];
        const sheet = workbook.Sheets[firstSheet];
        htmlContent = XLSX.utils.sheet_to_html(sheet);
      }

      const DOMPurify = interopDefault(await import("dompurify"));
      htmlContent = DOMPurify.sanitize(htmlContent, {
        ADD_ATTR: ["class", "target", "style"],
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<div class="h-full overflow-auto p-4">
  {#if loading}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
  {:else if error}
    <div class="text-xs text-red-500">{error}</div>
  {:else}
    <div class="prose prose-sm dark:prose-invert max-w-none">{@html htmlContent}</div>
  {/if}
</div>
