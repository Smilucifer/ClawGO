# 委员会功能 5 项改进计划（v2 — 审查后修订）

## Context

用户反馈 openInvest 委员会功能有 5 个问题：LLM 输出大量英文字段名影响可读性、用户档案注入位置错误（应在风控 R1 而非 CIO）、风控 R1 的 CONCENTRATION_PCT/PNL_PCT 占位符没有实际值、REGIME 计算结果未在前端展示、CIO 裁决和 Gate 标签仍为英文。

经 Claude/MiMo/DeepSeek 三路审查，采纳以下折中方案。

## 关键设计决策

| 维度 | 决策 | 理由 |
|------|------|------|
| Prompt 字段名 | 改中文 | 用户核心需求 |
| Parser | 双语支持（中文 key 优先，英文 fallback） | 向后兼容自定义 prompt |
| REGIME 字段名 | 保持英文 | 避免 format_regime_context ↔ QUANT_PROMPT 连锁反应 |
| 字段值 | 不中文化 | VERDICT/SIGNAL 值是系统内部值，parser normalize 依赖英文 |
| Profile 注入 | 双注入（Risk R1 + CIO） | Risk 评估流动性风险 + CIO 裁决需要 |
| Risk 数值 | 复用 build_portfolio_summary + Tushare 取 current_price | notional/shares ≠ 当前价格 |
| Gate notes | 后端直接输出中文 | 避免 i18n key 与普通文本混淆 |
| translate_field_names | 不实现 | 正则误伤风险高，parser 双语已覆盖 |

## 涉及文件

| 文件 | 改动 |
|------|------|
| `src-tauri/src/invest/committee/roles.rs` | prompt 模板字段名中文化（6 个 const） |
| `src-tauri/src/invest/committee/parser.rs` | extract_field 双语支持 + apply_r2_signal_override 双语 |
| `src-tauri/src/invest/committee/orchestrator.rs` | profile 双注入 + compute_risk_context + RegimeStep 事件扩展 |
| `src-tauri/src/invest/committee/events.rs` | RegimeStep 新增 regime/reason/strategy_hint/metrics 字段 |
| `src-tauri/src/invest/committee/analysis.rs` | Gate notes 中文化 |
| `src-tauri/src/invest/committee/archive.rs` | 归档报告中文化 |
| `src/lib/stores/invest-committee-store.svelte.ts` | SymbolProgress 存储 REGIME 数据 |
| `src/lib/components/invest/CommitteeLiveTab.svelte` | REGIME 卡片展示 + Gate tooltip |
| `src/lib/components/invest/CommitteeReplayTab.svelte` | 同上 |
| `src/lib/components/invest/DebateBlock.svelte` | Signal/Strength 标签中文化 |
| `messages/zh-CN.json` | 新增 i18n key |
| `messages/en.json` | 对应英文 key |

## 实现步骤

### Phase A: 低风险 UI 改善（先后端再前端）

#### A1: Gate notes 中文化 (`analysis.rs`)
- "Gate 1: macro signal inconsistency..." → "G1: 宏观信号与CIO裁决不一致..."
- "Gate 2: concentration..." → "G2: 集中度..."
- "Gate 3: dry powder..." → "G3: 可用子弹不足..."
- "Gate 4a/4b: ..." → "G4a/4b: ..."
- "Worker unavailable..." → "工作节点不可用..."

#### A2: 归档报告中文化 (`archive.rs`)
- "Final Verdict" → "最终裁决"
- "Sanity Check (3 Gates)" → "合理性检查"
- "Gate 1 (Signal Consistency)" → "G1 信号一致性"
- "Gate 2 (Concentration)" → "G2 集中度"
- "Gate 3 (Dry Powder)" → "G3 子弹充足"
- "Sentinel Override" → "哨兵覆盖"
- "Convergence" → "收敛状态"
- "Round Outputs" → "各轮输出"
- "CIO Reasoning" → "CIO 推理"

#### A3: DebateBlock 标签中文化 (`DebateBlock.svelte`)
- `Signal:` → `信号:`
- `strength:` → `强度:`

#### A4: i18n 更新 (`messages/zh-CN.json` + `en.json`)
新增 key：
- `invest_gate1_label`: "信号一致性" / "Signal Consistency"
- `invest_gate2_label`: "集中度" / "Concentration"
- `invest_gate3_label`: "子弹充足" / "Dry Powder"
- `invest_gate4_label`: "仓位合理" / "Position Check"
- `invest_gate1_desc`: "宏观信号与CIO裁决方向一致" / "Macro signal aligns with CIO verdict"
- `invest_gate2_desc`: "单一资产集中度不超过40%" / "Single asset concentration ≤ 40%"
- `invest_gate3_desc`: "可用子弹不低于应急储备" / "Dry powder above emergency buffer"
- `invest_gate4_desc`: "仓位与裁决匹配" / "Position size matches verdict"
- `invest_regime_label`: "市场状态" / "Regime"
- `invest_regime_reason`: "原因" / "Reason"
- `invest_regime_inputs`: "输入指标" / "Inputs"
- `invest_regime_hint`: "策略建议" / "Strategy Hint"
- `invest_signal_label`: "信号" / "Signal"
- `invest_strength_label`: "强度" / "Strength"

