<script lang="ts">
  import { claudeUsageStore } from "$lib/stores/claude-usage-store.svelte";
  import { t, currentLocale } from "$lib/i18n/index.svelte";
  import { onMount } from "svelte";

  let open = $state(false);
  let wrapperEl: HTMLDivElement | undefined = $state();

  const data = $derived(claudeUsageStore.data);
  const pct = (u: number | undefined | null) =>
    u == null ? "—" : `${Math.round(u * 100)}%`;
  const fiveHour = $derived(data?.five_hour?.utilization ?? null);
  const weekly = $derived(data?.seven_day?.utilization ?? null);

  // Color tiers, aligned with the context bar's semantic palette: <0.7 ok, <0.9 warn, else crit.
  const toneText = (u: number | null) =>
    u == null
      ? "text-muted-foreground"
      : u < 0.7
        ? "text-emerald-500"
        : u < 0.9
          ? "text-amber-500"
          : "text-red-500";
  const toneBar = (u: number | null) =>
    u == null
      ? "bg-muted-foreground/30"
      : u < 0.7
        ? "bg-emerald-500"
        : u < 0.9
          ? "bg-amber-500"
          : "bg-red-500";

  // Mini-ring geometry (viewBox 36, r15).
  const CIRC = 2 * Math.PI * 15;
  const dashOffset = (u: number | null) => (u == null ? CIRC : CIRC * (1 - Math.min(1, u)));

  /** Format an ISO-8601 reset timestamp from the API into a friendly local string. */
  function formatReset(iso: string | null): string | null {
    if (!iso) return null;
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso; // unparseable → show raw rather than nothing
    const loc = currentLocale();
    const time = d.toLocaleTimeString(loc, { hour: "2-digit", minute: "2-digit", hour12: false });
    const now = new Date();
    const startOf = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
    const dayDiff = Math.round((startOf(d) - startOf(now)) / 86_400_000);
    if (dayDiff === 0) return `${t("claudeUsage_today")} ${time}`;
    if (dayDiff === 1) return `${t("claudeUsage_tomorrow")} ${time}`;
    if (dayDiff > 1 && dayDiff < 7) {
      return `${d.toLocaleDateString(loc, { weekday: "short" })} ${time}`;
    }
    return `${d.toLocaleDateString(loc, { month: "short", day: "numeric" })} ${time}`;
  }

  onMount(() => {
    function onDocClick(e: MouseEvent) {
      if (open && wrapperEl && !wrapperEl.contains(e.target as Node)) {
        open = false;
      }
    }
    function onDocKeydown(e: KeyboardEvent) {
      if (open && e.key === "Escape") {
        open = false;
      }
    }
    document.addEventListener("mousedown", onDocClick, true);
    document.addEventListener("keydown", onDocKeydown);
    return () => {
      document.removeEventListener("mousedown", onDocClick, true);
      document.removeEventListener("keydown", onDocKeydown);
    };
  });
</script>

