# Preview Panel + Packy CX2CC Removal Design

**Date**: 2026-05-26
**Status**: Draft (revised after multi-review)
**Author**: Claude Code

---

## Overview

Two features:

1. **Packy CX2CC Complete Removal** — Remove all references to Packy CX2CC provider from frontend, backend, and configuration files.
2. **Preview Panel** — Replace inline FilesPanel preview with a new right-side parallel-squeeze preview panel supporting multiple file types with editing capabilities. **Tauri preview window (preview.rs) is preserved** — it serves a different purpose (localhost Web app preview + DOM element picking).

### Multi-Review Decisions (2026-05-27)

1. **preview.rs 保留** — Tauri 预览窗口（localhost Web 预览 + 元素选择）与文件内容预览是不同功能，不删除。
2. **直接上 Monaco/PDF/Office** — MVP 包含完整编辑器和渲染器，不做分阶段降级。HTML 文件以 iframe 渲染为网页，不按代码编辑器处理。
3. **布局策略** — 右侧可拖动并行面板，打开预览时自动隐藏侧边栏，关闭预览时恢复侧边栏。

---

## Feature 1: Packy CX2CC Complete Removal

### Scope

Remove ALL Packy CX2CC references from:

**Frontend**:
- `src/lib/utils/provider-catalog.ts` — Remove from `Phase7ProviderId` union type, `PHASE7_PROVIDERS` array, and `providerIdForRun()` function
- `src/lib/utils/platform-presets.ts` — Remove from `PLATFORM_PRESETS` array

**Backend**:
- `src-tauri/src/commands/balance.rs` — Remove Packy balance query logic
- `src-tauri/src/commands/session.rs` — Remove Packy-specific launch config template
- `src-tauri/src/agent/provider_claude_config.rs` — Remove Packy platform ID handling

**Documentation**:
- `CLAUDE.md` — Remove Packy references from provider documentation
- `messages/en.json`, `messages/zh-CN.json` — Remove Packy-related i18n strings

### Implementation Steps

1. Search for all "packy" references (case-insensitive) across the codebase
2. Remove frontend provider catalog and platform preset entries
3. Remove backend balance query and session config logic
4. Remove i18n strings
5. Update documentation
6. Verify build passes (`npm run build`, `cargo check`)

---

## Feature 2: Preview Panel

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  聊天页面 (+page.svelte)                                      │
│ ┌─────────────────────────┬─┬─────────────────────────────┐ │
│ │                         │ │                             │ │
│ │    聊天区域              │D│    预览面板                  │ │
│ │    (SessionPanel)       │r│    (PreviewPanel)           │ │
│ │                         │a│                             │ │
│ │                         │g│                             │ │
│ │                         │ │                             │ │
│ └─────────────────────────┴─┴─────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

- Preview panel is a right-side parallel component alongside chat area
- State is managed in a dedicated `preview-store.svelte.ts` (store-centric pattern)
- Width is persisted in localStorage
- Replaces inline FilesPanel preview only; **Tauri preview window (preview.rs) is preserved** as a separate developer tool
- **Opening preview auto-hides the sidebar**; closing preview restores it

### Component Structure

**New Components**:

1. **`src/lib/components/preview/PreviewPanel.svelte`** — Main preview panel
   - Props: `filepath: string | null`, `onClose: () => void`
   - Manages file loading, type detection, and content rendering
   - Dispatches save events for editable files

2. **`src/lib/components/preview/PreviewResizer.svelte`** — Drag handle
   - Props: `width: number`, `onResize: (width: number) => void`
   - Pointer event handling for drag resize
   - Double-click to reset to 50/50

3. **`src/lib/components/preview/MonacoEditor.svelte`** — Code editor
   - Props: `content: string`, `language: string`, `onChange: (value: string) => void`
   - Monaco Editor integration with Svelte
   - Theme follows system (light/dark)

4. **`src/lib/components/preview/MarkdownPreview.svelte`** — Markdown renderer
   - Props: `content: string`
   - Uses marked.js + highlight.js
   - Toggle between rendered and source view

5. **`src/lib/components/preview/HtmlPreview.svelte`** — HTML renderer
   - Props: `content: string`
   - Sandbox iframe rendering

6. **`src/lib/components/preview/ImagePreview.svelte`** — Image viewer
   - Props: `filepath: string`
   - Zoom controls

