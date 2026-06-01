# Plan: Memory Settings 迁移 + 记忆文件扩展

## Context

两个修复：
1. Settings 页面中的 "Memory Extraction" 整个 tab（提取配置 + 记忆衰减）应移到 `/memory-mgmt` 页面
2. `/memory` 路由需要解析 `~/.claude/memory/` 作为全局记忆，以及 `~/.claude/projects/*/memory/` 作为项目记忆（扫描所有项目，匹配到对应已打开项目分组显示）

## Task 1: Memory Extraction 设置迁移到 /memory-mgmt

### 1a. 从 Settings 页面移除 memory tab

**文件:** `src/routes/settings/+page.svelte`

- 删除 `SettingsTab` 类型中的 `"memory"` 选项（~line 52）
- 删除 `tabLabels` 中的 `memory` 条目（~line 72）
- 删除 `tabs` 数组中的 memory icon 条目（~line 98）
- 删除 memory extraction 状态变量（~lines 114-121）
- 删除 `loadMemoryExtractionConfig()`、`saveMemoryExtractionConfig()`、`debouncedSaveMemoryExtraction()` 函数（~lines 123-147）
- 删除 onMount 中 `memoryDreamEnabled` 加载（~line 1168）和 `loadMemoryExtractionConfig()` 调用（~line 1174）
- 删除 `{:else if activeTab === "memory"}` 整个渲染块（~lines 3552-3639）

### 1b. 在 /memory-mgmt 页面添加 "Extraction Config" tab

**文件:** `src/routes/memory-mgmt/+page.svelte`

- 添加新的 tab `MemTab` 类型扩展：`"userMemory" | "archived" | "extractionConfig"`
- 添加 tab 入口（i18n key `memoryMgmt_tab_extractionConfig` 已存在）
- 从 settings 页面复制 memory extraction 状态变量和 load/save/debounce 函数
- 添加 `$effect` 加载 `memoryDreamEnabled`（从 `api.getUserSettings()`）
- 在新 tab 的渲染块中复制 extraction config UI（enable toggle、endpoint、key、model、dream cycle）
- 需要导入 `Input` 组件和 `api`、`dbgWarn`
- 移除 memory-mgmt 顶部的 "已移至设置" 提示横幅（~lines 112-121）

## Task 2: 记忆文件扫描扩展

### 2a. Rust 后端：MemoryFileCandidate 增加 project_slug 字段

**文件:** `src-tauri/src/models.rs`

```rust
pub struct MemoryFileCandidate {
    pub path: String,
    pub label: String,
    pub scope: String, // "project" | "global" | "memory" | "global-memory"
    pub provider: Option<String>,
    pub exists: bool,
    pub project_slug: Option<String>, // NEW: 对应 ~/.claude/projects/{slug}
}
```

**文件:** `src/lib/types.ts`

```ts
export interface MemoryFileCandidate {
  path: string;
  label: string;
  scope: "project" | "global" | "memory" | "global-memory";
  provider?: "claude" | "codex";
  exists: boolean;
  projectSlug?: string; // NEW
}
```

### 2b. Rust 后端：扫描所有项目 memory + 全局 memory

**文件:** `src-tauri/src/commands/files.rs`

替换 ~lines 435-447 的项目 auto-memory 扫描代码（不再依赖 cwd）：
```rust
// Project auto-memory scope — scan ALL ~/.claude/projects/*/memory/*.md
if let Some(ref home) = crate::storage::home_dir() {
    let projects_dir = std::path::Path::new(&home).join(".claude").join("projects");
    if projects_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                let slug = entry.file_name().to_string_lossy().to_string();
                let memory_dir = entry.path().join("memory");
                if memory_dir.is_dir() {
                    let memory_files = scan_memory_md_files(&memory_dir, &memory_dir, 3, 50);
                    for mut f in memory_files {
                        f.project_slug = Some(slug.clone());
                        files.push(f);
                    }
                }
            }
        }
    }
}
```

在 global scope 部分（~line 412 之后）添加 `~/.claude/memory/` 扫描：
```rust
// Global auto-memory — scan ~/.claude/memory/*.md
{
    let global_memory_dir = std::path::Path::new(&home).join(".claude").join("memory");
    if global_memory_dir.is_dir() {
        let memory_files = scan_memory_md_files(&global_memory_dir, &global_memory_dir, 3, 50);
        for mut f in memory_files {
            f.scope = "global-memory".to_string();
            files.push(f);
        }
    }
}
```

同时更新 `scan_md_inner` 中对 `MemoryFileCandidate` 的构造，增加 `project_slug: None`。

### 2c. 前端 /memory 页面：按项目分组显示 memory 文件

**文件:** `src/routes/memory/+page.svelte`

1. 合并 "global" + "global-memory" 到 Global 区域
2. 在 Project 区域内按 `projectSlug` 子分组显示 memory 文件

```ts
let scopeGlobal = $derived([
  ...candidates.filter((c) => c.scope === "global"),
  ...candidates.filter((c) => c.scope === "global-memory"),
]);
```

### 2d. 前端 layout sidebar 同步更新

**文件:** `src/routes/+layout.svelte`

同样更新 `memoryScopeGlobal` 和过滤逻辑：
```ts
let memoryScopeGlobal = $derived(
  memoryCandidates.filter((c) => c.scope === "global" || c.scope === "global-memory")
);
```

sidebar 中每个 project folder 内的 memory 文件需要按 `projectSlug` 过滤，只显示属于该项目的 memory 文件。

## 关键文件

| 文件 | 变更类型 |
|------|----------|
| `src/routes/settings/+page.svelte` | 删除 memory tab 相关代码 |
| `src/routes/memory-mgmt/+page.svelte` | 添加 extraction config tab |
| `src-tauri/src/commands/files.rs` | 重写项目记忆扫描 + 新增全局记忆 |
| `src-tauri/src/models.rs` | 增加 project_slug 字段 |
| `src/lib/types.ts` | 增加 projectSlug 字段 + 扩展 scope |
| `src/routes/memory/+page.svelte` | 按项目分组显示 memory |
| `src/routes/+layout.svelte` | 同步更新 sidebar 分组 |

## 验证

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译通过
2. `npm run check` — Svelte 类型检查通过
3. Settings 页面不再有 Memory Extraction tab
4. /memory-mgmt 页面有 Extraction Config tab 且功能正常
5. /memory 页面 Global 区域显示 ~/.claude/memory/ 下的文件
6. /memory 页面 Project 区域按项目分组显示各项目的 memory 文件
