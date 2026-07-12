<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import * as api from "$lib/api";
  import type { AiCharacter } from "$lib/types";
  import Card from "$lib/components/Card.svelte";
  import Button from "$lib/components/Button.svelte";
  import Input from "$lib/components/Input.svelte";
  import { PHASE7_PROVIDERS } from "$lib/utils/provider-catalog";
  import { t } from "$lib/i18n/index.svelte";
  import type { MessageKey } from "$lib/i18n/types";
  import { dbgWarn } from "$lib/utils/debug";

  // ── State ──
  let characters = $state<AiCharacter[]>([]);
  let loading = $state(true);
  let showForm = $state(false);
  let editingId = $state<string | null>(null);
  let saving = $state(false);
  let deleteConfirmId = $state<string | null>(null);

  // ── Form fields ──
  let formLabel = $state("");
  let formRoleType = $state("custom");
  let formDefaultProvider = $state("claude");
  let formDefaultModel = $state("");
  let formRoleInstruction = $state("");
  let formIcon = $state("");
  let formPersonality = $state("");
  let formExpertise = $state<string[]>([]);
  let expertiseInput = $state("");
  let editingAvatar = $state<string | null>(null);
  let pendingAvatarPath = $state<string | null>(null);

  // Toast
  let toastMessage = $state<string | null>(null);
  let toastType = $state<"success" | "error">("success");
  let toastTimeout: ReturnType<typeof setTimeout> | null = null;

  function showToast(message: string, type: "success" | "error") {
    toastMessage = message;
    toastType = type;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(() => { toastMessage = null; }, 3000);
  }

  const ROLE_TYPES = ["planner", "executor", "custom"] as const;

  const ROLE_TYPE_KEYS: Record<string, MessageKey> = {
    planner: "settings_characters_planner",
    executor: "settings_characters_executor",
    custom: "settings_characters_custom",
  };

  // ── Load ──
  onMount(async () => {
    try {
      characters = await api.listCharacters();
    } catch (e) {
      dbgWarn("settings/characters", "load failed", e);
    } finally {
      loading = false;
    }
  });

  onDestroy(() => {
    if (toastTimeout) clearTimeout(toastTimeout);
  });

  // ── Form helpers ──
  function resetForm() {
    formLabel = "";
    formRoleType = "custom";
    formDefaultProvider = "claude";
    formDefaultModel = "";
    formRoleInstruction = "";
    formIcon = "";
    formPersonality = "";
    formExpertise = [];
    expertiseInput = "";
    editingAvatar = null;
    pendingAvatarPath = null;
    editingId = null;
  }

  function openCreateForm() {
    resetForm();
    showForm = true;
  }

  function openEditForm(char: AiCharacter) {
    editingId = char.id;
    formLabel = char.label;
    formRoleType = char.role_type;
    formDefaultProvider = char.default_provider;
    formDefaultModel = char.default_model ?? "";
    formRoleInstruction = char.role_instruction ?? "";
    formIcon = char.icon ?? "";
    formPersonality = char.personality ?? "";
    formExpertise = char.expertise ? [...char.expertise] : [];
    expertiseInput = "";
    editingAvatar = char.avatar_path ?? null;
    showForm = true;
  }

  function cancelForm() {
    showForm = false;
    resetForm();
  }

  async function pickAvatar() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg"] }],
      });
      if (!selected) return;
      const filePath = typeof selected === "string" ? selected : selected[0];
      if (editingId) {
        const path = await api.uploadCharacterAvatar(editingId, filePath);
        editingAvatar = path;
        showToast(t("settings_characters_avatar_updated"), "success");
      } else {
        pendingAvatarPath = filePath;
        editingAvatar = filePath;
      }
    } catch (err) {
      dbgWarn("settings/characters", "avatar upload failed", err);
      showToast(t("settings_characters_avatar_upload_failed"), "error");
    }
  }

  async function saveCharacter() {
    if (!formLabel.trim() || saving) return;
    saving = true;
    try {
      const label = formLabel.trim();
      const roleType = formRoleType;
      const roleInstruction = formRoleInstruction.trim() || null;
      const defaultProvider = formDefaultProvider;
      const defaultModel = formDefaultModel.trim() || null;
      const icon = formIcon.trim() || null;

      if (editingId) {
        const updated = await api.updateCharacter(editingId, {
          label,
          roleType,
          roleInstruction,
          defaultProvider,
          defaultModel,
          icon,
          avatarPath: editingAvatar ?? null,
          personality: formPersonality.trim() || null,
          expertise: formExpertise,
        });
        characters = characters.map((c) => (c.id === editingId ? updated : c));
      } else {
        const created = await api.createCharacter(
          label,
          roleType,
          roleInstruction,
          defaultProvider,
          defaultModel,
          icon,
        );
        if (pendingAvatarPath) {
          try {
            const avatarPath = await api.uploadCharacterAvatar(created.id, pendingAvatarPath);
            const updated = await api.updateCharacter(created.id, { avatarPath });
            characters = [...characters, updated];
          } catch (avatarErr) {
            dbgWarn("settings/characters", "avatar upload after create failed", avatarErr);
            showToast(t("settings_characters_avatar_upload_failed_created"), "error");
            characters = [...characters, created];
          }
        } else {
          characters = [...characters, created];
        }
      }
      showForm = false;
      resetForm();
    } catch (e) {
      dbgWarn("settings/characters", "save failed", e);
    } finally {
      saving = false;
    }
  }

  async function deleteCharacter(id: string) {
    try {
      await api.deleteCharacter(id);
      characters = characters.filter((c) => c.id !== id);
      deleteConfirmId = null;
    } catch (e) {
      dbgWarn("settings/characters", "delete failed", e);
    }
  }

  function roleTypeBadgeClass(roleType: string): string {
    if (roleType === "planner")
      return "bg-blue-500/10 text-blue-400 border-blue-500/20";
    if (roleType === "executor")
      return "bg-emerald-500/10 text-emerald-400 border-emerald-500/20";
    return "bg-muted text-muted-foreground border-border";
  }

  function truncate(text: string | undefined, maxLen: number): string {
    if (!text) return "";
    return text.length > maxLen ? text.slice(0, maxLen) + "..." : text;
  }

  function providerLabel(id: string): string {
    return PHASE7_PROVIDERS.find((p) => p.id === id)?.label ?? id;
  }

  function fileSrc(path: string | null | undefined): string {
    if (!path) return "";
    try {
      return (window as any).__TAURI__?.core?.convertFileSrc?.(path) ?? path;
    } catch {
      return path;
    }
  }
