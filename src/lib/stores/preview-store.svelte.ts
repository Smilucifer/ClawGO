// src/lib/stores/preview-store.svelte.ts
import { readTextFile, writeTextFile } from "$lib/api";

export interface PreviewState {
  isOpen: boolean;
  filepath: string | null;
  content: string | null;
  loading: boolean;
  error: string | null;
  dirty: boolean;
  language: string;
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

  let _lastCwd = "";

  /** Opens a file for preview. Returns true if the previous file had unsaved changes that were discarded. */
  function open(filepath: string, cwd: string): boolean {
    if (_state.isOpen && _state.filepath === filepath && _lastCwd === cwd) {
      close();
      return false;
    }
    const hadUnsaved = _state.dirty;
    _lastCwd = cwd;
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
    return hadUnsaved;
  }

  const TEXT_TYPES = new Set<PreviewFileType>(["code", "markdown", "html", "other"]);

  async function loadContent(filepath: string, cwd: string) {
    // Binary file types are loaded by their sub-components directly
    if (!TEXT_TYPES.has(_state.fileType)) {
      if (_state.filepath !== filepath) return;
      _state = { ..._state, content: null, loading: false, error: null };
      return;
    }
    try {
      const content = await readTextFile(filepath, cwd);
      if (_state.filepath !== filepath) return;
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
    const snapshot = _state.content;
    try {
      await writeTextFile(_state.filepath, snapshot, cwd);
      if (_state.content === snapshot) {
        _state = { ..._state, dirty: false, error: null };
      }
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
