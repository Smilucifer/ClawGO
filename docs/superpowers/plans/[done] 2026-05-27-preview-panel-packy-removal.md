# Preview Panel + Packy CX2CC Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Packy CX2CC provider entirely and build a right-side preview panel with Monaco editor, Markdown/HTML/Image/PDF/Office rendering, replacing the inline FilesPanel preview.

**Architecture:** Packy removal touches ~10 files across frontend/backend/i18n/docs. Preview panel adds a new `preview-store.svelte.ts`, 8 sub-components under `src/lib/components/preview/`, a `PreviewResizer`, and integrates into the chat page via sidebar auto-hide. Uses existing `readTextFile`/`writeTextFile` Tauri commands (no new backend required).

**Tech Stack:** Svelte 5 runes, Monaco Editor, marked.js, highlight.js, mammoth.js, SheetJS (xlsx), Tauri IPC

---

## Part 1: Packy CX2CC Complete Removal

### Task 1.1: Remove Packy from frontend provider catalog

**Files:**
- Modify: `src/lib/utils/provider-catalog.ts`
- Modify: `src/lib/utils/platform-presets.ts`

- [ ] **Step 1: Remove from provider-catalog.ts**

In `src/lib/utils/provider-catalog.ts`:

1. Remove `"packy-cx2cc"` from `Phase7ProviderId` union type (line 12):
```typescript
export type Phase7ProviderId =
  | "claude"
  | "codex"
  | "deepseek"
  | "glm"
  | "qwen"
  | "kimi"
  | "mimo-plan"
  | "mimo-api";
```

2. Remove the Packy entry from `PHASE7_PROVIDERS` array (lines 120-130):
```typescript
// Remove this entire block:
  {
    id: "packy-cx2cc",
    label: "Packy CX2CC",
    mode: "claude_compatible_api",
    executionAgent: "claude",
    platformId: "packy-cx2cc",
    defaultBaseUrl: "https://www.packyapi.com/anthropic",
    contextWindow: 1_000_000,
    requiredConfig: ["api_key"],
    defaultPermissionMode: "bypass",
  },
```

3. Remove the packy line from `providerIdForRun()` function (line 144):
```typescript
// Remove this line:
  if (platformId === "packy-cx2cc") return "packy-cx2cc";
```

- [ ] **Step 2: Remove from platform-presets.ts**

In `src/lib/utils/platform-presets.ts`, remove the Packy entry (lines 90-98):
```typescript
// Remove this entire block:
  {
    id: "packy-cx2cc",
    name: "Packy CX2CC",
    base_url: "https://www.packyapi.com/anthropic",
    auth_env_var: "ANTHROPIC_AUTH_TOKEN",
    description: "Packy CX2CC API",
    key_placeholder: "your-packy-cx2cc-key",
    category: "provider",
  },
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/utils/provider-catalog.ts src/lib/utils/platform-presets.ts
git commit -m "chore: remove Packy CX2CC from frontend provider catalog and platform presets"
```

---

### Task 1.2: Remove Packy from Rust backend

**Files:**
- Modify: `src-tauri/src/agent/provider_claude_config.rs`
- Modify: `src-tauri/src/commands/balance.rs`
- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/storage/settings.rs`
- Modify: `src-tauri/src/commands/onboarding.rs`

- [ ] **Step 1: Read backend files to locate exact Packy code**

Read each file to find Packy references:
```bash
grep -n -i "packy\|cx2cc" src-tauri/src/agent/provider_claude_config.rs src-tauri/src/commands/balance.rs src-tauri/src/commands/session.rs src-tauri/src/models.rs src-tauri/src/storage/settings.rs src-tauri/src/commands/onboarding.rs
```

- [ ] **Step 2: Remove Packy from provider_claude_config.rs**

Remove the `"packy-cx2cc"` arm from the `platform_id` match statement that resolves provider configuration.

- [ ] **Step 3: Remove Packy from balance.rs**

Remove the Packy balance query logic (packySession, packyTdcItoken, packyUserId related code).

- [ ] **Step 4: Remove Packy from session.rs**

Remove the Packy-specific launch config template builder.

- [ ] **Step 5: Remove Packy from models.rs, settings.rs, onboarding.rs**

Remove any Packy-related enum variants, struct fields, or default values.

- [ ] **Step 6: Run cargo check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```
Expected: PASS (no errors)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/
git commit -m "chore: remove Packy CX2CC from Rust backend"
```

---

### Task 1.3: Remove Packy from i18n and documentation

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `docs/changelog.md`

- [ ] **Step 1: Remove from messages/en.json**

Remove or rewrite lines containing "Packy":
- Line 1198: `"settings_balance_desc"` — remove "Packy console" from the description
- Lines 1202-1204: Remove `settings_balance_packySession`, `settings_balance_packyTdcItoken`, `settings_balance_packyUserId`

- [ ] **Step 2: Remove from messages/zh-CN.json**

Same as above, Chinese translations:
- Line 1198: `"settings_balance_desc"` — remove "Packy 控制台" from the description
- Lines 1202-1204: Remove the three Packy balance strings

- [ ] **Step 3: Remove from CLAUDE.md**

Remove all Packy CX2CC references from the provider documentation section.

- [ ] **Step 4: Remove from README.md**

Remove Packy CX2CC from the provider list and agent parameter documentation.

- [ ] **Step 5: Remove from docs/changelog.md**

Remove Packy references from changelog entries (keep entries, just strip Packy mentions).

- [ ] **Step 6: Run i18n check**

```bash
npm run i18n:check
```
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add messages/ CLAUDE.md README.md docs/changelog.md
git commit -m "chore: remove Packy CX2CC from i18n and documentation"
```

