# 委员会分析模式接入 + 废弃 Research 概念清理 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把后端已有的委员会分析模式（`Mode::Research` / `Mode::Holding`）接到前端，每标的可一键切换「研究 / 实盘」并持久化；同时清理废弃的 Group Chat「Research / Driver」概念残留。

**Architecture:** 模式与 kind（hold/watch）是正交两条轴。前端按 kind 推导默认模式（watch→研究、hold→实盘，仅起点猜测），用户手动切换的票写入一份独立的 symbol→mode JSON 覆盖表（`~/.claw-go/invest/committee_mode_overrides.json`，沿用 `committee_tuning.json` 先例）。入队时前端算出每 symbol 的 effective mode 存进 queue item，启动时随 `run_committee_stream` 的 `modes` 参数传给后端（后端链路已全通，仅前端漏传）。

**Tech Stack:** Rust（Tauri command + serde_json 文件持久化）、SvelteKit（Svelte 5 runes store）、Vitest（前端单测）、i18n（en/zh 双语）。

## Global Constraints

- Svelte 5 runes（`$state`/`$derived`/`$effect`/`$props`），行为写在 store 不写在路由组件。
- 任何 UI 文案改动必须同步更新 `src-tauri/messages/en.json` 和 `src-tauri/messages/zh-CN.json`，且通过 `npm run i18n:check`。
- 后端枚举名保持 `Mode::Research` / `Mode::Holding` 不变；传输字符串用 `"research"` / `"holding"`（对齐 `parse_mode_map`，`commands/invest.rs:512-515`）；UI 文案映射为「研究 / 实盘」。
- Conventional Commits（`feat:` / `chore:` / `docs:`）。
- Rust 验证用 `cargo check`（运行时受 CLAUDE.md §11 的 MSVC runtime 已知问题限制，单测无法跑通，靠 check 验证编译）。
- 委员会轻量状态持久化先例是 JSON 文件到 `~/.claw-go/invest/`（见 `committee_tuning.json`，`commands/invest.rs:385-421`），不进 `invest.db`。
- 持久化层不知道 symbol 的 kind；「等于默认值时删除条目」的判定在前端 store，后端只负责整表存/取。

---

## 文件结构

**事项 1（接入模式）：**
- Modify: `src-tauri/src/commands/invest.rs` — 新增 `committee_mode_overrides.json` 读写 + 两个 Tauri command
- Modify: `src-tauri/src/lib.rs:424-429` 附近 — 注册两个新 command
- Modify: `src/lib/stores/invest-committee-store.svelte.ts` — modeOverrides 状态 + effectiveMode/setSymbolMode + queue item 带 mode + `_startSymbol` 传 modes
- Modify: `src/lib/stores/invest-committee-store.test.ts` — effectiveMode/setSymbolMode/传参单测
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte` — 卡片模式切换控件 + 入队带 mode
- Modify: `src-tauri/messages/en.json` + `zh-CN.json` — 新增模式 UI 文案键

**事项 2（清理）：**
- Modify: `src-tauri/messages/en.json` + `zh-CN.json` — 删除孤儿键
- Modify: `CLAUDE.md` — Research/Driver 措辞
- Modify: `docs/[done] phase-4.5-research-followup-implementation-plan.md` — 标注产物已移除

---

## Task 1: 后端持久化 + Tauri command（覆盖表存取）

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`（在 `save_committee_tuning` 之后，约 line 421 后插入）
- Modify: `src-tauri/src/lib.rs`（约 line 429 后，`load_committee_queue` 注册附近）

**Interfaces:**
- Produces:
  - `get_committee_mode_overrides() -> Result<HashMap<String, String>, String>`（Tauri command；文件不存在返回空 map）
  - `save_committee_mode_overrides(overrides: HashMap<String, String>) -> Result<(), String>`（Tauri command；整表覆盖写入）

- [ ] **Step 1: 在 `commands/invest.rs` 的 committee tuning 区块后新增持久化函数 + command**

在 `save_committee_tuning`（约 line 421）之后插入：