<!-- Mini ring: fill inherits the segment's tone via currentColor. -->
{#snippet ring(u)}
  <svg class="block -rotate-90" width="14" height="14" viewBox="0 0 36 36" aria-hidden="true">
    <circle cx="18" cy="18" r="15" fill="none" stroke-width="5" style="stroke: hsl(var(--foreground) / 0.12)"></circle>
    <circle
      cx="18"
      cy="18"
      r="15"
      fill="none"
      stroke-width="5"
      stroke-linecap="round"
      stroke="currentColor"
      stroke-dasharray={CIRC}
      stroke-dashoffset={dashOffset(u)}
      style="transition: stroke-dashoffset 0.6s cubic-bezier(0.3, 0.8, 0.3, 1)"
    ></circle>
  </svg>
{/snippet}

{#snippet boltIcon()}
  <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 4 14h7l-1 8 9-12h-7z" /></svg>
{/snippet}
{#snippet calIcon()}
  <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M3 9h18M8 3v4M16 3v4" /></svg>
{/snippet}

{#snippet windowRow(icon, label, w)}
  {#if w}
    {@const reset = formatReset(w.resets_at)}
    <div class="py-2">
      <div class="flex items-baseline justify-between mb-1.5">
        <span class="flex items-center gap-1.5 font-medium text-foreground/85">
          <span class="text-muted-foreground">{@render icon()}</span>{label}
        </span>
        <span class="text-[13px] font-bold tabular-nums {toneText(w.utilization)}">{pct(w.utilization)}</span>
      </div>
      <div class="h-1.5 rounded-full bg-foreground/10 overflow-hidden">
        <div
          class="h-full rounded-full transition-all duration-700 ease-out {toneBar(w.utilization)}"
          style="width:{Math.min(100, Math.round(w.utilization * 100))}%"
        ></div>
      </div>
      {#if reset}
        <div class="mt-1.5 flex items-center gap-1.5 text-[10.5px] text-muted-foreground">
          <svg class="h-3 w-3 opacity-70" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" /></svg>
          {t("claudeUsage_resetsAt", { time: reset })}
        </div>
      {/if}
    </div>
  {/if}
{/snippet}

{#if data && !data.error}
  <div bind:this={wrapperEl} class="relative inline-flex items-center shrink-0">
    <button
      class="flex items-center gap-2 px-1.5 py-0.5 -my-0.5 rounded hover:bg-accent transition-colors"
      onclick={() => (open = !open)}
      title={t("claudeUsage_title")}
      aria-expanded={open}
      aria-haspopup="true"
    >
      <span class="flex items-center gap-1.5 {toneText(fiveHour)}">
        {@render ring(fiveHour)}
        <span class="text-muted-foreground">{t("claudeUsage_5h")}</span>
        <span class="font-semibold tabular-nums">{pct(fiveHour)}</span>
      </span>
      <span class="w-px h-3 bg-border"></span>
      <span class="flex items-center gap-1.5 {toneText(weekly)}">
        {@render ring(weekly)}
        <span class="text-muted-foreground">{t("claudeUsage_weekly")}</span>
        <span class="font-semibold tabular-nums">{pct(weekly)}</span>
      </span>
      {#if claudeUsageStore.stale}
        <span class="text-[10px] italic text-muted-foreground/70">{t("claudeUsage_stale")}</span>
      {/if}
    </button>

    {#if open}
      <div
        class="absolute z-50 top-full mt-1.5 right-0 w-72 p-3.5 rounded-lg bg-popover border border-border shadow-2xl font-sans text-xs"
      >
        <div class="flex items-center justify-between mb-3">
          <span class="flex items-center gap-2 text-[12.5px] font-semibold text-foreground">
            <svg class="h-3.5 w-3.5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 13l4-3" /><path d="M3.5 18a9 9 0 1 1 17 0" /></svg>
            {t("claudeUsage_title")}
          </span>
          {#if data.subscription_type}
            <span class="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-primary/15 border border-primary/25 text-primary">
              {data.subscription_type}
            </span>
          {/if}
        </div>

        <div class="divide-y divide-border/60">
          {@render windowRow(boltIcon, t("claudeUsage_5h"), data.five_hour)}
          {@render windowRow(calIcon, t("claudeUsage_weekly"), data.seven_day)}
          {@render windowRow(calIcon, t("claudeUsage_opus"), data.seven_day_opus)}
        </div>

        <div class="mt-3 pt-2.5 border-t border-border flex items-center justify-between text-[10.5px] text-muted-foreground">
          <span>{t("claudeUsage_plan")} <b class="font-semibold text-foreground/70">{data.subscription_type ?? "—"}</b></span>
          <span>{t("claudeUsage_tier")} <b class="font-semibold text-foreground/70">{data.rate_limit_tier ?? "—"}</b></span>
        </div>
      </div>
    {/if}
  </div>
{/if}