---

## Part 2: Preview Panel

### Task 2.1: Install npm dependencies

- [ ] **Step 1: Install Monaco Editor and related packages**

```bash
npm install monaco-editor @monaco-editor/svelte marked highlight.js mammoth xlsx
```

- [ ] **Step 2: Verify installation**

```bash
node -e "require('monaco-editor'); require('marked'); require('highlight.js'); require('mammoth'); require('xlsx'); console.log('All packages installed')"
```
Expected: "All packages installed"

- [ ] **Step 3: Commit**

```bash
git add package.json package-lock.json
git commit -m "chore: add preview panel dependencies (monaco, marked, highlight.js, mammoth, xlsx)"
```

---

### Task 2.2: Create preview-store.svelte.ts

**Files:**
- Create: `src/lib/stores/preview-store.svelte.ts`

- [ ] **Step 1: Write the preview store**

```typescript
// src/lib/stores/preview-store.svelte.ts
import { readTextFile, writeTextFile } from "$lib/api";

export interface PreviewState {
  isOpen: boolean;
  filepath: string | null;
  content: string | null;
  loading: boolean;
  error: string | null;
  dirty: boolean; // true when edited content differs from saved
  language: string; // for Monaco editor language detection
  fileType: PreviewFileType;
}

export type PreviewFileType =
  | "code"
  | "markdown"
  | "html"
  | "image"
  | "pdf"
  | "word"
  | "excel"
  | "other";

const CODE_EXTS = new Set([
  "ts", "tsx", "js", "jsx", "rs", "py", "svelte", "css", "scss", "less",
  "json", "toml", "yaml", "yml", "xml", "sql", "sh", "bash", "ps1",
  "go", "java", "c", "cpp", "h", "hpp", "rb", "php", "swift", "kt",
]);
const MARKDOWN_EXTS = new Set(["md", "mdx"]);
const HTML_EXTS = new Set(["htm", "html"]);
const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "ico"]);
const WORD_EXTS = new Set(["doc", "docx"]);
const EXCEL_EXTS = new Set(["xls", "xlsx"]);

const LANG_MAP: Record<string, string> = {
  ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript",
  rs: "rust", py: "python", svelte: "html", css: "css", scss: "scss",
  less: "less", json: "json", toml: "ini", yaml: "yaml", yml: "yaml",
  xml: "xml", sql: "sql", sh: "shell", bash: "shell", ps1: "powershell",
  go: "go", java: "java", c: "c", cpp: "cpp", h: "c", hpp: "cpp",
  rb: "ruby", php: "php", swift: "swift", kt: "kotlin",
};

export function detectFileType(filepath: string): PreviewFileType {
  const ext = filepath.split(".").pop()?.toLowerCase() ?? "";
  if (CODE_EXTS.has(ext)) return "code";
  if (MARKDOWN_EXTS.has(ext)) return "markdown";
  if (HTML_EXTS.has(ext)) return "html";
  if (IMAGE_EXTS.has(ext)) return "image";
  if (ext === "pdf") return "pdf";
  if (WORD_EXTS.has(ext)) return "word";
  if (EXCEL_EXTS.has(ext)) return "excel";
  return "other";
}

export function detectLanguage(filepath: string): string {
  const ext = filepath.split(".").pop()?.toLowerCase() ?? "";
  return LANG_MAP[ext] ?? "plaintext";
}

export function createPreviewStore() {
  let _state = $state<PreviewState>({
    isOpen: false,
    filepath: null,
    content: null,
    loading: false,
    error: null,
    dirty: false,
    language: "plaintext",
    fileType: "other",
  });

  function open(filepath: string, cwd: string) {
    // Toggle: close if same file
    if (_state.isOpen && _state.filepath === filepath) {
      close();
      return;
    }
    const ft = detectFileType(filepath);
    const lang = detectLanguage(filepath);
    _state = {
      isOpen: true,
      filepath,
      content: null,
      loading: true,
      error: null,
      dirty: false,
      language: lang,
      fileType: ft,
    };
    loadContent(filepath, cwd);
  }

  async function loadContent(filepath: string, cwd: string) {
    try {
      const content = await readTextFile(filepath, cwd);
      if (_state.filepath !== filepath) return; // stale
      _state = { ..._state, content, loading: false, error: null };
    } catch (e) {
      if (_state.filepath !== filepath) return;
      _state = { ..._state, content: null, loading: false, error: String(e) };
    }
  }

  function close() {
    _state = {
      isOpen: false,
      filepath: null,
      content: null,
      loading: false,
      error: null,
      dirty: false,
      language: "plaintext",
      fileType: "other",
    };
  }

  function updateContent(newContent: string) {
    _state = { ..._state, content: newContent, dirty: true };
  }

  async function save(cwd: string): Promise<boolean> {
    if (!_state.filepath || _state.content === null) return false;
    try {
      await writeTextFile(_state.filepath, _state.content, cwd);
      _state = { ..._state, dirty: false, error: null };
      window.dispatchEvent(new CustomEvent("clawgo:preview-saved", { detail: { filepath: _state.filepath } }));
      return true;
    } catch (e) {
      _state = { ..._state, error: String(e) };
      return false;
    }
  }

  return {
    get state() { return _state; },
    open,
    close,
    updateContent,
    save,
  };
}
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/stores/preview-store.svelte.ts
git commit -m "feat: add preview-store for file preview state management"
```