```rust
// ── Committee Mode Overrides ─────────────────────────────────────────────────

/// 每标的的分析模式覆盖表，持久化到 `~/.claw-go/invest/committee_mode_overrides.json`。
/// key = symbol，value = "research" | "holding"。只记录被用户手动改过、偏离默认推导的票
/// （默认：watch→research / hold→holding，由前端 store 推导）。后端只负责整表存/取，
/// 不参与默认值判定（persist 层不知道 symbol 的 kind）。
fn committee_mode_overrides_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go")
        .join("invest")
        .join("committee_mode_overrides.json")
}

#[tauri::command]
pub fn get_committee_mode_overrides() -> Result<std::collections::HashMap<String, String>, String> {
    let path = committee_mode_overrides_path();
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("read committee_mode_overrides: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("parse committee_mode_overrides: {}", e))
}

#[tauri::command]
pub fn save_committee_mode_overrides(
    overrides: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let path = committee_mode_overrides_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {}", e))?;
    }
    let json = serde_json::to_string_pretty(&overrides)
        .map_err(|e| format!("serialize mode overrides: {}", e))?;
    std::fs::write(&path, json).map_err(|e| format!("write committee_mode_overrides: {}", e))
}
```

- [ ] **Step 2: 在 `lib.rs` 注册两个 command**

在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 列表里，`commands::invest::load_committee_queue,`（line 429）后面加：

```rust
            commands::invest::get_committee_mode_overrides,
            commands::invest::save_committee_mode_overrides,
```

- [ ] **Step 3: 验证编译**

Run: `npm run rust:check`
Expected: 编译通过，无 error（warning 可接受）。若 `cargo clippy` 报未使用，确认 command 已在 `generate_handler!` 注册。

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): 委员会分析模式覆盖表后端持久化 + Tauri command"
```

---

## Task 2: Store 层 — modeOverrides 状态 + effectiveMode/setSymbolMode

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`
- Test: `src/lib/stores/invest-committee-store.test.ts`

**Interfaces:**
- Consumes: `get_committee_mode_overrides`、`save_committee_mode_overrides`（Task 1）
- Produces（store 公开方法/字段，供 Task 3、4 使用）:
  - `modeOverrides: Map<string, 'research' | 'holding'>`（`$state`）
  - `loadModeOverrides(): Promise<void>`
  - `effectiveMode(symbol: string, kind: 'hold' | 'watch'): 'research' | 'holding'`
  - `setSymbolMode(symbol: string, kind: 'hold' | 'watch', mode: 'research' | 'holding'): Promise<void>`
  - `QueueItem.mode?: 'research' | 'holding'`、`PersistedProgress` 不变（mode 存在 QueueItem 上，不进 progress）

- [ ] **Step 1: 写失败测试 — effectiveMode 默认推导**

在 `invest-committee-store.test.ts` 末尾新增 describe 块：

```typescript
describe('InvestCommitteeStore mode', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue([]);
    eventHandler = null;
  });

  it('effectiveMode defaults: watch→research, hold→holding', () => {
    const store = new InvestCommitteeStore();
    expect(store.effectiveMode('A', 'watch')).toBe('research');
    expect(store.effectiveMode('B', 'hold')).toBe('holding');
  });

  it('effectiveMode honors override over kind default', () => {
    const store = new InvestCommitteeStore();
    store.modeOverrides.set('A', 'holding'); // watch 票被改成实盘
    expect(store.effectiveMode('A', 'watch')).toBe('holding');
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts -t "mode"`
Expected: FAIL（`effectiveMode is not a function` / `modeOverrides` undefined）

- [ ] **Step 3: 实现 modeOverrides 状态 + effectiveMode**

在 `invest-committee-store.svelte.ts` 的 `InvestCommitteeStore` 类里，`rolePrompts` 字段附近（约 line 203）加状态：

```typescript
  modeOverrides = $state<Map<string, 'research' | 'holding'>>(new Map());
```

在 `// ── Tuning ──` 区块前（约 line 480 前）加方法：

