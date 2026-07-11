# Remove MemoryNode Subsystem Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **迁移说明:执行时第一步把本文件从 `~/.claude/plans/binary-wondering-cerf.md` 复制到标准路径 `docs/superpowers/plans/2026-07-11-remove-memory-subsystem.md`,原文件删除。**

**Goal:** 彻底移除 MemoryNode 记忆子系统(自动提取 / 注入 / dreaming / 角色记忆 CRUD / 手动面板 / embedding 基础设施),记忆功能整体下线。

**Architecture:** 记忆系统完全自包含于 `storage/memory_store.rs`、`group_chat/memory_*.rs`、`commands/characters.rs` 的记忆命令,以及前端 6 个记忆文件。按"先删调用点、再删被调模块"的顺序移除,使每个 commit 都能编译通过。验证靠 `cargo check` / `npm run check` 兜底找漏引用。

**Tech Stack:** Rust (Tauri 后端) + Svelte 5 (前端) + SQLite (memory_store,运行时产物不在仓库)。

## Global Constraints

- **绝不触碰**:invest dreaming(`crate::invest::dreaming` / `commands::invest` / 前端 `SystemDreamsTab` / `DreamingConfigPanel` 的 `invest` 分支)。它与 `group_chat::memory_dream` 是不同命名空间,零重叠。
- **绝不触碰**:备忘录系统(`GlobalMemoPanel` / `memo-store` / `list_memos`/`add_memo`/`update_memo`/`delete_memo`/`clear_memos`)。与 memory 零交集。
- **绝不触碰**:`/memory` 路由(MEMORY.md/CLAUDE.md markdown 编辑器,走 `files.rs` 的 `read_text_file`/`write_text_file` + `listMemoryFiles` + `memory-helpers.ts`)。
- 每个 Task 结尾必须 `cargo check`(后端)或 `npm run check`(前端)通过后再 commit。
- commit message 用英文 conventional commits;不 push;不用 `--no-verify`。
- 文件编辑单次输出 ≤50 行。

---

### Task 0: 迁移计划文件到标准路径

- [ ] **Step 1:** 创建目录并复制:`mkdir -p docs/superpowers/plans && cp ~/.claude/plans/binary-wondering-cerf.md docs/superpowers/plans/2026-07-11-remove-memory-subsystem.md`
- [ ] **Step 2:** 后续所有勾选进度写在 `docs/superpowers/plans/2026-07-11-remove-memory-subsystem.md`。

---

### Task 1: 删除自动提取(extraction)

**Files:**
- Modify: `src-tauri/src/agent/session_actor.rs`:删私聊自动提取块(1647-1681,含 `// ── Auto-extract memories...` 注释头)
- Modify: `src-tauri/src/group_chat/orchestrator.rs`:删 use(13)、群聊自动提取块(512-542)
- Delete: `src-tauri/src/group_chat/memory_extraction.rs`
- Modify: `src-tauri/src/group_chat/mod.rs:8`(删 `pub mod memory_extraction`)

**Interfaces:**
- Produces: 移除 `auto_extract_memories`/`can_extract`/`record_extraction`/`log_to_file`。注意 orchestrator 删 use 后,若 `log_to_file` 等在别处无用即无残留。

- [ ] **Step 1:** 删 `session_actor.rs` 私聊提取块(1647-1681)。保留外层 `on_user_turn_finished`/ralph 逻辑。
- [ ] **Step 2:** 删 `orchestrator.rs` 的 `use ...memory_extraction::{...}`(13)与群聊提取块(512-542,`// ── Auto-extraction` 起到闭合 `}`)。
- [ ] **Step 3:** 删整文件 `memory_extraction.rs` + `group_chat/mod.rs:8` 的模块声明。
- [ ] **Step 4: Verify**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

- [ ] **Step 5: Verify 零残留**

