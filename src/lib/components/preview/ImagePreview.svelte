<script lang="ts">
  import { onMount } from "svelte";

  let {
    filepath = "",
    cwd = "",
  }: {
    filepath: string;
    cwd: string;
  } = $props();

  let zoom = $state(1);
  let imageUrl = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      const { readFileAsBase64 } = await import("$lib/api");
      const base64 = await readFileAsBase64(filepath, cwd);
      const ext = filepath.split(".").pop()?.toLowerCase() ?? "";
      const mimeMap: Record<string, string> = {
        png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg",
        gif: "image/gif", svg: "image/svg+xml", webp: "image/webp",
        bmp: "image/bmp", ico: "image/x-icon",
      };
      const mime = mimeMap[ext] ?? "image/png";
      imageUrl = `data:${mime};base64,${base64}`;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<div class="flex flex-col items-center justify-center h-full bg-muted/20">
  {#if loading}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
  {:else if error}
    <div class="text-xs text-red-500 p-4">{error}</div>
  {:else if imageUrl}
    <div class="flex-1 overflow-auto flex items-center justify-center w-full">
      <img
        src={imageUrl}
        alt={filepath}
        style="transform: scale({zoom})"
        class="max-w-full max-h-full object-contain transition-transform"
      />
    </div>
    <div class="flex items-center gap-2 mt-2 mb-2">
      <button class="px-2 py-0.5 text-xs bg-muted rounded" onclick={() => (zoom = Math.max(0.25, zoom - 0.25))}>-</button>
      <span class="text-xs text-muted-foreground">{Math.round(zoom * 100)}%</span>
      <button class="px-2 py-0.5 text-xs bg-muted rounded" onclick={() => (zoom = Math.min(4, zoom + 0.25))}>+</button>
    </div>
  {/if}
</div>
