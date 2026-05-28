<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  let open = $state(false);
  let menuEl: HTMLDivElement | undefined = $state();

  function toggle() {
    open = !open;
  }

  function handleClickOutside(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) {
      open = false;
    }
  }

  function handleDoctor() {
    open = false;
    window.dispatchEvent(new CustomEvent("ocv:toggle-doctor"));
  }

  function handleReleaseNotes() {
    open = false;
    window.location.href = "/release-notes";
  }

  function handleSettings() {
    open = false;
    window.location.href = "/settings";
  }
</script>

<svelte:window onclick={handleClickOutside} />

<div class="relative" bind:this={menuEl}>
  <button
    class="flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
    onclick={toggle}
    title="More"
  >
    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="12" cy="12" r="1" />
      <circle cx="19" cy="12" r="1" />
      <circle cx="5" cy="12" r="1" />
    </svg>
  </button>

  {#if open}
    <div class="absolute right-0 top-full z-50 mt-1 min-w-[180px] rounded-md border border-border bg-popover p-1 shadow-md">
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm transition-colors hover:bg-muted"
        onclick={handleDoctor}
      >
        <span>&#x1FA7A;</span>
        <span>{t("moreMenu_doctor")}</span>
      </button>
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm transition-colors hover:bg-muted"
        onclick={handleReleaseNotes}
      >
        <span>&#x1F4CB;</span>
        <span>{t("moreMenu_releaseNotes")}</span>
      </button>
      <div class="my-1 h-px bg-border"></div>
      <button
        class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted"
        onclick={handleSettings}
      >
        <span>&#x2699;&#xFE0F;</span>
        <span>{t("moreMenu_allSettings")}</span>
      </button>
    </div>
  {/if}
</div>