```typescript
  // ── Analysis Mode (research/holding) ────────────────────────────────────

  /** Default mode by asset kind — only a starting guess, any symbol can be
   *  overridden. watch (not held) → research (intrinsic attractiveness);
   *  hold (held) → holding (portfolio-aware add/trim). */
  private _defaultMode(kind: 'hold' | 'watch'): 'research' | 'holding' {
    return kind === 'watch' ? 'research' : 'holding';
  }

  /** Effective mode = manual override if present, else kind default. */
  effectiveMode(symbol: string, kind: 'hold' | 'watch'): 'research' | 'holding' {
    return this.modeOverrides.get(symbol) ?? this._defaultMode(kind);
  }

  async loadModeOverrides() {
    try {
      const obj = await invoke<Record<string, string>>('get_committee_mode_overrides');
      const map = new Map<string, 'research' | 'holding'>();
      for (const [sym, m] of Object.entries(obj)) {
        if (m === 'research' || m === 'holding') map.set(sym, m);
      }
      this.modeOverrides = map;
    } catch (e) {
      console.error('get_committee_mode_overrides failed:', e);
      this.modeOverrides = new Map();
    }
  }

  /** Set a symbol's mode. If it equals the kind default, the override is
   *  removed (keeps the table minimal); otherwise it's recorded. Persists
   *  the full table to disk. */
  async setSymbolMode(symbol: string, kind: 'hold' | 'watch', mode: 'research' | 'holding') {
    const next = new Map(this.modeOverrides);
    if (mode === this._defaultMode(kind)) {
      next.delete(symbol);
    } else {
      next.set(symbol, mode);
    }
    this.modeOverrides = next;
    try {
      await invoke('save_committee_mode_overrides', {
        overrides: Object.fromEntries(next),
      });
    } catch (e) {
      console.error('save_committee_mode_overrides failed:', e);
    }
  }
```

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts -t "mode"`
Expected: PASS（两个 test 都过）

- [ ] **Step 5: 写失败测试 — setSymbolMode 写入/删除 + 落盘**

在同一 describe 块内追加：

```typescript
  it('setSymbolMode records non-default and persists', async () => {
    const store = new InvestCommitteeStore();
    await store.setSymbolMode('A', 'watch', 'holding'); // 偏离默认
    expect(store.modeOverrides.get('A')).toBe('holding');
    const save = invokeMock.mock.calls.find((c) => c[0] === 'save_committee_mode_overrides');
    expect(save?.[1]).toEqual({ overrides: { A: 'holding' } });
  });

  it('setSymbolMode back to default removes the override', async () => {
    const store = new InvestCommitteeStore();
    store.modeOverrides.set('A', 'holding');
    await store.setSymbolMode('A', 'watch', 'research'); // 回到 watch 默认
    expect(store.modeOverrides.has('A')).toBe(false);
    const save = invokeMock.mock.calls.find((c) => c[0] === 'save_committee_mode_overrides');
    expect(save?.[1]).toEqual({ overrides: {} });
  });
```

- [ ] **Step 6: 运行测试确认通过**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts -t "mode"`
Expected: PASS（实现已在 Step 3 完成，这两个 test 直接过）

- [ ] **Step 7: Commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts src/lib/stores/invest-committee-store.test.ts
git commit -m "feat(invest): committee store 增加 effectiveMode + setSymbolMode 覆盖逻辑"
```

---

## Task 3: Store 层 — queue item 带 mode + `_startSymbol` 传 modes

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`
- Test: `src/lib/stores/invest-committee-store.test.ts`

**Interfaces:**
- Consumes: `QueueItem`、`addToQueue`、`_startSymbol`、`_drainQueue`（现有）
- Produces:
  - `addToQueue(symbols, snapshot?, modes?)` — 新增可选 `modes?: Record<string, 'research'|'holding'>` 参数；入队时把每 symbol 的 mode 存进 `QueueItem.mode`
  - `_startSymbol` 调用 `run_committee_stream` 时带 `modes: { [symbol]: item.mode }`（item.mode 缺失则省略，后端回退 holding）

- [ ] **Step 1: 写失败测试 — addToQueue 带 modes，run_committee_stream 收到 modes**

在 `mode` describe 块内追加：

