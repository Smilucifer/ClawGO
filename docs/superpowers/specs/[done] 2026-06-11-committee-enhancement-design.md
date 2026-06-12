# 委员会直播功能增强 设计文档

**日期：** 2026-06-11
**版本：** v5.4.0
**状态：** 已完成 (v5.3.6, 2026-06-12)

---

## 概述

改进委员会直播功能的解析健壮性和错误提示，解决部分角色返回为空的问题。

**审查结论：** DeepSeek、MiMo Plan、Claude 三方审查，核心建议已采纳：
- 解析器用分层 `strip_prefix` 替代正则（保持零分配）
- hard_truncate 保留优化，采用"先解析后重构"策略
- 日志用 `fern` 配置 log crate 文件输出，不新建 logger.rs
- `extract_list_field_any` 同步修改
- `getStepState()` 增加 `failed` 状态
- Phase 2（删除远程功能）拆分为独立计划，不在本文档范围内

---

## 委员会解析器增强

### 1.1 放宽解析器格式要求

**目标：** 支持多种 LLM 输出格式，减少因格式不匹配导致的空值。

**修改文件：** `src-tauri/src/invest/committee/parser.rs`

**当前问题：**
- `extract_field` 已支持 `KEY: VALUE` 和 `KEY：VALUE`（中文冒号）
- 缺失 `KEY=VALUE`、`KEY = VALUE`、`**KEY**: VALUE`、`**KEY**=VALUE` 变体
- `extract_list_field_any` 重复实现了相同的 `strip_prefix` 逻辑，未同步更新

**支持的格式（6 种）：**
- `KEY: VALUE`（当前已有）
- `KEY：VALUE`（中文冒号，当前已有）
- `KEY=VALUE`（等号，新增）
- `KEY = VALUE`（等号+空格，新增）
- `**KEY**: VALUE`（Markdown 粗体+冒号，新增）
- `**KEY**=VALUE`（Markdown 粗体+等号，新增）

**实现方式：** 用分层 `strip_prefix` 替代正则，保持零分配特性：

```rust
fn extract_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        // 去除行首数字编号（如 "1. SIGNAL: ..."）
        let line = line.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ');

        // 尝试所有分隔符: KEY: KEY： KEY= KEY =
        for sep in &[": ", "：", "=", " = "] {
            if let Some(rest) = line.strip_prefix(key).and_then(|r| r.strip_prefix(sep)) {
                return Some(rest.trim().to_string());
            }
        }
        // Markdown 粗体: **KEY**: VALUE / **KEY**：VALUE / **KEY**=VALUE
        if let Some(rest) = line.strip_prefix("**").and_then(|r| r.strip_prefix(key)) {
            if let Some(rest) = rest.strip_prefix("**") {
                for sep in &[": ", "：", "=", " = "] {
                    if let Some(rest) = rest.strip_prefix(sep) {
                        return Some(rest.trim().to_string());
                    }
                }
            }
        }
    }
    None
}
```

**同步修改 `extract_list_field_any`：** 复用 `extract_field` 检测列表起始行，消除重复逻辑。

**测试覆盖：** 新增 8 个单元测试覆盖 6 种格式变体 + 行首编号 + Markdown 代码块。

---

### 1.2 优化 hard_truncate 策略

**目标：** 截断时优先保留关键字段（SIGNAL/VERDICT/STRENGTH）。

**修改文件：** `src-tauri/src/invest/committee/roles.rs`

**当前问题：**
- 简单截断到 max_chars，在换行符处断开
- 如果关键字段在输出末尾，可能被截断丢失
- `truncated` 标志赋值后从未被任何下游逻辑消费

**改进策略：** 采用"先解析后重构"策略

1. 给 `CommitteeRole` 添加 `critical_field_keys()` 方法，返回每个角色的关键字段名列表
2. 截断前先做轻量级解析（只提取关键字段行）
3. 如果关键字段在 max_chars 内，保留并截断非关键内容
4. 如果关键字段超出 max_chars，回退到当前的纯位置截断
5. `_attempt` 参数：第一次尝试保留关键字段，失败后降级为位置截断

