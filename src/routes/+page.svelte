<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";
  import { listRuns } from "$lib/api";
  import type { TaskRun } from "$lib/types";
  import { relativeTime, truncate } from "$lib/utils/format";

  let recentRuns = $state<TaskRun[]>([]);
  let loading = $state(true);

  onMount(async () => {
    try {
      const runs = await listRuns();
      // Sort by last_activity_at descending, take last 5
      recentRuns = runs
        .sort((a, b) => {
          const ta = a.last_activity_at || a.started_at || "";
          const tb = b.last_activity_at || b.started_at || "";
          return tb.localeCompare(ta);
        })
        .slice(0, 5);
    } catch {
      // Ignore errors — just show empty state
    } finally {
      loading = false;
    }
  });

  const featureCards = [
    {
      icon: "💬",
      name: "Chat",
      desc: "AI 对话、代码协作、多模型切换",
      route: "/chat",
      bg: "var(--accent-muted)",
      color: "var(--accent)",
    },
    {
      icon: "📈",
      name: "Invest",
      desc: "投资组合管理、委员会分析、交易记录",
      route: "/invest",
      bg: "rgba(138, 154, 118, 0.12)",
      color: "#8a9a76",
    },
    {
      icon: "📁",
      name: "Explorer",
      desc: "文件浏览、代码查看、Git 状态",
      route: "/explorer",
      bg: "rgba(201, 169, 110, 0.08)",
      color: "var(--text-secondary)",
    },
    {
      icon: "🧠",
      name: "Memory",
      desc: "Claude 记忆管理、知识库检索",
      route: "/memory",
      bg: "rgba(168, 122, 122, 0.12)",
      color: "#a87a7a",
    },
    {
      icon: "📚",
      name: "User Memory",
      desc: "用户记忆管理、领域知识库",
      route: "/memory-mgmt",
      bg: "rgba(201, 169, 110, 0.08)",
      color: "var(--text-secondary)",
    },
    {
      icon: "⚡",
      name: "Plugins",
      desc: "Skills、MCP、Hooks、插件管理",
      route: "/plugins",
      bg: "rgba(138, 154, 118, 0.12)",
      color: "#8a9a76",
    },
    {
      icon: "🕐",
      name: "History",
      desc: "会话历史、CLI Session 浏览",
      route: "/history",
      bg: "rgba(168, 122, 122, 0.12)",
      color: "#a87a7a",
    },
    {
      icon: "📊",
      name: "Usage",
      desc: "Token 用量统计、费用追踪",
      route: "/usage",
      bg: "rgba(201, 169, 110, 0.08)",
      color: "var(--text-secondary)",
    },
    {
      icon: "⚙️",
      name: "Settings",
      desc: "Provider 配置、API Key、角色管理",
      route: "/settings",
      bg: "var(--accent-muted)",
      color: "var(--accent)",
    },
  ];
</script>