```typescript
  it('passes per-symbol mode to run_committee_stream', async () => {
    const store = new InvestCommitteeStore();
    store.maxConcurrent = 2;
    await store.addToQueue(['A', 'B'], undefined, { A: 'research', B: 'holding' });
    const callA = streamCalls().find((c) => c[1].symbols[0] === 'A');
    const callB = streamCalls().find((c) => c[1].symbols[0] === 'B');
    expect(callA?.[1].modes).toEqual({ A: 'research' });
    expect(callB?.[1].modes).toEqual({ B: 'holding' });
  });
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts -t "passes per-symbol mode"`
Expected: FAIL（`modes` 为 undefined）

- [ ] **Step 3: QueueItem 加 mode 字段**

在 `invest-committee-store.svelte.ts` 的 `QueueItem` interface（约 line 143-148）加字段：

```typescript
export interface QueueItem {
  symbol: string;
  status: QueueItemStatus;
  error?: string;
  progress?: PersistedProgress | null;
  mode?: 'research' | 'holding';
}
```

- [ ] **Step 4: addToQueue 接收 modes 并写入 queue item**

修改 `addToQueue` 签名与入队逻辑（约 line 223-245）。把签名改为：

```typescript
  async addToQueue(
    symbols: string[],
    snapshot?: PortfolioSnapshot,
    modes?: Record<string, 'research' | 'holding'>,
  ) {
```

在 `this.queue.push({ symbol: sym, status: 'queued' });`（约 line 236）一行改为：

```typescript
      this.queue.push({ symbol: sym, status: 'queued', mode: modes?.[sym] });
```

- [ ] **Step 5: `_startSymbol` 读 queue item.mode 传给后端**

修改 `_startSymbol`（约 line 430-443）的 invoke 调用：

```typescript
  private _startSymbol(symbol: string) {
    this._markRunning(symbol);
    const item = this.queue.find((q) => q.symbol === symbol);
    const modes = item?.mode ? { [symbol]: item.mode } : undefined;
    invoke<CommitteeResult[]>('run_committee_stream', {
      symbols: [symbol],
      debateRounds: null,
      dryRun: false,
      modes,
    }).catch((e) => {
      const found = this.queue.find((q) => q.symbol === symbol);
      if (found && found.status === 'running') {
        this._settleQueue(symbol, 'failed', String(e));
      }
    });
  }
```

- [ ] **Step 6: retrySymbol 保留 mode**

`retrySymbol`（约 line 248-250）当前 `addToQueue([symbol])` 不带 mode，重跑会丢模式。改为查当前 item 的 mode 重新传入：

```typescript
  async retrySymbol(symbol: string) {
    const existing = this.queue.find((q) => q.symbol === symbol);
    const modes = existing?.mode ? { [symbol]: existing.mode } : undefined;
    await this.addToQueue([symbol], undefined, modes);
  }
```

- [ ] **Step 7: 运行 mode + queue 全部测试确认通过**

Run: `npm test -- src/lib/stores/invest-committee-store.test.ts`
Expected: PASS（含原有 queue 测试 + 新 mode 测试。原有 `enqueues symbols` 等测试不传 modes，item.mode 为 undefined，`_startSymbol` 省略 modes，不影响断言）

- [ ] **Step 8: Commit**

```bash
git add src/lib/stores/invest-committee-store.svelte.ts src/lib/stores/invest-committee-store.test.ts
git commit -m "feat(invest): queue item 携带分析模式，run_committee_stream 传 modes"
```

---

## Task 4: UI 层 — 卡片模式切换控件 + 入队带 mode + i18n

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`
- Modify: `src-tauri/messages/en.json`、`src-tauri/messages/zh-CN.json`

**Interfaces:**
- Consumes: `store.effectiveMode`、`store.setSymbolMode`、`store.modeOverrides`、`store.loadModeOverrides`、`addToQueue(symbols, snapshot, modes)`（Task 2/3）

- [ ] **Step 1: 新增 i18n 键（en + zh，必须同步）**

在 `src-tauri/messages/en.json` 找到 `invest_committee_include_watch` 附近，加入：

```json
  "invest_committee_mode_research": "Research",
  "invest_committee_mode_holding": "Live",
  "invest_committee_mode_overridden": "Manually set",
  "invest_committee_mode_switch_to_research": "Switch to research mode (intrinsic value, ignores portfolio)",
  "invest_committee_mode_switch_to_holding": "Switch to live mode (portfolio-aware, your cash/concentration/cost)",
