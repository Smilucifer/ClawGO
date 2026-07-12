<script lang="ts">
  import type { FileEntry } from "$lib/types";
  import { splitPath } from "$lib/utils/format";
  import { t } from "$lib/i18n/index.svelte";
  let {
    fileEntries = [],
    onScrollToTool,
  }: {
    fileEntries: FileEntry[];
    onScrollToTool?: (toolUseId: string) => void;
  } = $props();

  // ── Tree building ──────────────────────────────────────────────
  interface TreeNode {
    name: string;
    path: string;
    isDir: boolean;
    children: TreeNode[];
    entry?: FileEntry; // only for leaf files
  }

  let expandedDirs = $state(new Set<string>());
  function buildTree(entries: FileEntry[]): TreeNode[] {
    const root: TreeNode[] = [];
    const dirMap = new Map<string, TreeNode>();

    // Sort: directories first, then by name
    const sorted = [...entries].sort((a, b) => a.path.localeCompare(b.path));

    for (const entry of sorted) {
      const parts = splitPath(entry.path);
      let currentChildren = root;

      // Walk/create directory nodes
      for (let i = 0; i < parts.length - 1; i++) {
        const dirPath = parts.slice(0, i + 1).join("/");
        if (!dirMap.has(dirPath)) {
          const dirNode: TreeNode = {
            name: parts[i],
            path: dirPath,
            isDir: true,
            children: [],
          };
          dirMap.set(dirPath, dirNode);
          currentChildren.push(dirNode);
        }
        currentChildren = dirMap.get(dirPath)!.children;
      }

      // Add file leaf
      currentChildren.push({
        name: parts[parts.length - 1],
        path: entry.path,
        isDir: false,
        children: [],
        entry,
      });
    }

    // Sort each level: dirs first, then files, alpha within each
    function sortNodes(nodes: TreeNode[]): void {
      nodes.sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      for (const n of nodes) {
        if (n.isDir) sortNodes(n.children);
      }
    }
    sortNodes(root);

    return root;
  }

  function toggleDir(dirPath: string) {
    if (expandedDirs.has(dirPath)) {
      expandedDirs.delete(dirPath);
    } else {
      expandedDirs.add(dirPath);
    }
    // Force reactivity
    expandedDirs = new Set(expandedDirs);
  }

  // ── File type icons ────────────────────────────────────────────
  function fileIcon(name: string): string {
    const ext = name.includes(".") ? name.split(".").pop()?.toLowerCase() : "";
    switch (ext) {
      case "ts":
      case "tsx":
        return "TS";
      case "js":
      case "jsx":
        return "JS";
      case "rs":
        return "RS";
      case "svelte":
        return "Sv";
      case "json":
        return "{}";
      case "md":
        return "Md";
      case "html":
      case "htm":
        return "<>";
      case "css":
      case "scss":
      case "less":
        return "Cs";
      case "py":
        return "Py";
      case "toml":
      case "yaml":
      case "yml":
        return "Cfg";
      case "png":
      case "jpg":
      case "jpeg":
      case "gif":
      case "svg":
      case "webp":
        return "Img";
      default:
        return "..";
    }
  }

  function fileIconColor(name: string): string {
    const ext = name.includes(".") ? name.split(".").pop()?.toLowerCase() : "";
    switch (ext) {
      case "ts":
      case "tsx":
        return "bg-blue-500/15 text-blue-600 dark:text-blue-400";
      case "js":
      case "jsx":
        return "bg-yellow-500/15 text-yellow-600 dark:text-yellow-400";
      case "rs":
        return "bg-orange-500/15 text-orange-600 dark:text-orange-400";
      case "svelte":
        return "bg-rose-500/15 text-rose-600 dark:text-rose-400";
      case "json":
        return "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400";
      case "md":
        return "bg-slate-500/15 text-slate-600 dark:text-slate-400";
      case "css":
      case "scss":
      case "less":
        return "bg-pink-500/15 text-pink-600 dark:text-pink-400";
      default:
        return "bg-muted text-muted-foreground";
    }
  }

  // ── Action badge ───────────────────────────────────────────────
  function actionColor(action: FileEntry["action"]): { bg: string; text: string } {
    switch (action) {
      case "write":
        return { bg: "bg-amber-500/15", text: "text-amber-600 dark:text-amber-400" };
      case "edit":
        return { bg: "bg-amber-500/15", text: "text-amber-600 dark:text-amber-400" };
      case "read":
        return { bg: "bg-blue-500/15", text: "text-blue-600 dark:text-blue-400" };
      case "persisted":
        return { bg: "bg-emerald-500/15", text: "text-emerald-600 dark:text-emerald-400" };
    }
  }

  function actionLabel(action: FileEntry["action"]): string {
    switch (action) {
      case "write":
        return "W";
      case "edit":
        return "E";
      case "read":
        return "R";
      case "persisted":
        return "P";
    }
  }

  // ── Preview ────────────────────────────────────────────────────
  function previewFile(entry: FileEntry) {
    window.dispatchEvent(new CustomEvent("clawgo:preview-file", {
      detail: { filepath: entry.path },
    }));
    onScrollToTool?.(entry.toolUseId!);
  }

  // ── Filter by action ───────────────────────────────────────────
  let activeFilter = $state<FileEntry["action"] | null>(null);

  let filteredEntries = $derived(
    activeFilter
      ? fileEntries.filter((e) => e.action === activeFilter)
      : fileEntries,
  );

  let filteredTree = $derived(buildTree(filteredEntries));

  function setFilter(action: FileEntry["action"] | null) {
    activeFilter = activeFilter === action ? null : action;
  }

  // Count by action
  let counts = $derived.by(() => {
    const c = { write: 0, edit: 0, read: 0, persisted: 0 };
    for (const e of fileEntries) c[e.action]++;
    return c;
  });