<div class="index-page">
  <!-- Hero -->
  <div class="hero">
    <div class="hero-logo">
      <img src="/logo.png?v=2" alt="ClawGO" class="hero-logo-img" />
    </div>
    <h1 class="hero-title">ClawGO</h1>
    <p class="hero-subtitle">AI-native desktop workspace — chat, code, invest, remember</p>
  </div>

  <!-- Quick Actions -->
  <div class="quick-actions">
    <button class="action-btn primary" onclick={() => goto("/chat")}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/></svg>
      <span>新建对话</span>
    </button>
    <button class="action-btn" onclick={() => goto("/chat?group=new")}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><line x1="19" y1="8" x2="19" y2="14"/><line x1="22" y1="11" x2="16" y2="11"/></svg>
      <span>新建群聊</span>
    </button>
    <button class="action-btn" onclick={() => goto("/history")}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>
      <span>浏览历史</span>
    </button>
  </div>

  <!-- Recent Conversations -->
  {#if recentRuns.length > 0}
    <div class="section">
      <h2 class="section-title">最近对话</h2>
      <div class="recent-list">
        {#each recentRuns as run}
          <button class="recent-item" onclick={() => goto(`/chat?id=${run.id}`)}>
            <div class="recent-icon">
              {#if run.agent === "group-chat"}
                <span style="color: var(--accent);">✦</span>
              {:else}
                <span>💬</span>
              {/if}
            </div>
            <div class="recent-info">
              <div class="recent-name">{truncate(run.name || run.prompt || run.id, 40)}</div>
              <div class="recent-preview">{truncate(run.cwd || "", 60)}</div>
            </div>
            <div class="recent-time">{relativeTime(run.last_activity_at || run.started_at || "")}</div>
          </button>
        {/each}
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="loading-area">
      <div class="mini-spinner"></div>
    </div>
  {/if}

  <!-- Feature Grid -->
  <div class="section">
    <h2 class="section-title">功能入口</h2>
    <div class="feature-grid">
      {#each featureCards as card}
        <button class="feature-card" onclick={() => goto(card.route)}>
          <div class="feature-icon" style="background: {card.bg}; color: {card.color};">
            {card.icon}
          </div>
          <div class="feature-name">{card.name}</div>
          <div class="feature-desc">{card.desc}</div>
        </button>
      {/each}
    </div>
  </div>
</div>

<style>
  .index-page {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-8) var(--space-6);
    max-width: 960px;
    margin: 0 auto;
    width: 100%;
  }

  /* Hero */
  .hero {
    text-align: center;
    margin-bottom: var(--space-8);
    padding-top: var(--space-4);
  }
  .hero-logo-img {
    width: 72px;
    height: 72px;
    border-radius: var(--radius-xl);
    box-shadow: 0 0 40px rgba(201, 169, 110, 0.15);
    margin-bottom: var(--space-4);
  }
  .hero-title {
    font-size: 32px;
    font-weight: 700;
    color: var(--text-primary);
    margin-bottom: var(--space-2);
    letter-spacing: -0.02em;
  }
  .hero-subtitle {
    font-size: 15px;
    color: var(--text-tertiary);
    line-height: 1.5;
  }

  /* Quick Actions */
  .quick-actions {
    display: flex;
    gap: var(--space-3);
    justify-content: center;
    margin-bottom: var(--space-8);
    flex-wrap: wrap;
  }
  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-5);
    border-radius: var(--radius-full);
    font-size: 14px;
    font-weight: 500;
    color: var(--text-secondary);
    background: var(--bg-card);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all var(--duration-fast) var(--ease-out);
  }
  .action-btn:hover {
    border-color: var(--border-strong);
    color: var(--text-primary);
    transform: translateY(-1px);
  }
  .action-btn.primary {
    background: var(--accent-muted);
    color: var(--accent);
    border-color: var(--border-accent);
  }
  .action-btn.primary:hover {
    background: rgba(201, 169, 110, 0.25);
  }
  .action-btn svg {
    width: 18px;
    height: 18px;
  }

  /* Sections */
  .section {
    margin-bottom: var(--space-8);
  }
  .section-title {
    font-size: 13px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-tertiary);
    margin-bottom: var(--space-4);
  }

  /* Recent List */
  .recent-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .recent-item {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-lg);
    background: var(--bg-card);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all var(--duration-fast);
    text-align: left;
    width: 100%;
    font-family: inherit;
  }
  .recent-item:hover {
    border-color: var(--border-strong);
    background: var(--bg-hover);
  }
  .recent-icon {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--bg-input);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    flex-shrink: 0;
  }
  .recent-info {
    flex: 1;
    min-width: 0;
  }
  .recent-name {
    font-size: 14px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .recent-preview {
    font-size: 12px;
    color: var(--text-tertiary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    margin-top: 2px;
  }
  .recent-time {
    font-size: 11px;
    color: var(--text-tertiary);
    flex-shrink: 0;
  }

  /* Loading */
  .loading-area {
    display: flex;
    justify-content: center;
    padding: var(--space-6);
  }
  .mini-spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  /* Feature Grid */
  .feature-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-3);
  }
  @media (max-width: 768px) {
    .feature-grid { grid-template-columns: repeat(2, 1fr); }
  }
  @media (max-width: 480px) {
    .feature-grid { grid-template-columns: 1fr; }
  }
  .feature-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-4) var(--space-5);
    border-radius: var(--radius-xl);
    background: var(--bg-card);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all var(--duration-fast) var(--ease-out);
    text-align: left;
    font-family: inherit;
  }
  .feature-card:hover {
    border-color: var(--border-accent);
    transform: translateY(-2px);
  }
  .feature-icon {
    width: 44px;
    height: 44px;
    border-radius: var(--radius-lg);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 20px;
    margin-bottom: var(--space-1);
  }
  .feature-name {
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .feature-desc {
    font-size: 12px;
    color: var(--text-tertiary);
    line-height: 1.5;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