```

在 `src-tauri/messages/zh-CN.json` 对应位置加入：

```json
  "invest_committee_mode_research": "研究",
  "invest_committee_mode_holding": "实盘",
  "invest_committee_mode_overridden": "手动设置",
  "invest_committee_mode_switch_to_research": "切到研究模式（看标的内在价值，忽略组合）",
  "invest_committee_mode_switch_to_holding": "切到实盘模式（考虑你的现金/集中度/成本）",
```

- [ ] **Step 2: 运行 i18n 校验**

Run: `npm run i18n:check`
Expected: PASS（en/zh 键对齐，无缺失）

- [ ] **Step 3: `allAssets` 派生项附加 effective mode**

在 `CommitteeLiveTab.svelte` 的 `allAssets`（约 line 90-100），给每个 asset 附加 mode。改为：

```typescript
  const allAssets = $derived.by(() => {
    const assets: { symbol: string; name: string | null; kind: 'hold' | 'watch'; mode: 'research' | 'holding'; overridden: boolean }[] = [
      ...invest.holdHoldings.map((h) => ({
        symbol: h.symbol, name: h.name, kind: h.kind as 'hold',
        mode: store.effectiveMode(h.symbol, 'hold'),
        overridden: store.modeOverrides.has(h.symbol),
      })),
    ];
    if (includeWatch) {
      assets.push(
        ...invest.watchHoldings.map((h) => ({
          symbol: h.symbol, name: h.name, kind: h.kind as 'watch',
          mode: store.effectiveMode(h.symbol, 'watch'),
          overridden: store.modeOverrides.has(h.symbol),
        })),
      );
    }
    return assets;
  });
```

- [ ] **Step 4: 入队函数把 effectiveMode 传进 modes**

`runAll`（约 line 151-155）和 `runSymbol`（约 line 157-161）改为构造 modes：

```typescript
  function runAll() {
    if (allAssets.length === 0) return;
    const syms = allAssets.map((a) => a.symbol);
    const modes: Record<string, 'research' | 'holding'> = {};
    for (const a of allAssets) modes[a.symbol] = a.mode;
    store.addToQueue(syms, buildSnapshot(), modes);
  }

  function runSymbol(sym: string) {
    expandedSymbols.add(sym);
    expandedSymbols = new Set(expandedSymbols);
    const asset = allAssets.find((a) => a.symbol === sym);
    const modes = asset ? { [sym]: asset.mode } : undefined;
    store.addToQueue([sym], buildSnapshot(), modes);
  }
```

- [ ] **Step 5: 添加切换处理函数 + onMount 加载覆盖表**

在 `runSymbol` 附近加切换函数：

```typescript
  function toggleMode(sym: string, kind: 'hold' | 'watch', current: 'research' | 'holding', e: Event) {
    e.stopPropagation();
    const next = current === 'research' ? 'holding' : 'research';
    store.setSymbolMode(sym, kind, next);
  }
```

修改 `onMount`（约 line 174-176）：

```typescript
  onMount(() => {
    store.loadQueue();
    store.loadModeOverrides();
  });
