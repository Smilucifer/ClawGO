<script lang="ts">
  import { onMount } from "svelte";
  import { checkForUpdates } from "$lib/api";
  import { renderMarkdown } from "$lib/utils/markdown";
  import { t } from "$lib/i18n/index.svelte";
  import changelogRaw from "../../../docs/changelog_user.md?raw";

  let { open = $bindable(false) }: { open: boolean } = $props();

  let appVersion = $state("");
  let checkingUpdate = $state(false);
  let searchQuery = $state("");
  let expandedVersions = $state<Set<string>>(new Set());

  interface ChangelogVersion {
    version: string;
    date: string;
    title: string;
    rawText: string;
    html: string;
  }

  // Lazy-parsed changelog: only parsed on first open
  let allEntries = $state<ChangelogVersion[]>([]);
  let parsed = false;

  function ensureParsed() {
    if (parsed) return;
    parsed = true;
    allEntries = parseChangelog(changelogRaw);
  }

  /** Parse changelog.md into per-version entries.
   *  Accepts both `vX.Y.Z (date)` and `Phase X.Y+ (date)` / bare `Phase X` headers. */
  function parseChangelog(raw: string): ChangelogVersion[] {
    const entries: ChangelogVersion[] = [];
    const versionBlocks = raw.split(/^## /m).filter(Boolean);

    for (const block of versionBlocks) {
      // Match: v5.2.11 (2026-06-03) | Phase 10+ (2026-06-01) | Phase 6
      const headerMatch = block.match(
        /^((?:v[\d.]+|Phase\s+[\d.x+]+))(?:\s*\(([^)]+)\))?\s*\n/,
      );
      if (!headerMatch) continue;

      const version = headerMatch[1].trim();
      const date = headerMatch[2] ?? "";
      const rest = block.slice(headerMatch[0].length);

      // Extract title (### line) and body
      const titleMatch = rest.match(/^### (.+)\n/);
      const title = titleMatch ? titleMatch[1].trim() : "";
      const body = titleMatch ? rest.slice(titleMatch[0].length) : rest;
      const rawText = body.trim();

      entries.push({ version, date, title, rawText, html: renderMarkdown(rawText) });
    }

    return entries;
  }

  let filteredEntries = $derived.by(() => {
    if (!searchQuery.trim()) return allEntries;
    const q = searchQuery.trim().toLowerCase();
    return allEntries.filter(
      (e) =>
        e.version.toLowerCase().includes(q) ||
        e.title.toLowerCase().includes(q) ||
        e.rawText.toLowerCase().includes(q),
    );
  });

  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      appVersion = await getVersion();
    } catch {
      appVersion = "";
    }
  });

  // Lazy-parse on first open, auto-expand current version
  $effect(() => {
    if (open) {
      ensureParsed();
      if (appVersion && expandedVersions.size === 0) {
        expandedVersions = new Set([appVersion]);
      }
    }
  });

  function toggleVersion(version: string) {
    const next = new Set(expandedVersions);
    if (next.has(version)) {
      next.delete(version);
    } else {
      next.add(version);
    }
    expandedVersions = next;
  }

  function expandAll() {
    expandedVersions = new Set(filteredEntries.map((e) => e.version));
  }

  function collapseAll() {
    expandedVersions = new Set();
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) open = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }

  async function updateToLatest() {
    if (checkingUpdate) return;
    checkingUpdate = true;
    try {
      const info = await checkForUpdates();
      if (!info.latestVersion) {
        window.alert(t("appUpdate_checkFailed"));
        return;
      }
      if (!info.hasUpdate) {
        window.alert(
          t("appUpdate_upToDate", { version: info.currentVersion || appVersion || "-" }),
        );
        return;
      }
      if (!info.downloadUrl) {
        window.alert(t("appUpdate_checkFailed"));
        return;
      }
      try {
        const { open } = await import("@tauri-apps/plugin-shell");
        await open(info.downloadUrl);
      } catch {
        window.open(info.downloadUrl, "_blank");
      }
    } catch {
      window.alert(t("appUpdate_checkFailed"));
    } finally {
      checkingUpdate = false;
    }
  }
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
  >
    <div
      class="relative flex max-h-[85vh] w-full max-w-3xl flex-col rounded-xl border border-border bg-background shadow-2xl"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-border px-6 py-4">
        <div class="flex items-center gap-3">
          <span class="text-xs text-muted-foreground"
            >{appVersion ? `Claw GO v${appVersion}` : ""}</span
          >
          <button
            class="rounded-md border border-border px-2.5 py-1 text-xs font-medium text-foreground transition-colors hover:bg-muted disabled:cursor-not-allowed disabled:opacity-60"
            onclick={updateToLatest}
            disabled={checkingUpdate}
          >
            {checkingUpdate ? t("appUpdate_checking") : t("appUpdate_manual")}
          </button>
        </div>
        <button
          class="rounded-md p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground transition-colors"
          onclick={() => (open = false)}
          aria-label="Close"
        >
          <svg
            class="h-5 w-5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"><path d="M18 6 6 18M6 6l12 12" /></svg
          >
        </button>
      </div>

      <!-- Toolbar -->
      <div class="flex items-center gap-2 border-b border-border px-6 py-2.5">
        <svg
          class="h-4 w-4 text-muted-foreground shrink-0"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          ><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" /><path
            d="M14 2v4a2 2 0 0 0 2 2h4"
          /><path d="M10 9H8" /><path d="M16 13H8" /><path d="M16 17H8" /></svg
        >
        <span class="text-sm font-medium">{t("about_changelog")}</span>

        <div class="flex-1"></div>

        <!-- Search -->
        <div class="relative">
          <svg
            class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/50"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" /></svg
          >
          <input
            type="text"
            bind:value={searchQuery}
            placeholder={t("about_searchVersions")}
            class="h-7 w-48 rounded-md border border-border bg-background pl-8 pr-3 text-xs
              placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-ring"
          />
        </div>

        <button
          class="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
          onclick={expandAll}
        >
          {t("about_expandAll")}
        </button>
        <span class="text-muted-foreground/30">|</span>
        <button
          class="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
          onclick={collapseAll}
        >
          {t("about_collapseAll")}
        </button>
      </div>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto">
        {#if filteredEntries.length === 0}
          <div class="flex flex-col items-center gap-2 py-20 text-center">
            <p class="text-sm text-muted-foreground">{t("about_noMatching")}</p>
            {#if searchQuery}
              <button
                class="text-xs text-primary/70 hover:text-primary transition-colors"
                onclick={() => (searchQuery = "")}
              >
                {t("release_clearSearch")}
              </button>
            {/if}
          </div>
        {:else}
          <div class="px-6 py-4 space-y-2">
            {#each filteredEntries as entry}
              {@const isCurrent = appVersion && entry.version === appVersion}
              {@const isExpanded = expandedVersions.has(entry.version)}
              <div
                class="rounded-lg border transition-colors
                  {isCurrent
                  ? 'border-primary/30 bg-primary/5'
                  : 'border-border/50 hover:border-border'}"
              >
                <!-- Version header (clickable) -->
                <button
                  class="flex w-full items-center gap-2.5 px-4 py-3 text-left"
                  onclick={() => toggleVersion(entry.version)}
                >
                  <!-- Expand/collapse chevron -->
                  <svg
                    class="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-transform {isExpanded
                      ? 'rotate-90'
                      : ''}"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
                  >

                  <span
                    class="inline-flex items-center rounded px-2 py-0.5 text-xs font-mono font-semibold
                      {isCurrent
                      ? 'bg-primary/15 text-primary'
                      : 'bg-foreground/8 text-foreground/70'}"
                  >
                    {entry.version}
                  </span>

                  {#if isCurrent}
                    <span
                      class="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary"
                      >{t("release_current")}</span
                    >
                  {/if}

                  {#if entry.date}
                    <span class="text-[11px] text-muted-foreground">{entry.date}</span>
                  {/if}

                  {#if entry.title}
                    <span class="text-xs text-foreground/60 truncate">{entry.title}</span>
                  {/if}
                </button>

                <!-- Expanded content -->
                {#if isExpanded}
                  <div class="border-t border-border/30 px-4 py-3">
                    <article
                      class="prose prose-sm dark:prose-invert max-w-none changelog-content"
                    >
                      {@html entry.html}
                    </article>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div
        class="flex items-center justify-between border-t border-border px-6 py-3 text-xs text-muted-foreground"
      >
        <span>Apache License 2.0</span>
        <span>Copyright 2025-2026 Claw GO Contributors</span>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(.changelog-content) {
    font-size: 13px;
    line-height: 1.6;
  }

  :global(.changelog-content h3) {
    font-size: 14px;
    font-weight: 600;
    margin-top: 1rem;
    margin-bottom: 0.5rem;
  }

  :global(.changelog-content h4) {
    font-size: 13px;
    font-weight: 600;
    margin-top: 0.75rem;
    margin-bottom: 0.25rem;
  }

  :global(.changelog-content p) {
    margin-bottom: 0.35rem;
  }

  :global(.changelog-content ul),
  :global(.changelog-content ol) {
    padding-left: 1.25rem;
    margin-bottom: 0.5rem;
  }

  :global(.changelog-content li) {
    margin-bottom: 0.15rem;
  }

  :global(.changelog-content code) {
    font-size: 12px;
    padding: 1px 4px;
    border-radius: 3px;
    background: hsl(var(--muted));
  }

  :global(.changelog-content strong) {
    font-weight: 600;
  }

  :global(.changelog-content hr) {
    display: none;
  }
</style>