**每个角色的关键字段：**
- Macro: SIGNAL, STRENGTH, MARKET_PHASE, ONE_LINER
- Quant: SIGNAL, STRENGTH, REGIME, ONE_LINER, ADJUSTED_SIGNAL, ADJUSTED_STRENGTH
- Risk: SIGNAL, STRENGTH, ONE_LINER, L4_VETO, ADJUSTED_SIGNAL, ADJUSTED_STRENGTH
- CIO: VERDICT, CONFIDENCE, ONE_LINER
- L4: L4_EMOTION_ASSESSMENT, L4_RED_LIGHT, L4_GUARD_CLAUSE

**实现方式：**
```rust
pub fn hard_truncate(text: &str, role: CommitteeRole, attempt: u32) -> (String, bool) {
    let max = role.max_chars();
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }

    if attempt == 0 {
        // 第一次尝试：保留关键字段
        let critical_keys = role.critical_field_keys();
        let critical_lines = find_critical_lines(text, &critical_keys);
        let critical_len: usize = critical_lines.iter().map(|l| l.chars().count()).sum();

        if critical_len < max {
            let remaining = max - critical_len;
            let other_lines = extract_non_critical_lines(text, &critical_keys);
            let truncated_other = truncate_lines_to_chars(other_lines, remaining);
            let result = reconstruct_output(critical_lines, truncated_other);
            if result.chars().count() <= max {
                return (result, true);
            }
        }
        // 回退到纯位置截断
    }

    // 纯位置截断（attempt == 1 或关键字段本身超长）
    let truncated: String = text.chars().take(max).collect();
    if let Some(last_newline) = truncated.rfind('\n') {
        (truncated[..last_newline].to_string(), true)
    } else {
        (truncated, true)
    }
}
```

**Prompt 约束（补充措施）：** 在各角色 prompt 的 `length_constraint_suffix` 中追加：
> "关键字段（SIGNAL/VERDICT/STRENGTH 等）必须放在输出的前 3 行"

---

### 1.3 角色级别降级提示

**目标：** 在 ParsedFields 中增加 `fallback_reason` 字段，记录为什么字段为空。

**修改文件：**
- `src-tauri/src/invest/committee/parser.rs`
- `src-tauri/src/invest/committee/orchestrator.rs`
- `src/lib/stores/invest-committee-store.svelte.ts`

**ParsedFields 新增字段：**
```rust
pub struct ParsedFields {
    // ... 现有字段 ...
    pub fallback_reason: Option<String>,
}
```

**降级原因枚举：**
- `"llm_unavailable"` — LLM 调用失败（3 次重试后）
- `"format_mismatch"` — 输出格式不匹配
- `"truncated_critical"` — 关键字段被截断
- `"empty_response"` — LLM 返回空内容
- `"parse_error"` — 解析异常

**填充逻辑（按角色定义关键字段缺失条件）：**
```rust
fn detect_fallback_reason(parsed: &ParsedFields, role: CommitteeRole, raw_text: &str, truncated: bool) -> Option<String> {
    if raw_text == "[WORKER_UNAVAILABLE]" {
        return Some("llm_unavailable".to_string());
    }
    if raw_text.is_empty() {
        return Some("empty_response".to_string());
    }
    if truncated {
        // 检查关键字段是否被截断
        let critical_missing = match role {
            CommitteeRole::Macro => parsed.signal.is_none() && parsed.market_phase.is_none(),
            CommitteeRole::Quant => parsed.signal.is_none() && parsed.strength.is_none(),
            CommitteeRole::Risk => parsed.signal.is_none() && parsed.strength.is_none(),
            CommitteeRole::Cio => parsed.verdict.is_none(),
            CommitteeRole::L4Officer => parsed.l4_emotion_assessment.is_none() && parsed.l4_red_light.is_none(),
        };
        if critical_missing {
            return Some("truncated_critical".to_string());
        }
    }
    // 格式不匹配：关键字段全空
    let critical_missing = match role {
        CommitteeRole::Macro => parsed.signal.is_none() && parsed.strength.is_none() && parsed.market_phase.is_none(),
        CommitteeRole::Quant => parsed.signal.is_none() && parsed.strength.is_none() && parsed.regime.is_none(),
        CommitteeRole::Risk => parsed.signal.is_none() && parsed.strength.is_none(),
        CommitteeRole::Cio => parsed.verdict.is_none() && parsed.confidence.is_none(),
        CommitteeRole::L4Officer => parsed.l4_emotion_assessment.is_none() && parsed.l4_red_light.is_none(),
    };
    if critical_missing {
        return Some("format_mismatch".to_string());
    }
    None
}
```

