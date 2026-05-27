<script lang="ts">
  import { onMount } from "svelte";

  let {
    filepath = "",
    cwd = "",
  }: {
    filepath: string;
    cwd: string;
  } = $props();

  let fileUrl = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      const { readFileAsBase64 } = await import("$lib/api");
      const base64 = await readFileAsBase64(filepath, cwd);
      fileUrl = `data:application/pdf;base64,${base64}`;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<div class="h-full w-full">
  {#if loading}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
  {:else if error}
    <div class="flex items-center justify-center h-full text-xs text-red-500">{error}</div>
  {:else if fileUrl}
    <iframe src={fileUrl} class="h-full w-full border-0" title="PDF Preview"></iframe>
  {/if}
</div>
