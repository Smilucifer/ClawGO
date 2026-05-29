<script lang="ts">
  import { onMount } from 'svelte';
  import { investCommitteeStore } from '$lib/stores/invest-committee-store.svelte';

  const ROLES = [
    { key: 'macro', label: '宏观分析师' },
    { key: 'quant_r1', label: '量化分析师 R1' },
    { key: 'risk_r1', label: '风控官 R1' },
    { key: 'wealth', label: '财富配置官' },
    { key: 'quant_r2', label: '量化分析师 R2' },
    { key: 'risk_r2', label: '风控官 R2' },
    { key: 'cio', label: 'CIO' },
  ];

  let selectedRole = $state('macro');
  let promptText = $state('');
  let saving = $state(false);
  let saveMsg = $state('');

  const store = investCommitteeStore;

  $effect(() => {
    promptText = store.rolePrompts[selectedRole] ?? '';
  });

  onMount(() => {
    store.loadRolePrompts();
  });

  async function save() {
    saving = true;
    saveMsg = '';
    try {
      await store.saveRolePrompt(selectedRole, promptText);
      saveMsg = '已保存';
    } catch (e) {
      saveMsg = '保存失败: ' + String(e);
    } finally {
      saving = false;
      setTimeout(() => (saveMsg = ''), 2000);
    }
  }
</script>

<div class="space-y-4">
  <!-- Role selector tabs -->
  <div class="flex flex-wrap gap-1 border-b border-border pb-1">
    {#each ROLES as role}
      <button
        class="rounded-t px-3 py-1.5 text-sm transition-colors"
        class:bg-primary={selectedRole === role.key}
        class:text-primary-foreground={selectedRole === role.key}
        class:text-muted-foreground={selectedRole !== role.key}
        onclick={() => (selectedRole = role.key)}
      >
        {role.label}
      </button>
    {/each}
  </div>

  <!-- Prompt editor -->
  <div>
    <label class="mb-1 block text-sm text-muted-foreground">
      系统提示词 — {ROLES.find((r) => r.key === selectedRole)?.label}
    </label>
    <textarea
      class="h-64 w-full rounded border border-border bg-background p-3 font-mono text-sm"
      bind:value={promptText}
      placeholder="加载中..."
    ></textarea>
  </div>

  <!-- Save button -->
  <div class="flex items-center gap-3">
    <button
      class="rounded bg-primary px-4 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
      disabled={saving}
      onclick={save}
    >
      {saving ? '保存中...' : '保存'}
    </button>
    {#if saveMsg}
      <span class="text-sm text-muted-foreground">{saveMsg}</span>
    {/if}
  </div>
</div>
