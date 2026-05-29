# Event Watch 设计文档

> Phase 3c — openInvest Event Watch 新闻扫描层
> 创建: 2026-05-29
> 来源: `[wip] 2026-05-28-openinvest-investgui-port.md` §4 + §11.8

## 概述

Event Watch 是 openInvest 的新闻事件扫描层，从 Tushare 获取上市公司新闻和公告，经过规则初筛 + LLM 归一化后存储到 invest.db，提供事件流 UI 和委员会触发能力。

**数据源：** Tushare `major_news`（财经快讯）+ `anns_d`（上市公司公告）。不引入 RSS/feedparser。

**架构：** 独立 Rust 模块 `invest/event_scanner.rs`，不耦合委员会编排器。

---

## §1 数据流

```
┌──────────────┐  ┌──────────────┐
│ Tushare      │  │ Tushare      │
│ major_news   │  │ anns_d       │
└──────┬───────┘  └──────┬───────┘
       │                 │
       └────────┬────────┘
                │
         ┌──────▼──────┐
         │ 规则初筛     │  Rust 关键词匹配
         │ severity    │  HIGH/MEDIUM/LOW
         └──────┬──────┘
                │ (过滤掉 LOW)
         ┌──────▼──────┐
         │ LLM 归一化   │  复用 LlmClient
         │ 提取结构字段  │  one_line_claim, stance,
         │             │  severity, affected_symbols
         └──────┬──────┘
                │
         ┌──────▼──────┐
         │ 存储 events  │  invest.db events 表
         └──────┬──────┘
                │
    ┌───────────┼───────────┐
    │           │           │
┌───▼───┐  ┌───▼───┐
│事件流  │  │触发   │
│UI 列表 │  │委员会  │
│       │  │确认框  │
└───────┘  └───────┘

注：Macro prompt 注入属于 Phase 4 范围，Phase 3c 不实现。
```

---

## §2 数据源：Tushare API 集成

### 2.1 新增 TushareClient 方法

在 `src-tauri/src/tushare/client.rs` 新增：

**`major_news(start_date, end_date, src)`**
- Tushare API: `major_news`
- 参数: `start_date`/`end_date`（YYYYMMDD）, `src`（来源：sina/10jqka/cls/yuncaijing）
- 返回: `datetime`, `content`, `title`
- 用途: 盘中/盘后财经快讯，覆盖政策、市场异动

**`anns_d(ts_code, start_date, end_date)`**
- Tushare API: `anns_d`
- 参数: `ts_code`（股票代码）, `start_date`/`end_date`
- 返回: `ann_date`, `ts_code`, `name`, `title`, `url`
- 用途: 公司级公告（财报、增减持、重大事项）

### 2.2 扫描策略

- `major_news`：按来源轮询（sina + cls），取最近 2 小时的新闻
- `anns_d`：只扫描持仓股票（HOLD + WATCH）的公告，取最近 24 小时
- 去重：用 `(source, title, created_at)` 做唯一性检查，已存在的跳过

---

## §3 规则初筛：关键词 Severity 分级

Rust 端实现，零 LLM 成本。

### 3.1 关键词表

**HIGH 级（触发委员会评估）：**
- 货币政策：央行、降准、降息、加息、MLF、LPR、逆回购
- 系统风险：暴跌、熔断、ST、退市、暂停上市、重大违法
- 政策冲击：关税、制裁、禁令、反垄断、行业整顿

**MEDIUM 级（记录但不自动触发）：**
- 财报相关：财报、业绩预告、净利润、营收
- 资本运作：增持、减持、回购、定增、分红
- 行业动态：产能、订单、并购、重组

**LOW 级（直接过滤丢弃）：**
- 不含任何 HIGH/MEDIUM 关键词的新闻

### 3.2 匹配逻辑

```rust
fn classify_severity(title: &str, body: &str) -> Option<Severity> {
    let text = format!("{} {}", title, body);
    if HIGH_KEYWORDS.iter().any(|k| text.contains(k)) => Some(Severity::High)
    else if MEDIUM_KEYWORDS.iter().any(|k| text.contains(k)) => Some(Severity::Medium)
    else => None  // 过滤掉
}
```

返回 `None` 的直接跳过，不进 LLM。

---

## §4 LLM 归一化

对通过规则初筛的新闻，用 LLM 提取结构化字段。

