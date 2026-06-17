# Committee CLI Executor Phase 2 — 全角色 CLI 迁移 + 供应商统一

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将委员会全部角色（Quant/Risk/L4/CIO）从 API 模式迁移到 CLI `--print` 模式，移除 API 回退路径，统一从 App 系统设置读取供应商配置，并发数前端可调。

**Architecture:**
- 所有 role 通过 `CliCommitteeExecutor` spawn `claude --print` 子进程
- 每个 role 的数据（行情/资金流/财务指标/前序输出）在 Rust 侧预取后嵌入 system prompt
- 每个标的 7 次 CLI spawn（Macro + Quant R1 + Risk R1 + Quant R2 + Risk R2 + L4 + CIO）
- 并发度由前端配置，后端 Semaphore 控制
- 供应商从 `UserSettings.platform_credentials` 读取，不再维护独立的 `llm_config.json`
- 移除 `InvestLlmClient` trait、`OpenAiCompatClient`、`run_with_tool_loop`、`tools.rs` 工具定义等 API 模式遗留代码

**Tech Stack:** Rust, tokio (Command, Semaphore, timeout), `claude --print` CLI, existing `parse_role_output`/`hard_truncate` pipeline.

---

## 实施阶段

| Phase | 范围 | 目标 | 状态 |
|-------|------|------|------|
| Phase 1 | Macro 角色先行 | 验证 CLI 模式可行性 | ✅ 已完成（耗时验证未完成，移入 Phase 2） |
| **Phase 2** | **全角色迁移 + 供应商统一 + 清理** | **所有 role 走 CLI，移除 API 路径** | ❌ 待开始 |

**Phase 1 遗留项：**
- [ ] 耗时是否可接受（需手动测试）→ 已纳入 Task 6.2

---

## Task 1: 供应商配置统一 — 从 App 系统设置读取 + CLI `--settings` 支持

**目标：** 委员会不再维护独立的 `llm_config.json`，改为从 `UserSettings.platform_credentials` 读取。CLI executor 通过 `--settings` 临时 JSON 将供应商配置注入 CC CLI。

### 1.1 后端：`run_role()` 增加 `--settings` 参数支持

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs` — `run_role()` method

**Changes:**
- [ ] `run_role()` 新增参数 `settings_json: Option<&str>`（临时 settings 文件路径）
- [ ] 当 `settings_json` 为 `Some` 时，cmd.args 增加 `["--settings", settings_json]`
- [ ] 参照 `commands/session.rs` 中的 provider-native launch config generation，为选定供应商生成临时 JSON：
  - DeepSeek/MiMo Pro: 固定 URL 模板（只需 api_key）
  - GLM/QWEN/KIMI/Custom: 参数化模板（api_key + base_url + model）
  - JSON 合并原生 `~/.claude/settings.json` 作为 base，剥离敏感 key，覆盖供应商字段
- [ ] 在 `run_role()` 结束后清理临时 JSON 文件

### 1.2 后端：改造 `CommitteeConfig` 供应商解析

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` — `CommitteeConfig` struct (line 63-74), `build_committee_config` (commands/invest.rs)
- Modify: `src-tauri/src/commands/invest.rs` — `build_committee_config` function

**Changes:**
- [ ] `CommitteeConfig` 新增字段：`provider_platform_id: String`（替代 `role_providers: HashMap<CommitteeRole, ProviderId>`）
- [ ] `build_committee_config` 改为从 `get_user_settings()` 读取 `platform_credentials`，按 `provider_platform_id` 匹配凭据
- [ ] 移除 `role_providers` HashMap（全部 role 用同一供应商）
- [ ] 保留 `model_override` — 映射到 `--settings` JSON 的 `model` 字段

### 1.3 后端：移除 `ProviderId` 硬编码枚举

**Files:**
- Modify: `src-tauri/src/invest/llm/types.rs` (lines 9-55)

**Changes:**
- [ ] 将 `ProviderId` enum 改为 `ProviderConfig` struct：
  ```rust
  pub struct ProviderConfig {
      pub platform_id: String,
      pub base_url: String,
      pub default_model: String,
      pub api_key: String,
  }
  ```
