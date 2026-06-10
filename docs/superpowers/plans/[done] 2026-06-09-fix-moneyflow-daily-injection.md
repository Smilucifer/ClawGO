# 修复：委员会资金流向注入改为当日数据

## Context

`aggregate_moneyflow` 累加最近 5 天数据后注入到 Quant R1 / CIO prompt，导致：
- 当天主力大幅净流入 +12.10 亿，但 5 天累计显示"主力净流出 12.9 亿"（方向完全相反）
- 委员会 LLM 看到的是过时信号，无法反映当日资金动向

**目标**：prompt 注入当日资金流向，保留 5 天汇总给 LLM 工具调用。

## 修改文件

### 1. `src-tauri/src/tushare/client.rs` — 新增方法 + 结构体

- `format_moneyflow_summary_latest()`：只取 `trade_date` 最大的一行格式化
- `MoneyflowCachePayload`：typed 结构体（`summary` + `daily_summary` + `days`），`#[serde(default)]` 兼容旧缓存
- `to_cache_json()`：统一构建缓存 JSON，消除两处重复构造

### 2. `src-tauri/src/invest/committee/orchestrator.rs`

- `AssetContext` 新增 `money_flow_daily_summary: Option<String>`
- `refresh_asset_data` + `refresh_moneyflow_cache`：改用 `MoneyflowDc::to_cache_json()` 一行替代手动 JSON 构造
- `build_asset_context`：typed 反序列化 `MoneyflowCachePayload`（替代 `serde_json::Value` 手动字段访问）
- CIO 注入：改用 `money_flow_daily_summary`

### 3. `src-tauri/src/invest/committee/roles.rs` — Quant R1 prompt 双注入

同时注入当日+近5日资金流向：
```
- 资金流向（当日）: {{money_flow_daily_summary}}
- 资金流向（近5日）: {{money_flow_summary}}
```

### 4. `src-tauri/src/invest/committee/tools.rs`

`exec_moneyflow` 改用 `MoneyflowCachePayload` typed 反序列化（仍返回 5 天汇总）。

## 向后兼容

旧缓存无 `daily_summary` 字段 → `#[serde(default)]` 空字符串 → 解构时 fallback 到 `summary`。

## 验证

`cargo check` 通过。