</script>

<div class="flex flex-col h-full">
  <!-- Filter bar -->
  {#if fileEntries.length > 0}
    <div class="flex items-center gap-1 px-2 py-1 border-b shrink-0">
      {#each (["write", "edit", "read", "persisted"] as const) as action}
        {@const color = actionColor(action)}
        {@const isActive = activeFilter === action}
        <button
          class="inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[10px] font-bold transition-colors
            {isActive ? `${color.bg} ${color.text}` : 'text-muted-foreground hover:bg-accent/50'}"
          onclick={() => setFilter(action)}
          title={action}
        >
          {actionLabel(action)}
          {#if counts[action] > 0}
            <span class="text-[9px] opacity-60">{counts[action]}</span>
          {/if}
        </button>
      {/each}
      {#if activeFilter}
        <button
          class="ml-auto text-[10px] text-muted-foreground hover:text-foreground"
          onclick={() => setFilter(null)}
        >
          {t("filesPanel_clearFilter")}
        </button>
      {/if}
    </div>
  {/if}

  <!-- Tree / empty state -->
  <div class="flex-1 overflow-y-auto py-1">
    {#if fileEntries.length === 0}
      <div class="flex items-center justify-center h-32 text-xs text-muted-foreground/50">
        {t("filesPanel_noFiles")}
      </div>
    {:else}
      {#each filteredTree as node (node.path)}
        {@render renderNode(node, 0)}
      {/each}
    {/if}
  </div>

</div>

<!-- Tree node render fragment -->
{#snippet renderNode(node: TreeNode, depth: number)}
  {#if node.isDir}
    {@const isExpanded = expandedDirs.has(node.path)}
    <button
      class="w-full text-left px-1.5 py-0.5 hover:bg-accent/50 rounded-sm transition-colors flex items-center gap-1"
      style="padding-left: {depth * 12 + 4}px"
      onclick={() => toggleDir(node.path)}
    >
      <!-- Chevron -->
      <svg
        class="h-3 w-3 shrink-0 text-muted-foreground transition-transform {isExpanded ? 'rotate-90' : ''}"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="m9 18 6-6-6-6" />
      </svg>
      <!-- Folder icon -->
      <svg
        class="h-3.5 w-3.5 shrink-0 {isExpanded ? 'text-amber-500' : 'text-muted-foreground'}"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path
          d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
        />
      </svg>
      <span class="text-[11px] font-medium truncate">{node.name}</span>
      <span class="text-[10px] text-muted-foreground/50 ml-auto">{node.children.length}</span>
    </button>
    {#if isExpanded}
      {#each node.children as child (child.path)}
        {@render renderNode(child, depth + 1)}
      {/each}
    {/if}
  {:else if node.entry}
    {@const entry = node.entry}
    {@const color = actionColor(entry.action)}
    {@const iconColor = fileIconColor(node.name)}
    {@const canJump = !!entry.toolUseId}
    {#if canJump}
      <button
        class="w-full text-left px-1.5 py-0.5 hover:bg-accent/50 rounded-sm transition-colors group flex items-center gap-1.5"
        style="padding-left: {depth * 12 + 16}px"
        onclick={() => previewFile(entry)}
        title={entry.path}
      >
        <span
          class="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded text-[8px] font-bold {iconColor}"
        >
          {fileIcon(node.name)}
        </span>
        <span class="text-[11px] text-foreground truncate min-w-0 group-hover:underline"
          >{node.name}</span
        >
        <span
          class="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded text-[9px] font-bold {color.bg} {color.text} ml-auto"
        >
          {actionLabel(entry.action)}
        </span>
      </button>
    {:else}
      <div
        class="flex items-center gap-1.5 px-1.5 py-0.5 cursor-default"
        style="padding-left: {depth * 12 + 16}px"
      >
        <span
          class="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded text-[8px] font-bold {iconColor}"
        >
          {fileIcon(node.name)}
        </span>
        <span class="text-[11px] text-muted-foreground truncate min-w-0">{node.name}</span>
        <span
          class="inline-flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded text-[9px] font-bold {color.bg} {color.text} ml-auto"
        >
          {actionLabel(entry.action)}
        </span>
        <svg
          class="h-3 w-3 shrink-0 text-muted-foreground/30"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <title>{t("filesPanel_notLocatable")}</title>
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
          <line x1="8" y1="11" x2="14" y2="11" />
        </svg>
      </div>
    {/if}
  {/if}
{/snippet}
