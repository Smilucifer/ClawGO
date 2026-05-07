<script lang="ts">
  import { onMount } from "svelte";
  import { getUserSettings } from "$lib/api";
  import { t } from "$lib/i18n/index.svelte";
  import { RoomStore, type RoundtableSeatDraft } from "$lib/stores/room-store.svelte";
  import type {
    CcAgentProfile,
    ConnectionProfile,
    RoomResponseRef,
    RoomTurn,
    UserSettings,
  } from "$lib/types";
  import { truncate, formatDuration } from "$lib/utils/format";
  import { PHASE7_PROVIDERS, getPhase7Provider } from "$lib/utils/provider-catalog";
  import {
    canSendRoomMessage,
    roomParticipantBadge,
    roomParticipantMetaLabel,
    roomMessagePlaceholderKey,
  } from "$lib/utils/room-ui";

  type SeatAgent = RoundtableSeatDraft["agent"];

  interface SeatForm {
    agent: SeatAgent;
    profileId: string;
    connectionProfileId: string;
    label: string;
    model: string;
    platformId: string;
    prompt: string;
    role: string;
  }

  interface LatestParticipantResponse {
    turn: RoomTurn;
    response: RoomResponseRef;
  }

  const store = new RoomStore();

  let createName = $state("Roundtable");
  let createDescription = $state("");
  let createCwd = $state("");
  let showCreateDialog = $state(false);
  let settings = $state<UserSettings | null>(null);
  let seatForms = $state<SeatForm[]>(defaultSeatForms());
  let roundtableMessage = $state("");
  let summaryParticipantId = $state("");
  let deletingRoomId = $state("");
  let historyExpanded = $state(false);

  let ccProfiles = $derived((settings?.cc_agent_profiles ?? []).filter((p) => p.enabled !== false));
  let connectionProfiles = $derived(
    (settings?.connection_profiles ?? []).filter((p) => p.enabled !== false),
  );
  let roomParticipantCount = $derived(store.room?.participants.length ?? 0);
  let displayPanels = $derived.by(() => {
    const participants = store.room?.participants ?? [];
    if (store.room?.kind === "roundtable") {
      return [0, 1, 2].map((index) => ({ index, participant: participants[index] }));
    }
    return participants.map((p, index) => ({ index, participant: p }));
  });
  let participantLabelMap = $derived(
    Object.fromEntries(
      (store.room?.participants ?? []).map((p) => [p.participant.id, p.participant.label]),
    ),
  );
  let roomComposerPlaceholderKey = $derived(
    store.room ? roomMessagePlaceholderKey(store.room.kind) : "room_roundtablePlaceholder",
  );
  let canSendCurrentRoomMessage = $derived(
    store.room
      ? canSendRoomMessage(store.room.kind, roomParticipantCount, roundtableMessage)
      : false,
  );
  let completedPublicTurnCount = $derived(
    (store.room?.turns ?? []).filter((turn) => turn.mode !== "private" && turn.completed_at).length,
  );
  let completedDebatableTurnCount = $derived(
    (store.room?.turns ?? []).filter(
      (turn) => (turn.mode === "fanout" || turn.mode === "debate") && turn.completed_at,
    ).length,
  );
  let summaryParticipants = $derived(store.room?.participants ?? []);
  let canUseRoundtableActions = $derived(
    Boolean(store.room && store.room.kind === "roundtable" && !store.saving),
  );
  let canDebate = $derived(canUseRoundtableActions && completedDebatableTurnCount > 0);
  let canSummary = $derived(
    canUseRoundtableActions && completedPublicTurnCount > 0 && Boolean(summaryParticipantId),
  );

  $effect(() => {
    if (!summaryParticipants.some((item) => item.participant.id === summaryParticipantId)) {
      summaryParticipantId = summaryParticipants[0]?.participant.id ?? "";
    }
  });

  onMount(async () => {
    await Promise.all([store.loadRooms(), loadSettings()]);
    if (store.rooms.length > 0 && !store.selectedRoomId) {
      await selectRoom(store.rooms[0].id);
    } else if (store.rooms.length === 0) {
      showCreateDialog = true;
    }
  });

  async function loadSettings() {
    const loaded = await getUserSettings();
    settings = loaded;
    const profiles = (loaded.cc_agent_profiles ?? []).filter(
      (profile) => profile.enabled !== false,
    );
    if (!createCwd.trim() && loaded.working_directory) createCwd = loaded.working_directory;
    seatForms = defaultSeatForms().map((seat, index) => {
      const profile = profiles[index];
      return profile ? seatFromProfile(seat, profile, index) : seat;
    });
  }

  async function selectRoom(id: string) {
    await store.selectRoom(id);
    roundtableMessage = "";
    deletingRoomId = "";
  }

  function openCreateDialog() {
    if (!createName.trim()) createName = "Roundtable";
    showCreateDialog = true;
  }

  async function pickCreateCwd() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ directory: true, title: t("room_selectProjectFolder") });
      if (typeof selected === "string" && selected.trim()) {
        createCwd = selected;
      }
    } catch {
      // Desktop builds should use the native folder picker here. Do not fall
      // back to manual path entry; the room should share the app-level project
      // selection behavior.
    }
  }

  async function handleCreateRoundtable() {
    const cwd = createCwd.trim();
    if (!cwd) return;
    const seats = seatForms.map((seat, index) => {
      const agent = seat.agent;
      return {
        agent,
        prompt: cleanText(seat.prompt) || defaultSeatPrompt(index, agent),
        model: cleanText(seat.model) || undefined,
        platformId: cleanText(seat.platformId) || undefined,
        connectionProfileId: cleanText(seat.connectionProfileId) || undefined,
        label: cleanText(seat.label) || defaultSeatLabel(index, agent),
        role: cleanText(seat.role) || "participant",
      };
    });
    await store.createRoundtableWithParticipants(
      cleanText(createName) || "Roundtable",
      cleanText(createDescription),
      cwd,
      seats,
    );
    showCreateDialog = false;
    createName = "Roundtable";
    createDescription = "";
  }

  async function handleSendRoundtableMessage() {
    const message = roundtableMessage.trim();
    if (!store.room) return;
    if (!canSendRoomMessage(store.room.kind, roomParticipantCount, message)) return;
    await store.sendMessage(message);
    roundtableMessage = "";
  }

  async function handleDebate() {
    if (!canDebate) return;
    await store.sendDebate(roundtableMessage);
    roundtableMessage = "";
  }

  async function handleSummary() {
    if (!canSummary) return;
    await store.sendSummary(summaryParticipantId);
  }

  async function handleDeleteRoom(id: string) {
    await store.deleteRoom(id);
    deletingRoomId = "";
    if (store.rooms.length > 0) {
      await selectRoom(store.rooms[0].id);
    } else {
      showCreateDialog = true;
    }
  }

  function handleProfileChange(index: number, profileId: string) {
    const profile = ccProfiles.find((item) => item.id === profileId);
    seatForms = seatForms.map((seat, seatIndex) => {
      if (seatIndex !== index) return seat;
      if (!profile) return { ...seat, profileId: "" };
      return seatFromProfile(seat, profile, index);
    });
  }

  function handleConnectionProfileChange(index: number, profileId: string) {
    const profile = connectionProfiles.find((item) => item.id === profileId);
    seatForms = seatForms.map((seat, seatIndex) => {
      if (seatIndex !== index) return seat;
      if (!profile) return { ...seat, connectionProfileId: "" };
      return {
        ...seat,
        connectionProfileId: profile.id,
        agent: profile.agent,
        model: cleanText(profile.model) || seat.model,
        platformId: cleanText(profile.platform_id) || seat.platformId,
      };
    });
  }

  function updateSeat<K extends keyof SeatForm>(index: number, key: K, value: SeatForm[K]) {
    seatForms = seatForms.map((seat, seatIndex) => {
      if (seatIndex !== index) return seat;
      const next = { ...seat, [key]: value };
      // When label changes and prompt still looks auto-generated, sync it
      if (key === "label" && seat.prompt.trimStart().startsWith("You are ")) {
        next.prompt = defaultSeatPromptWithLabel(String(value));
      }
      return next;
    });
  }

  function defaultSeatForms(): SeatForm[] {
    return [
      {
        agent: "claude",
        profileId: "",
        connectionProfileId: "",
        label: "Claude",
        model: "",
        platformId: "",
        prompt: defaultSeatPrompt(0, "claude"),
        role: "participant",
      },
      {
        agent: "codex",
        profileId: "",
        connectionProfileId: "",
        label: "Codex",
        model: "",
        platformId: "",
        prompt: defaultSeatPrompt(1, "codex"),
        role: "participant",
      },
      {
        agent: "codex",
        profileId: "",
        connectionProfileId: "",
        label: "Codex",
        model: "",
        platformId: "",
        prompt: defaultSeatPrompt(2, "codex"),
        role: "participant",
      },
    ];
  }

  function seatFromProfile(seat: SeatForm, profile: CcAgentProfile, index: number): SeatForm {
    const agent = profileAgent(profile);
    return {
      ...seat,
      profileId: profile.id,
      connectionProfileId: seat.connectionProfileId,
      agent,
      label: cleanText(profile.label) || displayAgentLabel(agent),
      model: cleanText(profile.model),
      platformId: cleanText(profile.platform_id),
      prompt: cleanText(profile.prompt) || defaultSeatPrompt(index, agent),
      role: cleanText(profile.role) || "participant",
    };
  }

  function latestResponse(participantId?: string): LatestParticipantResponse | null {
    if (!participantId || !store.room) return null;
    for (let turnIndex = store.room.turns.length - 1; turnIndex >= 0; turnIndex -= 1) {
      const turn = store.room.turns[turnIndex];
      const response = [...turn.responses]
        .reverse()
        .find((item) => item.participant_id === participantId);
      if (response) return { turn, response };
    }
    return null;
  }

  function cleanText(value?: string): string {
    return value?.trim() ?? "";
  }

  function profileAgent(profile?: CcAgentProfile | null): SeatAgent {
    if (profile?.agent === "codex") return profile.agent;
    return "claude";
  }

  function profileLabel(profile: CcAgentProfile): string {
    const agent = profileAgent(profile);
    const model = cleanText(profile.model);
    const platform = cleanText(profile.platform_id);
    if (model && platform) return `${profile.label} · ${agent} · ${platform} · ${model}`;
    if (model || platform) return `${profile.label} · ${agent} · ${model || platform}`;
    return `${profile.label} · ${agent}`;
  }

  function connectionProfilesForAgent(agent: SeatAgent): ConnectionProfile[] {
    if (agent === "deepseek" || agent === "glm") return [];
    return connectionProfiles.filter((profile) => profile.agent === agent);
  }

  function connectionProfileLabel(profile: ConnectionProfile): string {
    const mode = profile.auth_mode === "api" ? "API" : "CLI";
    const model = cleanText(profile.model);
    return model ? `${profile.label} · ${mode} · ${model}` : `${profile.label} · ${mode}`;
  }

  function displayAgentLabel(agent: SeatAgent): string {
    return getPhase7Provider(agent).label;
  }

  function defaultSeatLabel(index: number, agent: SeatAgent): string {
    return `${displayAgentLabel(agent)} ${index + 1}`;
  }

  function defaultSeatPrompt(index: number, agent: SeatAgent): string {
    return defaultSeatPromptWithLabel(defaultSeatLabel(index, agent));
  }

  function defaultSeatPromptWithLabel(label: string): string {
    return `You are ${label} in a three-seat roundtable. Answer independently, be concrete, and keep your reasoning concise. Don't do any change. Only read, analyze and discuss. Roundtable outputs are discussable judgments — only produce evidence-based, verifiable claims. Do not fabricate sources, numbers, or events. If uncertain, state the uncertainty explicitly. Now wait for the topic.`;
  }

  function statusClass(status?: string): string {
    if (
      status === "running" ||
      status === "idle" ||
      status === "complete" ||
      status === "completed"
    )
      return "bg-emerald-500/15 text-emerald-500";
    if (status === "failed") return "bg-red-500/15 text-red-500";
    if (status === "pending") return "bg-amber-500/15 text-amber-500";
    if (status === "waiting") return "bg-muted text-muted-foreground";
    if (status === "stopped") return "bg-muted text-muted-foreground";
    return "bg-muted text-muted-foreground";
  }

  const STATUS_LABEL_KEY: Record<string, string> = {
    pending: "room_statusLabelStarting",
    running: "room_statusLabelRunning",
    idle: "room_statusLabelIdle",
    completed: "room_statusLabelCompleted",
    complete: "room_statusLabelCompleted",
    failed: "room_statusLabelFailed",
    stopped: "room_statusLabelStopped",
    waiting: "room_statusLabelWaiting",
  };

  function participantStatusLabel(status: string): string {
    const key = STATUS_LABEL_KEY[status];
    return key ? t(key) : status;
  }

  function roomKindLabel(kind: string): string {
    if (kind === "research") return t("room_kindResearch");
    if (kind === "driver") return t("room_kindDriver");
    return t("room_kindRoundtable");
  }

  function memoryKindLabel(kind: string): string {
    if (kind === "decision") return t("room_memoryDecision");
    if (kind === "lesson") return t("room_memoryLesson");
    return t("room_memoryFact");
  }

  function elapsedTime(startedAt?: string): string {
    if (!startedAt) return "";
    const ms = Date.now() - new Date(startedAt).getTime();
    return formatDuration(ms);
  }

  const TURN_CHIP_CLASS: Record<string, string> = {
    completed: "bg-emerald-500/20 text-emerald-400 border-emerald-500/30",
    failed: "bg-red-500/20 text-red-400 border-red-500/30",
  };

  function turnChipStatus(turn: RoomTurn): string {
    if (!turn.completed_at) return "pending";
    const anyFailed = turn.responses.some((r) => r.status === "failed");
    if (anyFailed) return "failed";
    return "completed";
  }

  function participantDotClass(status: string): string {
    const DOT_CLASS: Record<string, string> = {
      completed: "bg-emerald-500",
      complete: "bg-emerald-500",
      failed: "bg-red-500",
      absent: "bg-muted-foreground/30",
    };
    return DOT_CLASS[status] ?? "bg-amber-500";
  }
