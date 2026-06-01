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
    <h3 class="text-[13px] font-medium text-[var(--text-secondary)]">{t('invest_strategy')}</h3>
    {#if !editing}
      <button class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--bg-base)]" onclick={startNew}>
        {t('invest_strategy_add')}
      </button>
    {/if}
  </div>

  {#if editing}
    <div class="space-y-4 rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-4">
      <div>
        <label class="mb-1 block text-[12px] text-[var(--text-secondary)]">Strategy Name</label>
        <input class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-[13px] text-[var(--text-primary)]" bind:value={editName} />
      </div>
      <div class="grid grid-cols-2 gap-4">
        <div>
          <label class="mb-1 block text-[12px] text-[var(--text-secondary)]">{t('invest_strategy_max_single')} (%)</label>
          <input type="number" class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-[13px] text-[var(--text-primary)]" min="0" max="100" bind:value={editMaxSinglePct} />
        </div>
        <div>
          <label class="mb-1 block text-[12px] text-[var(--text-secondary)]">{t('invest_strategy_min_cash')} (%)</label>
          <input type="number" class="w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-[13px] text-[var(--text-primary)]" min="0" max="100" bind:value={editMinCashPct} />
        </div>
      </div>

      <div>
        <div class="mb-2 flex items-center justify-between">
          <label class="text-[12px] text-[var(--text-secondary)]">{t('invest_strategy_targets')}</label>
          <button class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-[var(--space-2)] py-[var(--space-0.5)] text-[11px] text-[var(--text-secondary)]" onclick={addTarget}>{t('invest_strategy_add_target')}</button>
        </div>
        {#if editTargets.length === 0}
          <p class="text-[11px] text-[var(--text-tertiary)]">{t('invest_strategy_no_targets')}</p>
        {:else}
          {#each editTargets as target, idx}
            <div class="mb-2 flex items-center gap-2">
              <input
                class="flex-1 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-2 py-1 text-[13px] text-[var(--text-primary)]"
                placeholder={t('invest_strategy_target_symbol')}
                bind:value={target.symbol}
              />
              <input
                type="number"
                class="w-20 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--bg-input)] px-2 py-1 text-[13px] text-[var(--text-primary)]"
                min="0" max="100"
                placeholder="%"
                bind:value={target.targetPct}
              />
              <span class="text-[11px] text-[var(--text-tertiary)]">%</span>
              <button class="rounded-[var(--radius-md)] px-2 py-1 text-[11px] text-[var(--color-error)] hover:bg-[var(--bg-hover)]" onclick={() => removeTarget(idx)}>&times;</button>
            </div>
          {/each}
        {/if}
      </div>

      <div class="flex gap-2">
        <button class="rounded-[var(--radius-md)] bg-[var(--accent)] px-4 py-1.5 text-[12px] text-[var(--bg-base)]" onclick={save}>
          {t('invest_strategy_save')}
        </button>
        <button class="rounded-[var(--radius-md)] bg-[var(--bg-input)] px-4 py-1.5 text-[12px] text-[var(--text-secondary)]" onclick={() => (editing = false)}>{t('invest_cancel')}</button>
        {#if editingId}
          <button class="ml-auto rounded-[var(--radius-md)] px-4 py-1.5 text-[12px] text-[var(--color-error)] hover:bg-[var(--bg-hover)]" onclick={() => deleteStrategy(editingId!)}>
            {t('invest_strategy_delete')}
          </button>
        {/if}
      </div>
    </div>
  {:else if investStore.strategies.length === 0}
    <p class="py-4 text-center text-[13px] text-[var(--text-tertiary)]">{t('invest_strategy_empty')}</p>
  {:else}
    <div class="space-y-3">
      {#each investStore.strategies as s}
        <div class="rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--bg-card)] p-4">
          <div class="flex items-center justify-between">
            <p class="text-[13px] font-medium text-[var(--text-primary)]">{s.name}</p>
            <button class="rounded-[var(--radius-md)] px-2 py-0.5 text-[11px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => startEdit(s)}>{t('invest_edit')}</button>
          </div>
          <p class="mt-1 text-[11px] text-[var(--text-tertiary)]">
            {t('invest_strategy_max_single')}: {s.maxSinglePct ?? '-'}% |
            {t('invest_strategy_min_cash')}: {s.minCashPct ?? '-'}%
          </p>
          {#if s.targets && s.targets.length > 0}
            <div class="mt-2 flex flex-wrap gap-2">
              {#each s.targets as target}
                <span class="rounded-[var(--radius-full)] bg-[var(--accent-muted)] px-3 py-1 text-[11px] font-bold text-[var(--accent)]">
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