#### A5: Gate tooltip 前端 (`CommitteeLiveTab.svelte` + `CommitteeReplayTab.svelte`)
- `G1 ✓` → `<span title={t('invest_gate1_desc')}>G1 {pass ? '✓' : '✗'}</span>`
- 同理 G2/G3/G4

### Phase B: REGIME 展示

#### B1: RegimeStep 事件扩展 (`events.rs`)
```rust
RegimeStep {
    symbol: String,
    success: bool,
    context_preview: String,
    step_index: usize,
    // 新增
    regime: Option<String>,
    reason: Option<String>,
    strategy_hint: Option<String>,
    metrics: Option<RegimeMetrics>,  // 复用 regime::RegimeMetrics
}
```

#### B2: orchestrator 发送扩展事件 (`orchestrator.rs`)
- `compute_regime_context` 返回后，解析 RegimeResult 填充新字段
- 需要 `compute_regime_for_symbol` 返回 RegimeResult（当前已返回）

#### B3: 前端存储 REGIME 数据 (`invest-committee-store.svelte.ts`)
- `SymbolProgress` 新增 `regimeData: { regime, reason, strategyHint, metrics } | null`
- `regime_step` 事件处理分支存储 regime 数据

#### B4: REGIME 卡片展示 (`CommitteeLiveTab.svelte` + `CommitteeReplayTab.svelte`)
- REGIME 步骤卡片不再只显示"已计算"
- 展示：regime 类型（uptrend/downtrend/...）、原因、关键指标（MA20/MA60/RSI/波动率/分位数）

### Phase C: Parser 双语支持

#### C1: extract_field 双语 (`parser.rs`)
- 新增 `FIELD_NAME_MAP`: 英文→中文映射表
- 修改 `extract_field()`: 先尝试中文 key，再尝试英文 key
- 修改 `extract_f64()` / `extract_bool()` / `extract_list_field()`: 同上
- 修改 `apply_r2_signal_override()`: ADJUSTED_SIGNAL → 调整信号, ADJUSTED_STRENGTH → 调整强度

#### C2: 单元测试
- 测试中文字段名解析
- 测试英文字段名 fallback
- 测试中英文混合格式
- 测试中文冒号 `：`

### Phase D: Prompt 字段名中文化

#### D1: 6 个 prompt 模板中文化 (`roles.rs`)

**字段名映射**（REGIME 保持英文）：
| 英文 | 中文 |
|------|------|
| SIGNAL | 信号 |
| STRENGTH | 强度 |
| SCORE | 评分 |
| KEY_HEADWIND | 最大利空 |
| KEY_TAILWIND | 最大利好 |
| ONE_LINER | 一句话 |
| KEY_DATA | 关键数据 |
| ADJUSTED_SIGNAL | 调整信号 |
| ADJUSTED_STRENGTH | 调整强度 |
| REGIME_PROTECTION_TRIGGERED | REGIME保护触发 |
| REASONING | 推理 |
| CONCENTRATION_PCT | 集中度 |
| DRY_POWDER_CNY | 可用子弹 |
| PNL_PCT | 浮盈比 |
| WORST_CASE_LOSS_PCT_AT_-20 | 极端损失 |
| ADJUSTED_STOP_LOSS | 调整止损 |
| VERDICT | 裁决 |
| CONFIDENCE | 置信度 |
| DOMINANT_VIEW | 主导观点 |
| SUGGESTED_ALLOC_CNY | 建议配置 |
| EXECUTION_PLAN | 执行计划 |
| RISK_PLAN | 风控计划 |
| PERSONAL_NOTE | 个人建议 |

**REGIME 上下文字段保持英文**（format_regime_context 不改）：
- REGIME / REASON / INPUTS / STRATEGY_HINT — 保持英文

**字段值保持英文**（parser normalize 依赖）：
- risk_on / risk_off / neutral
- bullish / bearish / neutral
- ok / concerned / high_risk
- BUY / ACCUMULATE / HOLD / TRIM / SELL
- quant / macro / risk
- lump-sum / pyramid / grid / none
- yes / no

#### D2: Parser 已在 C1 完成双语支持，无需额外改动

### Phase E: Profile 双注入 + Risk 数值预计算

#### E1: Profile 双注入 (`orchestrator.rs`)
- Risk R1: `if role == CommitteeRole::Risk && round == 1 { inject profile }`
- CIO: 保留原有 `if role == CommitteeRole::Cio { inject profile }`
- Risk R1 的 profile 注入使用更简洁的格式（控制在 250 字限制内）

#### E2: compute_risk_context (`orchestrator.rs`)
- 复用 `build_portfolio_summary()` 已有的持仓数据
- 从 Tushare 获取当前价格（复用 `compute_regime_for_symbol` 的 bars 数据中的最新 close）
- 计算 `concentration_pct = target_notional / total_notional * 100`
- 计算 `pnl_pct = (current_price - avg_cost) / avg_cost * 100`（avg_cost > 0 时）
- 获取 `cash` 作为 dry_powder
- 如果 Tushare 不可用，concentration_pct 仍可计算（从 holdings.notional），pnl_pct 标记 N/A
- 注入到 Risk R1 的 user message 中

#### E3: 前端 Signal/Strength 标签（已在 A3 完成）

## 验证

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml committee::parser::tests
cargo test --manifest-path src-tauri/Cargo.toml committee::analysis::tests
npm run check
npm run lint
npm run i18n:check
npm run build
```