### 4.1 输入/输出

**输入：** 新闻 title + body（或公告 title）

**输出格式（JSON）：**
```json
{
  "one_line_claim": "央行宣布降准50个基点",
  "stance": "bullish",
  "severity": "high",
  "affected_symbols": ["600519.SH", "000858.SZ"]
}
```

### 4.2 字段定义

- `one_line_claim`：一句话摘要，≤30 字
- `stance`：`bullish` / `bearish` / `neutral`
- `severity`：LLM 可修正规则初筛的级别
- `affected_symbols`：从新闻内容中提取的涉及股票代码（A 股 6 位格式）

### 4.3 实现

- 复用现有 `LlmClient` trait（OpenAI 兼容协议）
- 使用委员会配置中的 Provider/Model
- 归一化 prompt 模板存 `~/.claw-go/invest/prompts/event_normalizer.md`，用户可编辑
- 批量处理：多条新闻拼成一个 prompt（每条带编号），一次 LLM 调用提取多条，减少 API 调用
- LLM 返回解析失败时保留规则初筛的 severity，stance 默认 neutral，affected_symbols 为空

### 4.4 与持仓关联

- LLM 返回的 `affected_symbols` 与当前 HOLD + WATCH 持仓做交集
- 交集非空 → 标记为「关联持仓事件」，UI 高亮
- 交集为空 → 仅记录，不触发提示

---

## §5 存储

复用已有的 `events` 和 `event_sources` 表，无需新增表。

### 5.1 Schema 变更

新增一个字段（migration，在 `init_db` 中 inline 执行，先检查列是否存在）：

```rust
// 检查 events 表是否有 stance 列，没有则添加
let has_stance: bool = conn
    .query_row("SELECT COUNT(*) FROM pragma_table_info('events') WHERE name='stance'", [], |r| r.get(0))
    .unwrap_or(0);
if !has_stance {
    conn.execute_batch("ALTER TABLE events ADD COLUMN stance TEXT DEFAULT 'neutral';")?;
}
```

存储 LLM 归一化的 stance（bullish/bearish/neutral），前端用于颜色编码。

### 5.2 字段映射

| 列 | 来源 |
|---|---|
| `id` | UUID |
| `source` | `"tushare_major_news"` 或 `"tushare_anns_d"` |
| `event_type` | `"news"` 或 `"announcement"` |
| `title` | Tushare 原始标题 |
| `body` | LLM 归一化后的 `one_line_claim` |
| `symbols` | LLM 提取的 `affected_symbols`，逗号分隔 |
| `severity` | LLM 修正后的 severity |
| `stance` | LLM 提取的 stance |
| `triggered` | 0 → 用户确认触发后设为 1 |
| `trigger_verdict_id` | 触发后关联的 verdict ID |
| `created_at` | Tushare 原始时间 |

### 5.3 event_sources 预置数据

两条预置记录：
- `tushare_major_news`：config = `{"src": "sina,cls"}`
- `tushare_anns_d`：config = `{"symbols": "auto"}`（auto = 从持仓自动取）

### 5.4 复用现有 CRUD

- `save_event` / `list_events` / `mark_event_triggered` — 直接用
- `save_event_source` / `list_event_sources` — 直接用

---

## §6 UI：事件监控 Tab

替换现有的 `events` 占位符。

### 6.1 布局

```
事件监控 Tab
├── 顶部状态栏
│   ├── 上次扫描时间 + 下次扫描倒计时
│   ├── 扫描开关（启用/禁用 cron job）
│   └── [立即扫描] 按钮
│
├── 筛选栏
│   ├── 时窗：[24h] [48h] [7d] ← 按钮组
│   ├── 严重度：[全部] [HIGH] [MEDIUM] ← chips
│   └── 搜索框（标题关键词）
│
├── 事件流列表
│   ├── 每条事件卡片：
│   │   ├── 左侧：severity 徽章（HIGH 红 / MEDIUM 黄）+ stance 标签
│   │   ├── 中间：一句话摘要 + 时间 + 来源标签
│   │   ├── 右侧：关联持仓 chips（如有）+ [触发委员会] 按钮
│   │   └── 点击展开：原始标题 + body + 来源链接（公告有 url）
│   └── 空状态：「暂无事件，点击立即扫描」
│
└── 底部：事件源配置
    ├── 数据源列表（tushare_major_news / tushare_anns_d）
    ├── 启用/禁用开关
    └── 配置编辑（src 来源、关注股票）
```