```

- [ ] **Step 6: 卡片 header 加模式切换徽章**

在 `CommitteeLiveTab.svelte` 卡片 header 的 kind badge（约 line 350：`<span class="badge {asset.kind}">...`）之后插入模式切换按钮：

```svelte
        <button
          class="mode-toggle {asset.mode}"
          class:overridden={asset.overridden}
          onclick={(e) => toggleMode(asset.symbol, asset.kind, asset.mode, e)}
          title={asset.mode === 'research'
            ? t('invest_committee_mode_switch_to_holding')
            : t('invest_committee_mode_switch_to_research')}
        >
          {asset.mode === 'research' ? t('invest_committee_mode_research') : t('invest_committee_mode_holding')}
          {#if asset.overridden}<span class="mode-dot" title={t('invest_committee_mode_overridden')}></span>{/if}
        </button>
```

- [ ] **Step 7: 加样式**

在 `<style>` 块内 `.badge` 样式附近加：

```css
  .mode-toggle {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 2px 9px; border-radius: var(--radius-sm);
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.3px;
    border: 1px solid var(--border); background: var(--bg-input);
    cursor: pointer; flex-shrink: 0; transition: all 0.15s;
  }
  .mode-toggle:hover { border-color: var(--accent-muted); }
  .mode-toggle.research { color: var(--accent); }
  .mode-toggle.holding { color: var(--color-success); }
  .mode-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--accent-muted); }
