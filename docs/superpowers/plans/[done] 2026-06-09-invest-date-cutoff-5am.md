# Plan: invest 统计日期 5:00 AM 截止逻辑

## Context

当前 invest 模块所有日期统计（PnL 快照、委员会归档、verdict 日期、每日报告、Dreaming 等）直接使用 `chrono::Local::now()` 获取日历日期。问题：凌晨 0:00-5:00 之间运行的任务（如 dream_invest 3:00 AM cron）会被计入当天而非前一天，导致日期归属错误。

**目标：** 凌晨 5:00 之前，所有统计日期归属前一天。例如 6 月 9 日凌晨 3 点 → 统计日期 = 6 月 8 日。

---

## Step 1: 创建集中的日期工具模块

**新建文件:** `src-tauri/src/invest/date_utils.rs`

```rust
use chrono::{Local, Timelike};

/// Invest 统计截止时间：每天 05:00 CST。
/// 05:00 之前运行的任务归属前一天。
const INVEST_DATE_CUTOFF_HOUR: u32 = 5;

/// 返回 invest 统计日期（YYYY-MM-DD）。
/// 05:00 之前返回昨天，05:00 及之后返回今天。
pub fn get_invest_date() -> String {
    let now = Local::now();
    let date = if now.hour() < INVEST_DATE_CUTOFF_HOUR {
        now - chrono::Duration::days(1)
    } else {
        now
    };
    date.format("%Y-%m-%d").to_string()
}

/// 返回 invest 统计日期（YYYYMMDD 紧凑格式）。
pub fn get_invest_date_compact() -> String {
    let now = Local::now();
    let date = if now.hour() < INVEST_DATE_CUTOFF_HOUR {
        now - chrono::Duration::days(1)
    } else {
        now
    };
    date.format("%Y%m%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invest_date_format_yyyy_mm_dd() {
        let d = get_invest_date();
        assert_eq!(d.len(), 10);
        assert!(d.contains('-'));
    }

    #[test]
    fn invest_date_compact_format() {
        let d = get_invest_date_compact();
        assert_eq!(d.len(), 8);
        assert!(!d.contains('-'));
    }
}
```

**注册模块:** 在 `src-tauri/src/invest/mod.rs` 添加 `pub mod date_utils;`

---

## Step 2: 替换统计日期调用点

以下文件中的 `Local::now().format(...)` 用于**统计日期归属**，需替换：

| # | 文件 | 行 | 当前代码 | 替换为 |
|---|------|-----|---------|--------|
| 1 | `src-tauri/src/lib.rs:105` | PnL 快照 today | `Local::now().format("%Y-%m-%d")` | `date_utils::get_invest_date()` |
| 2 | `src-tauri/src/invest/scheduler/runner.rs:89` | 调度器 today | `Local::now().format("%Y-%m-%d")` | `date_utils::get_invest_date()` |
| 3 | `src-tauri/src/invest/committee/archive.rs:41` | archive_date_dir | `Local::now().format("%Y-%m-%d")` | `date_utils::get_invest_date()` |
| 4 | `src-tauri/src/invest/committee/archive.rs:86` | events.jsonl today_str | `Local::now().format("%Y-%m-%d")` | `date_utils::get_invest_date()` |
| 5 | `src-tauri/src/invest/committee/archive.rs:262` | load_archive today 锚点 | `Local::now()` → 需用 invest date 的 NaiveDate |
| 6 | `src-tauri/src/storage/invest/committees.rs:43` | verdict_date | `now.format("%Y%m%d")` | `date_utils::get_invest_date_compact()` |
| 7 | `src-tauri/src/invest/daily_report.rs:20` | report today | `Local::now().format("%Y-%m-%d")` | `date_utils::get_invest_date()` |
| 8 | `src-tauri/src/invest/dreaming/pipeline.rs:53` | dreaming today | `Local::now().naive_local().date()` | 需用 invest date 的 NaiveDate |

**不修改的文件**（API 查询边界，非统计归属）：
- `committee/tools.rs` — Tushare 历史数据 end_date
- `event_scanner.rs` — 新闻扫描日期范围
- `macro_refresh.rs` — 宏观数据刷新范围
- `regime.rs` — Regime 计算数据范围
- `committee/orchestrator.rs` — 资产数据刷新 API 参数

**注意：** `committees.rs` 中 `created_at` 时间戳保持不变（真实记录时间），只改 `verdict_date`（每日覆盖语义的键）。

---

## Step 3: 前端 Dashboard 提示 UI

**修改文件:** `src/routes/invest/+page.svelte`

在 header 区域（line 131 "openInvest" 之后、line 134 tab 导航之前）添加：

```svelte
<p class="mb-[var(--space-2)] text-[11px] text-[var(--text-tertiary)]">
  📅 {t('invest_date_rule')}
</p>
```

**i18n keys：**

```json
// en.json
"invest_date_rule": "Invest date: stats before 05:00 are attributed to the previous day."

// zh-CN.json
"invest_date_rule": "统计日期：每日 05:00 前运行的数据归属前一天。"
```

---

## Step 4: 前端 invest date 辅助函数

在 `src/lib/utils/format.ts` 中添加：

```typescript
/** 返回 invest 统计日期（YYYY-MM-DD），05:00 前归前一天 */
export function getInvestDate(): string {
  const now = new Date();
  if (now.getHours() < 5) now.setDate(now.getDate() - 1);
  return now.toISOString().split('T')[0];
}
```

将 `TradeDialog.svelte:19` 的默认日期替换为 `getInvestDate()`。

---

## Step 5: 验证

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml invest::date_utils:: -- --nocapture
npm run check
npm run lint
npm run build
```

---

## 关键文件清单

- **新建:** `src-tauri/src/invest/date_utils.rs`
- **修改:** `src-tauri/src/invest/mod.rs`
- **修改:** `src-tauri/src/lib.rs`
- **修改:** `src-tauri/src/invest/scheduler/runner.rs`
- **修改:** `src-tauri/src/invest/committee/archive.rs`
- **修改:** `src-tauri/src/storage/invest/committees.rs`
- **修改:** `src-tauri/src/invest/daily_report.rs`
- **修改:** `src-tauri/src/invest/dreaming/pipeline.rs`
- **修改:** `src/routes/invest/+page.svelte`
- **修改:** `src/lib/components/invest/TradeDialog.svelte`
- **修改:** `messages/en.json` + `messages/zh-CN.json`