Run: `grep -rn "memory_extraction\|auto_extract_memories\|can_extract\|record_extraction" src-tauri/src`
Expected: 无输出。

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor: remove automatic memory extraction from chat turns"
```

---

### Task 2: 删除记忆注入(injection)

**Files:**
- Modify: `src-tauri/src/commands/session.rs`:删私聊注入块(796-813)
- Modify: `src-tauri/src/group_chat/orchestrator.rs`:删 `fn inject_memories`(602-616)、两处调用(642、737)、上方 memory_config 解析(637-641、732-736)
- Delete: `src-tauri/src/group_chat/memory_injection.rs`
- Modify: `src-tauri/src/group_chat/mod.rs:4`(删 `pub mod memory_injection`)

**Interfaces:**
- Produces: 移除 `inject_memories_into_prompt`/`search_memories_for_injection`/`format_memory_injection`。`search_hybrid` 变成无调用者(Task 4 随 memory_store 删)。

- [ ] **Step 1:** 删 `session.rs` 私聊注入块(796-813,`// 3.5. Inject user memories...` 起)。
- [ ] **Step 2:** 删 `orchestrator.rs` 的 `fn inject_memories`(602-616);删 execute_actor_turn 里 memory_config 解析(637-641)+ `inject_memories(...)` 调用(642);删 pipe 路径同类块(732-737)。
- [ ] **Step 3:** 删整文件 `memory_injection.rs` + `group_chat/mod.rs:4`。
- [ ] **Step 4: Verify**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过(`search_hybrid` 若报 unused,Task 4 删除;如本步即报 dead_code,可 `#[allow]` 或直接进入 Task 4——优先继续)。

- [ ] **Step 5: Verify 零残留**

Run: `grep -rn "memory_injection\|inject_memories" src-tauri/src`
Expected: 无输出。

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor: remove memory injection into chat prompts"
```

---

### Task 3: 删除 dreaming 后台任务

**Files:**
- Modify: `src-tauri/src/lib.rs`:删后台 dream 任务整块(699-745)
- Delete: `src-tauri/src/group_chat/memory_dream.rs`
- Modify: `src-tauri/src/group_chat/mod.rs:3`(删 `pub mod memory_dream`)

**Interfaces:**
- Produces: 移除 `run_dream_cycle`/`should_run_dream`/`DreamCycleResult`/`DREAM_INTERVAL_SECS` 及死代码 `list_archived_memories`/`snapshot_memories`/`rollback_to_snapshot`。

- [ ] **Step 1:** 删 `lib.rs` 的 dream 后台任务块(699-745,`// Start background dream cycle task` 起到该 `spawn` 闭合 `});`)。
- [ ] **Step 2:** 删整文件 `memory_dream.rs` + `group_chat/mod.rs:3`。
- [ ] **Step 3: Verify**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

- [ ] **Step 4: Verify 零残留**

Run: `grep -rn "memory_dream\|run_dream_cycle\|DREAM_INTERVAL" src-tauri/src`
Expected: 无输出。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: remove memory dreaming background cycle"
```

---

### Task 4: 删除角色记忆命令 + 存储层 + migration

**Files:**
- Modify: `src-tauri/src/commands/characters.rs`:删 9 个记忆命令(128-270)、`create_character` 的 `memory_config: None`(38)、`update_character` 的 `memory_config` 参数(62)+赋值(104-106)
- Modify: `src-tauri/src/lib.rs`:invoke_handler 删 9 个命令注册(279-287)、`memory_store::init_db`(633)、migration 调用块(693 附近)
- Delete: `src-tauri/src/storage/memory_store.rs`、`src-tauri/src/group_chat/memory_migration.rs`
- Modify: `src-tauri/src/storage/mod.rs:14`(删 `pub mod memory_store`)、`src-tauri/src/group_chat/mod.rs:5`(删 `pub mod memory_migration`)

**Interfaces:**
- Consumes: Task 1-3 已移除 memory_store 的其它调用者;此时 `memory_store` 仅剩 characters.rs 命令 + migration + lib.rs init 引用。
- Produces: 移除 9 个 `*_character_memory`/`*_memory` 命令;`search_fts`/`find_duplicates`/`search_hybrid` 随文件删除。

- [ ] **Step 1:** 删 `characters.rs` 的 9 个记忆命令(128-270)及 `ALLOWED_MEMORY_TYPES`/`validate_memory_type` 之类仅记忆用的辅助。
- [ ] **Step 2:** 删 `characters.rs` 的 `memory_config: None`(38)、`update_character` 的 `memory_config` 参数(62)与赋值分支(104-106)。
- [ ] **Step 3:** 删 `lib.rs` invoke_handler 9 行注册(279-287)、`memory_store::init_db(&data_dir)`(633)、`memory_migration::migrate_jsonl_to_sqlite` 调用块(693 附近 match)。
- [ ] **Step 4:** 删整文件 `memory_store.rs` + `mod.rs:14`;删整文件 `memory_migration.rs` + `group_chat/mod.rs:5`。
- [ ] **Step 5: Verify**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

- [ ] **Step 6: Verify 零残留**

Run: `grep -rn "memory_store\|memory_migration\|character_memory\|pending_memories" src-tauri/src`
Expected: 无输出。

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "refactor: remove character-memory commands and memory storage layer"
```

