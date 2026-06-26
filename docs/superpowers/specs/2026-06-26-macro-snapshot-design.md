# MacroSnapshot: 委员会宏观指标快照注入

## 背景

投资委员会（Committee）运行时，宏观指标（上证指数、北向资金、VIX 等 17 个）已缓存在 `macro_cache` 表中，但仅作为自由文本注入 Macro 角色 prompt。用户希望在分析结果中直接展示精确数值，而非只看 LLM 的综合判断。

## 设计决策

| 决策点 | 结论 | 理由 |
|---|---|---|
| 数据来源 | 直接从 macro_cache 读取（方案 A） | 数据精确，不依赖 LLM 输出格式，不浪费 token |
| 数据归属 | `CommitteeResult` 顶层字段 | 语义上是委员会运行时的市场快照，不属于任何单一角色的 LLM 输出 |
| 过期处理 | 无条件注入，有值就填 | macro_cache 的刷新策略（交易时段每 15 分钟）已保证时效性；收盘后复盘数据仍有效，不应标记过期 |

## 数据结构

新增 `MacroSnapshot` struct（`parser.rs`）：

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroSnapshot {
    pub sh_composite_close: Option<f64>,   // 上证指数
    pub sh_composite_vol20: Option<f64>,   // 上证指数20日波动率(%)
    pub northbound_net: Option<f64>,       // 北向资金净流入(亿)
    pub vix: Option<f64>,                  // VIX恐慌指数
    pub gold: Option<f64>,                 // 国际金价(USD)
    pub advance_count: Option<f64>,        // 上涨家数
    pub decline_count: Option<f64>,        // 下跌家数
    pub two_market_volume: Option<f64>,    // 两市成交额(亿)
    pub limit_up_count: Option<f64>,       // 涨停家数
    pub limit_down_count: Option<f64>,     // 跌停家数
}
```

## 注入位置

`CommitteeResult`（`orchestrator.rs`）新增顶层字段：

```rust
pub struct CommitteeResult {
    // ... existing fields ...
    pub macro_snapshot: Option<MacroSnapshot>,
}
```

## 注入流程

1. `run_committee` 在 Macro phase 完成后、Step 2 REGIME 之前
2. 调用 `parser::build_macro_snapshot()` 读取 `macro_cache` 全量条目
3. 按 indicator key 映射到 `MacroSnapshot` 对应字段
4. 无过期判定，有值就填，无值为 None
5. 结果挂到 `CommitteeResult.macro_snapshot`

## 修改文件清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/invest/committee/parser.rs` | 新增 `MacroSnapshot` struct + `build_macro_snapshot()` 函数 |
| `src-tauri/src/invest/committee/orchestrator.rs` | `CommitteeResult` 加字段 + `run_committee` 中注入 |

## 不改动

- `ParsedFields` — 保持 LLM 解析的纯粹性
- `tools.rs` — prompt 注入逻辑不受影响
- `analysis.rs` — 不涉及
- `macro_cache.rs` — 已有 `load_all_macro_cache`，无需改动
- 前端 — `CommitteeResult` 序列化后自动携带 `macroSnapshot` 字段

## 验证

1. `cargo check` — 编译通过
2. `cargo clippy -- -D warnings` — 无警告
3. `cargo test parser::tests` — parser 测试不受影响
