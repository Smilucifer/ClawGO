# Committee Live Fallback 优化方案

**状态**: [wip]
**创建日期**: 2026-06-12
**问题描述**: 委员会直播偶发出现"输出缺少关键字段，可能需要重新分析"或"返回了空结果"错误

---

## 问题分析

### 错误来源
- `invest_fallback_empty_text`: "{role} 返回了空结果"
- `invest_fallback_missing_critical_fields`: "{role} 输出缺少关键字段，可能需要重新分析"

### 核心验证逻辑
`parser.rs` 的 `detect_fallback_reason` 函数（第 167-192 行）按三层验证：
1. `[WORKER_UNAVAILABLE]` 标记检测
2. 空文本检测
3. 角色特定关键字段检查（Macro→signal, Quant→signal+regime, Risk→signal, CIO→verdict, L4→l4_guard_clause）

### 根因分析
1. **解析失败无重试** - LLM 输出格式偶发偏离，解析失败后直接标记 fallback，无补偿
2. **解析器过于脆弱** - 只做逐行 `KEY: value` 前缀匹配，不处理 markdown 包装、多行值
3. **失败输出污染下游** - 前置角色 fallback 后，不完整文本仍注入后续角色上下文
4. **字符限制过紧** - Quant/Risk 的 `max_chars` 仅 350，字段多时易被截断
5. **前端无重试能力** - 用户只能看到警告，无法主动重试失败步骤

---

## 优化任务

### Task 1: 解析失败增加重试机制 ⭐ 最高优先级 ✅

**文件**: `src-tauri/src/invest/committee/orchestrator.rs`

**方案**: 在 `run_with_tool_loop` 中，`parse_role_output` + `detect_fallback_reason` 之后，如果检测到 `missing_critical_fields`，重新调用 LLM（最多 1-2 次），在 prompt 中附加格式提醒。

```rust
// 伪代码示意
let max_parse_retries = 2;
for attempt in 0..=max_parse_retries {
    let raw_text = llm_call_with_retry(...).await?;
    let parsed = parse_role_output(role, &raw_text);

    if detect_fallback_reason(role, &raw_text).is_none() {
        break; // 解析成功
    }

    if attempt < max_parse_retries {
        context.push(format!(
            "你的上一次输出缺少必要字段 {}，请严格按 KEY: VALUE 格式重新输出。",
            required_fields_for(role)
        ));
    }
}
```

**预期效果**: 消除大部分偶发格式偏离导致的 fallback

---

### Task 2: 放宽 hard_truncate 字符限制 ✅

**文件**: `src-tauri/src/invest/committee/roles.rs`

**当前限制**（第 264-341 行）:
| 角色 | max_chars | 建议调整 |
|------|-----------|----------|
| Macro | 400 | 400（够用） |
| Quant | 350 | **550** |
| Risk | 350 | **550** |
| CIO | 600 | 600（够用） |
| L4 | 400 | 400（够用） |

**理由**: Quant 需输出 SIGNAL/STRENGTH/REGIME/资金流向/估值/关键数据/买点/一句话；Risk 需输出 SIGNAL/集中度/估值/止损/建议。350 字符极度紧张。

---

### Task 3: 增强解析器容错性 ✅

**文件**: `src-tauri/src/invest/committee/parser.rs`

**改进点**:
1. **预处理函数** - 去除 markdown 标题 `#`、加粗 `*` 等包装
2. **多行字段值** - 读取当前行到下一个 `KEY:` 行之间的所有内容作为字段值
3. **字段名变体** - 增加常见别名映射（如 `direction` → `signal`）

```rust
fn normalize_line(line: &str) -> String {
    line.trim()
        .trim_start_matches('#')
        .trim_start_matches('*')
        .trim_end_matches('*')
        .trim()
        .to_string()
}
```

---

### Task 4: 失败输出注入下游时降级处理 ✅

**文件**: `src-tauri/src/invest/committee/orchestrator.rs`（`build_context_messages` 相关逻辑）

**方案**: 对于有 `fallback_reason` 的角色输出，在注入下游上下文时：
- 添加警告标记：`[注意：{role} 角色输出不完整，以下为原始文本]`
- 或为关键字段提供保守默认值（如 `SIGNAL: neutral`）再注入

**预期效果**: 减少连锁失败概率

---

### Task 5: 前端增加单步重试按钮 ✅

**文件**:
- `src/lib/components/invest/CommitteeLiveTab.svelte`
- `src-tauri/src/commands/invest.rs`（新增 IPC）
- `src-tauri/src/invest/committee/orchestrator.rs`（新增单角色执行函数）

**方案**:
1. fallback 警告条旁增加"重试"按钮
2. 后端暴露 `retry_committee_step(run_id, role)` 命令
3. 前端调用后更新该步骤状态

---

### Task 6: 所有角色 Prompt 增加职责白名单 ⭐ 高优先级 ✅

**文件**: `src-tauri/src/invest/committee/roles.rs`（5 个 PROMPT 常量）

**问题**: 当前 prompt 缺乏明确的职责边界约束，LLM 经常越界输出其他角色的内容，导致输出过长被截断或触发 `missing_critical_fields`。