- [ ] 添加 `ProviderConfig::from_credential(cred: &PlatformCredential) -> Result<Self, String>` 构造函数
- [ ] 移除 `base_url()` / `default_model()` / `platform_id()` 硬编码方法

### 1.4 后端：移除 `resolve_api_key` 和 `llm_config.json` 依赖

**Files:**
- Modify: `src-tauri/src/invest/llm/client.rs` (lines 365-401)

**Changes:**
- [ ] 移除 `resolve_api_key()` 函数（不再从 `llm_config.json` 读取）
- [ ] 移除 `get_llm_config_path()` 函数
- [ ] API key 从 `ProviderConfig.api_key` 直接获取

### 1.5 后端：移除 `get_llm_config` / `save_llm_config` IPC 命令

**Files:**
- Modify: `src-tauri/src/commands/invest.rs` — 移除 `get_llm_config`、`save_llm_config` 命令
- Modify: `src-tauri/src/lib.rs` — 移除命令注册

### 1.6 前端：重写 `ProviderConfigPanel`

**Files:**
- Modify: `src/lib/components/invest/ProviderConfigPanel.svelte`

**Changes:**
- [ ] 移除 API key / base_url / model 输入框
- [ ] 改为：全局供应商下拉框（从 `getUserSettings().platform_credentials` 读取已配置的连接列表）
- [ ] 保留 debate rounds 和 timeout 配置
- [ ] 供应商选择后自动填充 model（从 credential.models 读取，或允许手动输入）

### 1.7 前端：更新 store 类型

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts` (lines 10-22)

**Changes:**
- [ ] 移除 `InvestLlmProviderConfig` interface
- [ ] `InvestLlmConfig` 改为：
  ```typescript
  interface InvestLlmConfig {
    providerPlatformId: string;  // 从 platform_credentials 选取
    modelOverride?: string;       // 可选模型覆盖 → 映射到 --settings model
    debateRounds: number;
    timeoutSecs: number;
    maxConcurrentSymbols: number; // 新增：前端可调并发数
  }
  ```
- [ ] 移除 `loadConfig` / `saveConfig` 对 `get_llm_config` / `save_llm_config` 的调用

### 1.8 清理 `llm_config.json`

**Files:**
- Delete: `~/.claw-go/invest/llm_config.json`（用户数据，不在 repo 中）

---

## Task 2: 并发数前端可配置

**目标：** `MAX_CONCURRENT_SYMBOLS` 从硬编码 5 改为前端可调。

### 2.1 后端：`CommitteeConfig` 新增并发字段

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (line 63-74)

**Changes:**
- [ ] `CommitteeConfig` 新增 `max_concurrent_symbols: usize`，默认 5
- [ ] 移除 `orchestrator.rs:25` 的 `MAX_CONCURRENT_SYMBOLS` 常量
- [ ] `run_committee_batch_stream` 从 `config.max_concurrent_symbols` 创建 Semaphore

### 2.2 前端：并发数滑块

**Files:**
- Modify: `src/lib/components/invest/ProviderConfigPanel.svelte`

**Changes:**
- [ ] 在 debate rounds 旁新增并发数选择器（1-10 范围，滑块或下拉框）
- [ ] 自动保存

---

## Task 3: 扩展 `cli_executor.rs` — 新增 Quant/Risk/L4/CIO prompt 构建

**目标：** 为每个 role 编写 `build_cli_xxx_prompt()` 函数，预取对应数据嵌入 prompt。

### 3.1 Quant prompt 构建

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

**Changes:**
- [ ] 新增 `build_cli_quant_prompt()` 函数
- [ ] 预取数据：
  - `indicators::compute_precomputed_indicators(symbol)` → RSI/MA/HV/价格分位
  - `stock_data_cache` → 日线行情快照
  - `moneyflow` → 资金流向（主力/超大单/大单/中单/小单）
  - `format_recent_verdicts_for_prompt(symbol)` → 历史裁决
- [ ] 调用 `strip_tool_section()` 移除工具描述
- [ ] 替换 `{{precomputed_indicators}}` 等占位符

### 3.2 Risk prompt 构建

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

**Changes:**
- [ ] 新增 `build_cli_risk_prompt()` 函数
- [ ] 预取数据：
  - 持仓集中度（前 5 大持仓占比）
  - PnL 快照（近期盈亏趋势）
  - 估值指标（PE/PB/ROE/负债率）
  - `format_recent_verdicts_for_prompt(symbol)` → 历史裁决
- [ ] 调用 `strip_tool_section()` 移除工具描述

### 3.3 L4 Officer prompt 构建

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

**Changes:**
- [ ] 新增 `build_cli_l4_prompt()` 函数
- [ ] 预取数据：Macro/Quant/R1/Risk/R1 的输出摘要（通过 `round_outputs` 传入）
- [ ] 调用 `strip_tool_section()` 移除工具描述

### 3.4 CIO prompt 构建

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

**Changes:**
- [ ] 新增 `build_cli_cio_prompt()` 函数
- [ ] 预取数据：L4 输出 + 全部前序摘要（通过 `round_outputs` 传入）
- [ ] 调用 `strip_tool_section()` 移除工具描述

### 3.5 缓存数据格式化辅助函数

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`