---

### Task 2.3: Create PreviewResizer component

**Files:**
- Create: `src/lib/components/preview/PreviewResizer.svelte`

- [ ] **Step 1: Write PreviewResizer.svelte**

```svelte
<script lang="ts">
  let {
    width = 600,
    onResize = (_w: number) => {},
    minChatWidth = 400,
    minPreviewWidth = 320,
  }: {
    width: number;
    onResize?: (width: number) => void;
    minChatWidth?: number;
    minPreviewWidth?: number;
  } = $props();

  let dragging = $state(false);
  let startX = 0;
  let startWidth = 0;

  function onPointerDown(e: PointerEvent) {
    e.preventDefault();
    dragging = true;
    startX = e.clientX;
    startWidth = width;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const dx = startX - e.clientX;
    const containerWidth = window.innerWidth;
    const maxPreview = containerWidth - minChatWidth;
    const newWidth = Math.min(maxPreview, Math.max(minPreviewWidth, startWidth + dx));
    onResize(newWidth);
  }

  function onPointerUp(_e: PointerEvent) {
    dragging = false;
  }

  function onDoubleClick() {
    const half = Math.floor(window.innerWidth / 2);
    onResize(Math.max(minPreviewWidth, Math.min(half, window.innerWidth - minChatWidth)));
  }
</script>

<div
  class="relative w-1.5 cursor-col-resize flex-shrink-0 group hover:bg-primary/30 transition-colors {dragging ? 'bg-primary/50' : 'bg-border/50'}"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={onDoubleClick}
  role="separator"
  aria-orientation="vertical"
  aria-valuenow={width}
  tabindex="-1"
>
  <div class="absolute inset-y-0 -left-1 -right-1"></div>
  <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity">
    <div class="w-1 h-6 rounded-full bg-primary/60"></div>
  </div>
</div>
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/components/preview/PreviewResizer.svelte
git commit -m "feat: add PreviewResizer drag handle component"
```

---

### Task 2.4: Create preview sub-components

**Files:**
- Create: `src/lib/components/preview/PreviewPanel.svelte`
- Create: `src/lib/components/preview/MonacoEditor.svelte`
- Create: `src/lib/components/preview/MarkdownPreview.svelte`
- Create: `src/lib/components/preview/HtmlPreview.svelte`
- Create: `src/lib/components/preview/ImagePreview.svelte`
- Create: `src/lib/components/preview/PdfPreview.svelte`
- Create: `src/lib/components/preview/OfficePreview.svelte`