---

### 1.4 本地日志保存

**目标：** 委员会执行日志保存到本地文件，便于排查问题。

**日志路径：** `~/.claw-go/invest/logs/committee-YYYY-MM-DD.log`

**实现方式：** 用 `fern` 配置 log crate 的文件输出，复用现有 `log::info!`/`log::warn!`/`log::error!` 调用，不新建独立 logger 模块。

**修改文件：**
- `src-tauri/src/main.rs` 或 `src-tauri/src/lib.rs`（初始化时配置 fern）
- `Cargo.toml`（添加 fern 和 chrono 依赖）

**配置方式：**
```rust
// main.rs 或 lib.rs 初始化时
fern::Dispatch::new()
    .format(|out, message, record| {
        out.finish(format_args!(
            "[{}] [{}] {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            message
        ))
    })
    .level(log::LevelFilter::Info)
    // 控制台输出
    .chain(std::io::stdout())
    // 文件输出（委员会专用）
    .chain(fern::log_file(format!(
        "{}/invest/logs/committee-{}.log",
        dirs::home_dir().unwrap_or_default().display(),
        chrono::Local::now().format("%Y-%m-%d")
    ))?)
    .apply()?;
```

**日志轮转：** 按日期自动分文件，保留最近 30 天，超出自动删除。

**现有日志增强：** orchestrator.rs 中已有大量 `log::warn!`/`log::info!` 调用，只需补充以下关键节点的日志：
- 委员会开始/结束（含标的、批次、总耗时、总 token）
- 每个角色开始/结束（含耗时、token、解析结果）
- 降级触发时记录 `fallback_reason`

---

### 1.5 前端错误状态提示

**目标：** 检测 `[WORKER_UNAVAILABLE]` 等标记，显示友好提示；`getStepState()` 增加 `failed` 状态。

**修改文件：**
- `src/lib/stores/invest-committee-store.svelte.ts`（store 层处理）
- `src/lib/components/invest/pipeline-config.ts`（getStepState 增加 failed）
- `src/lib/components/invest/CommitteeLiveTab.svelte`（前端展示）

**改进 1：store 层处理 WORKER_UNAVAILABLE**

在 `invest-committee-store.svelte.ts` 的 `_handleCommitteeEvent` 中，收到 `role_complete` 事件时检测 `rawText === "[WORKER_UNAVAILABLE]"`，将该步骤标记为 `failed` 状态：

```typescript
// store 中新增 failedSteps Set
failedSteps: new Set<number>(),

// _handleCommitteeEvent 中
if (event.role_complete?.parsed?.rawText === '[WORKER_UNAVAILABLE]') {
  const stepIdx = roleToBackendIdx(event.role_complete.role, event.role_complete.round);
  progress.failedSteps.add(stepIdx);
}
```

**改进 2：getStepState() 增加 failed 状态**

```typescript
export function getStepState(
  symProgress: SymbolProgress | undefined,
  backendIdx: number,
  pipelineStarted: boolean,
): 'pending' | 'active' | 'done' | 'error' | 'failed' {
  if (!symProgress) return 'pending';
  if (backendIdx === -1) return pipelineStarted ? 'done' : 'pending';
  if (symProgress.activeStep === backendIdx) return 'active';
  if (symProgress.failedSteps?.has(backendIdx)) return 'failed';  // 新增
  for (const round of symProgress.completedRounds) {
    if (roleToBackendIdx(round.role, round.round) === backendIdx) return 'done';
  }
  if (symProgress.done && !symProgress.error) return 'done';
  if (symProgress.error && backendIdx >= symProgress.completedSteps) return 'error';
  return 'pending';
}
```

**改进 3：前端展示**

```svelte
{:else if round?.parsed?.rawText === '[WORKER_UNAVAILABLE]'}
  <div class="flex items-center gap-2 text-[12px] text-[var(--color-error)]">
    <span>⚠</span>
    <span>LLM 调用失败，请检查网络或 API Key 配置</span>
  </div>
{:else if round?.parsed?.fallbackReason}
  <div class="flex items-center gap-2 text-[12px] text-[var(--color-warning)]">
    <span>⚡</span>
    <span>{getFallbackMessage(round.parsed.fallbackReason)}</span>
  </div>
{:else if round?.parsed?.rawText}
  <div class="max-h-[200px] overflow-y-auto whitespace-pre-wrap ...">
    {round.parsed.rawText}
  </div>
{/if}
```

