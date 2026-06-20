# 委员会分析模式接入 + 废弃 Research 概念清理 — 设计文档

日期：2026-06-20
状态：待实现

## 背景

openInvest 委员会(`invest/committee/`)后端已经实现了两种分析模式 `Mode::Holding` / `Mode::Research`，全链路（orchestrator / cli_executor prompt 块 / analysis Gate2 跳过 / `run_committee_stream` 的 `modes` 参数）都通了。唯一缺口：前端 `invest-committee-store` 的 `_startSymbol` 调用 `run_committee_stream` 时**没传 `modes` 参数**，导致所有 symbol 都退化成默认的 `Mode::Holding`。

同时，CLAUDE.md 提到的 Claude Session Hub「Research / Driver / Roundtable」群聊概念，其代码（Rust 类型、storage、turn 枚举变体、前端消费者）已从代码树移除，只剩孤儿 i18n 字符串和过时文档引用，需要清理。

本设计涵盖两件**相互独立、可分别提交**的事：

- **事项 1（主）**：把后端已有的分析模式接到前端委员会，每标的可一键切换并持久化。
- **事项 2（清理）**：删除废弃的 Group Chat「Research / Driver」概念残留。

## 核心洞见：模式与 kind 是正交的两条轴

之前的误区是把「持不持有(kind)」和「用哪个分析模式」耦合成一条线。实际是 2×2 全组合，每格都成立：

| | 实盘模式 (Holding) | 研究模式 (Research) |
|---|---|---|
| **已持仓 (hold)** | ① 我的真实仓位，怎么加减 | ② 抛开我的持仓，这票对一个空仓账户值不值得买 |
| **未持仓 (watch)** | ③ 用我现有资金，该不该建这个仓 | ④ 对一个空仓账户，这票值不值得买 |

**模式这条轴选的是「站在谁的账户视角分析」：**

- **实盘模式 (`Mode::Holding`)** = 本账户视角。考虑我的现金够不够、集中度会不会太高、（若持有）真实成本盈亏。裁决 = 对我这个账户的加减仓动作。
- **研究模式 (`Mode::Research`)** = 空白账户视角。抛开组合（忽略现金/集中度），纯看标的自身（基本面/技术面/估值/催化剂）值不值得买。裁决 BUY/HOLD/SELL = 值得买 / 观望 / 规避（吸引力，而非加减仓）。成本基准取关注价（`HoldingKind::Watch` 记录）。

**UI 文案与后端枚举解耦**：后端枚举名保持 `Mode::Research` / `Mode::Holding` 不变；前端展示层映射为「研究 / 实盘」。传输用的字符串仍是 `"research"` / `"holding"`（对齐 `parse_mode_map`）。

**kind (hold/watch)** = 我实际持不持有，是独立的另一条轴，继续用现有 HOLD/WATCH 徽章展示，**不参与模式决策**。

## 默认模式与覆盖

- **默认（起点猜测，非绑定）**：`watch → 研究`，`hold → 持仓`。理由是便利性——刚加入关注的票多半想研究，持仓票多半看加减仓。这只是初始值，任何票都能一键改。
- `effectiveMode(symbol, kind) = override[symbol] ?? defaultByKind(kind)`
- 只有被**手动切换过**的票进独立覆盖表；恢复默认 = 从覆盖表删除该条（保持表精简）。
- kind 变化时（如 watch 转 hold）：若该票在覆盖表中，覆盖优先，不受 kind 变化影响；不在覆盖表中则跟随新 kind 的默认。

## 架构：三层改动（事项 1）

### 1. 持久化层（新）

委员会相关的轻量状态（`committee_tuning.json`、live-queue）走 **JSON 文件**持久化到 `~/.claw-go/invest/`（通过 `dirs::home_dir()`），不进 `invest.db`。覆盖表沿用此先例。

- 新增持久化函数（放在 `commands/invest.rs`，与 `committee_tuning` 的读写函数相邻，复用其 `~/.claw-go/invest/` 路径风格），持久化一份 `HashMap<String, String>`（symbol → "research" | "holding"），存为 `committee_mode_overrides.json`。
- 提供读写：load（文件不存在返回空 map）/ write（整表覆盖写入）。
- 注：persist 层不知道 symbol 的 kind（kind 在 portfolio 数据里）。「等于默认值时删除」的判定放在**前端 store**——store 决定 map 里该有哪些条目，然后整表传给后端写入。后端只负责「把这份 map 原样存盘」和「读出来」。

### 2. 命令层（新）

`commands/invest.rs` 新增两个 Tauri command：

- `get_committee_mode_overrides() -> HashMap<String, String>`
- `set_committee_mode_override(symbol: String, mode: Option<String>)`

在 `tauri::generate_handler!` 注册。

### 3. Store 层

`src/lib/stores/invest-committee-store.svelte.ts`：