**Changes:**
- [ ] 新增 `format_indicators_for_prompt(indicators)` → RSI/MA/HV/分位数文本块
- [ ] 新增 `format_moneyflow_for_prompt(symbol)` → 资金流向文本块
- [ ] 新增 `format_holdings_concentration_for_prompt()` → 持仓集中度文本块
- [ ] 新增 `format_pnl_for_prompt(symbol)` → PnL 趋势文本块
- [ ] 新增 `format_round_summaries_for_prompt(round_outputs)` → 前序角色输出摘要

---

## Task 4: 重构 orchestrator — 全角色走 CLI，移除 API 回退

**目标：** 所有 `run_*_phase` 函数统一走 CLI 路径，移除 API fallback 代码。

### 4.1 重构 `run_macro_phase`

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (line 1312-1363)

**Changes:**
- [ ] 移除 API fallback 分支（当前 line 1355-1382）
- [ ] CLI 失败时直接返回 `Err`，不再 fallback

### 4.2 新增 `run_quant_phase` / `run_risk_phase`

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`

**Changes:**
- [ ] 新增 `run_quant_phase()` — 调用 `cli_executor::build_cli_quant_prompt()` + `cli.run_role()`
- [ ] 新增 `run_risk_phase()` — 调用 `cli_executor::build_cli_risk_prompt()` + `cli.run_role()`
- [ ] 替换当前 `run_with_tool_loop` 调用

### 4.3 新增 `run_l4_phase` / `run_cio_phase`

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`

**Changes:**
- [ ] 新增 `run_l4_phase()` — 调用 `cli_executor::build_cli_l4_prompt()` + `cli.run_role()`
- [ ] 新增 `run_cio_phase()` — 调用 `cli_executor::build_cli_cio_prompt()` + `cli.run_role()`

### 4.4 更新 `run_committee` 主流程

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (line 1785-2087)

**Changes:**
- [ ] 移除 `client: &dyn InvestLlmClient` 参数（不再需要 API client）
- [ ] 所有 `run_*_phase` 调用改为 CLI 路径
- [ ] 移除 `run_debate_rounds` 中的 `run_with_tool_loop` 调用

### 4.5 更新 `run_committee_batch_stream`

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (line 2138-2216)

**Changes:**
- [ ] 移除 `client: Arc<dyn InvestLlmClient>` 参数
- [ ] 从 `config.max_concurrent_symbols` 创建 Semaphore
- [ ] 移除 `build_committee_config` 中的 provider 解析逻辑

---

## Task 5: 清理 API 模式遗留代码

**目标：** 移除不再使用的 API 模式代码。

### 5.1 删除 `run_with_tool_loop`

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (lines 1138-1306)

**Changes:**
- [ ] 删除 `run_with_tool_loop()` 函数（~170 行）
- [ ] 删除 `tool_result_message()` 辅助函数

### 5.2 删除或精简 `tools.rs`