```

- [ ] **Step 8: 构建验证**

Run: `npm run build`
Expected: 构建成功，无 TypeScript / Svelte 编译错误。

- [ ] **Step 9: Commit**

```bash
git add src/lib/components/invest/CommitteeLiveTab.svelte src-tauri/messages/en.json src-tauri/messages/zh-CN.json
git commit -m "feat(invest): 委员会卡片研究/实盘模式一键切换 + 入队带模式"
```

---

## Task 5: 清理废弃 Research/Driver i18n 与文档（事项 2）

**Files:**
- Modify: `src-tauri/messages/en.json`、`src-tauri/messages/zh-CN.json`
- Modify: `CLAUDE.md`
- Modify: `docs/[done] phase-4.5-research-followup-implementation-plan.md`

- [ ] **Step 1: 确认孤儿键零引用**

Run: `grep -rn "groupChat_kindResearch\|groupChat_kindDriver\|groupChat_driverPlaceholder\|groupChat_researchPlaceholder\|groupChat_turnResearch\|groupChat_turnReview\|groupChat_researchArtifact" src/`
Expected: 无任何匹配（全部零引用）。若有匹配，停止并报告——说明该键仍被使用，不能删。

- [ ] **Step 2: 确认 groupChat_kindRoundtable 是否仍被引用**

Run: `grep -rn "groupChat_kindRoundtable\|groupChat_roundtablePlaceholder" src/`
Expected: 记录结果。`groupChat_roundtablePlaceholder` 在 `GroupChatLayout.svelte:813` 有引用，**保留**。`groupChat_kindRoundtable` 若零引用则一并删除，有引用则保留。

- [ ] **Step 3: 从 en.json 删除孤儿键**

在 `src-tauri/messages/en.json` 删除这些行（连同末尾逗号，保持 JSON 合法）：
`groupChat_kindResearch`、`groupChat_kindDriver`、`groupChat_driverPlaceholder`、`groupChat_researchPlaceholder`、`groupChat_turnResearch`、`groupChat_turnReview`、`groupChat_researchArtifact`。
`groupChat_kindRoundtable` 按 Step 2 结果决定是否删。

- [ ] **Step 4: 从 zh-CN.json 删除相同的键**

在 `src-tauri/messages/zh-CN.json` 删除与 Step 3 完全相同的键集合。

- [ ] **Step 5: 运行 i18n 校验**

Run: `npm run i18n:check`
Expected: PASS（en/zh 删除集合一致，键仍对齐）

- [ ] **Step 6: 更新 CLAUDE.md 措辞**

在 `CLAUDE.md` 找到提及 Claude Session Hub 概念那句（约 line 7，列举 Group Chats, Memo, Roundtable, Driver/Copilot, Research）。把已废弃的 Research 与 Driver/Copilot 从「已实现概念」清单移除——改为只保留实际存在的概念（Group Chats、Memo、Roundtable）。具体改法：

原文：
```
adding Claude Session Hub concepts (Group Chats, Memo, Roundtable, Driver/Copilot, Research) and the openInvest quant subsystem
```
改为：
```
adding Claude Session Hub concepts (Group Chats, Memo, Roundtable) and the openInvest quant subsystem
```

- [ ] **Step 7: 标注废弃计划文档**

在 `docs/[done] phase-4.5-research-followup-implementation-plan.md` 文件顶部（第一行后）插入一行废弃声明：

```markdown
> **[废弃 2026-06-20]** 本计划的 Group Chat Research/Driver 产物已从代码树移除（Rust 类型、storage、turn 枚举变体、前端消费者均不存在）。文档保留作历史记录，不代表现状。
```

- [ ] **Step 8: 构建 + i18n 终检**

Run: `npm run build && npm run i18n:check`
Expected: 均 PASS。

- [ ] **Step 9: Commit**

```bash
git add src-tauri/messages/en.json src-tauri/messages/zh-CN.json CLAUDE.md "docs/[done] phase-4.5-research-followup-implementation-plan.md"
git commit -m "chore(group-chat): 清理废弃 Research/Driver 概念的孤儿 i18n 与文档引用"
```

---

## Task 6: 全量验证

- [ ] **Step 1: 前端质量门**

Run: `npm run build && npm run i18n:check && npm test -- src/lib/stores/invest-committee-store.test.ts`
Expected: 全部 PASS。

- [ ] **Step 2: Rust 质量门**

Run: `npm run rust:check`
Expected: 编译通过，clippy 无 `-D warnings` 报错。

- [ ] **Step 3: 手动冒烟（可选，需 `npm run tauri dev`）**

启动后到 `/invest` → 委员会 → Live 标签：
- 每张卡片 kind badge 旁出现「研究/实盘」切换徽章。
- watch 票默认显示「研究」，hold 票默认显示「实盘」。
- 点击徽章切换，出现「手动设置」小圆点；刷新后切换状态保留（落盘生效）。
- 切回默认值后小圆点消失（覆盖被移除）。
- 跑一个 symbol，确认后端按所选模式分析（研究模式下 Risk 角色忽略集中度/现金）。

---

## Self-Review

**Spec 覆盖检查：**
- 模式语义（研究/实盘正交于 kind）→ Task 2 `_defaultMode`/`effectiveMode`、Task 4 UI 文案。✓
- 默认推导（watch→研究、hold→实盘，仅起点）→ Task 2 `_defaultMode`。✓
- 独立覆盖表 + 只存手动改过的票 → Task 1 持久化、Task 2 `setSymbolMode`（等于默认即删）。✓
- 持久化先例 JSON/commands 层 → Task 1 沿用 `committee_tuning.json`。✓
- `_startSymbol` 补 modes（核心缺口）→ Task 3。✓
- UI 一键切换 + 覆盖标记 → Task 4 Step 6-7。✓
- kind 变化时覆盖优先 → `effectiveMode` 以 symbol 为 key，与 kind 解耦，覆盖天然优先。✓
- 错误处理（加载失败降级空 map、落盘失败 console.error 不阻断、后端回退 holding）→ Task 2 loadModeOverrides/setSymbolMode 的 catch、后端 `parse_mode_map` 已有。✓
- 事项 2 清理（i18n + CLAUDE.md + 废弃计划）→ Task 5。✓
- 委员会 Mode::Research ≠ 被清理的群聊 Research → Task 5 Step 1 grep 仅匹配 `groupChat_*` 前缀，不碰 invest。✓
- 测试（effectiveMode/setSymbolMode/传参 + Rust 编译）→ Task 2/3 单测、Task 1/6 cargo check。✓

**占位符扫描：** 无 TBD/TODO/「类似 Task N」/「适当处理」。所有代码步骤含完整代码。

**类型一致性：** `effectiveMode(symbol, kind)`、`setSymbolMode(symbol, kind, mode)`、`QueueItem.mode`、`addToQueue(symbols, snapshot?, modes?)`、`run_committee_stream` 的 `modes` 参数——跨 Task 2/3/4 命名与签名一致。后端 command 名 `get_committee_mode_overrides`/`save_committee_mode_overrides` 跨 Task 1/2 一致。

**说明：** Rust 单测受 §11 已知 runtime 问题无法运行，Task 1 用 `cargo check` 验证编译（后端逻辑极简——纯文件读写 + serde，无复杂分支，编译通过即可信）。