---

### Task 5: 删除 models + settings + embedding 基础设施

**Files:**
- Modify: `src-tauri/src/models.rs`:`UserSettings` 删 `memory_dream_enabled`(396)/`memory_extraction_enabled`(400)/`memory_extraction_min_confidence`(404)/`embedding_config`(377)及 Default(609、615-617)+ default fns;`AiCharacter` 删 `memory_config`(1954);删结构 `MemoryConfig`(2237-2254)/`MemoryExtractionConfig`(2161)/`MemoryNode`/`MemorySource`/`EmbeddingConfig`(2201)/`VectorSearchResult`(2231)
- Modify: `src-tauri/src/storage/settings.rs`:删 643-647、`apply_embedding_config` 调用(642)、`get_embedding_config`(519)/`update_embedding_config`(523)/`apply_embedding_config`(740) 三个死函数、use 里 `EmbeddingConfig`(2)

- [ ] **Step 1:** 删 `models.rs` 上述字段/结构/default 辅助函数。
- [ ] **Step 2:** 删 `settings.rs` 上述 patch 应用行、三个 embedding 函数、use 导入。
- [ ] **Step 3: Verify(check + clippy)**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过。

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: 无 error/warning。**这一步至关重要——CI(ci.yml:104)就是用它把关,dead_code/unused_import 在此会被当 error。** 若因删除产生任何 dead_code(如 Task 2 遗留的 `search_hybrid`,应已随 Task 4 删除),在此一并清理。

- [ ] **Step 4: Verify 零残留**

Run: `grep -rn "MemoryNode\|MemorySource\|MemoryConfig\|EmbeddingConfig\|embedding_config\|memory_dream_enabled\|memory_extraction" src-tauri/src`
Expected: 无输出。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor: remove memory/embedding models and settings fields"
```

---

### Task 6: 删除前端记忆面板 + store + 侧边栏入口

**Files:**
- Delete: `src/lib/components/UserMemoryPanel.svelte`、`CharacterMemoryPanel.svelte`、`MemoryAddModal.svelte`
- Delete: `src/lib/stores/user-memory-store.svelte.ts`(+test)、`character-memory-store.svelte.ts`(+test)、`src/lib/utils/memory-panel-helpers.ts`
- Modify: `src/routes/+layout.svelte`:删 import(29)、state(83)、`onToggleMemory` 函数(792-794)、`addEventListener("clawgo:toggle-memory",...)`(795)、`removeEventListener("clawgo:toggle-memory",...)`(882)、header "用户记忆"按钮块(2415-2424,`<button>` 起到 `</button>`)、渲染(2492-2494)
- Modify: `src/routes/settings/characters/+page.svelte`:删 CharacterMemoryPanel import(12)+渲染(358)、"记忆"按钮(328-331)、"Memory Config"卡片(556-595)+对应 form 状态变量

**Interfaces:**
- 保留 `GlobalMemoPanel` 相关(layout 28、82、`onToggleMemo`/`clawgo:toggle-memo`(791)、2393 附近、2487-2489)全部不动。

> **⚠️ 命名碰撞警示:** `clawgo:toggle-memo`(备忘录,**保留**) vs `clawgo:toggle-memory`(用户记忆,**删除**)——只差 `ry`。删除时只动带 `ry` 的 memory 那个;`onToggleMemo`/`toggle-memo` 全部保留。删完后 `grep -n "toggle-memo\b" src/routes/+layout.svelte` 应仍有备忘录的 2 处(add+remove)。

- [ ] **Step 1:** 删 6 个前端文件(+ 任何同名 `.test.ts`)。
- [ ] **Step 2:** 清理 `+layout.svelte`:import(29)、state(83)、`onToggleMemory` 函数(792-794)、其 add/removeEventListener(795、882)、按钮块(2415-2424)、渲染(2492-2494)。**确认 `onToggleMemo`/`clawgo:toggle-memo`/GlobalMemoPanel 全部保留。**
- [ ] **Step 3:** 清理 `settings/characters/+page.svelte` 的 CharacterMemoryPanel 引用、"记忆"按钮、Memory Config 卡片与 `formAutoLearn`/`formRetentionDays`/`formMaxRetrievalCount`/`formRelevanceThreshold` 状态。
- [ ] **Step 4: Verify 零残留**

Run: `grep -rn "UserMemoryPanel\|CharacterMemoryPanel\|MemoryAddModal\|memory-store\|memory-panel-helpers" src`
Expected: 无输出。

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "refactor(ui): remove user/character memory panels and sidebar entry"
```

---