**Files:**
- Modify: `src-tauri/src/invest/committee/tools.rs`

**Changes:**
- [ ] 移除 `ToolDef` 结构体和 `role_tool_defs()` 函数
- [ ] 移除 `execute_tool()` 函数
- [ ] 保留 `parse_tool_calls()` 如果 CLI 输出中仍有工具调用格式需要解析（需验证）
- [ ] 如果完全不需要，删除整个文件

### 5.3 删除 `InvestLlmClient` trait 和实现

**Files:**
- Delete: `src-tauri/src/invest/llm/client.rs`（或大幅精简）
- Modify: `src-tauri/src/invest/llm/types.rs` — 移除 `InvestLlmClient` trait

**Changes:**
- [ ] 删除 `OpenAiCompatClient` struct 和 `impl InvestLlmClient`
- [ ] 删除 `chat_stream()` 实现
- [ ] 删除 `resolve_api_key()` / `get_llm_config_path()`
- [ ] 评估是否保留 `LlmConfig` struct（CLI 模式下不需要，但 `ProviderConfig` 可能仍需 model 信息）

### 5.4 清理 `orchestrator.rs` 中的 API 相关导入

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs` (lines 1-22)

**Changes:**
- [ ] 移除不再使用的导入：`InvestLlmClient`, `LlmConfig`, `Message`, `ToolDef`, `collect_stream`, `CollectedResponse`
- [ ] 移除 `llm::governor` 导入（CLI 模式使用自己的 Semaphore）

### 5.5 清理 `commands/invest.rs` 中的 API client 创建

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`

**Changes:**
- [ ] 移除 `OpenAiCompatClient::new()` 调用
- [ ] 移除 `get_llm_config()` 调用
- [ ] `run_committee` / `run_committee_stream` 不再传入 client

### 5.6 清理 `roles.rs` 中的旧 API prompt 模板

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`

**Changes:**
- [ ] 移除 `load_prompt_for_round()` 函数（CLI 模式使用 `build_cli_xxx_prompt()` 替代）
- [ ] 移除 `{{placeholder}}` 占位符替换逻辑（数据已由 `cli_executor.rs` 预取注入）
- [ ] 评估是否保留 prompt 模板文本（CLI prompt 构建函数已内联模板，旧模板可能成为死代码）
- [ ] 保留 `length_constraint_suffix()` / `parse_role_output()` / `detect_fallback_reason()` 等解析函数（CLI 输出仍需要）

---

## Task 6: 失败标的重试机制 + UI 优化

**目标：** CLI 失败时支持按标的重试，重试按钮提升到标的标签层。

### 6.1 后端：单标的重试 API

**Files:**
- Modify: `src-tauri/src/commands/invest.rs`

**Changes:**
- [ ] 新增 `retry_committee_symbol(symbol: String, config: CommitteeConfig)` IPC 命令
- [ ] 复用 `run_committee()` 逻辑，对单个标的从头重跑完整 pipeline
- [ ] 失败时返回明确错误信息（而非静默吞掉）

### 6.2 前端：标的标签层重试按钮

**Files:**
- Modify: `src/lib/components/invest/CommitteeLiveTab.svelte`

**Changes:**
- [ ] 每个标的标签（symbol badge）上增加重试图标按钮（🔄），始终可见
- [ ] 点击触发 `retryCommitteeSymbol(symbol)` IPC
- [ ] 重试中显示 loading 状态（spinner 替代重试图标）
- [ ] 重试成功后刷新该标的的 pipeline 状态
- [ ] 移除下拉菜单中的重试入口（如果有）

### 6.3 前端：store 方法

**Files:**
- Modify: `src/lib/stores/invest-committee-store.svelte.ts`

**Changes:**
- [ ] 新增 `retryCommitteeSymbol(symbol: string)` 方法
- [ ] 调用 `retry_committee_symbol` IPC，更新该标的的状态为 running

---

## Task 7: 验证和测试

**来源：Phase 1 未完成验证 + Phase 2/3 原始验证标准**

### 7.1 编译验证

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` — 无错误
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` — 无警告
- [ ] `npm run check` — 前端无类型错误
- [ ] 全量代码清理后编译通过（Phase 3 验证标准）
- [ ] 无残留死代码 — `cargo clippy` + 手动 grep 无 `#[allow(dead_code)]` 掩盖（Phase 3 验证标准）