</script>

{#if loading}
  <div class="flex items-center justify-center py-20">
    <span class="text-sm text-muted-foreground">Loading...</span>
  </div>
{:else}
  <div class="max-w-4xl mx-auto p-6 animate-slide-up">
    <!-- Header -->
    <div class="flex items-center justify-between mb-6">
      <div>
        <a
          href="/settings"
          class="text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          {t("settings_title")}
        </a>
        <h1 class="text-2xl font-bold mt-1">{t("settings_characters")}</h1>
      </div>
      <Button onclick={openCreateForm} size="sm">
        + {t("settings_characters_new")}
      </Button>
    </div>

    <!-- Character list -->
    {#if characters.length === 0}
      <Card class="p-8 text-center">
        <p class="text-sm text-muted-foreground">{t("settings_characters")}</p>
      </Card>
    {:else}
      <div class="grid gap-3">
        {#each characters as char (char.id)}
          <Card class="p-4">
            <div class="flex items-start gap-3">
              <!-- Icon / Avatar -->
              {#if char.avatar_path}
                <img
                  src={fileSrc(char.avatar_path)}
                  alt=""
                  class="w-10 h-10 rounded-xl object-cover shrink-0 mt-0.5"
                />
              {:else}
                <span class="text-2xl shrink-0 mt-0.5">{char.icon || "\u{1F916}"}</span>
              {/if}

              <!-- Content -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <span class="font-medium text-sm">{char.label}</span>
                  <span
                    class="text-[10px] px-1.5 py-0.5 rounded border font-medium {roleTypeBadgeClass(
                      char.role_type,
                    )}"
                  >
                    {t(ROLE_TYPE_KEYS[char.role_type] ?? ("settings_characters_custom" as MessageKey))}
                  </span>
                </div>
                <p class="text-xs text-muted-foreground mb-1">
                  {providerLabel(char.default_provider)}
                  {#if char.default_model}
                    / {char.default_model}
                  {/if}
                </p>
                {#if char.personality}
                  <p class="text-xs text-muted-foreground/70 line-clamp-1 mt-0.5">
                    {truncate(char.personality, 80)}
                  </p>
                {/if}
                {#if char.expertise && char.expertise.length > 0}
                  <div class="flex flex-wrap gap-1 mt-1">
                    {#each char.expertise as tag}
                      <span class="text-[10px] bg-primary/5 text-primary px-1.5 py-0.5 rounded">{tag}</span>
                    {/each}
                  </div>
                {/if}
              </div>

              <!-- Actions -->
              <div class="flex items-center gap-1 shrink-0">
                <Button variant="ghost" size="icon" onclick={() => openEditForm(char)}>
                  <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
                    <path d="m15 5 4 4" />
                  </svg>
                </Button>
                <Button variant="ghost" size="icon" onclick={() => (deleteConfirmId = char.id)}>
                  <svg class="h-4 w-4 text-destructive" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M3 6h18" />
                    <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
                    <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                  </svg>
                </Button>
              </div>
            </div>
          </Card>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<!-- Create/Edit dialog -->
{#if showForm}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
    role="dialog"
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) cancelForm();
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") cancelForm();
    }}
  >
    <div
      role="document"
      class="w-full max-w-lg rounded-lg border bg-background shadow-xl p-6 space-y-4 animate-slide-up"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h2 class="text-lg font-semibold">
        {editingId ? t("settings_characters_edit") : t("settings_characters_new")}
      </h2>

      <!-- Label -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-label">
          {t("settings_characters_label")}
          <span class="text-destructive">*</span>
        </label>
        <Input
          bind:value={formLabel}
          placeholder={t("settings_characters_label")}
        />
      </div>

      <!-- Role type -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-role-type">
          {t("settings_characters_role_type")}
        </label>
        <select
          id="char-role-type"
          bind:value={formRoleType}
          class="flex h-9 w-full rounded-md border border-input bg-background text-foreground px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          {#each ROLE_TYPES as rt}
            <option value={rt}>{t(ROLE_TYPE_KEYS[rt])}</option>
          {/each}
        </select>
      </div>

      <!-- Default provider -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-provider">
          {t("settings_characters_provider")}
        </label>
        <select
          id="char-provider"
          bind:value={formDefaultProvider}
          class="flex h-9 w-full rounded-md border border-input bg-background text-foreground px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          {#each PHASE7_PROVIDERS as p}
            <option value={p.id}>{p.label}</option>
          {/each}
        </select>
      </div>

      <!-- Default model -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-model">
          {t("settings_characters_model")}
        </label>
        <Input
          bind:value={formDefaultModel}
          placeholder={t("settings_characters_model")}
        />
      </div>

      <!-- Role instruction -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-instruction">
          {t("settings_characters_role_instruction")}
        </label>
        <textarea
          id="char-instruction"
          bind:value={formRoleInstruction}
          rows={4}
          placeholder={t("settings_characters_role_instruction_hint")}
          class="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
        ></textarea>
        <p class="text-[11px] text-muted-foreground">
          {t("settings_characters_role_instruction_hint")}
        </p>
      </div>

      <!-- Icon -->
      <div class="space-y-1.5">
        <label class="text-sm font-medium" for="char-icon">
          {t("settings_characters_icon")}
        </label>
        <Input
          bind:value={formIcon}
          placeholder="🤖"
        />
      </div>

      <!-- Avatar upload -->
      <div>
        <label class="text-[10px] uppercase text-[#666] block mb-1">{t("settings_characters_avatar")}</label>
        <div class="flex gap-3 items-start">
          {#if editingAvatar}
            <img src={fileSrc(editingAvatar)} alt="" class="w-16 h-16 rounded-xl object-cover shrink-0" />
          {:else}
            <div class="w-16 h-16 rounded-xl bg-[#1a1a2e] flex items-center justify-center text-2xl shrink-0">?</div>
          {/if}
          <div class="flex flex-col gap-1">
            <button
              type="button"
              class="text-xs px-2 py-1 rounded border border-[#333] hover:border-[#555] transition-colors"
              onclick={pickAvatar}
            >{t("settings_characters_pick_avatar")}</button>
            {#if editingAvatar}
              <button
                class="text-[11px] text-destructive hover:underline text-left"
                onclick={() => (editingAvatar = null)}
              >{t("settings_characters_remove_avatar")}</button>
            {/if}
          </div>
        </div>
      </div>

      <!-- Personality -->
      <div class="space-y-1.5">
        <label class="text-[10px] uppercase text-[#666] block mb-1">Personality</label>
        <textarea
          bind:value={formPersonality}
          placeholder="Character personality..."
          rows={3}
          class="flex w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-none"
        ></textarea>
      </div>

      <!-- Expertise tags -->
      <div class="space-y-1.5">
        <label class="text-[10px] uppercase text-[#666] block mb-1">Expertise</label>
        {#if formExpertise.length > 0}
          <div class="flex flex-wrap gap-1 mb-1.5">
            {#each formExpertise as tag, i}
              <span class="inline-flex items-center gap-1 bg-primary/10 text-primary text-xs px-2 py-0.5 rounded">
                {tag}
                <button
                  class="hover:text-destructive transition-colors leading-none"
                  onclick={() => {
                    formExpertise = formExpertise.filter((_, idx) => idx !== i);
                  }}
                >&times;</button>
              </span>
            {/each}
          </div>
        {/if}
        <div class="flex gap-1">
          <input
            bind:value={expertiseInput}
            placeholder="Add expertise..."
            class="flex-1 h-9 rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                const tag = expertiseInput.trim();
                if (tag && !formExpertise.includes(tag)) {
                  formExpertise = [...formExpertise, tag];
                }
                expertiseInput = "";
              }
            }}
          />
          <button
            class="bg-[#1a1a2e] px-3 rounded text-xs hover:bg-[#252545] transition-colors"
            onclick={() => {
              const tag = expertiseInput.trim();
              if (tag && !formExpertise.includes(tag)) {
                formExpertise = [...formExpertise, tag];
              }
              expertiseInput = "";
            }}
          >+</button>
        </div>
      </div>

      <!-- Actions -->
      <div class="flex justify-end gap-2 pt-2">
        <Button variant="outline" onclick={cancelForm}>{t("common_cancel")}</Button>
        <Button
          onclick={saveCharacter}
          disabled={!formLabel.trim() || saving}
          loading={saving}
        >
          {t("common_save")}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- Delete confirmation -->
{#if deleteConfirmId}
  {@const char = characters.find((c) => c.id === deleteConfirmId)}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm animate-fade-in"
    role="dialog"
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) deleteConfirmId = null;
    }}
    onkeydown={(e) => {
      if (e.key === "Escape") deleteConfirmId = null;
    }}
  >
    <div
      role="document"
      class="w-full max-w-sm rounded-lg border bg-background shadow-xl p-6 space-y-4"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <p class="text-sm">
        {t("settings_characters_delete_confirm")}
        {#if char}
          <strong>{char.label}</strong>
        {/if}?
      </p>
      <div class="flex justify-end gap-2">
        <Button variant="outline" onclick={() => (deleteConfirmId = null)}>
          {t("common_cancel")}
        </Button>
        <Button
          variant="destructive"
          onclick={() => deleteConfirmId && deleteCharacter(deleteConfirmId)}
        >
          {t("common_delete")}
        </Button>
      </div>
    </div>
  </div>
{/if}

<!-- Toast -->
{#if toastMessage}
  <div
    class="fixed top-4 right-4 z-[60] rounded-lg border px-4 py-2 text-sm shadow-lg transition-opacity {toastType ===
    'success'
      ? 'border-green-500/30 bg-green-500/10 text-green-600 dark:text-green-400'
      : 'border-destructive/30 bg-destructive/10 text-destructive'}"
  >
    {toastMessage}
  </div>
{/if}
