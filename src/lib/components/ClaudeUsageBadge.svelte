<script lang="ts">
  import { claudeUsageStore } from "$lib/stores/claude-usage-store.svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { onMount } from "svelte";

  let open = $state(false);
  let wrapperEl: HTMLDivElement | undefined = $state();

  const data = $derived(claudeUsageStore.data);
  const pct = (u: number | undefined | null) =>
    u == null ? "—" : `${Math.round(u * 100)}%`;
  const fiveHour = $derived(data?.five_hour?.utilization ?? null);
  const weekly = $derived(data?.seven_day?.utilization ?? null);

  // Color tiers: <0.7 green, <0.9 amber, else red
  const tone = (u: number | null) =>
    u == null
      ? "text-muted-foreground"
      : u < 0.7
        ? "text-green-500"
        : u < 0.9
          ? "text-amber-500"
          : "text-red-500";

  // Bar fill color matching tone
  const barColor = (u: number | null) =>
    u == null
      ? "bg-muted-foreground/30"
      : u < 0.7
        ? "bg-green-500"
        : u < 0.9
          ? "bg-amber-500"
          : "bg-red-500";

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

{#if data && !data.error}
  <div bind:this={wrapperEl} class="relative inline-flex items-center">
    <button
      class="flex items-center gap-1.5 px-2 py-0.5 rounded text-xs hover:bg-accent transition-colors"
      onclick={() => (open = !open)}
      title="Claude usage"
    >
      <span class={tone(fiveHour)}>{t("claudeUsage_5h")} {pct(fiveHour)}</span>
      <span class="text-muted-foreground/40">·</span>
      <span class={tone(weekly)}>{t("claudeUsage_weekly")} {pct(weekly)}</span>
    </button>

    {#if open}
      <div
        class="absolute z-50 top-full mt-1 right-0 w-64 p-3 rounded-lg bg-background border border-border shadow-xl text-xs space-y-2"
      >
        <!-- 5h window -->
        {#if data.five_hour}
          <div>
            <div class="flex justify-between mb-0.5">
              <span class="text-foreground/80">{t("claudeUsage_5h")}</span>
              <span class={tone(data.five_hour.utilization)}>{pct(data.five_hour.utilization)}</span>
            </div>
            <div class="h-1.5 rounded bg-secondary overflow-hidden">
              <div
                class="h-full {barColor(data.five_hour.utilization)}"
                style="width:{Math.min(100, Math.round(data.five_hour.utilization * 100))}%"
              ></div>
            </div>
            {#if data.five_hour.resets_at}
              <div class="text-[10px] text-muted-foreground mt-0.5">
                {t("claudeUsage_resetsAt", { time: data.five_hour.resets_at })}
              </div>
            {/if}
          </div>
        {/if}

        <!-- 7-day window -->
        {#if data.seven_day}
          <div>
            <div class="flex justify-between mb-0.5">
              <span class="text-foreground/80">{t("claudeUsage_weekly")}</span>
              <span class={tone(data.seven_day.utilization)}>{pct(data.seven_day.utilization)}</span>
            </div>
            <div class="h-1.5 rounded bg-secondary overflow-hidden">
              <div
                class="h-full {barColor(data.seven_day.utilization)}"
                style="width:{Math.min(100, Math.round(data.seven_day.utilization * 100))}%"
              ></div>
            </div>
            {#if data.seven_day.resets_at}
              <div class="text-[10px] text-muted-foreground mt-0.5">
                {t("claudeUsage_resetsAt", { time: data.seven_day.resets_at })}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Opus 7-day window -->
        {#if data.seven_day_opus}
          <div>
            <div class="flex justify-between mb-0.5">
              <span class="text-foreground/80">{t("claudeUsage_opus")}</span>
              <span class={tone(data.seven_day_opus.utilization)}>{pct(data.seven_day_opus.utilization)}</span>
            </div>
            <div class="h-1.5 rounded bg-secondary overflow-hidden">
              <div
                class="h-full {barColor(data.seven_day_opus.utilization)}"
                style="width:{Math.min(100, Math.round(data.seven_day_opus.utilization * 100))}%"
              ></div>
            </div>
            {#if data.seven_day_opus.resets_at}
              <div class="text-[10px] text-muted-foreground mt-0.5">
                {t("claudeUsage_resetsAt", { time: data.seven_day_opus.resets_at })}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Plan + Tier footer -->
        <div class="pt-1 border-t border-border text-[10px] text-muted-foreground flex justify-between">
          <span>{t("claudeUsage_plan")}: {data.subscription_type ?? "—"}</span>
          <span>{t("claudeUsage_tier")}: {data.rate_limit_tier ?? "—"}</span>
        </div>
      </div>
    {/if}
  </div>
{/if}