### 7.2 Phase 1 遗留验证

- [ ] **耗时是否可接受** — Phase 1 未完成项。Macro 角色 CLI 耗时应 ≤ API 路径的 1.5 倍（含进程冷启动）

### 7.3 Phase 2 功能验证

- [ ] **Quant 输出包含完整技术指标分析** — RSI/MA/HV/价格分位/资金流向均有覆盖
- [ ] **Risk 输出包含风险评估** — 集中度/PnL/估值指标/风险信号均有覆盖
- [ ] **耗时比纯 API 路径更快或持平** — 单标的 7 次 CLI spawn 总耗时 vs 原 API 路径
- [ ] **5 并发稳定运行** — 批量 5 标的同时运行无崩溃/超时/数据串扰

### 7.4 Phase 3 质量验证

- [ ] **所有角色输出质量不低于 API 版本** — 对比 CLI 输出 vs 原 API 输出的字段覆盖率和合理性
- [ ] **供应商切换** — 从 App 系统设置选择不同供应商后，委员会使用对应凭据
- [ ] **并发数调整** — 前端修改并发数后，批量运行遵守新限制

### 7.5 回归验证

- [ ] 委员会直播页面：RoleStart/RoleComplete 事件正常触发
- [ ] 委员会回放：历史裁决正常加载
- [ ] 委员会归档：verdict 正常写入
- [ ] 单标的委员会运行：7 个 role 全部通过 CLI 完成
- [ ] 批量运行（3+ 标的）：并发正常，Semaphore 生效
- [ ] 失败标的重试：标的标签层重试按钮可用，重跑完整 pipeline 成功

---

## 文件变更汇总

| Action | File | Description |
|--------|------|-------------|
| **Modify** | `src-tauri/src/invest/committee/cli_executor.rs` | `run_role()` 增加 `--settings` + 4 个 build_cli_xxx_prompt + 5 个数据格式化函数 |
| **Modify** | `src-tauri/src/invest/committee/orchestrator.rs` | 全角色 CLI 路径 + 移除 API fallback + 移除 run_with_tool_loop |
| **Modify** | `src-tauri/src/invest/committee/roles.rs` | 移除 load_prompt_for_round + 旧 API prompt 模板 |
| **Modify** | `src-tauri/src/invest/llm/types.rs` | ProviderId enum → ProviderConfig struct |
| **Delete/Major** | `src-tauri/src/invest/llm/client.rs` | 移除 OpenAiCompatClient / resolve_api_key |
| **Modify** | `src-tauri/src/invest/committee/tools.rs` | 精简或删除 |
| **Modify** | `src-tauri/src/commands/invest.rs` | 移除 API client 创建 / llm_config 读取 + 新增 retry 命令 |
| **Modify** | `src/lib/components/invest/ProviderConfigPanel.svelte` | 供应商从 App 设置选取 + 并发数滑块 |
| **Modify** | `src/lib/components/invest/CommitteeLiveTab.svelte` | 标的标签层重试按钮 |
| **Modify** | `src/lib/stores/invest-committee-store.svelte.ts` | 类型重写 + retryCommitteeSymbol 方法 |

---

## 预估工作量

| Task | 复杂度 | 预估时间 |
|------|--------|---------|
| Task 1: 供应商统一 + CLI --settings | 高 | 3-4 小时 |
| Task 2: 并发数可配置 | 低 | 30 分钟 |
| Task 3: CLI prompt 构建 | 高 | 3-4 小时 |
| Task 4: Orchestrator 重构 | 高 | 2-3 小时 |
| Task 5: 清理 API 代码（含 roles.rs） | 中 | 1.5-2.5 小时 |
| Task 6: 失败标的重试 + UI | 低 | 1 小时 |
| Task 7: 验证测试（含 Phase 1 遗留） | 中 | 1.5 小时 |
| **总计** | | **13-17 小时** |