- 新增 `modeOverrides = $state<Map<string, 'research' | 'holding'>>(new Map())`。
- `loadModeOverrides()`：从 `get_committee_mode_overrides` 加载，进程内随 `loadQueue` 一起初始化。
- `effectiveMode(symbol: string, kind: 'hold' | 'watch'): 'research' | 'holding'`：`override ?? (kind === 'watch' ? 'research' : 'holding')`。
- `setSymbolMode(symbol, kind, mode)`：若 `mode === defaultByKind(kind)` 则从 map 删除并调用 `set_committee_mode_override(symbol, null)`；否则写入 map 并 `set_committee_mode_override(symbol, mode)`。
- **修复缺口**：`_startSymbol`（约 line 432）和批量入队路径，调用 `run_committee_stream` 时补 `modes` 参数。`modes` 是 `Record<symbol, mode>`，由调用方根据 effectiveMode 算出。由于 `_startSymbol` 单 symbol 启动，需要拿到该 symbol 的 kind——kind 在入队时已知，方案：`addToQueue` 接收的 symbol 列表附带 kind（或在 queue item 上记录 effectiveMode 快照），`_startSymbol` 读取它构造单元素 modes map。

实现细节：queue item 增加 `mode?: 'research' | 'holding'` 字段，入队时由 `CommitteeLiveTab` 用 `effectiveMode(symbol, kind)` 算好传入（kind 在 UI 层已知，store 不需要访问 portfolio）。`_startSymbol` 读 `queueItem.mode` 构造 `modes`。这样 store 不依赖 invest-store，边界干净。queue item 的 `mode` 字段纳入现有 `PersistedProgress` / queue 持久化。

### 4. UI 层

`src/lib/components/invest/CommitteeLiveTab.svelte` 卡片 header（约 line 345-381）：

- 现有 HOLD/WATCH `kind` badge **保留不动**（资产类型，与模式无关）。
- `allAssets` 派生项给每个 asset 附加 `mode = store.effectiveMode(symbol, kind)`。
- 新增模式切换控件：显示当前 effectiveMode（如「研究」/「持仓」小徽章/按钮），点击在 research ⇄ holding 间切换，切换即调用 `store.setSymbolMode` 落盘。
- 被手动覆盖的票（symbol 在 `modeOverrides` 中）加轻微视觉标记（如小圆点），让用户一眼看出「这只票手动调过、没走默认」。
- 入队时（`runAll` / `runSymbol`）把每个 symbol 的 `effectiveMode` 写进 queue item 的 `mode` 字段。

### 数据流

```
用户点切换 → setSymbolMode → (写/删 modeOverrides + set_committee_mode_override 落盘)
入队 → CommitteeLiveTab 算 effectiveMode → queue item.mode
启动 → _startSymbol 读 queue item.mode → run_committee_stream({ modes: { [sym]: mode } })
后端 → parse_mode_map → Mode 枚举 → orchestrator 按模式跑 → 裁决
```

## 事项 2：清理范围

1. **i18n（先 grep 确认零引用再删）**：`messages/en.json` + `messages/zh-CN.json` 删除孤儿键：`groupChat_kindResearch`、`groupChat_kindDriver`、`groupChat_driverPlaceholder`、`groupChat_researchPlaceholder`、`groupChat_turnResearch`、`groupChat_turnReview`、`groupChat_researchArtifact`。`groupChat_kindRoundtable` 需单独确认是否仍被引用（roundtable 创建路径仍在用），有引用则保留。
2. **文档**：`CLAUDE.md` overview 把 Research / Driver 从「已实现概念」措辞中移除或标注废弃。
3. **归档**：`docs/[done] phase-4.5-research-followup-implementation-plan.md` 标注其产物已不在代码树（或归档）。

注意：事项 2 仅删除「Group Chat Research/Driver」相关。openInvest 委员会的 `Mode::Research`（事项 1 接入的）是完全不同的东西，**不在清理范围**。

## 错误处理

- 覆盖表加载失败：`loadModeOverrides` catch 后降级为空 map（全走默认推导），与现有 `loadQueue` 容错风格一致。
- `set_committee_mode_override` 落盘失败：console.error，内存 map 仍更新（本次会话生效），不阻断分析。
- 后端 `parse_mode_map` 已对未知/缺失值回退 `Holding`，前端漏传或传错不会崩。

## 测试

- **Rust**：`committee_mode_overrides` 的 load/set/delete 逻辑单测；`set_mode_override(sym, None)` 删除条目、`Some(m)` 写入。（`cargo check` 验证编译；运行受 §11 已知 MSVC runtime 问题限制。）
- **前端**：`invest-committee-store.test.ts` 增加：
  - `effectiveMode` 推导（watch→research / hold→holding，覆盖优先）。
  - `setSymbolMode`：改成非默认值写入 map、改回默认值从 map 删除。
  - `_startSymbol` / 入队路径正确把 `modes` 传给 `run_committee_stream`（mock invoke 断言参数）。

## 提交划分

- Commit 1（事项 1）：`feat(invest): 委员会分析模式前端接入(研究/实盘)+每标的覆盖持久化`
- Commit 2（事项 2）：`chore(group-chat): 清理废弃 Research/Driver 概念的孤儿 i18n 与文档引用`

## 验证

`npm run build`、`npm run i18n:check`（清理后 i18n 键必须 en/zh 对齐）、`npm run rust:check`、相关单测。