- [ ] **Step 1: Write MonacoEditor.svelte**

```svelte
<script lang="ts">
  import { onMount } from "svelte";

  let {
    content = "",
    language = "plaintext",
    onChange = (_value: string) => {},
    theme = "vs-dark",
  }: {
    content: string;
    language: string;
    onChange?: (value: string) => void;
    theme?: string;
  } = $props();

  let containerRef: HTMLDivElement | undefined = $state();
  let editor: import("monaco-editor").editor.IStandaloneCodeEditor | undefined;
  let disposed = false;

  onMount(async () => {
    const monaco = await import("monaco-editor");
    if (!containerRef || disposed) return;

    editor = monaco.editor.create(containerRef, {
      value: content,
      language,
      theme,
      minimap: { enabled: false },
      lineNumbers: "on",
      scrollBeyondLastLine: false,
      wordWrap: "on",
      automaticLayout: true,
      readOnly: false,
      fontSize: 13,
      fontFamily: "'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace",
      tabSize: 2,
      "semanticHighlighting.enabled": true,
      suggest: { showWords: false, showSnippets: false },
      quickSuggestions: false,
      parameterHints: { enabled: false },
    });

    editor.onDidChangeModelContent(() => {
      if (!disposed) onChange(editor!.getValue());
    });

    return () => {
      disposed = true;
      editor?.dispose();
    };
  });

  // Sync content from outside (e.g. file reload)
  $effect(() => {
    if (editor && content !== editor.getValue()) {
      editor.setValue(content);
    }
  });
</script>

<div bind:this={containerRef} class="h-full w-full"></div>
```

