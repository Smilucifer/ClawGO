<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import { getTransport } from '$lib/transport';

  let profileLoading = $state(false);
  let profileSaved = $state(false);
  let profileError = $state('');
  let profileForm = $state({
    familyBackupAvailable: false,
    accountPurpose: 'default',
    lifestyleNotes: '',
    displayName: undefined as string | undefined,
    riskTolerance: undefined as string | undefined,
    exchangeBufferCny: undefined as number | undefined,
    familySupport: undefined as string | undefined,
  });

  const accountPurposeOptions = $derived([
    { value: 'default', label: t('settings_profile_purpose_default') },
    { value: 'pocket_money', label: t('settings_profile_purpose_pocket_money') },
    { value: 'long_term', label: t('settings_profile_purpose_long_term') },
    { value: 'retirement', label: t('settings_profile_purpose_retirement') },
    { value: 'education', label: t('settings_profile_purpose_education') },
    { value: 'other', label: t('settings_profile_purpose_other') },
  ]);

  const familySupportOptions = $derived([
    { value: 'none', label: t('settings_profile_family_support_none') },
    { value: 'occasional', label: t('settings_profile_family_support_occasional') },
    { value: 'partial', label: t('settings_profile_family_support_partial') },
    { value: 'full', label: t('settings_profile_family_support_full') },
  ]);

  onMount(() => {
    loadProfile();
  });

  async function loadProfile() {
    profileLoading = true;
    try {
      const p = await getTransport().invoke<{
        familyBackupAvailable: boolean;
        accountPurpose: string;
        lifestyleNotes: string;
        displayName?: string;
        riskTolerance?: string;
        exchangeBufferCny?: number;
        familySupport?: string;
      }>('get_user_profile');
      // Normalize legacy account purpose values to current set
      const PURPOSE_MAP: Record<string, string> = {
        short_term: 'pocket_money',
        speculation: 'pocket_money',
        dividend: 'long_term',
        hedge: 'long_term',
      };
      const validPurposes = ['default', 'pocket_money', 'long_term', 'retirement', 'education', 'other'];
      const normalizedPurpose = validPurposes.includes(p.accountPurpose)
        ? p.accountPurpose
        : (PURPOSE_MAP[p.accountPurpose] ?? 'default');

      profileForm = {
        familyBackupAvailable: p.familyBackupAvailable,
        accountPurpose: normalizedPurpose,
        lifestyleNotes: p.lifestyleNotes,
        displayName: p.displayName,
        riskTolerance: p.riskTolerance,
        exchangeBufferCny: p.exchangeBufferCny,
        familySupport: p.familySupport,
      };
    } catch (e) {
      console.error('[profile] load error:', e);
      profileError = t('settings_profile_load_failed');
    } finally {
      profileLoading = false;
    }
  }

  async function saveProfile() {
    profileSaved = false;
    profileError = '';
    try {
      await getTransport().invoke('save_user_profile', {
        profile: {
          familyBackupAvailable: profileForm.familyBackupAvailable,
          accountPurpose: profileForm.accountPurpose,
          lifestyleNotes: profileForm.lifestyleNotes,
          displayName: profileForm.displayName,
          riskTolerance: profileForm.riskTolerance,
          exchangeBufferCny: profileForm.exchangeBufferCny,
          familySupport: profileForm.familySupport,
        },
      });
      profileSaved = true;
      setTimeout(() => (profileSaved = false), 3000);
    } catch (e) {
      console.error('[profile] save error:', e);
      profileError = t('settings_profile_save_failed');
    }
  }
</script>

<div class="rounded-[var(--radius-lg)] border border-border bg-[var(--bg-card)] p-4 space-y-4">
  <h3 class="text-[13px] font-semibold text-[var(--text-primary)]">{t('settings_profile_title')}</h3>
  <p class="text-[12px] text-[var(--text-secondary)]">{t('settings_profile_desc')}</p>

  {#if profileLoading}
    <p class="text-[13px] text-[var(--text-secondary)]">{t('invest_loading')}</p>
  {:else}
    <!-- Family Backup Toggle -->
    <label class="flex items-center gap-3 cursor-pointer">
      <input
        type="checkbox"
        checked={profileForm.familyBackupAvailable}
        onchange={(e) => (profileForm.familyBackupAvailable = e.currentTarget.checked)}
        class="h-4 w-4 rounded-[var(--radius-sm)] border border-border"
      />
      <div>
        <span class="text-[13px] text-[var(--text-primary)]">{t('settings_profile_family_backup')}</span>
        <p class="text-[12px] text-[var(--text-secondary)]">{t('settings_profile_family_backup_desc')}</p>
      </div>
    </label>

    <!-- Account Purpose -->
    <div class="space-y-1">
      <label class="text-[13px] font-medium text-[var(--text-primary)]">{t('settings_profile_account_purpose')}</label>
      <p class="text-[12px] text-[var(--text-secondary)]">{t('settings_profile_account_purpose_desc')}</p>
      <select
        class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
        bind:value={profileForm.accountPurpose}
      >
        {#each accountPurposeOptions as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
      {#if profileForm.accountPurpose !== 'default'}
        <p class="text-[12px] text-[var(--text-tertiary)] mt-1">
          {t(`settings_profile_purpose_${profileForm.accountPurpose}_desc` as any)}
        </p>
      {/if}
    </div>

    <!-- Family Support -->
    <div class="space-y-1">
      <label class="text-[13px] font-medium text-[var(--text-primary)]">{t('settings_profile_family_support')}</label>
      <p class="text-[12px] text-[var(--text-secondary)]">{t('settings_profile_family_support_desc')}</p>
      <select
        class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
        bind:value={profileForm.familySupport}
      >
        {#each familySupportOptions as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <!-- Lifestyle Notes -->
    <div class="space-y-1">
      <label class="text-[13px] font-medium text-[var(--text-primary)]">{t('settings_profile_lifestyle_notes')}</label>
      <p class="text-[12px] text-[var(--text-secondary)]">{t('settings_profile_lifestyle_notes_desc')}</p>
      <textarea
        class="w-full rounded-[var(--radius-md)] border border-border bg-[var(--bg-input)] px-[var(--space-3)] py-[var(--space-1)] text-[13px] text-[var(--text-primary)]"
        rows="4"
        bind:value={profileForm.lifestyleNotes}
        placeholder={t('settings_profile_lifestyle_notes_placeholder')}
      ></textarea>
    </div>

    <!-- Display Name -->
    {#if profileForm.displayName}
      <div class="text-[12px] text-[var(--text-secondary)]">
        {t('settings_profile_display_name')}: {profileForm.displayName}
      </div>
    {/if}

    <!-- Save -->
    <div class="flex gap-2 pt-2">
      <button
        class="rounded-[var(--radius-md)] bg-[var(--accent)] px-4 py-[var(--space-1)] text-[13px] font-medium text-[#1a1918]"
        onclick={saveProfile}
      >{t('settings_save')}</button>
      {#if profileSaved}
        <span class="self-center text-[12px] text-[var(--color-success)]">{t('settings_saved')}</span>
      {/if}
      {#if profileError}
        <span class="self-center text-[12px] text-[var(--color-error)]">{profileError}</span>
      {/if}
    </div>
  {/if}
</div>
