<script lang="ts">
  import { FILE_PATH_PATTERN, hasFilePath } from "$lib/utils/file-path-linkifier";

  let { text = "" }: { text: string } = $props();

  function handleClick(filepath: string) {
    window.dispatchEvent(new CustomEvent("clawgo:preview-file", {
      detail: { filepath },
    }));
  }

  const pathTestRe = new RegExp(FILE_PATH_PATTERN.source);
</script>

{#if hasFilePath(text)}
  {@const parts = text.split(FILE_PATH_PATTERN)}
  {#each parts as part}
    {@const isPath = pathTestRe.test(part)}
    {#if isPath}
      <button
        class="text-blue-500 hover:text-blue-400 hover:underline cursor-pointer bg-transparent border-0 p-0 font-inherit text-inherit"
        onclick={() => handleClick(part)}
        title={part}
      >
        {part}
      </button>
    {:else}
      {part}
    {/if}
  {/each}
{:else}
  {text}
{/if}
