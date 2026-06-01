<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import { fmtTime, fmtDateTime } from "$lib/i18n/format";
  import MarkdownContent from "./MarkdownContent.svelte";
  import FileAttachment from "./FileAttachment.svelte";
  import FilePathLinks from "./FilePathLinks.svelte";
  import { IMAGE_TYPES } from "$lib/utils/file-types";
  import type { ChatMessage, Attachment } from "$lib/types";

  let {
    message,
    attachments,
    thinkingText,
    assistantLabel,
    onRewind,
  }: {
    message: ChatMessage;
    attachments?: Attachment[];
    thinkingText?: string;
    assistantLabel?: string;
    onRewind?: () => void;
  } = $props();

  function isImage(att: Attachment): boolean {
    return (IMAGE_TYPES as readonly string[]).includes(att.type);
  }

  const isUser = $derived(message.role === "user");

  let hovered = $state(false);
  let copied = $state(false);
  let collapsed = $state(true);
  let thinkingCollapsed = $state(true);

  const lineCount = $derived(message.content.split("\n").length);
  const isLong = $derived(isUser && lineCount > 10);

  function formatTime(ts: string): string {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return "";
    const now = new Date();
    const isToday =
      d.getFullYear() === now.getFullYear() &&
      d.getMonth() === now.getMonth() &&
      d.getDate() === now.getDate();
    return isToday ? fmtTime(d) : fmtDateTime(d);
  }

  function formatFullTime(ts: string): string {
    return fmtDateTime(ts);
  }

  async function copyContent() {
    try {
      await navigator.clipboard.writeText(message.content);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Silently fail
    }
  }
</script>

<div
  class="msg-entry w-full"
  role="group"
  onmouseenter={() => (hovered = true)}
  onmouseleave={() => (hovered = false)}
>
  <div class="chat-content-width py-4">
    <!-- Header: avatar + name + copy button + timestamp -->
    <div class="mb-1.5 flex items-center gap-3">
      {#if isUser}
        <div class="msg-avatar user-avatar">
          {(t("chat_roleYou") || "Y").charAt(0)}
        </div>
        <span class="text-sm font-semibold text-foreground">{t("chat_roleYou")}</span>
      {:else}
        <div class="msg-avatar ai-avatar">&#10022;</div>
        <span class="text-sm font-semibold text-foreground">
          {assistantLabel || t("chat_roleClaude")}
        </span>
      {/if}
      {#if onRewind}
        <button
          class="ml-auto p-1 rounded-md text-muted-foreground/50 hover:bg-muted hover:text-foreground transition-all duration-150 {hovered
            ? 'opacity-100'
            : 'opacity-0'}"
          onclick={onRewind}
          title={t("rewind_toHere")}
          data-export-exclude
        >
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" />
            <path d="M3 3v5h5" />
          </svg>
        </button>
      {/if}
      <button
        class="{onRewind
          ? ''
          : 'ml-auto'} p-1 rounded-md text-muted-foreground/50 hover:bg-muted hover:text-foreground transition-all duration-150 {hovered ||
        copied
          ? 'opacity-100'
          : 'opacity-0'}"
        onclick={copyContent}
        title={t("chat_copyMessage")}
        data-export-exclude
      >
        {#if copied}
          <svg
            class="h-3.5 w-3.5 text-emerald-500"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg
          >
        {:else}
          <svg
            class="h-3.5 w-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            ><rect width="14" height="14" x="8" y="8" rx="2" /><path
              d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
            /></svg
          >
        {/if}
      </button>
      <span class="text-[10px] text-muted-foreground" title={formatFullTime(message.timestamp)}>
        {formatTime(message.timestamp)}
      </span>
    </div>
    <!-- Content: indented to align with text after icon -->
    <div class="pl-7 text-sm text-foreground leading-relaxed">
      {#if isUser}
        {#if attachments && attachments.length > 0}
          <div class="flex flex-wrap gap-2 mb-2">
            {#each attachments as att}
              {#if isImage(att) && att.contentBase64}
                <img
                  src="data:{att.type};base64,{att.contentBase64}"
                  alt={att.name}
                  class="max-h-48 max-w-xs rounded-md border border-border object-contain"
                />
              {:else}
                <FileAttachment name={att.name} size={att.size} mimeType={att.type} />
              {/if}
            {/each}
          </div>
        {/if}
        {#if isLong}
          <p
            class="whitespace-pre-wrap {collapsed ? 'max-h-24 overflow-hidden' : ''}"
            style={collapsed
              ? "mask-image: linear-gradient(to bottom, black 70%, transparent);"
              : ""}
          >
            <FilePathLinks text={message.content} />
          </p>
          <button
            class="mt-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
            onclick={() => (collapsed = !collapsed)}
          >
            {collapsed
              ? t("common_showAllLines", { count: String(lineCount) })
              : t("common_collapse")}
          </button>
        {:else}
          <p class="whitespace-pre-wrap"><FilePathLinks text={message.content} /></p>
        {/if}
      {:else}
        {#if thinkingText}
          <button
            class="mb-2 flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-400 transition-colors"
            onclick={() => (thinkingCollapsed = !thinkingCollapsed)}
          >
            <svg
              class="h-3 w-3 transition-transform {thinkingCollapsed ? '' : 'rotate-90'}"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"><path d="m9 18 6-6-6-6" /></svg
            >
            {t("chat_thoughtProcess")}
          </button>
          {#if !thinkingCollapsed}
            <div
              class="mb-3 rounded-md border border-blue-500/20 bg-blue-500/5 px-3 py-2 text-xs text-blue-300/80 whitespace-pre-wrap leading-relaxed"
            >
              {thinkingText.trimEnd()}
            </div>
          {/if}
        {/if}
        <div class="prose-chat">
          <MarkdownContent text={message.content} />
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .msg-entry {
    animation: fadeInUp var(--duration-normal, 200ms) var(--ease-out, ease-out);
  }

  .msg-avatar {
    width: 30px;
    height: 30px;
    border-radius: var(--radius-md, 8px);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .msg-avatar.user-avatar {
    background: var(--accent-muted, hsl(38 46% 62% / 0.15));
    color: var(--accent-color, hsl(38 46% 62%));
  }
  .msg-avatar.ai-avatar {
    background: rgba(201, 169, 110, 0.12);
    color: var(--accent-color, hsl(38 46% 62%));
  }
</style>