</script>

<div class="flex h-full min-h-0 bg-background">
  <aside class="w-72 shrink-0 border-r border-border bg-muted/20">
    <div class="border-b border-border p-3">
      <div class="flex items-center justify-between gap-2">
        <h1 class="text-sm font-semibold">{t("room_title")}</h1>
        <button
          class="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
          onclick={() => store.loadRooms()}
        >
          {t("room_refresh")}
        </button>
      </div>
      <button
        class="mt-3 w-full rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground disabled:opacity-50"
        disabled={store.saving}
        onclick={openCreateDialog}
      >
        {t("room_newRoundtable")}
      </button>
    </div>

    <div class="h-[calc(100%-90px)] overflow-y-auto p-2">
      {#if store.loading && store.rooms.length === 0}
        <div class="flex justify-center py-8">
          <div
            class="h-4 w-4 animate-spin rounded-full border-2 border-primary/30 border-t-primary"
          ></div>
        </div>
      {:else if store.rooms.length === 0}
        <p class="px-2 py-8 text-center text-sm text-muted-foreground">{t("room_empty")}</p>
      {:else}
        {#each store.rooms as room}
          <button
            class={store.selectedRoomId === room.id
              ? "mb-1 flex w-full flex-col gap-1 rounded-md bg-accent px-2.5 py-2 text-left text-accent-foreground transition-colors"
              : "mb-1 flex w-full flex-col gap-1 rounded-md px-2.5 py-2 text-left transition-colors hover:bg-accent/50"}
            onclick={() => selectRoom(room.id)}
          >
            <div class="flex items-center gap-2">
              <span class="min-w-0 flex-1 truncate text-sm font-medium">{room.name}</span>
              <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                >{roomParticipantBadge(room.kind, room.participant_count)}</span
              >
              <span class="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                >{roomKindLabel(room.kind)}</span
              >
            </div>
            {#if room.description}
              <p class="truncate text-xs text-muted-foreground">{room.description}</p>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </aside>

  <section class="flex min-w-0 flex-1 flex-col">
    {#if store.error}
      <div
        class="border-b border-destructive/30 bg-destructive/10 px-5 py-2 text-sm text-destructive"
      >
        {store.error}
      </div>
    {/if}

    {#if store.room}
      <div class="flex items-start justify-between gap-4 border-b border-border px-5 py-4">
        <div class="min-w-0">
          <h2 class="truncate text-lg font-semibold">{store.room.name}</h2>
          <div class="mt-1 flex flex-wrap gap-2 text-xs text-muted-foreground">
            {#if store.room.description}<span>{store.room.description}</span>{/if}
            {#if store.room.cwd}<span>{store.room.cwd}</span>{/if}
            <span>{roomKindLabel(store.room.kind)}</span>
            <span
              >{t("room_participantsCount", {
                count: String(store.room.participants.length),
              })}</span
            >
          </div>
        </div>
        <div class="flex shrink-0 items-center gap-2">
          {#if deletingRoomId === store.room.id}
            <span class="text-xs text-muted-foreground">{t("room_deletePrompt")}</span>
            <button
              class="rounded-md bg-destructive px-2 py-1 text-xs text-destructive-foreground"
              onclick={() => handleDeleteRoom(store.room!.id)}
            >
              {t("room_confirmYes")}
            </button>
            <button
              class="rounded-md border border-border px-2 py-1 text-xs"
              onclick={() => (deletingRoomId = "")}
            >
              {t("room_confirmNo")}
            </button>
          {:else}
            <button
              class="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
              onclick={() => (deletingRoomId = store.room!.id)}
            >
              {t("room_delete")}
            </button>
          {/if}
        </div>
      </div>

      <div class="min-h-0 flex-1 flex flex-col">
        <div class="min-h-0 flex-1 grid grid-cols-1 gap-3 p-3 xl:grid-cols-3">
          {#each displayPanels as panel}
            {@const participant = panel.participant}
            {@const latest = latestResponse(participant?.participant.id)}
            {@const status = latest?.response.status ?? participant?.run?.status ?? "waiting"}
            {@const runElapsed = elapsedTime(participant?.run?.started_at)}
            <article class="flex min-h-0 flex-col rounded-md border border-border bg-card overflow-hidden">
              <header class="shrink-0 border-b border-border px-4 py-2.5">
                <div class="flex items-start justify-between gap-2">
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <span class="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted text-xs font-semibold">
                        {panel.index + 1}
                      </span>
                      <div class="min-w-0">
                        <h4 class="truncate text-sm font-semibold">
                          {participant?.participant.label ?? t("room_waitingForParticipant")}
                        </h4>
                        <p class="truncate text-[11px] text-muted-foreground">
                          {participant
                            ? roomParticipantMetaLabel(
                                participant.participant.agent,
                                participant.run?.platform_id,
                                participant.run?.model,
                              )
                            : t("room_waitingResponse")}
                        </p>
                      </div>
                    </div>
                  </div>
                  <div class="flex shrink-0 items-center gap-1.5">
                    {#if runElapsed}
                      <span class="text-[10px] text-muted-foreground/70 tabular-nums">{runElapsed}</span>
                    {/if}
                    <span class={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${statusClass(status)}`}>
                      {participantStatusLabel(status)}
                    </span>
                  </div>
                </div>
              </header>

              <div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
                {#if latest?.response.preview}
                  <p class="whitespace-pre-wrap break-words text-sm leading-6">
                    {latest.response.preview}
                  </p>
                  <p class="mt-3 rounded bg-muted/40 px-2 py-1.5 text-xs text-muted-foreground">
                    {t("room_lastPrompt")}: {truncate(latest.turn.user_input, 180)}
                  </p>
                {:else if latest?.response.error}
                  <p class="text-sm text-destructive">{latest.response.error}</p>
                {:else}
                  <div class="flex h-full items-center justify-center rounded-md border border-dashed border-border px-4 text-center text-sm text-muted-foreground">
                    {participant ? t("room_noResponseYet") : t("room_waitingForParticipant")}
                  </div>
                {/if}
              </div>
            </article>
          {/each}
        </div>

        {#if store.room.kind === "research" && store.room.research_artifact}
          <div class="shrink-0 border-t border-border px-3 py-2">
            <details class="text-sm">
              <summary class="cursor-pointer text-xs font-medium text-muted-foreground hover:text-foreground transition-colors">
                {t("room_researchArtifact")} · {store.room.research_artifact.topic} · v{store.room.research_artifact.schema_version}
              </summary>
              <div class="mt-2 space-y-2">
                {#each store.room.research_artifact.memory_candidates as candidate}
                  <div class="rounded border border-border/70 px-2 py-1.5">
                    <div class="flex flex-wrap gap-2 text-xs">
                      <span class="rounded bg-muted px-1.5 py-0.5 text-muted-foreground">
                        {memoryKindLabel(candidate.kind)}
                      </span>
                    </div>
                    <p class="mt-1 text-xs text-foreground">{candidate.text}</p>
                  </div>
                {/each}
              </div>
            </details>
          </div>
        {/if}

        <div class="shrink-0 border-t border-border">
          <button
            class="flex w-full items-center gap-2 px-3 py-2 text-xs text-muted-foreground hover:bg-accent/50 transition-colors"
            onclick={() => (historyExpanded = !historyExpanded)}
          >
            <svg
              class="h-3 w-3 shrink-0 transition-transform duration-150 {historyExpanded ? 'rotate-90' : ''}"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"
            ><path d="m9 18 6-6-6-6" /></svg
            >
            <span class="font-medium">{t("room_history")}</span>
            <span class="tabular-nums">{store.room.turns.length}</span>
            {#if !historyExpanded && store.room.turns.length > 0}
              <span class="text-muted-foreground/50">{t("room_historyCollapsed")}</span>
            {/if}
          </button>

          {#if historyExpanded}
            <div class="max-h-64 overflow-y-auto border-t border-border px-3 pb-3">
              {#if store.room.turns.length === 0}
                <p class="py-6 text-center text-xs text-muted-foreground">{t("room_noTurns")}</p>
              {:else}
                <div class="space-y-2 pt-2">
                  {#each store.room.turns as turn}
                    {@const chipStatus = turnChipStatus(turn)}
                    <div class="rounded-md border border-border bg-card/60">
                      <div class="flex items-center gap-2 px-3 py-2">
                        <span class="shrink-0 rounded border px-1.5 py-0.5 text-[10px] font-medium {TURN_CHIP_CLASS[chipStatus] ?? 'bg-amber-500/20 text-amber-400 border-amber-500/30'}">
                          #{turn.idx}
                        </span>
                        <span class="min-w-0 flex-1 truncate text-xs text-muted-foreground">
                          {turn.user_input}
                        </span>
                        <span class="shrink-0 text-[10px] text-muted-foreground/60">
                          {turn.mode}
                        </span>
                        <div class="flex shrink-0 items-center gap-1">
                          {#each store.room.participants as p}
                            {@const resp = turn.responses.find((r) => r.participant_id === p.participant.id)}
                            {@const ps = resp?.status ?? "absent"}
                            <span
                              class="h-1.5 w-1.5 rounded-full {participantDotClass(ps)}"
                              title="{p.participant.label}: {ps}"
                            ></span>
                          {/each}
                        </div>
                      </div>
                      {#if turn.responses.length > 0}
                        <div class="border-t border-border px-3 py-2">
                          {#each turn.responses as response}
                            <div class="flex items-start gap-2 py-0.5 text-xs">
                              <span class="shrink-0 font-medium text-muted-foreground">
                                {participantLabelMap[response.participant_id] ?? response.participant_id}:
                              </span>
                              <span class={`shrink-0 rounded px-1 py-px text-[10px] ${statusClass(response.status)}`}>
                                {participantStatusLabel(response.status)}
                              </span>
                              {#if response.preview}
                                <span class="min-w-0 flex-1 truncate text-muted-foreground/70">
                                  {truncate(response.preview, 120)}
                                </span>
                              {/if}
                              {#if response.error}
                                <span class="text-destructive truncate">{response.error}</span>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
        <div class="shrink-0 border-t border-border p-3">
          {#if store.room.kind === "roundtable"}
            <div class="mb-2 flex flex-wrap items-center gap-2 text-xs">
              <span class="text-muted-foreground">
                {t("room_roundtableToolbarStatus", {
                  count: String(completedPublicTurnCount),
                })}
              </span>
              <button
                class="rounded-md border border-border px-2.5 py-1.5 hover:bg-accent disabled:opacity-50"
                disabled={!canDebate}
                title={t("room_debateTitle")}
                onclick={handleDebate}
              >
                {t("room_debate")}
              </button>
              <button
                class="rounded-md border border-border bg-amber-500/10 px-2.5 py-1.5 hover:bg-amber-500/15 disabled:opacity-50"
                disabled={!canSummary}
                title={t("room_summaryTitle")}
                onclick={handleSummary}
              >
                {t("room_summary")}
              </button>
              <label class="flex items-center gap-1 text-muted-foreground">
                <span>{t("room_summaryTarget")}</span>
                <select
                  class="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground disabled:opacity-50"
                  bind:value={summaryParticipantId}
                  disabled={store.saving || summaryParticipants.length === 0}
                >
                  {#each summaryParticipants as participant}
                    <option value={participant.participant.id}>{participant.participant.label}</option
                    >
                  {/each}
                </select>
              </label>
            </div>
          {/if}
          <div class="flex gap-2">
            <textarea
              class="min-h-12 flex-1 resize-none rounded-md border border-border bg-background px-2 py-1.5 text-sm"
              placeholder={t(roomComposerPlaceholderKey)}
              bind:value={roundtableMessage}
              onkeydown={(event) => {
                if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
                  event.preventDefault();
                  void handleSendRoundtableMessage();
                }
              }}
            ></textarea>
            <button
              class="w-24 rounded-md bg-primary px-3 py-1.5 text-sm text-primary-foreground disabled:opacity-50"
              disabled={store.saving || !canSendCurrentRoomMessage}
              onclick={handleSendRoundtableMessage}
            >
              {t("room_send")}
            </button>
          </div>
        </div>
      </div>
    {:else}
      <div class="flex h-full items-center justify-center">
        <div class="text-center">
          <p class="text-sm text-muted-foreground">{t("room_selectOrCreate")}</p>
          <button
            class="mt-3 rounded-md bg-primary px-3 py-2 text-sm text-primary-foreground"
            onclick={openCreateDialog}
          >
            {t("room_newRoundtable")}
          </button>
        </div>
      </div>
    {/if}
  </section>
</div>

{#if showCreateDialog}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-background/80 p-4 backdrop-blur-sm"
  >
    <div
      class="max-h-[92vh] w-full max-w-6xl overflow-y-auto rounded-md border border-border bg-background shadow-lg"
    >
      <div class="border-b border-border px-5 py-4">
        <div class="flex items-start justify-between gap-4">
          <div>
            <h2 class="text-lg font-semibold">{t("room_roundtableSetup")}</h2>
            <p class="mt-1 text-sm text-muted-foreground">{t("room_createStartsThree")}</p>
          </div>
          <button
            class="rounded-md border border-border px-2 py-1 text-xs hover:bg-accent"
            onclick={() => (showCreateDialog = false)}
          >
            {t("room_confirmNo")}
          </button>
        </div>
      </div>

      <div class="space-y-4 p-5">
        <div class="grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_220px]">
          <input
            class="rounded-md border border-border bg-background px-3 py-2 text-sm"
            placeholder={t("room_namePlaceholder")}
            bind:value={createName}
          />
          <input
            class="rounded-md border border-border bg-background px-3 py-2 text-sm"
            placeholder={t("room_descriptionPlaceholder")}
            bind:value={createDescription}
          />
          <button
            type="button"
            class="flex min-w-0 items-center justify-between gap-2 rounded-md border border-border bg-background px-3 py-2 text-left text-sm hover:bg-accent"
            onclick={pickCreateCwd}
            title={createCwd || t("room_selectProjectFolder")}
          >
            <span class={createCwd ? "min-w-0 truncate" : "min-w-0 truncate text-muted-foreground"}>
              {createCwd || t("room_projectPathUnset")}
            </span>
            <span class="shrink-0 text-xs text-muted-foreground">{t("common_browse")}</span>
          </button>
        </div>

        <div class="grid gap-3 xl:grid-cols-3">
          {#each seatForms as seat, index}
            <section class="rounded-md border border-border bg-card p-4">
              <div class="mb-3 flex items-center gap-2">
                <span
                  class="flex h-8 w-8 items-center justify-center rounded-md bg-muted text-sm font-semibold"
                  >{index + 1}</span
                >
                <h3 class="text-sm font-semibold">{t("room_seat")} {index + 1}</h3>
              </div>

              {#if ccProfiles.length > 0}
                <label class="mb-2 block text-xs font-medium text-muted-foreground">
                  {t("room_profile")}
                  <select
                    class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                    value={seat.profileId}
                    onchange={(event) => handleProfileChange(index, event.currentTarget.value)}
                  >
                    <option value="">{t("room_manualProfile")}</option>
                    {#each ccProfiles as profile}
                      <option value={profile.id}>{profileLabel(profile)}</option>
                    {/each}
                  </select>
                </label>
              {/if}

              <div class="grid grid-cols-2 gap-2">
                <label class="block text-xs font-medium text-muted-foreground">
                  {t("room_agent")}
                  <select
                    class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                    value={seat.agent}
                    onchange={(event) => {
                      const agent = event.currentTarget.value as SeatAgent;
                      const provider = getPhase7Provider(agent);
                      updateSeat(index, "agent", agent);
                      updateSeat(index, "connectionProfileId", "");
                      updateSeat(index, "platformId", provider.platformId ?? "");
                      updateSeat(index, "model", provider.defaultModel ?? "");
                      updateSeat(index, "label", defaultSeatLabel(index, agent));
                    }}
                  >
                    {#each PHASE7_PROVIDERS as provider}
                      <option value={provider.id}>{provider.label}</option>
                    {/each}
                  </select>
                </label>
                <label class="block text-xs font-medium text-muted-foreground">
                  {t("room_model")}
                  <input
                    class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                    value={seat.model}
                    placeholder="default"
                    oninput={(event) => updateSeat(index, "model", event.currentTarget.value)}
                  />
                </label>
              </div>

              {#if connectionProfilesForAgent(seat.agent).length > 0}
                <label class="mt-2 block text-xs font-medium text-muted-foreground">
                  Connection
                  <select
                    class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                    value={seat.connectionProfileId}
                    onchange={(event) =>
                      handleConnectionProfileChange(index, event.currentTarget.value)}
                  >
                    <option value="">Default connection</option>
                    {#each connectionProfilesForAgent(seat.agent) as profile}
                      <option value={profile.id}>{connectionProfileLabel(profile)}</option>
                    {/each}
                  </select>
                </label>
              {/if}

              <label class="mt-2 block text-xs font-medium text-muted-foreground">
                {t("room_seatLabel")}
                <input
                  class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                  value={seat.label}
                  oninput={(event) => updateSeat(index, "label", event.currentTarget.value)}
                />
              </label>

              <label class="mt-2 block text-xs font-medium text-muted-foreground">
                {t("room_slotPrompt")}
                <textarea
                  class="mt-1 min-h-28 w-full resize-y rounded-md border border-border bg-background px-2 py-1.5 text-sm"
                  value={seat.prompt}
                  oninput={(event) => updateSeat(index, "prompt", event.currentTarget.value)}
                ></textarea>
              </label>
            </section>
          {/each}
        </div>
      </div>

      <div class="flex items-center justify-between gap-3 border-t border-border px-5 py-4">
        <p class="text-xs text-muted-foreground">
          {createCwd ? createCwd : t("room_requiredProjectFolder")}
        </p>
        <button
          class="rounded-md bg-primary px-4 py-2 text-sm text-primary-foreground disabled:opacity-50"
          disabled={!createCwd.trim() || store.saving}
          onclick={handleCreateRoundtable}
        >
          {t("room_createRoundtable")}
        </button>
      </div>
    </div>
  </div>
{/if}