### 6.2 交互

- 点击「触发委员会」→ 弹出确认对话框（§7）
- 已触发的事件显示 `✓ 已触发` 标记，按钮灰掉
- severity HIGH 且未触发的事件置顶 + 左边框高亮

---

## §7 触发委员会确认对话框

```
┌─────────────────────────────────────────────┐
│  ⚠️ 事件触发委员会                          │
│                                             │
│  检测到高影响事件：                          │
│  「央行宣布降准50个基点」                    │
│  severity: HIGH | stance: bullish           │
│                                             │
│  关联持仓：600519, 000858                    │
│                                             │
│  辩论轮数：[4 ▼]                            │
│                                             │
│  是否立即启动委员会评估？                    │
│                                             │
│  [取消]                    [确认启动]        │
└─────────────────────────────────────────────┘
```

### 行为

- 关联持仓从事件的 `symbols` 字段取
- 辩论轮数下拉复用委员会的 6 档（1/2/3/4/6/8），默认 4
- 点击「确认启动」→ 调用 `run_committee_stream`，传入关联持仓 symbols + 辩论轮数
- 同时调用 `mark_event_triggered` 标记事件已触发
- 跳转到委员会直播 Tab 并自动开始流式展示

---

## §8 Cron Job 调度

### 8.1 调度规则

- 工作日 8-22 点每 30 分钟：`*/30 8-22 * * 1-5`
- 周末 9:00 / 18:00 各一次：`0 9,18 * * 0,6`
- 不需要 `is_trading_day` 守卫（事件扫描覆盖非交易时段）

### 8.2 扫描流程

1. 读取 `event_sources` 获取启用的数据源配置
2. `major_news`：查询最近 2 小时的新闻（sina + cls 来源）
3. `anns_d`：查询持仓股票最近 24 小时的公告
4. 规则初筛 → 过滤 LOW
5. LLM 归一化（批量）
6. 去重（`(source, title)` 唯一）→ 存入 `events` 表
7. 检查是否有 HIGH severity 且关联持仓的未触发事件 → 发 toast 通知

### 8.3 Tauri background task

- 复用现有 PnL 快照的 `tauri::async_runtime::spawn` 模式
- 在 `lib.rs` 启动时注册 cron job
- 扫描间隔、启用状态从 `event_sources` 配置读取

### 8.4 手动触发

- UI 的「立即扫描」按钮直接调用同一扫描函数
- 显示 spinner → 完成后 toast「扫描完成，发现 N 条新事件」

---

## §9 关键文件清单

### 新增文件

| 文件 | 用途 |
|------|------|
| `src-tauri/src/invest/event_scanner.rs` | 事件扫描器主模块 |
| `src/lib/components/invest/EventWatchTab.svelte` | 事件监控 Tab UI |
| `~/.claw-go/invest/prompts/event_normalizer.md` | LLM 归一化 prompt 模板 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `src-tauri/src/tushare/client.rs` | 新增 `major_news` / `anns_d` 方法 |
| `src-tauri/src/storage/invest/mod.rs` | `events` 表 migration 加 `stance` 列 |
| `src-tauri/src/storage/invest/events.rs` | `Event` struct 加 `stance` 字段 |
| `src-tauri/src/commands/invest.rs` | 新增 `scan_events` / `get_scan_status` Tauri 命令 |
| `src-tauri/src/lib.rs` | 注册新命令 + 启动事件扫描 cron |
| `src/routes/invest/+page.svelte` | 替换 events 占位符为 EventWatchTab |
| `src/lib/stores/invest-store.svelte.ts` | 新增事件相关状态和方法 |
| `messages/en.json` + `messages/zh-CN.json` | i18n keys |

---

## §10 实施范围

Phase 3c 实施内容：
1. Tushare `major_news` + `anns_d` API 集成
2. 规则初筛（关键词 severity）
3. LLM 归一化（复用 LlmClient，批量处理）
4. `events` 表 stance 字段 migration
5. 事件扫描 cron job（独立于委员会）
6. Event Watch Tab UI（事件流 + 筛选 + 状态栏 + 数据源配置）
7. 触发委员会确认对话框
8. i18n 更新
