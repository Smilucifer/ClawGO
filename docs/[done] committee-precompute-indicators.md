# 委员会管道优化 — 确定性指标预计算

## 目标

将委员会管道中确定性的技术指标计算从 LLM 工具调用提取到 Rust 预计算层，减少不必要的 LLM 推理轮次。

## 背景

当前 `run_with_tool_loop()` 中，LLM 每次工具调用触发一次完整推理（输入 + 输出 + 网络延迟）。部分工具计算的是纯数学指标，无需 LLM 参与。

## 设计原则

- 只提取**数学定义明确**的指标（MA、RSI、波动率、分位数）
- 不涉及主观打分（财务健康、市场情绪、时间框架对齐等仍由 LLM 判断）
- 采用渐进策略：先注入预计算数据，保留工具作兜底
- 预计算指标仅注入 Quant 角色（不污染 Macro/Risk/CIO）
- 不影响缓存系统（不新增缓存类型）

---

## Step 1: 修复 `get_company_news`（Tushare → AkShare）

**根因**: `exec_company_news` 调用 `TushareClient::major_news()`，返回市场级快讯（无法按个股筛选），且用户无 Tushare 新闻权限。

**修复**:
- `exec_company_news` 改用 `InternationalClient::fetch_akshare_stock_news()`（已有 RPC 方法，返回个股新闻）
- 返回格式改为 `[日期] 标题 — 来源`，限最多 5 条

**文件**: `src-tauri/src/invest/committee/tools.rs`

## Step 2: 新建 `indicators.rs` 统一指标计算

**目的**: 消除 `regime.rs` 和 `tools.rs` 之间的代码重复，提供共享的指标计算原语。

**函数**:
- `compute_ma(prices: &[f64], period: usize) -> f64`
- `compute_ma_series(closes: &[f64], period: usize) -> Vec<f64>`
- `compute_rsi14(closes_chrono: &[f64]) -> f64` — Wilder 平滑，<15 bar 返回 50.0
- `compute_volatility(daily_returns: &[f64]) -> f64`
- `compute_price_percentile(current: f64, prices: &[f64]) -> f64`

**文件**: `src-tauri/src/invest/indicators.rs`

## Step 3: 重构 `regime.rs`

- `compute_regime_for_symbol()` 改为调用 `indicators::{compute_ma, compute_rsi14, compute_volatility, compute_price_percentile}`
- 删除 `regime.rs` 内的 `compute_rsi14` 私有副本
- 行为保持不变（MA20/MA60、RSI-14、波动率、分位数、趋势分类）

## Step 4: 预计算指标注入 Quant Prompt

- `load_prompt_for_round()` 中检测 `round == "R1"` 且 `role == "quant"` 时
- 从 `AssetContext` 的 `bars` 字段（已由 `build_asset_context` 填充）计算 MA5/MA20/MA60/MA120、RSI-14、波动率、分位数、趋势分类
- 格式化为 `{{precomputed_indicators}}` placeholder 注入 Quant R1 prompt
- 不影响 `run_with_tool_loop` 流程，工具列表不变

**文件**: `src-tauri/src/invest/committee/roles.rs`

## Step 5: 移除 `exec_company_info` 工具

- 从 `role_tool_defs()` 的 Risk 工具列表中移除 `get_company_info`
- 从 `run_tool()` 分支中删除对应实现
- 数据已被 `{{pe_ttm}}`/`{{pb}}`/`{{roe}}` 等 placeholder 覆盖
- 保留 `get_company_news` 和 `get_moneyflow`（部分数据不在 placeholder 中）

## Step 6: 清理 tools.rs 重复代码

- `exec_multi_timeframe` 中的 MA/RSI/波动率/分位数计算改为调用 `indicators.rs`
- `exec_regime_step` 中的趋势分类逻辑提取为共享函数
- `exec_moneyflow` 保持不变（缓存读取 + 格式化）

---

## 文件变更总览

| 文件 | 操作 |
|------|------|
| `src-tauri/src/invest/indicators.rs` | **新建** — 共享指标计算 |
| `src-tauri/src/invest/mod.rs` | 添加 `pub mod indicators;` |
| `src-tauri/src/invest/committee/tools.rs` | 修复 `get_company_news`；清理 `exec_multi_timeframe` 重复代码；移除 `exec_company_info` |
| `src-tauri/src/invest/committee/roles.rs` | Quant R1 prompt 添加 `{{precomputed_indicators}}` placeholder |
| `src-tauri/src/invest/regime.rs` | 复用 `indicators.rs`，删除私有 `compute_rsi14` |

## 验证

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
- 确认 `build_asset_context` 后 `bars` 数据可用