7. **`src/lib/components/preview/PdfPreview.svelte`** — PDF viewer
   - Props: `filepath: string`
   - iframe-based rendering

8. **`src/lib/components/preview/OfficePreview.svelte`** — Word/Excel viewer
   - Props: `filepath: string`, `fileType: 'word' | 'excel'`
   - mammoth.js for Word, SheetJS for Excel

**Modified Components**:

1. **`src/lib/components/FilesPanel.svelte`** — Remove inline preview, dispatch preview events
2. **`src/routes/chat/+page.svelte`** — Integrate preview panel and resizer
3. **Chat message renderer** — Add file path link detection

### Data Flow

**File Loading**:
```
User clicks file → FilesPanel/ChatMessage dispatches event
     ↓
Chat page receives event, sets previewState.filepath
     ↓
PreviewPanel detects filepath change
     ↓
Determines file type by extension → calls readTextFile (Tauri command)
     ↓
Renders appropriate content renderer
```

**File Saving** (text files only):
```
User edits content → Monaco Editor triggers onChange
     ↓
PreviewPanel detects content change, shows save button
     ↓
User clicks save → calls saveTextFile (Tauri command)
     ↓
Shows save success/failure toast
```

**Event Communication**:
- `clawgo:preview-file` — Open file preview (carries filepath)
- `clawgo:close-preview` — Close preview panel
- `clawgo:preview-saved` — File save success notification

### Layout & Resizing

**Layout Strategy**: 打开预览面板时自动隐藏侧边栏，关闭时恢复。避免三列布局在窄屏（1366px 笔记本）上的挤压问题。

**Layout Structure**:
```svelte
<div class="flex h-full">
  <!-- Chat area -->
  <div style="width: calc(100% - {previewState.isOpen ? previewWidth : 0}px)" class="min-w-0">
    <SessionPanel ... />
  </div>

  <!-- Drag handle (only when preview is open) -->
  {#if previewState.isOpen}
    <PreviewResizer bind:width={previewWidth} />
  {/if}

  <!-- Preview panel (only when preview is open) -->
  {#if previewState.isOpen}
    <div style="width: {previewWidth}px" class="flex-shrink-0">
      <PreviewPanel filepath={previewState.filepath} onClose={handleClose} />
    </div>
  {/if}
</div>
```

**Sidebar Auto-Hide Logic**:
- 打开预览 → 隐藏侧边栏（`sidebarVisible = false`）
- 关闭预览 → 恢复侧边栏（`sidebarVisible = true`）
- 侧边栏隐藏期间，用户可通过 sidebar toggle 手动恢复（此时预览仍保持打开）

**Constraints**:
- Minimum chat width: 400px
- Minimum preview width: 320px
- Default preview width: 50% of window width (侧边栏已隐藏，不会挤压)
- Double-click handle: reset to 50/50
- Width persisted in localStorage

**Close Behavior**:
- Click close button on preview panel
- Press ESC key
- Click same file again (toggle)
- Close restores sidebar and expands chat to 100%

### File Type Handling

| File Type | Extensions | Renderer | Editable |
|-----------|-----------|----------|----------|
| Code | .ts, .tsx, .js, .jsx, .rs, .py, .svelte, .css, .json, .toml, .yaml, .yml | MonacoEditor | Yes |
| Markdown | .md, .mdx | MarkdownPreview | Yes (source mode) |
| HTML | .htm, .html | HtmlPreview (iframe sandbox 渲染为网页) | No |
| Images | .png, .jpg, .jpeg, .gif, .svg, .webp | ImagePreview | No |
| PDF | .pdf | PdfPreview | No |
| Word | .doc, .docx | OfficePreview | No |
| Excel | .xls, .xlsx | OfficePreview | No |
| Other | * | MonacoEditor (plain text) | Yes |

**Dependencies**:
```json
{
  "monaco-editor": "^0.50.0",
  "@monaco-editor/svelte": "^0.2.0",
  "marked": "^12.0.0",
  "highlight.js": "^11.10.0",
  "mammoth": "^1.8.0",
  "xlsx": "^0.18.5"
}
```

**Monaco Editor Config**:
- Language auto-detection by file extension
- Theme follows system (light/dark)
- Minimal features: edit, search, replace
- IntelliSense disabled (no language server)

### Integration Points

#### 1. FilesPanel Integration

Remove inline preview state (`previewPath`, `previewContent`, `previewLoading`). On file click, dispatch event:

