<script lang="ts">
  let {
    content = "",
  }: {
    content: string;
  } = $props();
</script>

<div class="h-full w-full">
  {#if content}
    <iframe srcdoc={content} class="h-full w-full border-0" sandbox="allow-scripts" title="HTML Preview"></iframe>
  {/if}
</div>
