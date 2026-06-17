# 委员会直播 — 状态保留 + 并发限制

## Context

两个问题：
1. `runCommittee` 无条件重置所有状态，导致未在本次运行范围内的持仓状态丢失
2. `run_committee_batch_stream` / `run_committee_batch` 用 `for + tokio::spawn` 一次性启动所有符号，没有符号级并发限制

## 场景枚举（状态保留）

| 场景 | 触发方式 | 行为 |
|------|----------|------|
| S1 Run All，部分已完成 | Run All | 跳过已完成的（done && !error），只运行未完成的 |
| S2 Run All，全部未完成 | Run All | 全部运行，等同当前行为 |
| S3 Run All，全部已完成 | Run All | 全部重新运行（用户明确要求） |
| S4 Run Selected，含已完成 | Run Selected | 选中的全部运行（含重新运行已完成的），未选中的保留状态 |
| S5 Run Selected，单个已完成 | Run Selected | 重新运行该符号，其他全部保留 |
| S6 Run Selected，全部未完成 | Run Selected | 运行选中的，其他保留 |
| S7 运行中再次运行 | Run All/Selected | 被 `store.running` 守卫阻止，无变化 |

**关键差异**：Run All 跳过已完成（增量模式）；Run Selected 尊重用户选择（可重新运行已完成的）。

## 修改范围

### Part A：前端状态保留（store + 组件）

#### A1：`src/lib/stores/invest-committee-store.svelte.ts` — `runCommittee` 限定重置范围

当前问题：
```ts
this.results = [];                    // 清空全部结果
this.toolCallHistory = [];            // 清空全部工具调用
this.perSymbolProgress = new Map();   // 替换整个 Map
```

改为：
```ts
this.activeSymbols = symbols;
const runSet = new Set(symbols);

// results：只移除本次要运行的符号的旧结果，保留其他
this.results = this.results.filter(r => !runSet.has(r.symbol));

// perSymbolProgress：只重置本次要运行的符号，保留其他
for (const s of symbols) {
  this.perSymbolProgress.set(s, { /* fresh state */ });
}
// 不再整体替换 Map

// toolCallHistory：只移除本次要运行的符号的记录，保留其他
this.toolCallHistory = this.toolCallHistory.filter(e => !runSet.has(e.symbol));
```

#### A2：`symbol_complete` 事件 — 替换而非追加

```ts
const idx = this.results.findIndex(r => r.symbol === event.symbol);
if (idx >= 0) this.results[idx] = event.result;
else this.results.push(event.result);
```

#### A3：`src/lib/components/invest/CommitteeLiveTab.svelte` — Run All 过滤已完成

```ts
// Run All: 跳过已完成的
const toRun = allAssets
  .filter(a => {
    const p = store.perSymbolProgress.get(a.symbol);
    return !(p?.done && !p.error);
  })
  .map(a => a.symbol);
runSymbols(toRun);
```

Run Selected 不做过滤，直接传入 `selectedSymbols`。

### Part B：后端符号级并发限制

#### B1：`src-tauri/src/invest/committee/orchestrator.rs` — `run_committee_batch_stream`

当前：`for + tokio::spawn` 一次性启动全部。

改为：用 `tokio::sync::Semaphore` 限制并发为 5：
```rust
const MAX_CONCURRENT_SYMBOLS: usize = 5;

pub async fn run_committee_batch_stream(...) -> Vec<Result<CommitteeResult, String>> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SYMBOLS));
    let mut handles = Vec::with_capacity(symbols.len());

    for symbol in symbols {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        // ... clone config, emitter, portfolio ...
        handles.push((symbol.clone(), tokio::spawn(async move {
            let _permit = permit;  // 持有 permit 直到任务完成
            run_committee(&*client, &symbol, &config, Some(emitter), dry_run, Some(portfolio)).await
        })));
    }
    // ... collect results ...
}
```

注意：`acquire_owned().await` 在 for 循环内会阻塞，当已有 5 个任务运行时，第 6 个会等待直到有任务完成。这正是我们要的效果。

#### B2：`run_committee_batch` — 同样加并发限制

同 B1 的模式，应用于非 streaming 版本。

### Part C：字符限制提升 + `**` 去除节约字符

#### C1：`src-tauri/src/invest/committee/roles.rs` — `max_chars()` 提升

```rust
// 当前
Self::Quant | Self::Risk => 550,
Self::Cio => 600,
// 改为
Self::Quant | Self::Risk => 700,
Self::Cio => 700,
```

同步更新 `test_max_chars` 测试断言（Quant/Risk 550→700，CIO 600→700）。

#### C2：`roles.rs` — `hard_truncate` 前去除 `**`

在 `hard_truncate()` 入口处，先 strip 掉所有 `**` 标记，再进行截断计算：
```rust
pub fn hard_truncate(text: &str, role: CommitteeRole, _round: usize) -> (String, bool) {
    // 去除 ** 节约字符数
    let text = strip_bold_markers(text);
    let max = role.max_chars();
    // ... 原有截断逻辑 ...
}

fn strip_bold_markers(text: &str) -> String {
    text.replace("**", "")
}
```

这样做的好处：
- 每对 `**` 节省 4 个字符，LLM 输出中粗体标记常见，累计可省 20-40 字符
- parser 的 `matches_key_line` 已支持无 `**` 的纯文本格式，不受影响
- `extract_field` 中的 `strip_markdown_formatting` 也会清理残留，双重保险
- replay 页面的 `raw_text` 显示为纯文本，无粗体渲染（可接受）

#### C3：`roles.rs` — `length_constraint_suffix` 同步

prompt 中的字符限制描述自动跟随 `max_chars()`，无需额外改动。

## 不改动

- `pipeline-config.ts` — `getStepState` 已正确处理有/无 progress 的情况
- `invest-verdict.ts` — 不涉及
- Governor（`llm/governor.rs`）— 保留现有 per-provider 8 并发限制，与符号级限制互补
- `run_committee_stream` Tauri command — 无需修改，它调用 `run_committee_batch_stream`

## 验证

1. `cargo check --manifest-path src-tauri/Cargo.toml` — Rust 编译检查
2. `cargo test --manifest-path src-tauri/Cargo.toml committee::roles::tests` — roles 单元测试
2. `npm run check` — TypeScript 类型检查
3. `npm run lint` — 代码规范
4. `npm run build` — 构建验证
