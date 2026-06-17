# 修复委员会 R2 Fallback + CIO 总资产幻觉

## Context

委员会直播中两个问题：
1. 量化 R2 和风控 R2 **总是**显示"输出缺少关键字段，可能需要重新分析"
2. CIO 报告中"总资产约 1,562 CNY"是 LLM 幻觉 — CIO prompt 未注入真实组合数据

真实 demo (`002384.SZ.md`) 证实：R2 实际产出了完整内容，但被错误标记为 `[WORKER_UNAVAILABLE]`，导致 CIO 被安全阀强制降级为 HOLD+低置信度。

## 根因分析

### Bug 1（必现）：`detect_fallback_reason` 不区分 R1/R2 轮次

`parser.rs:181-183`：对所有 Quant 轮次都检查 `regime`。但 Quant R2 prompt 只要求输出 `调整信号 | 调整强度 | 调整买点 | 保护触发 | 推理`，不要求 REGIME → `parsed.regime` 永远为 `None` → 100% 触发 `missing_critical_fields`。

### Bug 2（间歇性）：CLI 路径缺少重试

`orchestrator.rs:1633-1643`：CLI 执行路径解析一次就结束，不调用 `retry_on_fallback`。当 LLM 格式偏移时一次失败就报错。

### Bug 3：CIO prompt 未注入 portfolio 数据

`cli_executor.rs:642`：`build_cli_cio_prompt` 不接收 `portfolio_data`，CIO 无法获知总资产/持仓/现金，只能靠猜。Risk R1 输出了 `可用子弹: 460.63 CNY`，但 CIO 不知道总资产 → 幻觉 "总资产约1,562 CNY"。

### 级联效应

`orchestrator.rs:1079`：`fallback_reason.is_some()` → 下游替换为 `[WORKER_UNAVAILABLE]` → CIO 安全阀触发 → verdict=HOLD + confidence≤0.4。

## 修复方案

### Fix 1: `detect_fallback_reason` 加入 `round` 参数

**文件**: `parser.rs` — Quant R2 不要求 `regime`：
```rust
CommitteeRole::Quant => {
    if round >= 2 { parsed.signal.is_none() }
    else { parsed.signal.is_none() || parsed.regime.is_none() }
}
```

### Fix 2: 更新所有调用点 + 单元测试

**文件**: `orchestrator.rs` — ~10 处调用改为 `detect_fallback_reason(role, round, &parsed)`。
**文件**: `parser.rs` — ~10 个测试更新 round 参数，新增 Quant R2 regime=None 不触发 fallback 测试。

### Fix 3: CLI 路径增加重试

**文件**: `orchestrator.rs` — `run_role_phase` 中 `Ok(raw_text)` 分支，解析后检测 fallback，触发则重调 `cli.run_role` 一次。

### Fix 4: CIO prompt 注入 portfolio 数据

**文件**: `cli_executor.rs` — `build_cli_cio_prompt` 新增 `portfolio_summary: &str` 参数，在 CLI additions 中注入。
**文件**: `orchestrator.rs` — `run_role_phase` 中 Cio 分支传入 `build_portfolio_summary(portfolio_data)`。

## 验证

1. `cargo check --manifest-path src-tauri/Cargo.toml`
2. `cargo test --manifest-path src-tauri/Cargo.toml parser::tests -- --nocapture`
3. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
4. `npm run build`
