<script lang="ts">
  import type { RoomTurn, RoomTurnSnapshot } from "$lib/types";
  import { getRoomTurnSnapshot } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import { roomTurnModeKey } from "$lib/utils/room-ui";

  let {
    roomId,
    turns,
    activeSnapshot = $bindable(null),
  }: {
    roomId: string;
    turns: RoomTurn[];
    activeSnapshot: RoomTurnSnapshot | null;
  } = $props();

  let loading = $state(false);

  function turnStatus(turn: RoomTurn): "complete" | "running" | "failed" | "pending" {
    if (turn.responses.some((r) => r.status === "failed")) return "failed";
    if (turn.responses.some((r) => r.status === "running")) return "running";
    if (turn.completed_at) return "complete";
    return "pending";
  }

  function statusColor(status: string): string {
    switch (status) {
      case "complete":
        return "bg-green-500";
      case "running":
        return "bg-amber-500";
      case "failed":
        return "bg-red-500";
      default:
        return "bg-gray-400";
    }
  }

  async function handleClick(turn: RoomTurn) {
    if (activeSnapshot?.turn.id === turn.id) {
      activeSnapshot = null;
      return;
    }
    loading = true;
    try {
      activeSnapshot = await getRoomTurnSnapshot(roomId, turn.id);
    } catch (e) {
      console.error("Failed to load snapshot:", e);
    } finally {
      loading = false;
    }
  }

  function exitSnapshot() {
    activeSnapshot = null;
  }
</script>

<div class="flex flex-col gap-1 max-h-64 overflow-y-auto px-3 py-2">
  {#if turns.length === 0}
    <p class="text-sm text-muted-foreground">{t("room_noTurns")}</p>
  {:else}
    {#each turns as turn (turn.id)}
      {@const status = turnStatus(turn)}
      {@const isActive = activeSnapshot?.turn.id === turn.id}
      <button
        class="flex items-start gap-2 text-left rounded px-2 py-1.5 hover:bg-accent/50 transition-colors {isActive
          ? 'bg-accent'
          : ''}"
        onclick={() => handleClick(turn)}
        disabled={loading}
      >
        <span class="mt-1.5 h-2.5 w-2.5 rounded-full shrink-0 {statusColor(status)}"></span>
        <span class="flex flex-col gap-0.5 min-w-0">
          <span class="text-xs font-medium">
            Turn {turn.idx} · {t(roomTurnModeKey(turn.mode))}
          </span>
          <span class="text-xs text-muted-foreground truncate">
            {turn.user_input.slice(0, 60)}{turn.user_input.length > 60 ? "…" : ""}
          </span>
        </span>
      </button>
    {/each}
  {/if}
</div>

{#if activeSnapshot}
  <div
    class="shrink-0 border-t border-purple-300 bg-purple-50 dark:bg-purple-950/30 px-3 py-2 flex items-center justify-between"
  >
    <span class="text-sm font-medium text-purple-700 dark:text-purple-300">
      {t("room_snapshotBanner", { turn: String(activeSnapshot.turn.idx) })}
    </span>
    <button
      class="text-xs text-purple-600 dark:text-purple-400 hover:underline"
      onclick={exitSnapshot}
    >
      {t("room_snapshotExit")}
    </button>
  </div>
{/if}