**修改方案**: 在每个角色 prompt 的"输出要求"部分增加 **职责白名单**，明确"你只负责什么"：

#### Macro ✅ 已完成

改动：
- 移除 `analyze_multi_timeframe` 工具（技术面不该由 Macro 做）
- 移除"一句话"字段，改为"信号理由"+"市场阶段理由"
- 添加职责白名单
- 添加三条禁令（技术面、操作建议、抱怨工具）

输出格式：
```
信号: risk_on | risk_off | neutral
强度: 0-10
信号理由: <一句话>
市场阶段: 主升 | 分歧 | 退潮 | 冰点 | 混沌
市场阶段理由: <一句话>
敏感度: positive | negative | neutral
敏感度理由: <一句话≤20字>
情绪温度: 乐观 | 中性 | 谨慎 | 恐慌
宏观催化剂: <事件或"无">
```

#### Quant R1 + R2 ✅ 已完成

改动：
- `max_chars` 350→550（R1 和 R2 共享）
- R1 添加职责白名单 + 三条禁令
- R2 添加职责白名单 + 三条禁令

#### Risk R1 ✅ 已完成

改动：
- 添加职责白名单 + 三条禁令（技术面、宏观判断、买点建议）
- `信号` → `风险信号` 避免与 Quant/Macro 混淆
- parser 已适配新字段名

#### Risk R2 ✅ 已完成

改动：
- 添加职责白名单 + 三条禁令
- `调整信号` → `调整风险信号` 保持一致
- parser 已适配新字段名

#### CIO ✅ 已完成

改动：添加职责白名单 + 两条禁令

#### L4 Officer ✅ 已完成

改动：添加职责白名单 + 两条禁令

---

## 所有 Prompt 检查完成 ✅

**预期效果**:
- 白名单明确每个角色"只做什么"，比黑名单更易遵循
- 所有角色输出长度大幅缩短，不再触发截断
- 职责边界清晰，减少 `missing_critical_fields` 触发率

---

### Task 7: 降低并发量 8→5 ✅

**文件**: `src-tauri/src/invest/llm/governor.rs`（第 18 行）

**问题**: 当前每个 provider 的并发限制为 8，可能导致：
- API 限流（429）概率增加
- LLM 在高并发下输出质量下降（更容易返回格式不规范的内容）
- 内存占用较高

**修改**:

```diff
// governor.rs 第 18 行
- semaphores.insert(*provider, Arc::new(Semaphore::new(8)));
+ semaphores.insert(*provider, Arc::new(Semaphore::new(5)));
```

同时更新注释（第 9 行）：

```diff
- /// Each provider gets an independent Semaphore with 8 permits.
- /// 5 assets x 3 roles = 15 concurrent requests capped to 8 per provider.
+ /// Each provider gets an independent Semaphore with 5 permits.
+ /// 5 assets x 3 roles = 15 concurrent requests capped to 5 per provider.
```

**预期效果**:
- 降低 API 限流概率
- LLM 输出质量可能提升（并发压力减小）
- 后续可观察效果，再决定是否需要进一步调整

---

## 已完成的改动

### Task 6: 所有角色 Prompt 职责白名单 ✅

| 角色 | 改动 |
|------|------|
| Macro ✅ | 移除 `analyze_multi_timeframe` 工具、移除"一句话"字段、添加职责白名单 + 禁令 |
| Quant R1 ✅ | 添加职责白名单 + 禁令 |
| Quant R2 ✅ | 添加职责白名单 + 禁令 |
| Risk R1 ✅ | 添加职责白名单 + 禁令、`信号` → `风险信号` |
| Risk R2 ✅ | 添加职责白名单 + 禁令、`调整信号` → `调整风险信号` |
| CIO ✅ | 添加职责白名单 + 禁令 |
| L4 ✅ | 添加职责白名单 + 禁令 |

### Task 7: 降低并发量 ✅

- `governor.rs`: `Semaphore::new(8)` → `Semaphore::new(5)`

### Parser 适配 ✅

- `parse_risk`: 支持 `风险信号` 字段名
- `apply_r2_signal_override`: 支持 `调整风险信号` 字段名

### max_chars 调整 ✅

| 角色 | 原值 | 新值 |
|------|------|------|
| Macro | 600 | 500 |
| Quant | 350 | 550 |
| Risk | 350 | 550 |
| CIO | 600 | 600（不变） |
| L4 | 250 | 250（不变） |

---

## 待完成的优化（后续任务）

- Task 1: 解析失败重试机制
- Task 2: 增强解析器容错性
- Task 3: 失败输出降级注入下游
- Task 4: 前端单步重试按钮

---

## 实施顺序

1. **Task 1 + Task 2** - 快速见效，改动集中，预计 30 分钟 ✅
2. **Task 3 + Task 4** - 中期优化，需仔细测试解析器兼容性 ✅
3. **Task 5** - 体验增强，前后端联动 ✅

---

## 验证方式

- 构造各种 LLM 输出格式（markdown 包装、多行值、字段缺失）进行单元测试
- 实际运行委员会直播 10+ 次，观察 fallback 率是否下降
- 检查 Quant/Risk 角色在放宽限制后是否不再被截断
