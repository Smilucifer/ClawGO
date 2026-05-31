<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import { investStore } from '$lib/stores/invest-store.svelte';
  import type { StrategyTarget } from '$lib/types';

  let { tushareToken }: { tushareToken: string } = $props();

  let editing = $state(false);
  let editName = $state('');
  let editMaxSinglePct = $state<number | null>(null);
  let editMinCashPct = $state<number | null>(null);
  let editTargets = $state<StrategyTarget[]>([]);
  let editingId = $state<string | null>(null);

  function startNew() {
    editing = true;
    editingId = null;
    editName = 'New Strategy';
    editMaxSinglePct = 30;
    editMinCashPct = 10;
    editTargets = [];
  }

  function startEdit(s: (typeof investStore.strategies)[number]) {
    editing = true;
    editingId = s.id;
    editName = s.name;
    editMaxSinglePct = s.maxSinglePct ?? null;
    editMinCashPct = s.minCashPct ?? null;
    editTargets = (s.targets ?? []).map((t) => ({ ...t }));
  }

  function addTarget() {
    editTargets = [...editTargets, { symbol: '', targetPct: 0, name: '' }];
  }

  function removeTarget(idx: number) {
    editTargets = editTargets.filter((_, i) => i !== idx);
  }

  async function save() {
    await investStore.saveStrategy(
      editingId,
      editName,
      editTargets,
      editMaxSinglePct,
      editMinCashPct,
    );
    editing = false;
  }

  async function deleteStrategy(id: string) {
    await investStore.deleteStrategy(id);
    editing = false;
  }
</script>

<div>
  <div class="mb-4 flex items-center justify-between">
    <h3 class="text-sm font-medium text-muted-foreground">{t('invest_strategy')}</h3>
    {#if !editing}
      <button class="rounded bg-primary px-3 py-1 text-sm text-primary-foreground" onclick={startNew}>
        {t('invest_strategy_add')}
      </button>
    {/if}
  </div>

  {#if editing}
    <div class="space-y-4 rounded-lg border p-4">
      <div>
        <label class="mb-1 block text-sm">Strategy Name</label>
        <input class="w-full rounded border bg-background px-3 py-1.5 text-sm" bind:value={editName} />
      </div>
      <div class="grid grid-cols-2 gap-4">
        <div>
          <label class="mb-1 block text-sm">{t('invest_strategy_max_single')} (%)</label>
          <input type="number" class="w-full rounded border bg-background px-3 py-1.5 text-sm" min="0" max="100" bind:value={editMaxSinglePct} />
        </div>
        <div>
          <label class="mb-1 block text-sm">{t('invest_strategy_min_cash')} (%)</label>
          <input type="number" class="w-full rounded border bg-background px-3 py-1.5 text-sm" min="0" max="100" bind:value={editMinCashPct} />
        </div>
      </div>

      <div>
        <div class="mb-2 flex items-center justify-between">
          <label class="text-sm">{t('invest_strategy_targets')}</label>
          <button class="rounded bg-muted px-2 py-0.5 text-xs" onclick={addTarget}>{t('invest_strategy_add_target')}</button>
        </div>
        {#if editTargets.length === 0}
          <p class="text-xs text-muted-foreground">{t('invest_strategy_no_targets')}</p>
        {:else}
          {#each editTargets as target, idx}
            <div class="mb-2 flex items-center gap-2">
              <input
                class="flex-1 rounded border bg-background px-2 py-1 text-sm"
                placeholder={t('invest_strategy_target_symbol')}
                bind:value={target.symbol}
              />
              <input
                type="number"
                class="w-20 rounded border bg-background px-2 py-1 text-sm"
                min="0" max="100"
                placeholder="%"
                bind:value={target.targetPct}
              />
              <span class="text-xs text-muted-foreground">%</span>
              <button class="rounded px-2 py-1 text-xs text-destructive hover:bg-muted" onclick={() => removeTarget(idx)}>&times;</button>
            </div>
          {/each}
        {/if}
      </div>

      <div class="flex gap-2">
        <button class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground" onclick={save}>
          {t('invest_strategy_save')}
        </button>
        <button class="rounded bg-muted px-4 py-1.5 text-sm" onclick={() => (editing = false)}>{t('invest_cancel')}</button>
        {#if editingId}
          <button class="ml-auto rounded px-4 py-1.5 text-sm text-destructive hover:bg-muted" onclick={() => deleteStrategy(editingId!)}>
            {t('invest_strategy_delete')}
          </button>
        {/if}
      </div>
    </div>
  {:else if investStore.strategies.length === 0}
    <p class="py-4 text-center text-sm text-muted-foreground">{t('invest_strategy_empty')}</p>
  {:else}
    <div class="space-y-3">
      {#each investStore.strategies as s}
        <div class="rounded-lg border p-4">
          <div class="flex items-center justify-between">
            <p class="font-medium">{s.name}</p>
            <button class="rounded px-2 py-0.5 text-xs hover:bg-muted" onclick={() => startEdit(s)}>{t('invest_edit')}</button>
          </div>
          <p class="mt-1 text-xs text-muted-foreground">
            {t('invest_strategy_max_single')}: {s.maxSinglePct ?? '-'}% |
            {t('invest_strategy_min_cash')}: {s.minCashPct ?? '-'}%
          </p>
          {#if s.targets && s.targets.length > 0}
            <div class="mt-2 flex flex-wrap gap-2">
              {#each s.targets as target}
                <span class="rounded bg-muted px-2 py-0.5 text-xs">
                  {target.symbol} {target.targetPct}%
                </span>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