- [ ] **Step 2: Write MarkdownPreview.svelte**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n/index.svelte";

  let {
    content = "",
  }: {
    content: string;
  } = $props();

  let renderedHtml = $state("");
  let sourceMode = $state(false);
  let loading = $state(true);

  async function render() {
    loading = true;
    try {
      const [markedMod, hljsMod] = await Promise.all([
        import("marked"),
        import("highlight.js"),
      ]);
      const { marked } = markedMod;
      const hljs = hljsMod.default;
      marked.setOptions({
        highlight: (code: string, lang: string) => {
          if (lang && hljs.getLanguage(lang)) {
            return hljs.highlight(code, { language: lang }).value;
          }
          return hljs.highlightAuto(code).value;
        },
      });
      const parsed = marked.parse(content);
      renderedHtml = typeof parsed === "string" ? parsed : "";
    } catch {
      renderedHtml = `<pre>${escapeHtml(content)}</pre>`;
    } finally {
      loading = false;
    }
  }

  function escapeHtml(s: string): string {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  $effect(() => {
    if (!sourceMode) render();
  });

  onMount(() => render());
</script>

<div class="flex flex-col h-full">
  <div class="flex items-center gap-1 px-2 py-1 border-b shrink-0">
    <button
      class="text-[10px] px-1.5 py-0.5 rounded {sourceMode ? 'bg-muted text-muted-foreground' : 'bg-primary/15 text-primary'} transition-colors"
      onclick={() => (sourceMode = false)}>
      Preview
    </button>
    <button
      class="text-[10px] px-1.5 py-0.5 rounded {sourceMode ? 'bg-primary/15 text-primary' : 'bg-muted text-muted-foreground'} transition-colors"
      onclick={() => (sourceMode = true)}>
      Source
    </button>
  </div>
  <div class="flex-1 overflow-auto">
    {#if loading}
      <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
    {:else if sourceMode}
      <pre class="text-[11px] font-mono leading-relaxed whitespace-pre-wrap p-4">{content}</pre>
    {:else}
      <div class="p-4 prose prose-sm dark:prose-invert max-w-none">{@html renderedHtml}</div>
    {/if}
  </div>
</div>
```

- [ ] **Step 3: Write HtmlPreview.svelte**

```svelte
<script lang="ts">
  let {
    content = "",
  }: {
    content: string;
  } = $props();

  let blobUrl = $state("");

  $effect(() => {
    // Revoke old blob URL
    if (blobUrl) URL.revokeObjectURL(blobUrl);
    const blob = new Blob([content], { type: "text/html" });
    blobUrl = URL.createObjectURL(blob);
    return () => {
      if (blobUrl) URL.revokeObjectURL(blobUrl);
    };
  });
</script>

<div class="h-full w-full">
  {#if blobUrl}
    <iframe src={blobUrl} class="h-full w-full border-0" sandbox="allow-scripts" title="HTML Preview"></iframe>
  {/if}
</div>
```

- [ ] **Step 4: Write ImagePreview.svelte**

```svelte
<script lang="ts">
  let {
    filepath = "",
    cwd = "",
  }: {
    filepath: string;
    cwd: string;
  } = $props();

  let zoom = $state(1);
  let imageUrl = $state("");

  $effect(() => {
    // For local files in Tauri, use asset:// protocol or read as base64
    // For now, we use the file path directly — Tauri's IPC handles resolution
    const ext = filepath.split(".").pop()?.toLowerCase() ?? "";
    const mimeMap: Record<string, string> = {
      png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg",
      gif: "image/gif", svg: "image/svg+xml", webp: "image/webp",
      bmp: "image/bmp", ico: "image/x-icon",
    };
    const mime = mimeMap[ext] ?? "image/png";
    // Read file as base64 via Tauri command
    import("$lib/api").then(({ readTextFile }) => {
      readTextFile(filepath, cwd).then((content) => {
        imageUrl = `data:${mime};base64,${content}`;
      }).catch(() => {
        // For binary files we need a different approach — use file path directly
        imageUrl = `asset://localhost/${encodeURIComponent(filepath)}`;
      });
    });
  });
</script>

<div class="flex flex-col items-center justify-center h-full bg-muted/20">
  {#if imageUrl}
    <img
      src={imageUrl}
      alt={filepath}
      style="transform: scale({zoom})"
      class="max-w-full max-h-full object-contain transition-transform"
    />
  {/if}
  <div class="flex items-center gap-2 mt-2">
    <button class="px-2 py-0.5 text-xs bg-muted rounded" onclick={() => (zoom = Math.max(0.25, zoom - 0.25))}>-</button>
    <span class="text-xs text-muted-foreground">{Math.round(zoom * 100)}%</span>
    <button class="px-2 py-0.5 text-xs bg-muted rounded" onclick={() => (zoom = Math.min(4, zoom + 0.25))}>+</button>
  </div>
</div>
```

- [ ] **Step 5: Write PdfPreview.svelte**

```svelte
<script lang="ts">
  let {
    filepath = "",
  }: {
    filepath: string;
  } = $props();

  let fileUrl = $state("");
  let error = $state<string | null>(null);

  $effect(() => {
    // Read PDF as base64 data URL via Tauri
    import("$lib/api").then(({ readFileAsBase64 }) => {
      readFileAsBase64(filepath).then((base64) => {
        fileUrl = `data:application/pdf;base64,${base64}`;
      }).catch((e) => {
        // Fallback: try asset protocol
        error = null;
        fileUrl = `asset://localhost/${encodeURIComponent(filepath)}`;
      });
    }).catch(() => {
      error = "Cannot load PDF";
    });
  });
</script>

<div class="h-full w-full">
  {#if fileUrl}
    <iframe src={fileUrl} class="h-full w-full border-0" title="PDF Preview"></iframe>
  {:else if error}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">{error}</div>
  {:else}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
  {/if}
</div>
```

- [ ] **Step 6: Write OfficePreview.svelte**

```svelte
<script lang="ts">
  import { onMount } from "svelte";

  let {
    filepath = "",
    fileType = "word",
    cwd = "",
  }: {
    filepath: string;
    fileType: "word" | "excel";
    cwd: string;
  } = $props();

  let htmlContent = $state("");
  let error = $state<string | null>(null);
  let loading = $state(true);

  onMount(async () => {
    try {
      const { readFileAsBuffer } = await import("$lib/api");
      const buffer = await readFileAsBuffer(filepath, cwd);

      if (fileType === "word") {
        const mammoth = await import("mammoth");
        const result = await mammoth.convertToHtml({ arrayBuffer: buffer });
        htmlContent = result.value;
      } else {
        const XLSX = await import("xlsx");
        const workbook = XLSX.read(buffer, { type: "array" });
        const firstSheet = workbook.SheetNames[0];
        const sheet = workbook.Sheets[firstSheet];
        htmlContent = XLSX.utils.sheet_to_html(sheet);
      }
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });
</script>

<div class="h-full overflow-auto p-4">
  {#if loading}
    <div class="flex items-center justify-center h-full text-xs text-muted-foreground">Loading...</div>
  {:else if error}
    <div class="text-xs text-red-500">{error}</div>
  {:else}
    <div class="prose prose-sm dark:prose-invert max-w-none">{@html htmlContent}</div>
  {/if}
</div>
```

- [ ] **Step 7: Write PreviewPanel.svelte** (main container)

```svelte
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";
  import type { PreviewState } from "$lib/stores/preview-store.svelte";
  import MonacoEditor from "./MonacoEditor.svelte";
  import MarkdownPreview from "./MarkdownPreview.svelte";
  import HtmlPreview from "./HtmlPreview.svelte";
  import ImagePreview from "./ImagePreview.svelte";
  import PdfPreview from "./PdfPreview.svelte";
  import OfficePreview from "./OfficePreview.svelte";

  let {
    state,
    cwd = "",
    onClose = () => {},
    onContentChange = (_value: string) => {},
    onSave = () => {},
  }: {
    state: PreviewState;
    cwd?: string;
    onClose?: () => void;
    onContentChange?: (value: string) => void;
    onSave?: () => void;
  } = $props();

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
    if ((e.ctrlKey || e.metaKey) && e.key === "s") {
      e.preventDefault();
      onSave();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="flex flex-col h-full border-l border-border bg-background">
  <!-- Header -->
  <div class="flex items-center justify-between px-3 py-1.5 border-b shrink-0 bg-muted/20">
    <span class="text-[11px] font-mono text-muted-foreground truncate min-w-0" title={state.filepath ?? ""}>
      {state.filepath ?? ""}
    </span>
    <div class="flex items-center gap-1 shrink-0 ml-2">
      {#if state.dirty}
        <button
          class="text-[10px] px-1.5 py-0.5 rounded bg-primary/15 text-primary hover:bg-primary/25 transition-colors"
          onclick={onSave}
          title="Save (Ctrl+S)">
          {t("filesPanel_save")}
        </button>
      {/if}
      <button
        class="rounded p-0.5 hover:bg-accent transition-colors"
        onclick={onClose}
        title="Close (Esc)">
        <svg class="h-3.5 w-3.5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18" /><path d="m6 6 12 12" />
        </svg>
      </button>
    </div>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-hidden">
    {#if state.loading}
      <div class="flex items-center justify-center h-full">
        <div class="h-5 w-5 rounded-full border-2 border-muted-foreground/30 border-t-primary animate-spin"></div>
      </div>
    {:else if state.error}
      <div class="flex items-center justify-center h-full text-xs text-red-500 p-4 text-center">
        {state.error}
      </div>
    {:else if state.content !== null}
      {#if state.fileType === "code" || state.fileType === "other"}
        <MonacoEditor content={state.content} language={state.language} onChange={onContentChange} />
      {:else if state.fileType === "markdown"}
        <MarkdownPreview content={state.content} />
      {:else if state.fileType === "html"}
        <HtmlPreview content={state.content} />
      {:else if state.fileType === "image"}
        <ImagePreview filepath={state.filepath!} {cwd} />
      {:else if state.fileType === "pdf"}
        <PdfPreview filepath={state.filepath!} />
      {:else if state.fileType === "word" || state.fileType === "excel"}
        <OfficePreview filepath={state.filepath!} fileType={state.fileType === "word" ? "word" : "excel"} {cwd} />
      {/if}
    {:else}
      <div class="flex items-center justify-center h-full text-xs text-muted-foreground">
        {t("filesPanel_noContent")}
      </div>
    {/if}
  </div>
</div>
```

- [ ] **Step 8: Commit**

```bash
git add src/lib/components/preview/
git commit -m "feat: add preview panel components (Monaco, Markdown, HTML, Image, PDF, Office)"
```

---

### Task 2.5: Modify FilesPanel to dispatch preview events

**Files:**
- Modify: `src/lib/components/FilesPanel.svelte`

- [ ] **Step 1: Remove inline preview state and replace with event dispatch**

In `src/lib/components/FilesPanel.svelte`:

1. Remove inline preview state variables (lines 27-29):
```typescript
// REMOVE:
let previewPath = $state<string | null>(null);
let previewContent = $state<string | null>(null);
let previewLoading = $state(false);
```

2. Replace `previewFile` function with event dispatch:
```typescript
// REPLACE the previewFile function (lines 190-208):
function previewFile(entry: FileEntry) {
  window.dispatchEvent(new CustomEvent("clawgo:preview-file", {
    detail: { filepath: entry.path },
  }));
  onScrollToTool?.(entry.toolUseId!);
}
```

3. Remove `closePreview` function (lines 210-213)

4. Remove the preview pane template section (lines 283-317):
```svelte
<!-- REMOVE lines 283-317: the entire preview pane block -->
```

5. In `setFilter` (line 226-230), remove `previewPath = null; previewContent = null;`:
```typescript
function setFilter(action: FileEntry["action"] | null) {
  activeFilter = activeFilter === action ? null : action;
  // REMOVE: previewPath = null; previewContent = null;
}
```

6. Remove the `hidden` class binding on the tree container (line 271):
```svelte
<!-- CHANGE from: -->
<div class="flex-1 overflow-y-auto py-1" class:hidden={previewPath !== null}>
<!-- TO: -->
<div class="flex-1 overflow-y-auto py-1">
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/components/FilesPanel.svelte
git commit -m "feat: refactor FilesPanel to dispatch preview events instead of inline preview"
```

---

### Task 2.6: Integrate preview panel into chat page

**Files:**
- Modify: `src/routes/chat/+page.svelte`

- [ ] **Step 1: Add preview store import and initialization**

At the top of the script section in `src/routes/chat/+page.svelte`:

1. Add imports:
```typescript
import { createPreviewStore, detectFileType } from "$lib/stores/preview-store.svelte";
import PreviewPanel from "$lib/components/preview/PreviewPanel.svelte";
import PreviewResizer from "$lib/components/preview/PreviewResizer.svelte";
```

2. Add preview store and width state after other state declarations (after line 162):
```typescript
const previewStore = createPreviewStore();
let previewWidth = $state(
  typeof window !== "undefined"
    ? parseInt(localStorage.getItem("clawgo:preview-width") || "600")
    : 600,
);
```

3. Add a `wasSidebarOpen` ref to remember sidebar state before auto-hide:
```typescript
let _previewSavedSidebar = false;
```

4. Add preview event handlers:
```typescript
function handlePreviewFile(e: CustomEvent<{ filepath: string }>) {
  const cwd = store.effectiveCwd || store.run?.cwd || localStorage.getItem("clawgo:project-cwd") || "";
  // Save sidebar state before auto-hiding
  if (!previewStore.state.isOpen && !sidebarCollapsed) {
    _previewSavedSidebar = true;
    sidebarCollapsed = true;
  }
  previewStore.open(e.detail.filepath, cwd);
}

function handleClosePreview() {
  previewStore.close();
  // Restore sidebar if it was auto-hidden
  if (_previewSavedSidebar && sidebarCollapsed) {
    sidebarCollapsed = false;
  }
  _previewSavedSidebar = false;
}

function handlePreviewResize(width: number) {
  previewWidth = width;
  localStorage.setItem("clawgo:preview-width", String(width));
}

function handlePreviewContentChange(value: string) {
  previewStore.updateContent(value);
}

async function handlePreviewSave() {
  const cwd = store.effectiveCwd || store.run?.cwd || "";
  const ok = await previewStore.save(cwd);
  if (ok) {
    showChatToast("File saved");
  } else {
    showChatToast("Save failed: " + (previewStore.state.error || "unknown error"));
  }
}
```

5. Add event listeners in the `onMount` block (alongside other event listeners, around line 1500):
```typescript
window.addEventListener("clawgo:preview-file", handlePreviewFile as EventListener);
window.addEventListener("clawgo:close-preview", handleClosePreview);

// In the return cleanup:
window.removeEventListener("clawgo:preview-file", handlePreviewFile as EventListener);
window.removeEventListener("clawgo:close-preview", handleClosePreview);
```

- [ ] **Step 2: Add preview panel to template layout**

In the template section, wrap the main content area with a flex row that includes the preview panel.

Find the main content area div (around line 3933: `<!-- Main content area -->`) and restructure:

```svelte
<!-- Main content area + preview panel -->
<div class="flex flex-1 min-w-0">
  <!-- Chat area (shrinks when preview opens) -->
  <div
    class="flex flex-1 flex-col min-w-0"
    style="width: {previewStore.state.isOpen ? `calc(100% - ${previewWidth}px)` : '100%'}"
  >
    <!-- Status bar -->
    <SessionStatusBar ... />
    <!-- MCP panel, preview URL bar, main area, prompt input — all existing content -->
    <!-- (Keep all existing content inside this div) -->
  </div>

  <!-- Preview panel + resizer -->
  {#if previewStore.state.isOpen}
    <PreviewResizer width={previewWidth} onResize={handlePreviewResize} />
    <div style="width: {previewWidth}px" class="flex-shrink-0 h-full overflow-hidden">
      <PreviewPanel
        state={previewStore.state}
        cwd={store.effectiveCwd || store.run?.cwd || ""}
        onClose={handleClosePreview}
        onContentChange={handlePreviewContentChange}
        onSave={handlePreviewSave}
      />
    </div>
  {/if}
</div>
```

- [ ] **Step 3: Handle ESC key for closing preview (add to existing keydown handler)**

Find the existing keyboard handler and add:
```typescript
if (e.key === "Escape" && previewStore.state.isOpen) {
  previewStore.close();
  return;
}
```

- [ ] **Step 4: Commit**

```bash
git add src/routes/chat/+page.svelte
git commit -m "feat: integrate preview panel into chat page with sidebar auto-hide"
```

---

### Task 2.7: Add file path link detection in chat messages

**Files:**
- Modify: `src/lib/components/ChatMessage.svelte` (or the message render path)

- [ ] **Step 1: Add file path regex and click handler**

In the chat message content rendering, detect file paths and convert to clickable links.

The file path regex pattern (from the spec):
```typescript
const FILE_PATH_REGEX = /(?:^|\s)((?:src|lib|app|pages|components|utils|hooks|stores|routes|api)\/[\w\-./]+\.\w+)/g;
```

This should be implemented in the component that renders user message content. Check `ChatMessage.svelte` to find the right place.

- [ ] **Step 2: Implement click handler**

When a file path is clicked, dispatch:
```typescript
window.dispatchEvent(new CustomEvent("clawgo:preview-file", {
  detail: { filepath: matchedPath },
}));
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/components/ChatMessage.svelte
git commit -m "feat: add file path link detection in chat messages for preview panel"
```

---

### Task 2.8: Add i18n strings for preview panel

**Files:**
- Modify: `messages/en.json`
- Modify: `messages/zh-CN.json`

- [ ] **Step 1: Add English strings**

In `messages/en.json`, add:
```json
{
  "filesPanel_save": "Save",
  "filesPanel_noContent": "No content to preview"
}
```

- [ ] **Step 2: Add Chinese strings**

In `messages/zh-CN.json`, add:
```json
{
  "filesPanel_save": "保存",
  "filesPanel_noContent": "无预览内容"
}
```

- [ ] **Step 3: Run i18n check**

```bash
npm run i18n:check
```
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add messages/en.json messages/zh-CN.json
git commit -m "chore: add preview panel i18n strings"
```

---

### Task 2.9: Add binary file reading support (Tauri command)

**Files:**
- Modify: `src-tauri/src/commands/files.rs` (or find the existing read_text_file command location)
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Add read_file_as_base64 Tauri command**

In the Rust command file:
```rust
#[tauri::command]
pub async fn read_file_as_base64(
    path: String,
    cwd: Option<String>,
) -> Result<String, String> {
    let base = cwd.unwrap_or_default();
    let full_path = std::path::PathBuf::from(&base).join(&path);
    let bytes = std::fs::read(&full_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
```

Register the command in `main.rs`.

- [ ] **Step 2: Add read_file_as_buffer API function**

In `src/lib/api.ts`:
```typescript
export async function readFileAsBase64(path: string, cwd?: string): Promise<string> {
  return invoke<string>("read_file_as_base64", { path, cwd: cwd ?? null });
}

export async function readFileAsBuffer(path: string, cwd?: string): Promise<ArrayBuffer> {
  const base64 = await readFileAsBase64(path, cwd);
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/ src/lib/api.ts
git commit -m "feat: add read_file_as_base64 Tauri command for binary file preview"
```

---

### Task 2.10: Final verification

- [ ] **Step 1: Run frontend build**

```bash
npm run build
```
Expected: PASS (no errors)

- [ ] **Step 2: Run Rust check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```
Expected: PASS (no errors)

- [ ] **Step 3: Run i18n check**

```bash
npm run i18n:check
```
Expected: PASS

- [ ] **Step 4: Run lints**

```bash
npm run lint
```
Expected: PASS (or only pre-existing issues)

- [ ] **Step 5: Commit any remaining changes**

```bash
git add -A
git commit -m "chore: final verification fixes for preview panel and Packy removal"
```
