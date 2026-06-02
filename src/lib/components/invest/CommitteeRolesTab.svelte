<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';
  import ProviderConfigPanel from '$lib/components/invest/ProviderConfigPanel.svelte';

  const store = investCommitteeStore;

  let saving = $state(false);
  let saveMsg = $state('');
  let editingKey = $state<string | null>(null);
  let editText = $state('');

  onMount(() => {
    store.loadRolePrompts();
  });

  function startEdit(key: string) {
    editingKey = key;
    editText = store.rolePrompts[key] ?? '';
  }

  function cancelEdit() {
    editingKey = null;
    editText = '';
  }

  async function savePrompt(key: string) {
    saving = true;
    saveMsg = '';
    try {
      await store.saveRolePrompt(key, editText);
      saveMsg = t('invest_roles_saved');
      editingKey = null;
    } catch (e) {
      saveMsg = t('invest_roles_save_failed') + String(e);
    } finally {
      saving = false;
      setTimeout(() => (saveMsg = ''), 2000);
    }
  }
</script>

<div class="space-y-6">
  <!-- Two-panel layout -->
  <div class="grid gap-5 lg:grid-cols-[45%_1fr]">
    <!-- Left panel -->
    <div class="space-y-5">
      <!-- Verdict options -->
      <div class="rounded-[var(--radius-lg)] border border-border p-4 bg-[var(--bg-card)]">
        <h3 class="mb-3 text-sm font-semibold flex items-center gap-2 text-[var(--text-primary)]">
          <span class="h-4 w-0.5 rounded-full bg-[var(--accent)]"></span>
          {t('invest_roles_verdict_options')}
        </h3>
        <ul class="space-y-2">
          {#each [
            { key: 'BUY', desc: t('invest_roles_verdict_buy_desc') },
            { key: 'ACCUMULATE', desc: t('invest_roles_verdict_accumulate_desc') },
            { key: 'HOLD', desc: t('invest_roles_verdict_hold_desc') },
            { key: 'TRIM', desc: t('invest_roles_verdict_trim_desc') },
            { key: 'SELL', desc: t('invest_roles_verdict_sell_desc') },
          ] as v}
            <li class="flex items-start gap-2 text-xs">
              <span class="min-w-[90px] font-[var(--font-mono)] font-bold text-[var(--accent)]">{v.key}</span>
              <span class="text-[var(--text-secondary)]">{v.desc}</span>
            </li>
          {/each}
        </ul>
      </div>

      <!-- REGIME hard rules -->
      <div class="rounded-[var(--radius-lg)] border border-border p-4 bg-[var(--bg-card)]">
        <h3 class="mb-1 text-sm font-semibold flex items-center gap-2 text-[var(--text-primary)]">
          <span class="h-4 w-0.5 rounded-full bg-[var(--accent)]"></span>
          {t('invest_roles_regime_rules')}
        </h3>
        <p class="mb-3 text-xs text-[var(--text-secondary)] leading-relaxed">
          {t('invest_roles_regime_desc')}
        </p>
        <div class="grid gap-4 sm:grid-cols-[55%_1fr]">
          <!-- Thresholds -->
          <table class="w-full text-xs font-[var(--font-mono)]">
            <tbody>
              {#each [
                ['trend_ma_spread_pct', '= 3'],
                ['crash_atr_pct_min', '= 5'],
                ['crash_drawdown_30d_pct', '= 20'],
                ['crash_deep_drawdown_30d_pct', '= 30'],
                ['recovery_rebound_pct', '= 10'],
                ['recovery_quantile_max', '= 0.5'],
                ['low_quantile_threshold', '= 0.2'],
                ['high_quantile_threshold', '= 0.8'],
              ] as [name, val]}
                <tr class="border-b border-border">
                  <td class="py-1 pr-2 text-[#3b82f6]">{name}</td>
                  <td class="py-1 text-right text-[var(--color-warning)]">{val}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          <!-- Priorities -->
          <div>
            <div class="mb-1.5 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
              {t('invest_roles_priority_type')}
            </div>
            <ol class="space-y-1.5 text-xs">
              {#each [
                { n: '1', name: 'crash', detail: t('invest_roles_regime_crash') },
                { n: '2', name: 'uptrend', detail: t('invest_roles_regime_uptrend') },
                { n: '3', name: 'downtrend', detail: t('invest_roles_regime_downtrend') },
                { n: '4', name: 'range_bound', detail: t('invest_roles_regime_range') },
                { n: '5', name: 'unknown', detail: t('invest_roles_regime_unknown') },
              ] as p}
                <li class="flex items-start gap-1.5">
                  <span class="min-w-[14px] font-bold text-[var(--accent)]">{p.n}.</span>
                  <span>
                    <span class="font-semibold text-[var(--text-primary)]">{p.name}</span>
                    <span class="text-[var(--text-secondary)]"> &mdash; {p.detail}</span>
                  </span>
                </li>
              {/each}
            </ol>
          </div>
        </div>
      </div>
    </div>

    <!-- Right panel: CIO Sanity Check -->
    <div class="rounded-[var(--radius-lg)] border border-border p-4 bg-[var(--bg-card)]">
      <h3 class="mb-3 text-sm font-semibold flex items-center gap-2 text-[var(--text-primary)]">
        <span class="h-4 w-0.5 rounded-full bg-[var(--accent)]"></span>
        {t('invest_roles_sanity_check')}
      </h3>
      <ul class="space-y-2.5">
        {#each [
          t('invest_roles_sanity_1'),
          t('invest_roles_sanity_2'),
          t('invest_roles_sanity_3'),
          t('invest_roles_sanity_4'),
          t('invest_roles_sanity_5'),
        ] as rule}
          <li class="relative pl-3.5 text-xs leading-relaxed text-[var(--text-secondary)] before:absolute before:left-0 before:top-[5px] before:h-1 before:w-1 before:rounded-full before:bg-[var(--color-warning)]">
            {@html rule}
          </li>
        {/each}
      </ul>
    </div>
  </div>

  <!-- Save message -->
  {#if saveMsg}
    <div class="text-xs text-[var(--text-secondary)]">{saveMsg}</div>
  {/if}

  <!-- Role cards -->
  <div class="text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
    {t('invest_roles_section_label')}
  </div>

  {#each [
    {
      key: 'macro',
      color: '#8b5cf6',
      badge: 'MACRO',
      nameCn: t('invest_roles_macro_cn'),
      nameEn: 'Macro Strategist',
      desc: t('invest_roles_macro_desc'),
      meta: 'temp 0.7 · tools ✓',
      prompts: [{ key: 'macro', label: t('invest_roles_prompt_full') }],
    },
    {
      key: 'quant',
      color: '#3b82f6',
      badge: 'QUANT',
      nameCn: t('invest_roles_quant_cn'),
      nameEn: 'Quant Analyst',
      desc: t('invest_roles_quant_desc'),
      meta: 'temp 0.7 · tools ✗',
      prompts: [
        { key: 'quant', label: t('invest_roles_prompt_r1') },
        { key: 'quant_r2', label: t('invest_roles_prompt_r2') },
      ],
    },
    {
      key: 'risk',
      color: 'var(--color-warning)',
      badge: 'RISK',
      nameCn: t('invest_roles_risk_cn'),
      nameEn: 'Risk Officer',
      desc: t('invest_roles_risk_desc'),
      meta: 'temp 0.7 · tools ✗',
      prompts: [
        { key: 'risk', label: t('invest_roles_prompt_r1') },
        { key: 'risk_r2', label: t('invest_roles_prompt_r2') },
      ],
    },
    {
      key: 'l4_officer',
      color: 'var(--color-error)',
      badge: 'L4',
      nameCn: t('invest_roles_l4_officer_cn'),
      nameEn: 'L4 Execution Officer',
      desc: t('invest_roles_l4_officer_desc'),
      meta: 'temp 0.3 · tools ✗',
      prompts: [{ key: 'l4_officer', label: t('invest_roles_prompt_full') }],
    },
    {
      key: 'cio',
      color: 'var(--color-success)',
      badge: 'CIO',
      nameCn: t('invest_roles_cio_cn'),
      nameEn: 'Chief Investment Officer',
      desc: t('invest_roles_cio_desc'),
      meta: 'temp 0.1 · tools ✗',
      prompts: [{ key: 'cio', label: t('invest_roles_prompt_full') }],
    },
  ] as role}
    <div class="overflow-hidden rounded-[var(--radius-lg)] border border-border border-l-[3px]"
      style="border-left-color: {role.color}">
      <!-- Role header -->
      <div class="flex items-center gap-3 p-4">
        <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-[var(--radius-md)] text-[10px] font-extrabold tracking-wider text-white"
          style="background: {role.color}">
          {role.badge}
        </div>
        <div class="min-w-0 flex-1">
          <div class="flex items-baseline gap-2">
            <span class="text-sm font-bold text-[var(--text-primary)]">{role.nameCn}</span>
            <span class="text-xs text-[var(--text-secondary)]">{role.nameEn}</span>
          </div>
          <div class="mt-0.5 text-xs text-[var(--text-secondary)]">{role.desc}</div>
        </div>
        <div class="shrink-0 text-xs text-[var(--text-tertiary)]">{role.meta}</div>
      </div>

      <!-- Role body -->
      <div class="px-4 pb-4">
        <!-- Hard rules -->
        <div class="mb-3 text-[11px] font-medium uppercase tracking-wider text-[var(--text-tertiary)]">
          {t('invest_roles_hard_rules')}
        </div>
        <ul class="mb-3 space-y-1">
          {#each (
            role.key === 'macro' ? [
              t('invest_roles_hard_macro_1'),
              t('invest_roles_hard_macro_2'),
            ] : role.key === 'quant' ? [
              t('invest_roles_hard_quant_1'),
              t('invest_roles_hard_quant_2'),
              t('invest_roles_hard_quant_3'),
              t('invest_roles_hard_quant_4'),
              t('invest_roles_hard_quant_5'),
            ] : role.key === 'risk' ? [
              t('invest_roles_hard_risk_1'),
              t('invest_roles_hard_risk_2'),
              t('invest_roles_hard_risk_3'),
            ] : role.key === 'l4_officer' ? [
              t('invest_roles_hard_l4_1'),
              t('invest_roles_hard_l4_2'),
              t('invest_roles_hard_l4_3'),
            ] : [
              t('invest_roles_hard_cio_1'),
              t('invest_roles_hard_cio_2'),
              t('invest_roles_hard_cio_3'),
              t('invest_roles_hard_cio_4'),
            ]
          ) as rule}
            <li class="relative pl-3 text-xs leading-relaxed text-[var(--text-secondary)] before:absolute before:left-0 before:top-[5px] before:h-1 before:w-1 before:rounded-full before:bg-[var(--text-tertiary)]">
              {@html rule}
            </li>
          {/each}
        </ul>

        <!-- Prompt sections -->
        {#each role.prompts as prompt}
          {@const currentText = store.rolePrompts[prompt.key] ?? ''}
          {@const wordCount = currentText.length}
          <details class="mb-1.5">
            <summary class="cursor-pointer select-none py-1.5 text-xs text-[var(--text-secondary)] hover:text-[var(--text-primary)] flex items-center gap-1.5">
              <span class="text-[9px] text-[var(--text-tertiary)]">&#9654;</span>
              {prompt.label}
              <span class="text-[var(--text-tertiary)]">({wordCount} {t('invest_roles_chars')})</span>
            </summary>
            <div class="mt-1">
              {#if editingKey === prompt.key}
                <textarea
                  class="h-48 w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] p-2.5 font-[var(--font-mono)] text-xs leading-relaxed text-[var(--text-secondary)] focus:border-[#3b82f6] focus:outline-none focus:ring-1 focus:ring-[#3b82f6]/30"
                  bind:value={editText}
                ></textarea>
                <div class="mt-2 flex items-center gap-2">
                  <button
                    class="rounded-[var(--radius-md)] bg-[var(--accent)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-white disabled:opacity-50"
                    disabled={saving}
                    onclick={() => savePrompt(prompt.key)}
                  >{saving ? t('invest_roles_saving') : t('invest_roles_save')}</button>
                  <button
                    class="rounded-[var(--radius-md)] px-[var(--space-3)] py-[var(--space-1)] text-[12px] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]"
                    onclick={cancelEdit}
                  >{t('invest_roles_cancel')}</button>
                </div>
              {:else}
                <div
                  class="max-h-48 overflow-y-auto rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] p-2.5 font-[var(--font-mono)] text-xs leading-relaxed text-[var(--text-secondary)] cursor-pointer hover:border-[var(--text-tertiary)]"
                  onclick={() => startEdit(prompt.key)}
                  role="button"
                  tabindex="0"
                  onkeydown={(e) => { if (e.key === 'Enter') startEdit(prompt.key); }}
                >
                  {currentText || t('invest_roles_click_to_edit')}
                </div>
              {/if}
            </div>
          </details>
        {/each}
      </div>
    </div>
  {/each}

  <!-- Provider Configuration -->
  <div class="rounded-[var(--radius-lg)] border border-border p-4 bg-[var(--bg-card)]">
    <h3 class="mb-3 text-sm font-semibold flex items-center gap-2 text-[var(--text-primary)]">
      <span class="h-4 w-0.5 rounded-full bg-[var(--accent)]"></span>
      {t('invest_committee_config')}
    </h3>
    <ProviderConfigPanel />
  </div>
</div>
