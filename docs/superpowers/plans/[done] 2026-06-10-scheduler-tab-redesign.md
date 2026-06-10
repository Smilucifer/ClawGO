# SchedulerTab 重设计：卡片布局 + 可视化 Cron 构建器

> **Status: ✅ 完成** (2026-06-10, v5.3.0)
> 含后端 next_run 计算、状态映射合并、4 项 simplify 审查修复。

## Context

当前 SchedulerTab 使用纯表格布局 + 手写 cron 表达式输入。用户需要懂 cron 语法才能修改调度。`humanCron()` 是硬编码映射表，覆盖有限。本次重设计将其升级为卡片式监控面板 + 可视化 cron 辅助生成器。

## 改动范围

### 1. `src/lib/components/invest/SchedulerTab.svelte` — 完全重写

**布局**: 表格 → 卡片列表

每张卡片结构：
- **Header（始终可见）**: 状态灯 + 任务名/描述 + cron 可读显示 + 下次倒计时 + 开关
- **Detail（点击展开）**: 左右双栏
  - 左栏：调度预设列表 + "自定义"展开 5 字段可视化构建器
  - 右栏：最近运行时间线（时间点 + 状态 + 耗时 + 消息）
- **底部操作栏**: 立即运行 + 查看全部日志

**可视化 Cron 构建器**（方案 B 核心）：

"自定义"预设展开后显示 5 个字段行，每行一个 select：

| 字段 | 选项 | 说明 |
|------|------|------|
| 分钟 | 0, 15, 30, 45, */15, */30, 自定义输入 | 常用分钟值 |
| 小时 | *, 指定(多选), 范围 8-22 | 小时选择 |
| 日 | *（每天）, 1-31 指定 | 日期 |
| 月 | *（每月）, 1-12 指定 | 月份 |
| 星期 | *（每天）, 1-5（工作日）, 6,0（周末）, 指定 | 星期 |

每行选择后实时拼接为 cron 表达式，底部显示：
- 生成的 cron 表达式（monospace）
- 可读描述（如"工作日 9:30, 11:00"）

选择预设时自动回填 5 字段；自定义构建器修改时实时更新 cron。

**关键函数**:
- `humanCron(expr)` — 从硬编码 map 改为通用解析器，解析 5 段 cron 为可读中文/英文
- `parseCronToFields(expr)` — cron 字符串 → 5 字段对象（回填构建器用）
- `fieldsToCron(fields)` — 5 字段对象 → cron 字符串
- `computeNextRun(expr)` — 前端纯 JS 解析 cron 表达式，计算下次执行时间（后端 `next_run` 字段当前始终为 None，不依赖它）
- `formatCountdown(ms)` — 毫秒 → "1h 23m" / "12m" 格式
- 保留 `toggle()`, `runNow()`, `loadJobs()`, `loadLogs()`, `saveCron()`

### 2. `messages/en.json` + `messages/zh-CN.json` — 新增 i18n keys

新增约 20 个 key：

```
invest_scheduler_next_run        — "下次" / "Next"
invest_scheduler_countdown       — "{time}后" / "in {time}"
invest_scheduler_paused          — "已暂停" / "Paused"
invest_scheduler_trading_day     — "交易日" / "Trading Day"
invest_scheduler_presets         — "调度预设" / "Presets"
invest_scheduler_custom          — "自定义…" / "Custom…"
invest_scheduler_recent_runs     — "最近运行" / "Recent Runs"
invest_scheduler_no_runs         — "暂无运行记录" / "No runs yet"
invest_scheduler_view_all_logs   — "查看全部日志" / "All Logs"
invest_scheduler_footer          — 调度器信息
invest_scheduler_cron_minute     — "分钟" / "Minute"
invest_scheduler_cron_hour       — "小时" / "Hour"
invest_scheduler_cron_day        — "日" / "Day"
invest_scheduler_cron_month      — "月" / "Month"
invest_scheduler_cron_weekday    — "星期" / "Weekday"
invest_scheduler_cron_preview    — "预览" / "Preview"
invest_scheduler_cron_generated  — "生成表达式" / "Generated"
```

保留所有现有 key（向后兼容），删除不再使用的 key 可以后续清理。

### 3. 无需后端改动

- `CronJob` 结构体已有 `nextRun` 字段（后端未填充，前端纯 JS 计算倒计时）
- `update_cron_schedule` 接口不变
- `get_cron_job_logs` 已满足时间线需求
- `list_cron_jobs` 返回格式不变

## 验证

```bash
npm run check          # Svelte 类型检查
npm run lint           # ESLint
npm run i18n:check     # i18n key 完整性
npm run build          # 完整构建
```
