<script lang="ts">
  import * as api from "$lib/api";
  import type {
    GroupChatDetail,
    GroupChatParticipantDetail,
    AiCharacter,
    RunStatus,
  } from "$lib/types";
  import { getPhase7Provider, providerIdForRun } from "$lib/utils/provider-catalog";
  import { t } from "$lib/i18n/index.svelte";
  import { dbg, dbgWarn } from "$lib/utils/debug";
  import MarkdownContent from "./MarkdownContent.svelte";
  import { getTransport } from "$lib/transport";
  import type { BusEvent } from "$lib/types";
  import { type PastedBlock, handleRichPaste, composeRichMessageText } from "$lib/utils/rich-input";

  let { groupChat }: { groupChat: api.GroupChatRunIndexEntry | null } = $props();

  // ── State ──
  let detail = $state<GroupChatDetail | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let panelOpen = $state(true);

  // ── Character picker ──
  let characters = $state<AiCharacter[]>([]);
  let pickerOpen = $state(false);
  let pickerLoading = $state(false);
  let quickLabel = $state("");
  let creating = $state(false);

  // ── Composer ──
  let composerText = $state("");
  let participantModels = $state<Map<string, string>>(new Map());
  let pastedBlocks = $state<PastedBlock[]>([]);
  let mentionOpen = $state(false);
  let mentionQuery = $state("");
  let mentionIndex = $state(0);

  let textareaEl: HTMLTextAreaElement | undefined = $state();

  // ── Derived ──
  let participants = $derived(detail?.participants ?? []);

  // ── Bus event state (permission prompts & thinking for participants) ──
  interface PendingPermission {
    requestId: string;
    toolName: string;
    toolUseId: string;
    toolInput: Record<string, unknown>;
    participantLabel: string;
  }
  let pendingPermissions = $state<Map<string, PendingPermission>>(new Map()); // keyed by run_id
  let thinkingTexts = $state<Map<string, string>>(new Map()); // keyed by run_id, accumulated thinking
  let thinkingCollapsed = $state<Map<string, boolean>>(new Map()); // keyed by run_id
  let collapsedResponses = $state<Map<string, boolean>>(new Map()); // keyed by turn.id + participant_id
  let removingParticipant = $state<string | null>(null); // run_id being removed
  let unlistenBus: (() => void) | undefined;

  function startBusListener() {
    if (unlistenBus) return;
    const transport = getTransport();
    transport.listen<BusEvent>("bus-event", (ev) => {
      const pid = participantRunIds().find((p) => p.runId === ev.run_id);
      if (!pid) return;
      if (ev.type === "permission_prompt") {
        const p = pendingPermissions;
        // Only track for planner runs (executor has bypass mode)
        p.set(ev.run_id, {
          requestId: ev.request_id,
          toolName: ev.tool_name,
          toolUseId: ev.tool_use_id,
          toolInput: ev.tool_input as Record<string, unknown>,
          participantLabel: pid.label,
        });
        pendingPermissions = new Map(p);
      } else if (ev.type === "thinking_delta" && ev.text) {
        const current = thinkingTexts.get(ev.run_id) ?? "";
        thinkingTexts.set(ev.run_id, current + ev.text);
        thinkingTexts = new Map(thinkingTexts);
      } else if (ev.type === "message_complete" || ev.type === "run_state") {
        // Clear thinking on message complete or run state change
        if (ev.type === "message_complete" && thinkingTexts.has(ev.run_id)) {
          thinkingTexts.delete(ev.run_id);
          thinkingTexts = new Map(thinkingTexts);
        }
      } else if (ev.type === "session_init" && ev.model && detail) {
        // Update participant's model to match actual runtime model
        const updated = { ...detail };
        const participant = updated.participants.find(
          (p) => p.run?.id === ev.run_id,
        );
        if (participant?.run) {
          participant.run.model = ev.model;
          detail = updated;
        } else {
          participantModels.set(ev.run_id, ev.model);
          participantModels = new Map(participantModels);
        }
      }
    }).then((unlisten) => {
      unlistenBus = unlisten;
    });
  }

  function stopBusListener() {
    unlistenBus?.();
    unlistenBus = undefined;
  }

  // Map of run_id -> { label, role } for quick lookups
  function participantRunIds(): Array<{ label: string; role: string; runId: string }> {
    return participants.map((p) => ({
      label: p.participant.label,
      role: p.participant.role ?? "custom",
      runId: p.participant.run_id,
    }));
  }

  async function respondPermission(runId: string, requestId: string, behavior: "allow" | "deny") {
    try {
      await api.respondPermission(runId, requestId, behavior);
    } catch (e) {
      dbgWarn("GroupChatLayout", "respondPermission failed", e);
    }
    const p = pendingPermissions;
    p.delete(runId);
    pendingPermissions = new Map(p);
  }

  let filteredMentionParticipants = $derived.by(() => {
    if (!mentionOpen) return [];
    const q = mentionQuery.toLowerCase();
    return participants.filter(
      (p) =>
        p.participant.label.toLowerCase().includes(q) ||
        p.participant.agent.toLowerCase().includes(q),
    );
  });

  // ── Load detail ──
  async function loadDetail() {
    if (!groupChat) return;
    loading = true;
    error = null;
    try {
      detail = await api.getGroupChat(groupChat.room_id);
    } catch (e) {
      dbgWarn("GroupChatLayout", "loadDetail failed", e);
      error = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  // Load on mount and when room changes
  $effect(() => {
    const roomId = groupChat?.room_id;
    if (roomId) loadDetail();
  });

  // Bus event lifecycle: start listener when participants exist, stop on cleanup
  $effect(() => {
    if (detail && participants.length > 0) {
      startBusListener();
    }
    return () => {
      stopBusListener();
    };
  });

  // ── Character picker ──
  async function openPicker() {
    pickerOpen = true;
    if (characters.length === 0) {
      pickerLoading = true;
      try {
        characters = await api.listCharacters();
      } catch (e) {
        dbgWarn("GroupChatLayout", "listCharacters failed", e);
        error = "Failed to load characters";
      } finally {
        pickerLoading = false;
      }
    }
  }

  function closePicker() {
    pickerOpen = false;
    quickLabel = "";
  }

  async function addCharacterParticipant(char: AiCharacter) {
    if (!detail || creating) return;
    creating = true;
    try {
      const cwd = detail.cwd ?? "/";
      const provider = getPhase7Provider(char.default_provider);
      const updated = await api.createGroupChatClaudeParticipant(
        detail.id,
        char.role_instruction ?? `You are ${char.label}.`,
        cwd,
        char.default_model ?? provider.defaultModel,
        provider.platformId,
        undefined, // connectionProfileId
        char.label,
        char.role_type,
        char.id,
      );
      detail = updated;
      closePicker();
    } catch (e) {
      dbgWarn("GroupChatLayout", "addCharacterParticipant failed", e);
      error = "Failed to add participant";
    } finally {
      creating = false;
    }
  }

  async function addQuickParticipant() {
    if (!detail || creating || !quickLabel.trim()) return;
    creating = true;
    try {
      const cwd = detail.cwd ?? "/";
      const updated = await api.createGroupChatClaudeParticipant(
        detail.id,
        `You are ${quickLabel.trim()}.`,
        cwd,
        undefined,
        undefined,
        undefined,
        quickLabel.trim(),
        "custom",
      );
      detail = updated;
      closePicker();
    } catch (e) {
      dbgWarn("GroupChatLayout", "addQuickParticipant failed", e);
      error = "Failed to add participant";
    } finally {
      creating = false;
    }
  }

  // ── Composer: @mention ──
  function handleComposerInput() {
    const pos = textareaEl?.selectionStart ?? composerText.length;
    // Scan backwards for @
    let atPos = -1;
    for (let i = pos - 1; i >= 0; i--) {
      const ch = composerText[i];
      if (ch === "@") {
        if (i === 0 || /\s/.test(composerText[i - 1])) {
          atPos = i;
        }
        break;
      }
      if (/\s/.test(ch)) break;
    }
    if (atPos >= 0) {
      mentionOpen = true;
      mentionQuery = composerText.slice(atPos + 1, pos);
      mentionIndex = 0;
    } else {
      mentionOpen = false;
    }
  }

  function selectMention(p: GroupChatParticipantDetail) {
    if (!textareaEl) return;
    const pos = textareaEl.selectionStart ?? composerText.length;
    // Find the @ position
    let atPos = -1;
    for (let i = pos - 1; i >= 0; i--) {
      if (composerText[i] === "@") {
        if (i === 0 || /\s/.test(composerText[i - 1])) {
          atPos = i;
        }
        break;
      }
      if (/\s/.test(composerText[i])) break;
    }
    if (atPos < 0) return;
    const prefix = composerText.slice(0, atPos + 1);
    const suffix = composerText.slice(pos);
    composerText = prefix + p.participant.label + " " + suffix;
    mentionOpen = false;
    requestAnimationFrame(() => {
      if (textareaEl) {
        const newPos = atPos + 1 + p.participant.label.length + 1;
        textareaEl.selectionStart = textareaEl.selectionEnd = newPos;
        textareaEl.focus();
      }
    });
  }

  function handleComposerKeydown(e: KeyboardEvent) {
    if (mentionOpen && filteredMentionParticipants.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        mentionIndex = Math.min(
          mentionIndex + 1,
          filteredMentionParticipants.length - 1,
        );
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        mentionIndex = Math.max(mentionIndex - 1, 0);
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        selectMention(filteredMentionParticipants[mentionIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        mentionOpen = false;
        return;
      }
    }
    // Enter without shift → send
    if (e.key === "Enter" && !e.shiftKey && !mentionOpen && (composerText.trim() || pastedBlocks.length > 0)) {
      e.preventDefault();
      handleSend();
    }
  }

  let sendGeneration = 0;
  let sendCompleted = 0;

  async function handleSend() {
    const text = composeRichMessageText(composerText, pastedBlocks);
    if (!text || !detail) return;
    if (sendCompleted !== sendGeneration && sendGeneration > 0) return;
    dbg("GroupChatLayout", "send", { len: text.length, pasteBlocks: pastedBlocks.length });

    const gen = ++sendGeneration;
    sendCompleted = 0;
    const roomId = detail.id;

    // Fire-and-forget: clear composer immediately for responsive UX
    composerText = "";
    pastedBlocks = [];
    if (textareaEl) textareaEl.style.height = "auto";

    // Poll for incremental updates while turn executes in background
    const pollTimer = setInterval(async () => {
      if (gen !== sendGeneration) { clearInterval(pollTimer); return; }
      if (sendCompleted === gen) return;
      try {
        const updated = await api.getGroupChat(roomId);
        if (gen === sendGeneration && detail?.id === roomId) detail = updated;
      } catch { /* ignore poll errors */ }
    }, 1500);

    try {
      const updated = await api.sendGroupChatMessage(roomId, text);
      if (gen === sendGeneration && detail?.id === roomId) {
        detail = updated;
      }
    } catch (e) {
      dbgWarn("GroupChatLayout", "send failed", e);
    } finally {
      clearInterval(pollTimer);
      sendCompleted = gen;
    }
  }

  async function removeParticipant(runId: string, label: string) {
    if (!detail || removingParticipant) return;
    const msg = t("groupChat_removeParticipantConfirm", { label });
    if (!confirm(msg)) return;
    removingParticipant = runId;
    try {
      detail = await api.removeGroupChatParticipant(detail.id, runId);
    } catch (e) {
      dbgWarn("GroupChatLayout", "removeParticipant failed", e);
    } finally {
      removingParticipant = null;
    }
  }

  function handlePaste(e: ClipboardEvent) {
    const result = handleRichPaste(e, pastedBlocks.length);
    if (result.preventDefault) e.preventDefault();
    if (result.pastedBlocks.length > 0) {
      pastedBlocks = [...pastedBlocks, ...result.pastedBlocks];
    }
  }

  function removePastedBlock(id: string) {
    pastedBlocks = pastedBlocks.filter((b) => b.id !== id);
  }

  function insertSummaryCommand() {
    if (!textareaEl) return;
    composerText = "@summary " + composerText;
    requestAnimationFrame(() => {
      if (textareaEl) {
        textareaEl.selectionStart = textareaEl.selectionEnd = composerText.length;
        textareaEl.focus();
      }
    });
  }

  function autoResize() {
    if (!textareaEl) return;
    textareaEl.style.height = "auto";
    const max = 4 * 24;
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, max) + "px";
  }

  // ── Status helpers ──
  function statusColor(status: RunStatus | undefined): string {
    switch (status) {
      case "running":
        return "bg-green-500";
      case "completed":
        return "bg-blue-500";
      case "failed":
        return "bg-red-500";
      case "stopped":
        return "bg-yellow-500";
      case "pending":
        return "bg-orange-400";
      case "idle":
        return "bg-cyan-400";
      default:
        return "bg-muted-foreground/40";
    }
  }

  function statusLabel(status: RunStatus | undefined): string {
    switch (status) {
      case "running":
        return t("groupChat_statusLabelRunning");
      case "completed":
        return t("groupChat_statusLabelCompleted");
      case "failed":
        return t("groupChat_statusLabelFailed");
      case "stopped":
        return t("groupChat_statusLabelStopped");
      case "pending":
        return t("groupChat_statusLabelStarting");
      case "idle":
        return t("groupChat_statusLabelIdle");
      default:
        return "";
    }
  }

  function providerLabel(agent: string, platformId?: string | null): string {
    const id = providerIdForRun(agent, platformId);
    return getPhase7Provider(id).label;
  }

  function roleBadgeColor(role: string): string {
    switch (role) {
      case "planner":
        return "bg-purple-500/15 text-purple-400";
      case "executor":
        return "bg-blue-500/15 text-blue-400";
      default:
        return "bg-muted text-muted-foreground";
    }
  }

  // ── Timeline helpers ──
  function getParticipantInfo(participantId: string, runId?: string): GroupChatParticipantDetail | undefined {
    // Primary: match by participant.id
    const byId = participants.find((p) => p.participant.id === participantId);
    if (byId) return byId;
    // Fallback: match by run_id (covers stale participant references)
    if (runId) {
      const byRun = participants.find((p) => p.participant.run_id === runId);
      if (byRun) return byRun;
    }
    return undefined;
  }

  function roleLabel(role: string): string {
    switch (role) {
      case "planner": return t("groupChat_rolePlanner");
      case "executor": return t("groupChat_roleExecutor");
      case "reviewer": return t("groupChat_roleReviewer");
      default: return role;
    }
  }

  function roleCardBorder(role: string): string {
    switch (role) {
      case "planner": return "border-purple-500/20";
      case "executor": return "border-blue-500/20";
      case "reviewer": return "border-emerald-500/20";
      default: return "border-border/50";
    }
  }

  function roleCardBg(role: string): string {
    switch (role) {
      case "planner": return "bg-purple-500/3";
      case "executor": return "bg-blue-500/3";
      case "reviewer": return "bg-emerald-500/3";
      default: return "";
    }
  }

  function roleHeaderBg(role: string): string {
    switch (role) {
      case "planner": return "bg-purple-500/8";
      case "executor": return "bg-blue-500/8";
      case "reviewer": return "bg-emerald-500/8";
      default: return "bg-muted/30";
    }
  }

  function roleAvatarBg(role: string): string {
    switch (role) {
      case "planner": return "bg-purple-600";
      case "executor": return "bg-blue-600";
      case "reviewer": return "bg-emerald-600";
      default: return "bg-muted-foreground";
    }
  }
</script>

<div class="flex h-full overflow-hidden bg-background">
  <!-- Main content area (placeholder for timeline) -->
  <div class="flex-1 flex flex-col min-w-0">
    <!-- Header bar -->
    <div class="flex items-center justify-between border-b border-border px-4 py-2.5 shrink-0">
      <div class="flex items-center gap-2 min-w-0">
        <h2 class="text-sm font-semibold truncate">{detail?.name ?? groupChat?.room_name ?? t("groupChat_defaultName")}</h2>
        {#if detail}
          <span class="text-[11px] text-muted-foreground shrink-0">
            {t("groupChat_participantsCount", { count: String(participants.length) })}
          </span>
        {/if}
      </div>
      <button
        class="flex h-7 w-7 items-center justify-center rounded-lg text-muted-foreground/60 hover:text-foreground hover:bg-accent transition-colors shrink-0"
        onclick={() => (panelOpen = !panelOpen)}
        title={panelOpen ? t("groupChat_hidePanel") : t("groupChat_showPanel")}
      >
        <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
          <circle cx="9" cy="7" r="4" />
          <path d="M22 21v-2a4 4 0 0 0-3-3.87" />
          <path d="M16 3.13a4 4 0 0 1 0 7.75" />
        </svg>
      </button>
    </div>

    <!-- Loading / error states -->
    {#if loading}
      <div class="flex-1 flex items-center justify-center">
        <div class="h-5 w-5 rounded-full border-2 border-border border-t-muted-foreground animate-spin"></div>
      </div>
    {:else if error}
      <div class="flex-1 flex items-center justify-center">
        <p class="text-sm text-destructive">{error}</p>
      </div>
    {:else if detail}
      <!-- Pending permission prompts -->
      {#each [...pendingPermissions.entries()] as [runId, perm] (perm.requestId)}
        <div class="mx-4 mt-3 rounded-lg border border-yellow-500/30 bg-yellow-500/5 p-3 flex items-center justify-between">
          <div class="min-w-0">
            <p class="text-xs font-medium text-yellow-400/90">
              {perm.participantLabel} wants to use <span class="font-mono">{perm.toolName}</span>
            </p>
            {#if Object.keys(perm.toolInput).length > 0}
              <pre class="mt-1 text-[10px] text-muted-foreground truncate max-w-md">{JSON.stringify(perm.toolInput)}</pre>
            {/if}
          </div>
          <div class="flex items-center gap-1.5 shrink-0 ml-3">
            <button
              class="rounded-md bg-green-600/20 px-3 py-1 text-[11px] font-medium text-green-400 hover:bg-green-600/30 transition-colors"
              onclick={() => respondPermission(runId, perm.requestId, "allow")}
            >{t("common_allow")}</button>
            <button
              class="rounded-md bg-red-600/20 px-3 py-1 text-[11px] font-medium text-red-400 hover:bg-red-600/30 transition-colors"
              onclick={() => respondPermission(runId, perm.requestId, "deny")}
            >{t("common_deny")}</button>
          </div>
        </div>
      {/each}

      <!-- Message timeline -->
      <div class="flex-1 overflow-y-auto p-4 space-y-6">
        {#if detail.turns.length === 0}
          <div class="flex items-center justify-center h-full">
            <div class="text-center text-muted-foreground max-w-sm">
              <p class="text-sm">{t("groupChat_noTurns")}</p>
            </div>
          </div>
        {:else}
          {#each detail.turns as turn, turnIdx (turn.id)}
            <!-- Turn divider -->
            {#if turnIdx > 0}
              <div class="flex items-center gap-3 my-2">
                <div class="flex-1 border-t border-border/20"></div>
                <span class="text-[10px] text-muted-foreground/40 shrink-0">{turn.started_at.slice(11, 19)}</span>
                <div class="flex-1 border-t border-border/20"></div>
              </div>
            {/if}

            <!-- User message bubble (right-aligned) -->
            <div class="flex justify-end">
              <div class="max-w-[70%]">
                <div class="rounded-2xl rounded-br-sm border border-blue-500/15 bg-blue-500/10 px-3 py-2">
                  <p class="text-xs">{turn.user_input}</p>
                </div>
                <div class="text-right mt-1">
                  <span class="text-[10px] text-muted-foreground/40">{turn.started_at.slice(11, 19)}</span>
                </div>
              </div>
            </div>

            <!-- AI participant responses (left-aligned, independent bubbles) -->
            {#each turn.responses as resp (resp.participant_id)}
              {@const pinfo = getParticipantInfo(resp.participant_id, resp.run_id)}
              {@const role = pinfo?.participant.role ?? "custom"}
              {@const roleName = roleLabel(role)}
              {@const hasThinking = thinkingTexts.has(resp.run_id)}
              {@const thinkingContent = thinkingTexts.get(resp.run_id) ?? ""}
              {@const isThinkingOpen = !(thinkingCollapsed.get(resp.run_id) ?? true)}

              <div class="flex justify-start">
                <div class="max-w-[85%]">
                  <div class="rounded-2xl rounded-bl-sm border {roleCardBorder(role)} {roleCardBg(role)} overflow-hidden">
                    <!-- Message header -->
                    <div class="flex items-center gap-2 px-3 py-1.5 {roleHeaderBg(role)}">
                      <span class="w-5 h-5 rounded-full {roleAvatarBg(role)} text-[10px] font-bold text-white flex items-center justify-center shrink-0">
                        {pinfo?.participant.label?.charAt(0)?.toUpperCase() ?? "?"}
                      </span>
                      <span class="text-xs font-medium truncate">{pinfo?.participant.label ?? resp.participant_id}</span>
                      <span class="text-[10px] rounded px-1 py-0.5 {roleBadgeColor(role)}">{roleName}</span>
                      <span class="text-[10px] text-muted-foreground/50">{providerLabel(pinfo?.participant.agent ?? "", pinfo?.run?.platform_id)}</span>
                      {#if resp.status}
                        <span class="ml-auto w-1.5 h-1.5 rounded-full {statusColor(resp.status as RunStatus)} shrink-0"></span>
                      {/if}
                    </div>

                    <!-- Thinking toggle (if available) -->
                    {#if hasThinking}
                      <button
                        class="flex items-center gap-1 px-3 py-1 text-[10px] text-blue-400/60 hover:text-blue-400 transition-colors w-full"
                        onclick={() => {
                          const key = resp.run_id;
                          const m = thinkingCollapsed;
                          m.set(key, !(m.get(key) ?? true));
                          thinkingCollapsed = new Map(m);
                        }}
                      >
                        <svg class="h-2.5 w-2.5 transition-transform {isThinkingOpen ? 'rotate-90' : ''}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="m9 18 6-6-6-6"/></svg>
                        {t("chat_thoughtProcess")}
                      </button>
                      {#if isThinkingOpen}
                        <div class="px-3 py-1.5 text-[10px] text-blue-300/70 border-t border-blue-500/10 whitespace-pre-wrap break-all">
                          {thinkingContent}
                        </div>
                      {/if}
                    {/if}

                    <!-- Message body -->
                    {#if resp.preview}
                      {@const collapseKey = turn.id + "-" + resp.participant_id}
                      {@const lineCount = resp.preview.split("\n").length}
                      {@const isLong = lineCount > 15}
                      {@const isCollapsed = collapsedResponses.get(collapseKey) !== false && isLong}
                      <div class="px-3 py-2 text-xs">
                        <div
                          class="{isCollapsed ? 'max-h-40 overflow-hidden' : ''}"
                          style={isCollapsed ? "mask-image: linear-gradient(to bottom, black 70%, transparent);" : ""}
                        >
                          <MarkdownContent text={resp.preview} />
                        </div>
                        {#if isLong}
                          <button
                            class="mt-1 text-[11px] text-muted-foreground hover:text-foreground transition-colors"
                            onclick={() => {
                              collapsedResponses = new Map(collapsedResponses).set(collapseKey, (collapsedResponses.get(collapseKey) ?? true) ? false : true);
                            }}
                          >
                            {collapsedResponses.get(collapseKey) !== false
                              ? t("common_showAllLines", { count: String(lineCount) })
                              : t("common_collapse")}
                          </button>
                        {/if}
                      </div>
                    {:else if resp.error}
                      <div class="px-3 py-2 text-xs">
                        <p class="text-red-400/80">{resp.error}</p>
                      </div>
                    {:else}
                      <div class="px-3 py-2 text-xs">
                        <div class="flex items-center gap-1 py-1">
                          <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0s"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0.15s"></span>
                          <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0.3s"></span>
                        </div>
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {/each}
            <!-- Thinking indicator for targeted participants with no response yet (last turn only) -->
            {#if turnIdx === detail.turns.length - 1}
            {#each turn.target_participant_ids.filter((tid) => !turn.responses.some((r) => r.participant_id === tid)) as tid (tid)}
              {@const pinfo = getParticipantInfo(tid, tid)}
              <div class="flex justify-start">
                <div class="max-w-[85%]">
                  <div class="rounded-2xl rounded-bl-sm border border-border/30 bg-muted/20 overflow-hidden">
                    <div class="flex items-center gap-2 px-3 py-1.5 bg-muted/10">
                      <span class="w-5 h-5 rounded-full bg-muted-foreground/20 text-[10px] font-bold text-muted-foreground flex items-center justify-center shrink-0">
                        {pinfo?.participant.label?.charAt(0)?.toUpperCase() ?? "?"}
                      </span>
                      <span class="text-xs font-medium truncate">{pinfo?.participant.label ?? tid}</span>
                    </div>
                    <div class="px-3 py-2 text-xs">
                      <div class="flex items-center gap-1.5 py-1">
                        <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0s"></span>
                        <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0.15s"></span>
                        <span class="w-1.5 h-1.5 rounded-full bg-muted-foreground/30 animate-bounce" style="animation-delay: 0.3s"></span>
                        <span class="text-[10px] text-muted-foreground/50 ml-1">{t("groupChat_thinking")}</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            {/each}
            {/if}
          {/each}
        {/if}
      </div>

      <!-- Composer area -->
      <div class="border-t border-border bg-muted/30 px-4 py-3 shrink-0 relative">
        <!-- Toolbar -->
        <div class="flex items-center gap-1 mb-2">
          <button
            class="shrink-0 rounded-md border border-border/50 px-2 py-0.5 text-[11px] text-muted-foreground/70 hover:text-foreground hover:bg-accent hover:border-border transition-colors"
            onclick={insertSummaryCommand}
            title={t("groupChat_summaryTitle")}
          >
            @{t("groupChat_summary")}
          </button>
          <button
            class="shrink-0 rounded-md border border-border/50 px-2 py-0.5 text-[11px] text-muted-foreground/70 hover:text-foreground hover:bg-accent hover:border-border transition-colors"
            onclick={() => {
              composerText = "@debate " + composerText;
              requestAnimationFrame(() => {
                if (textareaEl) {
                  textareaEl.selectionStart = textareaEl.selectionEnd = composerText.length;
                  textareaEl.focus();
                }
              });
            }}
            title={t("groupChat_debateTitle")}
          >
            @{t("groupChat_debate")}
          </button>
        </div>

        <!-- Paste block chips -->
        {#if pastedBlocks.length > 0}
          <div class="flex flex-wrap gap-1.5 mb-2">
            {#each pastedBlocks as block (block.id)}
              <div class="flex items-center gap-1 rounded-md border border-border/50 bg-muted/50 px-2 py-1 text-[11px]">
                <svg class="h-3 w-3 text-muted-foreground/60 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" /><path d="M14 2v6h6" /><path d="M16 13H8" /><path d="M16 17H8" /><path d="M10 9H8" />
                </svg>
                <span class="truncate max-w-[120px]">{block.preview}</span>
                <span class="text-muted-foreground/40">{block.lineCount}L</span>
                <button class="text-muted-foreground/40 hover:text-foreground ml-0.5" onclick={() => removePastedBlock(block.id)} title="Remove">
                  <svg class="h-2.5 w-2.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg>
                </button>
              </div>
            {/each}
          </div>
        {/if}

        <!-- Textarea -->
        <textarea
          bind:this={textareaEl}
          bind:value={composerText}
          onkeydown={handleComposerKeydown}
          oninput={() => { autoResize(); handleComposerInput(); }}
          onpaste={handlePaste}
          placeholder={t("groupChat_roundtablePlaceholder")}
          rows={1}
          class="w-full resize-none bg-transparent px-3 py-2 text-sm text-foreground placeholder:text-muted-foreground/60 focus:outline-none border border-border rounded-lg"
          style="min-height: 36px;"
        ></textarea>

        <!-- @mention dropdown -->
        {#if mentionOpen && filteredMentionParticipants.length > 0}
          <div class="absolute bottom-full left-4 right-4 mb-1 z-50 rounded-lg border border-border bg-background shadow-lg max-h-[200px] overflow-y-auto">
            {#each filteredMentionParticipants as p, i (p.participant.id)}
              <button
                class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm transition-colors {i === mentionIndex ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/50'}"
                onmouseenter={() => (mentionIndex = i)}
                onclick={() => selectMention(p)}
              >
                <span class="w-1.5 h-1.5 rounded-full {statusColor(p.run?.status)} shrink-0"></span>
                <span class="truncate text-xs font-medium">{p.participant.label}</span>
                <span class="text-[10px] text-muted-foreground/60 shrink-0">{p.participant.agent}</span>
              </button>
            {/each}
          </div>
        {/if}

        <!-- Send button -->
        <div class="flex justify-end mt-2">
          <button
            class="flex h-7 items-center gap-1.5 rounded-lg px-3 text-xs font-medium transition-colors {(composerText.trim() || pastedBlocks.length > 0) && (sendCompleted === sendGeneration || sendGeneration === 0)
              ? 'bg-primary text-primary-foreground hover:bg-primary/90'
              : 'text-muted-foreground/40'}"
            onclick={handleSend}
            disabled={(!composerText.trim() && pastedBlocks.length === 0) || (sendCompleted !== sendGeneration && sendGeneration > 0)}
          >
            <span>{t("groupChat_send")}</span>
            <svg class="h-3.5 w-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 12h14" /><path d="m12 5 7 7-7 7" />
            </svg>
          </button>
        </div>
      </div>
    {/if}
  </div>

  <!-- Collapsible participant side panel -->
  {#if panelOpen}
    <div class="w-64 border-l border-border bg-background flex flex-col shrink-0">
      <div class="flex items-center justify-between px-3 py-2.5 border-b border-border shrink-0">
        <span class="text-xs font-semibold text-muted-foreground">{t("groupChat_participants")}</span>
        <button
          class="flex h-6 items-center gap-1 rounded px-1.5 text-[11px] font-medium text-primary hover:bg-accent transition-colors"
          onclick={openPicker}
        >
          <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 5v14" /><path d="M5 12h14" />
          </svg>
          {t("groupChat_addParticipant")}
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-2 space-y-1">
        {#if participants.length === 0}
          <p class="text-[11px] text-muted-foreground/60 text-center py-4">{t("groupChat_noParticipants")}</p>
        {/if}
        {#each participants as p (p.participant.id)}
          {@const status = p.run?.status}
          {@const provider = providerLabel(p.participant.agent, p.run?.platform_id)}
          <div class="rounded-lg border border-border/50 px-3 py-2 hover:bg-accent/30 transition-colors">
            <div class="flex items-center gap-2">
              <span class="w-2 h-2 rounded-full {statusColor(status)} shrink-0" title={statusLabel(status)}></span>
              <span class="text-xs font-medium truncate flex-1">{p.participant.label}</span>
              <span class="text-[10px] rounded px-1 py-0.5 shrink-0 {roleBadgeColor(p.participant.role)}">
                {p.participant.role}
              </span>
              <button
                class="ml-auto shrink-0 text-muted-foreground/40 hover:text-destructive transition-colors disabled:opacity-50"
                title={t("groupChat_removeParticipant")}
                disabled={removingParticipant === p.participant.run_id}
                onclick={() => removeParticipant(p.participant.run_id, p.participant.label)}
              >
                <svg class="h-3 w-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                  <path d="M3 6h18" /><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" /><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                </svg>
              </button>
            </div>
            <div class="flex items-center gap-1.5 mt-1 ml-4">
              <span class="text-[10px] text-muted-foreground/60 truncate">{provider}</span>
              {#if p.run?.model || participantModels.get(p.participant.run_id)}
                <span class="text-[10px] text-muted-foreground/40 truncate">/ {p.run?.model ?? participantModels.get(p.participant.run_id)}</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<!-- Character picker overlay -->
{#if pickerOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center" role="dialog" aria-modal="true">
    <div class="fixed inset-0 bg-black/60 backdrop-blur-sm" onclick={closePicker} role="presentation"></div>
    <div class="relative z-50 w-full max-w-md rounded-lg border bg-background p-4 shadow-lg max-h-[80vh] flex flex-col">
      <h3 class="text-sm font-semibold mb-3">{t("groupChat_addParticipant")}</h3>

      <!-- Quick create -->
      <div class="flex items-center gap-2 mb-3">
        <input
          type="text"
          bind:value={quickLabel}
          placeholder={t("groupChat_quickCreatePlaceholder")}
          class="flex-1 h-8 rounded-md border border-border bg-background px-2 text-xs outline-none focus:border-ring"
          onkeydown={(e) => { if (e.key === "Enter" && quickLabel.trim()) addQuickParticipant(); }}
        />
        <button
          class="h-8 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50"
          onclick={addQuickParticipant}
          disabled={creating || !quickLabel.trim()}
        >
          {t("groupChat_quickCreate")}
        </button>
      </div>

      <div class="text-[11px] text-muted-foreground/60 mb-2">{t("groupChat_orSelectCharacter")}</div>

      <!-- Character list -->
      <div class="flex-1 overflow-y-auto space-y-1 min-h-0">
        {#if pickerLoading}
          <div class="flex items-center justify-center py-6">
            <div class="h-4 w-4 rounded-full border-2 border-border border-t-muted-foreground animate-spin"></div>
          </div>
        {:else if characters.length === 0}
          <p class="text-[11px] text-muted-foreground/60 text-center py-4">{t("groupChat_noCharacters")}</p>
        {:else}
          {#each characters as char (char.id)}
            <button
              class="flex w-full items-center gap-2.5 rounded-lg border border-border/50 px-3 py-2 text-left hover:bg-accent/50 transition-colors disabled:opacity-50"
              onclick={() => addCharacterParticipant(char)}
              disabled={creating}
            >
              <span class="text-sm shrink-0">{char.icon ?? "?"}</span>
              <div class="flex-1 min-w-0">
                <div class="text-xs font-medium truncate">{char.label}</div>
                <div class="text-[10px] text-muted-foreground/60 truncate">
                  {char.role_type} &middot; {getPhase7Provider(char.default_provider).label}
                  {#if char.default_model}
                    / {char.default_model}
                  {/if}
                </div>
              </div>
              <span class="text-[10px] rounded px-1 py-0.5 shrink-0 {roleBadgeColor(char.role_type)}">
                {char.role_type}
              </span>
            </button>
          {/each}
        {/if}
      </div>

      <div class="flex justify-end mt-3 pt-2 border-t border-border">
        <button
          class="h-8 rounded-md border border-border px-3 text-xs font-medium text-muted-foreground hover:bg-accent transition-colors"
          onclick={closePicker}
        >
          {t("groupChat_confirmNo")}
        </button>
      </div>
    </div>
  </div>
{/if}