**降级提示映射：**
```typescript
function getFallbackMessage(reason: string): string {
  const messages: Record<string, string> = {
    'llm_unavailable': 'LLM 服务不可用，已降级为 HOLD',
    'format_mismatch': '输出格式异常，部分字段可能缺失',
    'truncated_critical': '输出过长，部分内容被截断',
    'empty_response': 'LLM 返回空内容',
    'parse_error': '解析异常',
  };
  return messages[reason] || '未知异常';
}
```

**步骤圆点状态：**
- `done` → 绿色勾 ✓
- `failed` → 红色叉 ✗（新增，区分于 `error`）
- `error` → 灰色叉 ✗
- `active` → 蓝色脉冲 ◉
- `pending` → 灰色字母

---

## 删除远程功能（独立计划）

> **注意：** 此部分已拆分为独立计划，不在本文档实施范围内。以下仅作参考。

### 影响范围
- 后端：`ssh.rs`、`session.rs` SSH 分支、`claude_stream.rs` SSH 路径、`diagnostics.rs` 3 个命令
- 前端：Settings Remote Tab（CRUD + SSH Key 向导）、API 层 3 个函数、TypeScript 类型
- Store：`session-store` 的 `remoteHostName` / `isRemote`
- i18n：约 40 个翻译 key
- 兼容性：`RunMeta` 中的 `remote_host_name`、`remote_cwd`、`remote_host_snapshot` 字段需要 `serde::default` 处理

### 验证清单
- [ ] `npm run check` 通过
- [ ] `npm run lint` 通过
- [ ] Settings 页面正常显示 6 个标签页
- [ ] Chat 页面正常工作
- [ ] 已有 run 数据反序列化不失败

---

## 实现顺序（3 个独立 PR）

| PR | 内容 | 修改文件 | 风险 | 依赖 |
|----|------|----------|------|------|
| PR-A | 解析器格式放宽 + extract_list_field_any 统一 | parser.rs | 低 | 无 |
| PR-B | 前端 WORKER_UNAVAILABLE 提示 + getStepState failed + fallback_reason | store, pipeline-config.ts, CommitteeLiveTab.svelte | 低 | 无 |
| PR-C | hard_truncate 优化 + prompt 约束 | roles.rs | 中 | PR-A |

**建议顺序：** PR-A → PR-B → PR-C（PR-A 和 PR-B 可并行）

---

## 风险评估

| 改进项 | 风险 | 缓解措施 |
|--------|------|----------|
| 1.1 放宽解析器 | 低 | 分层 strip_prefix，不影响现有逻辑 |
| 1.2 优化截断 | 中 | 两阶段尝试，失败回退到原有逻辑 |
| 1.3 降级提示 | 低 | 新增字段，向后兼容 |
| 1.4 本地日志 | 低 | 复用 log crate + fern，不改业务代码 |
| 1.5 前端提示 | 低 | store 层处理 + getStepState 新增状态 |

**关键风险点：**
- `extract_list_field_any` 需同步修改，否则等号格式的列表字段仍无法解析
- `RunMeta` 兼容性（删除远程功能时需 `serde::default` 处理）

---

## 测试计划

### 单元测试（parser.rs）
- `extract_field` 的 6 种格式变体（`:`、`：`、`=`、` = `、`**KEY**:`、`**KEY**=`）
- `extract_list_field_any` 在 `=` 分隔符下的列表解析
- 行首数字编号去除
- `detect_fallback_reason` 各角色关键字段缺失条件

### 单元测试（roles.rs）
- `hard_truncate` 各角色的边界 case（关键字段在末尾、关键字段本身超长）
- `_attempt` 参数的两阶段行为

### 前端测试
- `getStepState()` 对 `failed` 状态的处理
- WORKER_UNAVAILABLE 的前端渲染（需 mock store 状态）

### 集成测试
- 运行委员会，验证日志文件生成
- 模拟 LLM 失败场景，验证前端提示和圆点状态

---

## 相关文档

- [openInvest 源码索引](../../memory/reference-openinvest-source.md)
- [changelog.md](../changelog.md)