```svelte
<button onclick={() => {
  dispatch('preview-file', { filepath: entry.path });
  onScrollToTool?.(entry.toolUseId!);
}}>
```

#### 2. Chat Message File Links

Detect file paths in message content and convert to clickable links:

```typescript
const FILE_PATH_REGEX = /(?:^|\s)((?:src|lib|app|pages|components|utils|hooks|stores|routes|api)\/[\w\-./]+\.\w+)/g;
```

Click dispatches `clawgo:preview-file` event.

#### 3. preview.rs 保留

`preview.rs`（Tauri 预览窗口 + localhost Web 预览 + DOM 元素选择 + picker_bridge.js）与 Preview Panel 是不同功能，**完整保留**：

- `src-tauri/src/commands/preview.rs` — 保留
- `src-tauri/src/commands/picker_bridge.js` — 保留
- `open_preview_window` / `close_preview_window` Tauri 命令 — 保留
- "Pick Element → Insert to Chat" 工作流 — 保留

Preview Panel（文件内容预览）与 preview.rs（Web 应用预览 + 元素选择）并存，互不干扰。

#### 4. Chat Page Integration

```svelte
<script>
  let previewState = $state({
    isOpen: false,
    filepath: null as string | null,
  });

  let previewWidth = $state(
    parseInt(localStorage.getItem('previewWidth') || '600')
  );

  function handlePreviewFile(filepath: string) {
    previewState = { isOpen: true, filepath };
  }

  function handleClosePreview() {
    previewState = { isOpen: false, filepath: null };
  }

  function handleResize(width: number) {
    previewWidth = width;
    localStorage.setItem('previewWidth', String(width));
  }
</script>

<div class="flex h-full">
  <div style="width: calc(100% - {previewState.isOpen ? previewWidth : 0}px)" class="min-w-0">
    <!-- Existing chat content -->
  </div>

  {#if previewState.isOpen}
    <PreviewResizer width={previewWidth} onResize={handleResize} />
    <PreviewPanel
      filepath={previewState.filepath}
      onClose={handleClosePreview}
    />
  {/if}
</div>
```

### New Tauri Command

**`saveTextFile`** — Save text content to file:

```rust
#[tauri::command]
pub async fn save_text_file(
    app: AppHandle,
    path: String,
    content: String,
    cwd: String,
) -> Result<(), String> {
    let full_path = resolve_path(&path, &cwd)?;
    std::fs::write(&full_path, content)
        .map_err(|e| format!("Failed to save file: {}", e))?;
    Ok(())
}
```

---

## Implementation Order

1. **Feature 1**: Packy CX2CC removal (simpler, no new dependencies)
   - Remove frontend references
   - Remove backend references
   - Update docs and i18n
   - Verify build

2. **Feature 2**: Preview Panel
   - Install dependencies (Monaco, marked, mammoth, SheetJS)
   - Create `preview-store.svelte.ts` (state management)
   - Create PreviewPanel and sub-components
   - Implement PreviewResizer with drag handling
   - Modify FilesPanel to dispatch events
   - Add file path detection to chat messages
   - Integrate into chat page layout
   - Add sidebar auto-hide/restore logic
   - Add saveTextFile Tauri command
   - Test all file types
   - Verify build

---

## Open Questions

None — all requirements clarified during brainstorming.

---

## Dependencies

### Frontend (npm)
- `monaco-editor` — Code editor
- `@monaco-editor/svelte` — Svelte integration
- `marked` — Markdown parser
- `highlight.js` — Syntax highlighting
- `mammoth` — Word document converter
- `xlsx` — Excel parser

### Backend (Cargo)
No new Rust dependencies.

---

## Testing Strategy

1. **Unit tests**: File type detection, path resolution
2. **Component tests**: Preview panel rendering, resizer behavior
3. **Integration tests**: File loading, saving, event communication
4. **Manual tests**: All file types, drag resize, close behavior, persistence

---

## Success Criteria

1. Packy CX2CC no longer appears in any UI or configuration
2. Preview panel opens from FilesPanel and chat message file links
3. All specified file types render correctly
4. Text files can be edited and saved
5. Drag resize works smoothly with localStorage persistence
6. Preview panel only shows on chat page
7. preview.rs Tauri 预览窗口功能完好，与 Preview Panel 并存
8. Build passes (`npm run build`, `cargo check`)
