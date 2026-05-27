<script lang="ts">
  let {
    content = "",
  }: {
    content: string;
  } = $props();

  let blobUrl = $state("");

  $effect(() => {
    if (blobUrl) URL.revokeObjectURL(blobUrl);
    const blob = new Blob([content], { type: "text/html" });
    blobUrl = URL.createObjectURL(blob);
    return () => {
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  });
</script>

<div class="h-full w-full">
  {#if blobUrl}
    <iframe src={blobUrl} class="h-full w-full border-0" sandbox="allow-scripts" title="HTML Preview"></iframe>
  {/if}
</div>