### Task 7: 删除前端 api + types 记忆定义

**Files:**
- Modify: `src/lib/api.ts`:删 `MemoryConfig`/`MemoryExtractionConfig` 类型导入(54-55)、10 个记忆函数(1608-1666)、`updateCharacter` 的 `memoryConfig` 参数(301)
- Modify: `src/lib/types.ts`:删 memory 设置字段(260-264)、`embedding_config`(265)、`AiCharacter.memory_config`(330)、`EmbeddingConfig`(284)、`MemoryExtractionConfig`(1747)、`MemoryConfig`(1754)、`MemoryNode`
- Modify: `src/lib/components/invest/DreamingConfigPanel.svelte`:删死的 `'user_memory'` 分支(10、16),只留 `'invest'`

**Interfaces:**
- Consumes: Task 6 已删所有 store/面板对这些 api 函数的调用。

- [ ] **Step 1:** 删 `api.ts` 的类型导入(54-55)、10 个记忆函数(list/get/create/update/delete/search Character Memory + listPending/approve/reject)+ `updateCharacter` 的 memoryConfig 参数。保留 `listMemoryFiles`(551)。
- [ ] **Step 2:** 删 `types.ts` 上述类型/字段(注意 `AiCharacter.memory_config` 在 330)。
- [ ] **Step 3:** 删 `DreamingConfigPanel.svelte` 的 `user_memory` 分支,保留 invest 分支。
- [ ] **Step 4: Verify**

Run: `npm run check`
Expected: 0 errors(仅既有 a11y warning)。

- [ ] **Step 5: Verify build**

Run: `npm run build`
Expected: `✓ built`。

- [ ] **Step 6: Verify 零残留**

Run: `grep -rn "CharacterMemor\|MemoryNode\|MemoryConfig\|EmbeddingConfig\|listPendingMemories\|approveMemory" src`
Expected: 无输出。

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "refactor(ui): remove memory api functions and type definitions"
```

---

## 手动冒烟(全部 Task 完成后)

- [ ] 启动 app(既有 `npm run tauri dev` 或打包启动方式)
- [ ] 开一个私聊 + 一个群聊,各发一轮,确认无 panic、回复正常
- [ ] 右侧**备忘录**面板:增/删/查正常(证明未误伤)
- [ ] `/memory` markdown 编辑器:打开、编辑、保存正常
- [ ] **invest 页 dreaming**:配置面板 + traces 完全正常(证明隔离)
- [ ] 设置→角色页:无"记忆"按钮 / 无 Memory Config 卡片,新建/编辑角色仍能保存

## Self-Review 结论

- **Spec 覆盖**:提取(T1)/注入(T2)/dreaming(T3)/角色命令+存储(T4)/models+settings+embedding(T5)/前端面板(T6)/前端 api+types(T7)——全覆盖。
- **顺序正确性**:先删调用点再删被调模块,每个 Task 后 `cargo check`/`npm run check` 必须绿,保证增量可编译。
- **隔离验证**:invest dreaming / 备忘录 / `/memory` 编辑器 三条保留线在 Global Constraints 钉死,并在冒烟清单逐条确认。
- **类型一致性**:`search_fts`/`find_duplicates`/`search_hybrid` 随 memory_store.rs 整体删除(仅记忆使用),不产生悬空引用。

### 第二轮 review 修正记录(行号经干净 Grep/Read 核实)

1. **CI clippy 缺口**(严重):CI 用 `cargo clippy -- -D warnings`(ci.yml:104)把关,本地 `cargo check` 不因 dead_code 失败,会漏。已在 T5 加 clippy 验证步。
2. **命名碰撞**(严重):`clawgo:toggle-memo`(备忘录,留)vs `clawgo:toggle-memory`(用户记忆,删)只差 `ry`。已在 T6 加警示 + 删后 grep 校验。
3. **layout 事件接线不完整**:原只写"事件(793)",实为 `onToggleMemory` 函数(792-794)+ add(795)+ remove(882)三处。已补全。
4. **api.ts 类型导入遗漏**:`MemoryConfig`/`MemoryExtractionConfig` 导入在 54-55,原计划未删。已补。
5. **types.ts AiCharacter.memory_config 遗漏**:字段在 330(不止 1747/1754 两个 interface)。已补;`MemoryExtractionConfig`=1747、`MemoryConfig`=1754 行号已校正。
- **已确认无问题**:`VectorSearchResult` 仅 models.rs 定义处出现(无外部使用者,可安全删);characters.rs 记忆命令边界 128(注释)→261(reject_memory)→约 270,准确。
