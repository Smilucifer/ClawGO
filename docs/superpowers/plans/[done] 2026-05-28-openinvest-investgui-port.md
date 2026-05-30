# openInvest + invest-gui → ClawGO 功能移植计划

> 状态:[done] Phase 1 + Phase 2 + Phase 3a + Phase 3b + Phase 3c + Phase 4a + Phase 4b 全部完成。RFC D1-D11 全部决议已确认(2026-05-29)。
> 创建:2026-05-28
> 更新:
> - 2026-05-29(v1) — 整合完整研究结果:5 角色 system prompt、编排算法、Dreaming 对比、Provider 体系
> - 2026-05-29(v2) — 多路审查后锁定 D1–D11 工程决议、HOLD/WATCH 双类、Dashboard 现金 KPI、系统二级页 7 Tab、用户档案、Phase 3 拆分(详见「十一、v2 增量决议」)
> - 2026-05-29(v3) — RFC 全部确认,同步 RFC 关键设计到 §6.1/§7.3/§7.7/§8.3
> - 2026-05-29(v4) — Phase 2 实施完成(v3.2.0),13 项审查修复,更新 Phase 2 任务状态
> - 2026-05-29(v5) — Phase 3a 实施完成,14 项审查修复,更新 Phase 3a 任务状态
> - 2026-05-30(v6) — Phase 4a 实施完成(Scheduler + Verdict Review + Dreaming + FTS5 + Archived),14 项审查修复
> - 2026-05-30(v6) — Phase 3c 实施完成(Event Watch),8 项审查修复,更新 Phase 3c 任务状态
> 配套文档:`[done] 2026-05-29-committee-engineering-rfc.md`
> Phase 2 实施计划:`[done] 2026-05-29-phase2-dashboard-portfolio-pnl.md`
> Phase 2 设计文档:`[done] 2026-05-29-phase2-dashboard-portfolio-pnl-design.md`
> Phase 3c 实施计划:`[done] 2026-05-29-event-watch-impl.md`

## 背景

openInvest（Python 后端）和 invest-gui（React 前端）是 AI 投资委员会系统，包含持仓管理、5 角色辩论决策、交易记录、策略配置等功能。目标是将这些功能移植到 ClawGO 中，统一使用 ClawGO 已有技术栈。

## 范围：仅 A 股市场

不考虑美股、ETF（海外）、加密货币等。Tushare MCP 完整覆盖 A 股行情、财务、资金流向、板块、ETF 数据。

---

## 一、侧边栏与导航改动

### 1.1 新增 openInvest 独立入口

在侧边栏图标轨道中新增 openInvest 入口，位于 `/plugins` 和 `/memory` 之间：

| 顺序 | 路由 | 图标 | label | 说明 |
|------|------|------|-------|------|
| 1 | `/chat` | message | `nav_chat` | 不变 |
| 2 | `/explorer` | folder | `nav_explorer` | 不变 |
| 3 | `/plugins` | zap | `nav_extend` | 不变 |
| **4** | **`/invest`** | **trending-up** | **`nav_invest`** | **新增 — openInvest 投资委员会** |
| 5 | `/memory` | book | `nav_memory` | 不变 |
| 6 | `/usage` | chart | `nav_usage` | 不变 |
| 7 | `/history` | clock | `nav_history` | 不变 |
| 8 | `/settings` | settings | `nav_settings` | 不变 |

`/invest` 页面内部有自己的 Tab 导航，包含所有投资功能子页面。

### 1.2 记忆管理独立路由 `[已确认]`

记忆管理从设置中拆出，作为独立路由 `/memory-mgmt`，位于设置和历史之间：

| 顺序 | 路由 | 图标 | label | 说明 |
|------|------|------|-------|------|
| 1-4 | ... | ... | ... | 不变 |
| 5 | `/memory` | book | `nav_memory` | 不变 — 记忆文件编辑器 |
| 6 | `/usage` | chart | `nav_usage` | 不变 |
| 7 | `/settings` | settings | `nav_settings` | 不变 |
| **8** | **`/memory-mgmt`** | **database** | **`nav_memory_mgmt`** | **新增 — 记忆管理** |
| 9 | `/history` | clock | `nav_history` | 不变 |

### 1.3 Doctor 入口 `[已确认]`

使用标题栏 `[···]` 下拉菜单，包含 Doctor 诊断面板入口。

### 1.4 Memory Extraction 「应用并重载」按钮 `[已确认]`

在 Memory Extraction 配置区底部添加：
- 「应用并重载」按钮 — 保存配置 + 触发 `clawgo:memory-config-reloaded` 事件 + 显示 toast 确认
- 配置变更后按钮高亮（dirty state），保存后恢复

---

## 二、记忆管理页面设计 `/memory-mgmt`

### 2.0 统一记忆架构：三 Scope 模型 `[已确认]`

**核心设计：`memories` 表新增 `scope` 字段，将记忆分为三个互相隔离的域。**

```
scope='global'    — 跨项目通用（用户身份、偏好、技能）
scope='project'   — 项目专属（技术栈、已知问题、架构决策）
scope='invest'    — 投资专属（委员会裁决、策略偏好、持仓上下文）
```

**注入隔离逻辑：**
- 通用聊天 → 只注入 `global`
- 项目群聊 → 注入 `global` + 当前 `project`（按 `project_id` 匹配工作目录）
- 委员会运行 → 只注入 `invest` + `domain_insights`（独立表，Dreaming 输出）
- 三个 scope 互不可见，投资记忆不会泄漏到日常聊天

**去掉 Approve 流程 `[已确认]`：**
- 当前 `pending → approved → archived` 简化为 `active → archived`（衰减淘汰）/ `deleted`（用户手动）
- LLM 自动提取的记忆直接 `status='active'`，confidence 默认 1.0
- 靠 Dream 衰减自然淘汰弱记忆，不再需要人工审批

**Schema：**
```sql
CREATE TABLE memories (
    id          TEXT PRIMARY KEY,
    scope       TEXT NOT NULL DEFAULT 'global',  -- global | project | invest
    project_id  TEXT,                             -- scope='project' 时有效，存项目路径
    content     TEXT NOT NULL,
    memory_type TEXT NOT NULL DEFAULT 'fact',
    confidence  REAL NOT NULL DEFAULT 1.0,        -- 直接 1.0，去掉 approve
    source_kind TEXT NOT NULL DEFAULT 'extraction',
    source_run_id TEXT,
    source_group_chat_id TEXT,
    tags        TEXT NOT NULL DEFAULT '[]',
    status      TEXT NOT NULL DEFAULT 'active',   -- active | archived | deleted
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE INDEX idx_memories_scope ON memories(scope);
CREATE INDEX idx_memories_project ON memories(project_id);
CREATE INDEX idx_memories_status ON memories(status);
```

**两条 Dreaming 路径：**
| 路径 | scope | 目标表 | 机制 |
|------|-------|--------|------|
| 用户记忆 Dream | global / project | `memories` | 合并近似 + 衰减归档 |
| 投资 Dream | invest → domain_insights | `domain_insights` | 三阶段统计管道（Light→REM→Deep） |

**Claude Code 原生记忆保持独立**（`~/.claude/projects/` markdown 文件）：
- 它是对话层的，每次 Claude Code 会话自动加载
- 与 ClawGO 应用层 memory.db 互补，不冲突，不需要合并

### 2.1 页面结构

```
/memory-mgmt（独立路由）
├── 顶部：2 个子 Tab 切换
│   ├── 用户记忆 (user) — 默认
│   └── 提取配置 (extraction)
│
├── 用户记忆 Tab
│   ├── Scope 筛选：[全部] [global] [project] [invest]  ← 新增
│   ├── 搜索栏 + 新增按钮
│   ├── 记忆卡片列表（scope 徽章 + 类型徽章 + 描述 + 时间 + 操作）
│   │   ├── Scope 标签：global(蓝) / project(绿) / invest(金)
│   │   ├── 类型筛选：user / feedback / project / reference
│   │   ├── Dreaming 生成的记忆带「💤」标识
│   │   ├── 操作：编辑 / 删除
│   │   └── 空状态提示
│   ├── Dreaming 控制区（用户记忆：global + project scope）
│   │   ├── [💤 Dreaming] 开关（启用/禁用自动 dreaming）
│   │   ├── 自动间隔设置（分钟/小时）
│   │   ├── [手动触发 Dream] 按钮
│   │   ├── [回滚上次 Dream] 按钮（快照有效时可用）
│   │   └── Dream Trace 列表（trigger/时间/状态/摘要）
│   └── 底部：导入/导出按钮
│
└── 提取配置 Tab
    ├── 启用/禁用开关
    ├── Chat API Endpoint
    ├── Chat API Key（带显示/隐藏）
    ├── Chat Model
    ├── ── 分隔线 ──
    └── [应用并重载] 按钮（dirty 时高亮）
```

**注：** 投资 Dream（三阶段管道）的控制在 `/invest` 定时任务 Tab 中，不在 `/memory-mgmt`。
`/memory-mgmt` 的 Dreaming 只管用户记忆（global/project）的合并+衰减。

### 2.2 与现有入口的关系

| 现有入口 | 改动 |
|----------|------|
| 侧边栏 `/memory` 路由 | **保留** — 仍为记忆文件编辑器（Markdown） |
| 标题栏灯泡图标 (UserMemoryPanel) | **保留** — 快捷入口，面板内容不变 |
| CommandPalette `ocv:toggle-memory` | **保留** |
| `/settings/characters` 角色记忆按钮 | **移除** — 角色记忆不再单独管理 |
| 设置页旧 `memory` Tab | **移除** — 由独立路由取代 |

### 2.3 角色记忆移除 `[已确认]`

**移除角色记忆（Character Memory）：**
- `/memory-mgmt` 不再有「角色记忆」Tab
- AiCharacter 角色库中移除记忆入口
- `CharacterMemoryPanel.svelte` 标记为废弃（可保留代码但不导入）
- 后端 `search_character_memories` / `get_character_memory` 命令保留但不再被 UI 调用

**原因：** 委员会角色使用独立 prompt 注入，不进 AiCharacter 库，不需要角色记忆管理。用户记忆（user/feedback/project/reference）已足够覆盖所有需求。

### 2.4 Dreaming 控制与回滚 `[已确认]`

参考 PilotDeck 的 Dreaming 管理模式，在用户记忆 Tab 底部添加 Dreaming 控制区：

**控制项：**
- **Dreaming 开关**：启用/禁用自动 dreaming（默认关闭）
- **自动间隔**：数字输入 + 分钟/小时选择器（默认 120 分钟）
- **手动触发 Dream**：点击立即运行一次 dreaming 管道
- **回滚上次 Dream**：快照有效时可用，点击恢复到 dream 前状态

**回滚机制（参考 PilotDeck）：**
- Dream 执行前：自动快照当前 `domain_insights` 表数据
- Dream 成功后：保留「before」快照，直到下次 dream 或手动清除
- 回滚时：验证当前状态与快照的「after」匹配，恢复「before」状态
- 安全机制：如果 dream 后又手动编辑了记忆，回滚按钮自动禁用（`rollbackReady=false`）

**Dream Trace 审计：**
- 每次 dream 运行产生 trace 记录（trigger/时间/状态/变更摘要）
- 在用户记忆 Tab 中展示 dream trace 列表

---

## 三、openInvest 页面设计

### 3.1 路由结构

```
/invest                     — 主页面（Tab 导航）
/invest                     — Dashboard（默认 Tab）
```

### 3.2 Tab 导航（页面内部）

```
┌─────────────────────────────────────────────────────────────────────┐
│  [Dashboard] [委员会] [策略] [交易记录] [事件监控] [历史命中率] [定时任务] │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│                         Tab 内容区                                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

| Tab | 来源 | 核心功能 |
|-----|------|----------|
| Dashboard | invest-gui Dashboard | 持仓卡片、总资产、PnL、最新裁决摘要、PnL 趋势图 |
| 委员会 | invest-gui committee/ | 7 子 Tab（见下方） |
| 策略 | invest-gui Strategy | 目标配置 CRUD（stock/cash 比例、资产列表） |
| 交易记录 | openInvest History | 买进/卖出流水 + 交易表格 |
| 事件监控 | openInvest Event Watch | 新闻事件流 + 触发记录 + 配置 |
| 历史命中率 | openInvest verdict_review | 事后回顾：1d/7d/30d 命中率 + 波动率自适应阈值 + 方向性指标 |
| 定时任务 | openInvest scheduler | Cron job 管理（开关/调度/手动触发/运行日志） |

### 3.3 委员会子 Tab（7 Tab）

| 子 Tab | 功能 | 关键组件 |
|--------|------|----------|
| 直播 | 触发委员会 + SSE 实时推送 | 启动按钮、StatusBadge、SseIndicator |
| 决议归档 | 历史裁决列表 + 详情 | 左右双栏、CommitteeDetail |
| 决策回放 | PipelineFlow 动画 | PipelineFlow 组件（Svelte 重写） |
| 角色配置 | 5 角色 prompt + 规则 | AgentCard × 5、REGIME 硬规则表 |
| 命中率 | 历史 accuracy 统计 | KPI 卡片、按时间/verdict 表格 |
| LLM 用量 | Token/cost 统计 | KPI 卡片、按角色拆分表 |
| Tool 调用 | 工具调用日志 | 过滤栏 + 调用卡片列表 |

### 3.4 持仓管理与交易流水

**Dashboard 持仓区：**
- 持仓表格：股票/代码/持仓量/成本价/现价/盈亏/最新裁决
- 「调整持仓」按钮（非简单「添加」）→ 弹出买入/卖出对话框
- 买入：选择股票（Tushare 搜索）+ 数量 + 价格（或市价）→ 加权平均成本计算 → 扣减现金
- 卖出：选择已有持仓 + 数量 + 价格 → 减少持仓 → 增加现金
- 修改：直接编辑持仓字段（成本价修正等）

**交易流水（`record_external_trade` 原子事务）：**
```
买进流程：
1. 查找/创建 holding
2. 加权平均成本：new_avg = (cur_avg * cur_qty + amount) / new_qty
3. Upsert holding
4. 扣减 cash[currency]
5. 追加 history.jsonl 审计记录

卖出流程：
1. 减少 target.units（clamp to 0）
2. 增加 cash[currency]
3. 追加 history.jsonl 审计记录
```

**交易记录 Tab：**
- 表格：日期/股票/方向(买/卖)/数量/价格/金额/状态
- 筛选：按日期/股票/方向
- 导出：CSV

**PnL 快照（参考 openInvest `pnl_snapshot.py`）：**
- 定时：工作日 4 次,贴盘前/盘中/盘后(`30 9,11 * * 1-5` + `0 13,15 * * 1-5`,即 9:30 / 11:00 / 13:00 / 15:00,北京时间)— 已对齐 RFC §4.6
- 守卫：`is_trading_day` (非交易日自动跳过)
- 流程：读取持仓 → Tushare MCP 获取现价 → 计算每资产+总体 PnL% → 追加 `pnl_history.jsonl`
- Dashboard 渲染：PnL 趋势折线图（日/周/月视图）
- 与沪深300基准对比

### 3.5 PipelineFlow 动画（Svelte 重写）

原 invest-gui 使用 Framer Motion，ClawGO 改用 **Svelte transitions + CSS animations**：

```
[Macro] ──●── [Quant + Risk] ──●── [Round 2] ──●── [CIO]
  紫色       流动光点    蓝+橙        流动光点       黄色
```

- 节点：48×48 圆形，角色颜色编码（macro=紫、quant=蓝、risk=橙、wealth=绿、cio=黄）
- 状态：pending(灰) → active(脉冲光晕) → done(绿勾) / error(红叉)
- 连接线：渐变填充 + 流动光点（3 个圆点循环运动）
- 动态轮数：根据辩论轮数自动拉长，`overflow-x-auto`
- 入场动画：Svelte `transition:fly={{ x: 20 }}`

---

## 四、Event Watch 新闻扫描层

### 4.1 架构

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ Tushare 新闻 │  │  RSS Feed    │  │ 财联社公告   │
│ (MCP 已有)   │  │ (feedparser) │  │ (MCP 已有)   │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │
       └────────┬────────┴────────┬────────┘
                │                 │
         ┌──────▼──────┐   ┌─────▼──────┐
         │ LLM 归一化  │   │ 向量存储   │
         │ (flash 批)  │   │ (SQLite    │
         │             │   │  FTS5)     │
         └──────┬──────┘   └────────────┘
                │
         ┌──────▼──────┐
         │ 三重过滤    │
         │ severity    │
         │ stance      │
         │ entity match│
         └──────┬──────┘
                │
    ┌───────────┼───────────┐
    │           │           │
┌───▼───┐  ┌───▼───┐  ┌───▼───┐
│触发   │  │注入   │  │通知   │
│委员会 │  │Macro  │  │Toast  │
│确认框 │  │prompt │  │推送   │
└───────┘  └───────┘  └───────┘
```

### 4.2 触发确认 `[已确认]`

事件监控触发委员会重跑时，弹出确认对话框：

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
│  是否立即启动委员会评估？                    │
│                                             │
│  [取消]                    [确认启动]        │
└─────────────────────────────────────────────┘
```

### 4.3 数据源适配（A 股）

| openInvest 源 | ClawGO 替代 |
|---------------|-------------|
| DuckDuckGo News | Tushare `news` / `major_news` MCP |
| RSS Feed | 保留 RSS（财新/第一财经等中文源） |
| yfinance News | Tushare `anns_d`（上市公司公告） |

### 4.4 存储

- 复用 ClawGO SQLite FTS5 架构（已在记忆系统中使用）
- 新增 `events` 表：event_id, one_line_claim, event_type, stance, severity, affected_symbols, created_at
- 新增 `event_sources` 表：event_id, source_url, source_name, fetched_at

### 4.5 UI（事件监控 Tab）

```
事件监控 Tab
├── 顶部：状态栏（上次扫描时间、下次扫描倒计时、开关）
├── 事件流列表
│   ├── 每条：severity 徽章 + stance 标签 + 一句话摘要 + 时间 + 关联股票
│   └── 点击展开：详情 + 来源链接 + 关联委员会裁决
└── 底部：配置区（扫描频率、最低 severity、关注股票/板块）
```

---

## 五、历史命中率（AccuracyTab）

**注意：openInvest 没有传统回测（backtest），而是事后回顾系统（verdict_review）。**

### 5.1 架构

```
历史命中率 Tab
├── KPI 卡片区
│   ├── 总裁决数
│   ├── 30 天命中率（整体）
│   └── 方向性命中率（不含 HOLD）← 真实 alpha 指标
│
├── 诚实解读 Banner（方向性命中率 < 50% 时自动显示）
│   └── 解释 HOLD 天然高命中的原因
│
├── 按时间窗口表
│   └── 1d / 7d / 30d：样本数 + 命中率 + 进度条
│
├── 按裁决类型表
│   └── BUY / ACCUMULATE / HOLD / TRIM / SELL：样本数 + 平均置信度 + 1d/7d/30d 命中率
│
└── 完整报告（可折叠）
    └── verdict_accuracy.md 内容
```

### 5.2 命中率计算逻辑

- **BUY/ACCUMULATE（方向 up）**：hit = `return_pct > 0`
- **SELL/TRIM（方向 down）**：hit = `return_pct < 0`
- **HOLD（方向 flat）**：hit = `abs(return_pct) < flat_threshold`
- **HOLD 阈值**：`K_FLAT × atr% × √(days)`，上限 8%（波动率自适应，不同资产不同阈值）
- **Gold CNY proxy**：`(GC=F × USDCNY)` 复合收益

### 5.3 后端

- `verdict_review` Rust 模块（从 Python `jobs/verdict_review.py` 移植）
- 扫描 `.committee/<date>/<symbol>.md` + `.backtest/` 目录
- 输出：`verdict_review.jsonl`（结构化）+ `verdict_accuracy.md`（人类可读报告）
- Tauri 命令：`get_verdict_review_summary`、`get_verdict_review_report`、`get_verdict_review_data`
- 定时任务：每日 08:00 工作日自动运行 review

### 5.4 决策归档

- 每次委员会运行后 `_persist()` 写入 `.committee/<date>/<symbol>.md`
- 文件内容：Header + 宏观快照 + CIO Memo + 各角色输出 + 多轮辩论
- 同时记录到 `events.jsonl`（append-only 审计日志）
- Rust 存储：`storage/verdict_archive.rs`

### 5.5 Dreaming Insight Score（记忆置信度）

**移除向量数据库后完全保留**，因为它是纯确定性公式：

```
reliable: score = hit_rate × 0.7 + min(count/10, 1.0) × 0.3
caution:  score = quality × 0.7 + sample × 0.3  (lift-based)
```

- 毕业阈值：`score >= 0.8` 且 `count >= 3`
- K_FLAT 故意不学习（防 Goodhart）
- Crash regime 样本排除但保留日志
- 与 LanceDB/embedding 完全无关

---

## 五 bis、定时任务管理（Cron Job 管理页）

### 5b.1 概述

openInvest 有多个定时任务需要统一管理。在 `/invest` 页面新增「定时任务」Tab，集中管理所有定时任务的开关、调度、手动触发和运行日志。

### 5b.2 定时任务清单

> 已对齐 RFC §4.6 优化后 cron 表。所有任务统一通过 `is_trading_day(date)` 守卫,非交易日自动跳过(`requires_trading_day=true` 的任务)。

| 任务 | Cron 表达式 | trading_day 守卫 | 说明 | 默认状态 |
|------|------------|------------------|------|----------|
| PnL 快照 | `30 9,11 * * 1-5` + `0 13,15 * * 1-5` | ✅ | 工作日 4 次,贴盘前/盘中/盘后(9:30 / 11:00 / 13:00 / 15:00) | 启用 |
| Verdict Review | `0 17 * * 1-5` | ✅ | 工作日 17:00 评估当日裁决准确性 | 启用 |
| 每日报告 | `0 22 * * 1-5` | ✅ | 工作日 22:00 生成当日 PnL + 持仓摘要 | 启用 |
| Event Watch 扫描 | `*/30 8-22 * * 1-5` + `0 9,18 * * 0,6` | ❌ | 工作日 8-22 点每 30 分钟,周末 9:00 / 18:00 各一次(节假日也有政策新闻) | 启用 |
| Dreaming(用户记忆) | 可配置(默认 120 分钟) | ❌ | 路径 A:合并近似 + 衰减归档(global/project scope) | 禁用 |
| Dreaming(投资) | `0 3 * * *` | ✅ | 路径 B:3 阶段统计管道(invest → domain_insights),昨日是交易日才跑 | 禁用 |
| Payday Check | `0 9 25 * *` | ❌ | 每月 25 日检查工资入账(按自然日) | 启用 |

### 5b.3 UI 设计

```
定时任务 Tab
├── 任务列表
│   ├── 每行：任务名 + Cron 表达式（人类可读） + 状态开关 + 上次运行 + 下次运行
│   ├── 操作：手动触发 / 查看日志 / 编辑调度
│   └── 状态指示：运行中（spinner）/ 空闲 / 错误（红色）
│
├── 手动触发
│   └── 点击「运行」→ 立即执行一次，显示 spinner + 结果 toast
│
└── 运行日志
    └── 选中任务 → 展示最近 N 次运行记录（时间/状态/耗时/摘要）
```

### 5b.4 Tauri 命令

```
commands/scheduler.rs — list_cron_jobs, get_cron_job_detail, toggle_cron_job,
                        update_cron_schedule, trigger_cron_job, get_cron_job_logs
```

### 5b.5 存储

- 配置：`~/.claw-go/invest/scheduler.json`（任务开关 + 自定义 cron 表达式）
- 日志：`~/.claw-go/invest/scheduler_logs.jsonl`（追加式运行记录）
- Tauri background task 调度器（替代 APScheduler）

---

## 六、数据层（Rust 存储模块）

### 6.1 新增存储模块

> **RFC 同步 (2026-05-29)**: 持仓/交易/现金/PnL/裁决/事件/洞察 迁入独立 `invest.db`(D4),与 `memory.db` 物理隔离。使用 SQLite WAL + `BEGIN IMMEDIATE` 保证 `record_external_trade` 原子性(D3)。详见 RFC §3。

| 模块 | 文件 | 数据模型 |
|------|------|----------|
| 持仓管理 | `storage/portfolio.rs` | Holdings: symbol(A股代码), qty, avg_cost, status, ccy; cash: HashMap<ccy, f64> |
| 交易记录 | `storage/trades.rs` | Trades: symbol, side(buy/sell), qty, price, date, status; history.jsonl 审计 |
| 策略配置 | `storage/strategy.rs` | Strategy: target_allocation, target_assets, constraints |
| 事件存储 | `storage/events.rs` | Events: event_id, claim, type, stance, severity, symbols |
| 决策归档 | `storage/verdict_archive.rs` | Verdict: date, symbol, verdict, confidence, macro_snapshot, debate_text |
| 命中率回顾 | `storage/verdict_review.rs` | Review: actual_returns, hits, regime, macro_shock; verdict_review.jsonl |
| PnL 快照 | `storage/pnl.rs` | PnL: timestamp, total_value, per_asset_pnl, pnl_pct; pnl_history.jsonl |
| 领域洞察 | `storage/insights.rs` | Insights: asset, verdict, regime, window, hit_rate, sample_count |
| 记忆 Scope | `storage/memory_store.rs` 改动 | `memories` 表新增 `scope`(global/project/invest) + `project_id` 字段，去掉 approve |
| 定时任务 | `storage/scheduler.rs` | CronJob: name, cron_expr, enabled, last_run, next_run; scheduler_logs.jsonl |

### 6.2 Tauri 命令

```
commands/portfolio.rs  — get_portfolio, buy_stock, sell_stock, adjust_holding, record_external_trade
commands/trades.rs     — get_trades, get_trade_history, get_trade_audit_log
commands/strategy.rs   — get_strategy, update_strategy
commands/events.rs     — get_events, scan_events, get_event_detail
commands/backtest.rs   — run_backtest, list_backtests, get_backtest_result
commands/verdict_review.rs — get_verdict_review_summary, get_verdict_review_report, get_verdict_review_data
commands/committee.rs  — run_committee, get_committee_status, list_verdicts
commands/pnl.rs        — get_pnl_history, get_pnl_snapshot, trigger_pnl_snapshot
commands/dreaming.rs   — get_dream_config, set_dream_config, trigger_dream, rollback_dream, list_dream_traces
commands/memory.rs     — list_memories(scope_filter, project_filter), update_memory_scope, migrate_approve_to_active
commands/scheduler.rs  — list_cron_jobs, get_cron_job_detail, toggle_cron_job, update_cron_schedule, trigger_cron_job, get_cron_job_logs
```

---

## 七、AI 委员会（独立编排，不进 AiCharacter 库）`[已确认]`

### 7.1 架构决策

**关键决策：委员会角色不注册为 AiCharacter，使用独立 prompt 注入机制。**

原因：
- AiCharacter 是通用角色模板，委员会角色有严格的信息隔离要求
- 委员会编排流程（多轮辩论、收敛检测、SENTINEL 覆写）与 Group Chat orchestrator 差异较大
- 需要暴露可编辑的 system prompt，且与现有群聊系统隔离

实现方式：
- 委员会角色 prompt 存储在 `~/.claw-go/invest/prompts/` 目录下
- 每个角色一个 `{role}.md` 文件，用户可在「角色配置」Tab 中编辑
- 编排器（Rust）读取 prompt + 注入数据，通过 Provider 体系调用 LLM
- 委员会运行完全独立于 Group Chat，不复用 orchestrator

### 7.2 角色配置（5 角色）

| 角色 | 存储文件 | 职责 | 信息隔离 | 工具权限 |
|------|----------|------|----------|----------|
| MacroStrategist | `macro.md` | 宏观分析（利率、汇率、系统性风险） | 仅看 macro_data + event_brief | ✅ 可调用白名单工具 |
| QuantAnalyst | `quant.md` | 技术分析（RSI、均线、百分位、趋势、regime） | 仅看 market_data + regime | ❌ 无工具 |
| RiskOfficer | `risk.md` | 风险评估（集中度、浮盈缓冲、尾部风险、行为模式） | 仅看 portfolio + wealth_context + prior_insights | ❌ 无工具 |
| WealthContextOfficer | `wealth.md` | 解读用户 off-portfolio 财务背景 | 仅看 user.md + portfolio cash | ❌ 无工具 |
| CIO | `cio.md` | 综合裁决（BUY/ACCUMULATE/HOLD/TRIM/SELL） | 看所有（宏观 + 辩论历史 + portfolio + wealth） | ❌ 无工具 |

### 7.3 工具白名单 `[已确认]`

> **RFC 同步**: LLM 调用层设计详见 RFC §1 — `InvestLlmClient` trait、`LlmGovernor`(per-provider Semaphore(8))、重试退避、proxy 复用、输出长度硬截断(D9)、streaming via Tauri event channel(D11)。

委员会成员使用白名单工具，严格禁用 MCP、hook、自定义 slash command 等额外功能。

**白名单工具（5 个，严格还原 openInvest）：**

| 工具 | ClawGO 实现 | 数据来源 | 可用角色 |
|------|-------------|----------|----------|
| `get_history_data` | Rust 函数 | Tushare MCP `daily` | Macro |
| `analyze_multi_timeframe` | Rust 函数 | Tushare MCP `daily` → Rust 计算 RSI/MA/percentile | Macro |
| `get_macro_snapshot` | Rust 函数 | Tushare A 股本地化指标(HV20/HV60、10Y 国债 ETF、DR007、北向资金、两融余额、涨跌停广度) — 详见 RFC §2 | Macro |
| `query_dreaming_insights` | FTS5 查询 | ClawGO `domain_insights` 表 | Macro |
| `get_recent_committee_verdicts` | 存储查询 | ClawGO `verdicts` 存储 | Macro |

**禁用项：**
- 所有 MCP 服务器（不注入 `UserSettings.mcp_servers`）
- 所有 hook（不合并 `~/.claude/settings.json` 中的 hooks）
- 所有自定义 slash command
- 权限提升（`--dangerously-skip-permissions` 等）
- `--append-system-prompt`（使用独立 prompt 替代）

**实现方式：**
- 委员会编排器在构建 LLM 请求时，仅注入白名单工具定义
- 不走 ClawGO 的 `build_parameterized_env` / `--settings` 路径
- 直接通过 Provider API 调用，传入 system prompt + 白名单 tools + 数据
- `search_enabled` 布尔值控制：只有 Macro 为 `true`，其余 4 个角色为 `false`

### 7.4 委员会编排算法（严格还原 openInvest）

```
run_committee(symbols):
  # 1. 共享数据加载（每 session 一次）
  wealth_view   = load_wealth_context()           # user.md + portfolio cash
  event_brief   = resolve_event_brief(symbols)    # EventStore RAG
  macro_view    = run_macro_view(macro_data, event_brief)  # Macro 一次 LLM 调用

  # 2. 逐标的并行
  for symbol in symbols:
    run_committee_for_symbol(symbol, macro_view, event_brief, wealth_view)

run_committee_for_symbol(symbol):
  market_data      = analyze_multi_timeframe(df)    # Tushare MCP → Rust 计算
  regime_brief     = format_regime_brief(metrics)   # Rust regime classification
  portfolio_summary = build_portfolio_summary(pm)
  prior_insights   = query_memory_insights(symbol)   # ClawGO FTS5 记忆查询

  # Round 1: 独立分析，并行
  macro_r1 = macro_view + event_brief            # Macro 预计算结果（共享）
  quant_r1 = ask(quant_prompt_r1, market_data + regime)
  risk_r1  = ask(risk_prompt_r1,  portfolio + wealth + prior_insights)
  risk_r1  = sentinel_override(risk_r1, true_concentration)  # 硬覆写
  wealth_r1 = ask(wealth_prompt_r1, user_md + portfolio_cash)  # WealthContext 独立输出

  # Round 2..N: 交叉挑战，并行
  for round in 2..max_rounds:
    debate_block = format_debate_history(macro_r1, quant_history, risk_history, wealth_r1)
    quant_rN = ask(quant_prompt_r2, debate_block + macro_view)  # macro_view 注入辩论
    risk_rN  = ask(risk_prompt_r2,  debate_block)
    risk_rN  = sentinel_override(risk_rN, true_concentration)
    if check_convergence(quant_history, risk_history):
      break

  # CIO 最终裁决
  cio_brief = macro_view + debate_history + portfolio + wealth + prior_insights
  cio_result = ask(cio_prompt, cio_brief)
  cio_result = sanity_check(cio_result)  # 3 道防护门
```

### 7.5 关键机制

**收敛检测：**
- 至少 2 轮，提取每轮 SIGNAL + STRENGTH
- 收敛 = Quant 最近 2 轮稳定 AND Risk 最近 2 轮稳定
- 稳定 = 相同 SIGNAL 且 |STRENGTH 差| < 1.0

**SENTINEL 覆写：**
- 每次 Risk Officer 回复后，从 portfolio_summary 提取真实集中度
- 若 LLM 输出的 CONCENTRATION_PCT 与真实值差 > 0.3%，强制覆写

**CIO Sanity Check（3 道防护门）：**
1. Gate 1（过度自信）：verdict=BUY 且 confidence ≥ 0.95 → 降级为 ACCUMULATE, confidence=0.6
2. Gate 2（分配上限）：alloc_cny 限 ±100,000 CNY
3. Gate 3（worker 不可用）：若 brief 含 `[WORKER_UNAVAILABLE]` → HOLD, confidence ≤ 0.4

**委员会逻辑审查（2026-05-29）：**

| # | 问题 | 状态 | 说明 |
|---|------|------|------|
| 1 | Round 2 辩论缺少 macro_view 注入 | ✅ 已修复 | Quant R2 现在接收 `debate_block + macro_view`，避免宏观上下文丢失 |
| 2 | WealthContextOfficer 未参与辩论 | ✅ 已修复 | 新增 `wealth_r1` 输出，加入 `debate_block` 传递给后续轮次 |
| 3 | 收敛检测覆盖范围 | ✅ 正确 | 仅检测 Quant + Risk 的 SIGNAL/STRENGTH 稳定性（Macro 和 WealthContext 不参与收敛判定） |
| 4 | SENTINEL 覆写阈值 | ✅ 正确 | 0.3% 差异阈值足够严格，防止 LLM 集中度幻觉 |
| 5 | CIO Gate 1 overdrive | ✅ 正确 | BUY>=0.95 降级为 ACCUMULATE+0.6，与 openInvest `parse_cio_memo()` 一致 |
| 6 | 工具白名单角色分配 | ✅ 正确 | 5 个工具仅 Macro 可用，`search_enabled` 仅 Macro=true |
| 7 | 信息隔离完整性 | ✅ 正确 | 5 角色各自只看规定数据，CIO 看所有但不调用工具 |
| 8 | Dreaming scope 隔离 | ✅ 已修复 | 用户记忆 Dream（global/project）与投资 Dream（invest→domain_insights）完全隔离 |

### 7.6 LLM 配置 `[已确认]`

委员会使用 ClawGO 已配置的 Provider，通过下拉框选择：

```
┌─────────────────────────────────────────────────────┐
│  委员会 LLM 配置                                     │
│                                                     │
│  Provider: [DeepSeek v3          ▼]                 │
│  Model:    [deepseek-chat        ▼]                 │
│                                                     │
│  ※ 所有角色共用同一 Provider/Model                   │
│  ※ 可在角色配置中为单个角色覆盖                      │
└─────────────────────────────────────────────────────┘
```

- 全局默认：一个 Provider + Model 应用于所有 5 个角色
- 角色级覆盖：每个角色可单独选择不同的 Provider/Model
- 下拉选项来自 ClawGO 已配置的 Provider 列表（`PHASE7_PROVIDERS`）
- 不支持自由输入 API endpoint

### 7.7 数据准备层

> **RFC 同步**: `get_macro_snapshot` 输出 schema 详见 RFC §2.3 — A 股本地化替代 Yahoo(VIX→HV20/HV60、TNX→国债 ETF、Fed→DR007、S&P500→沪深300、Gold→黄金 ETF)，新增北向资金/两融/涨跌停广度。Crash regime 定义改为「5 日跌幅>8% 且跌停占比>5%」。

**Rust 后端预处理：**
- `portfolio_summary`：持仓市值、集中度、浮盈/浮亏、cash 比例
- `regime_brief`：REGIME 分类（uptrend/downtrend/range_bound/crash/recovery）
- `market_data`：多时间框架技术指标（RSI、MA、percentiles）
- `wealth_context_view`：从 user.md 解读 off-portfolio 财务信息

**MCP 实时获取：**
- 行情数据通过 Tushare MCP tool 实时拉取
- 未来考虑通过 Python miniqmt 接口获取

### 7.8 System Prompt 严格还原

5 个角色的 system prompt 必须严格还原 openInvest 原始版本，包括：
- 完整的角色描述、专长、职责
- 硬规则表（REGIME 约束、信息隔离规则）
- 输出格式要求（SIGNAL/STRENGTH/VERDICT 等结构化输出）
- 所有 `{{asset_name}}`、`{{asset_symbol}}` 等模板变量
- 禁止事项（如禁止提供个人财务建议、禁止说"作为 AI"等）

prompt 文件以原始 markdown 存储，用户可在「角色配置」Tab 中查看和编辑。

---

## 八、Dreaming 长期记忆整合 `[已确认]`

### 8.1 对比分析

| 维度 | openInvest Dreaming | PilotDeck Dreaming | ClawGO 记忆系统 |
|------|---------------------|---------------------|-----------------|
| 存储 | `insights/*.md` + `insights.db` | SQLite + 快照目录 | SQLite FTS5（v3.0.0） |
| 管道 | 3 阶段统计：Light→REM→Deep | DreamRewriteRunner（LLM） | HeartbeatIndexer + DreamRewriteRunner |
| 成本 | 近零（纯统计） | 中等（LLM 调用） | 中等 |
| 回滚 | ❌ 无 | ✅ 快照+回滚 | ❌ 无（需新增） |
| 审计 | DREAMS.md 日志 | DreamTrace 完整记录 | ❌ 无（需新增） |

### 8.2 整合方案

**两条 Dreaming 路径，完全隔离：**

**路径 A：用户记忆 Dream（global/project scope）**
- 位置：`/memory-mgmt` 用户记忆 Tab 底部控制区
- 机制：现有 ClawGO 逻辑 — 合并近似（Jaccard >= 0.85）+ 衰减归档（×0.98，< 30.0 归档）
- scope：只操作 `scope='global'` 和 `scope='project'` 的记忆
- 投资记忆（`scope='invest'`）不受影响

**路径 B：投资 Dream（invest → domain_insights）**
- 位置：`/invest` 定时任务 Tab（cron job）
- 机制：openInvest 3 阶段统计管道移植为 Rust 模块
  - Light Sleep：从委员会裁决记录中提取 (asset, verdict, regime) 元组
  - REM Sleep：按 (asset, verdict, regime) 聚合计算命中率
  - Deep Sleep：阈值过滤（score ≥ 0.8, count ≥ 3），写入 `domain_insights`
- 输出：独立 `domain_insights` 表（不是 memories 表）
- 查询：Risk Officer 的 `query_dreaming_insights` 通过 FTS5 查 `domain_insights`

**存储：**
- `domain_insights` 表独立于 `memories`，schema 与 openInvest 一致
- 字段：slug, asset, verdict, regime, hit_rate, sample_count, source_score, body, created_at
- FTS5 索引支持按 asset/regime/verdict 搜索

**触发：**
- 路径 A：`/memory-mgmt` 手动触发或自动间隔
- 路径 B：定时任务 Tab cron 调度（默认 03:00）+ 手动触发

### 8.3 Dreaming 开关与回滚 `[已确认]`

**Dreaming 开关（参考 PilotDeck）：**
- `/memory-mgmt` 用户记忆 Tab 底部新增 Dreaming 控制区
- 开关：启用/禁用自动 dreaming（默认关闭）
- 自动间隔：数字输入 + 分钟/小时选择器
- 手动触发按钮：立即运行一次完整管道

**回滚机制（参考 PilotDeck 快照架构）：**

```
Dream 执行流程：
1. 快照当前 domain_insights 表 → last_dream/before.json
2. 执行 3 阶段管道，写入新 insights
3. 快照执行后状态 → last_dream/after.json
4. 保存 metadata（trigger/时间/摘要）

回滚流程：
1. 验证：当前状态 == after.json → rollbackReady=true
2. 恢复 before.json 中的 insights 数据
3. 清除 last_dream/ 快照
4. 记录回滚 trace
```

- 安全机制：dream 后手动编辑了记忆 → `rollbackReady=false`，回滚按钮禁用
- 回滚确认对话框：「回滚将恢复到上次 Dream 前的状态，当前 Dream 结果将被覆盖。继续？」

**Dream Trace 审计：**
- 每次 dream 运行产生 trace 记录
- 字段：trigger(manual/scheduled/rollback)/时间/状态/变更摘要/步骤详情
- 在用户记忆 Tab 中展示 dream trace 列表，点击查看详细 timeline

---

## 九、隐藏功能清单（已实现但无 UI 入口）

### 9.1 需要新增 UI 入口的功能

| 功能 | 当前入口 | 建议 |
|------|----------|------|
| Doctor 诊断面板 | 仅 CommandPalette | 标题栏 `[···]` 下拉菜单 `[已确认]` |
| 用户记忆面板 | 仅 CommandPalette + 标题栏灯泡 | 保留现有入口，在 `/memory-mgmt` 也加入口 |
| `/settings/characters` | 仅 Settings Tab 内链接 | 保留，Settings Tab 改为更醒目的卡片 |
| `/release-notes` | 仅聊天页版本号链接 | 在设置页 About 区域添加链接 |

### 9.2 已实现 API 但无 UI 调用

| API | 建议 |
|-----|------|
| `updatePromptFavoriteTags()` | 在提示词收藏列表中添加标签编辑 |
| `updatePromptFavoriteNote()` | 在提示词收藏列表中添加备注编辑 |
| `listPromptTags()` | 在提示词收藏筛选中使用 |
| `searchCharacterMemories()` | 在角色记忆 Tab 中接入 |
| `get_character_memory` (Tauri) | 在角色记忆 Tab 中接入 |

### 9.3 孤立组件（可清理或复用）

| 组件 | 状态 | 建议 |
|------|------|------|
| `BackgroundTaskPanel.svelte` | 未被导入 | 功能已被 ToolActivity 内联，可删除 |
| `ChatToolbar.svelte` | 未被导入 | 评估是否合并到聊天页 |
| `ProjectSelector.svelte` | 未被导入 | 评估是否在侧边栏使用 |
| `PlatformSelector.svelte` | 未被导入 | 评估是否在设置页使用 |
| `GroupChatStepper.svelte` | 未被导入 | 功能已被 GroupChatLayout 覆盖，可删除 |
| `ToolSelector.svelte` | 未被导入 | 评估是否在设置页使用 |

### 9.4 未使用 Store 方法

| 方法 | 建议 |
|------|------|
| `createRoundtableWithParticipants()` | 保留，未来可作为群聊快捷操作 |
| `checkOnboardingNeeded()` | 评估是否在首次使用时触发 |
| `createPlannerCharacter()` | 评估是否在 onboarding 中使用 |

---

## 十、暂不移植（已重新评估）

| 功能 | 原计划 | 新评估 |
|------|--------|--------|
| Event Watch | 暂不移植 | **纳入计划** — Phase 3 |
| PipelineFlow | 暂不移植 | **纳入计划** — Phase 3 |
| 历史命中率 | openInvest verdict_review | **纳入计划** — Phase 4（事后回顾，非回测） |
| Dreaming 三阶段记忆系统 | 暂不移植 | **纳入计划** — 作为 FTS5 领域插件 |
| CommSec 邮件解析 | 暂不移植 | 仍暂不移植 — 澳洲券商，不适用 |

---

## 十一、v2 增量决议（2026-05-29 多路审查后锁定）

> 来源：`[wip] 2026-05-29-committee-engineering-rfc.md` + 后续 UI 讨论。所有 D 编号决议均已确认,未做的需另起讨论。

### 11.1 D1–D11 工程决议汇总 `[已确认]`

| ID | 主题 | 决议 |
|----|------|------|
| **D1** | LLM 协议层 | 只支持 **OpenAI 兼容协议**,只保留 **DeepSeek** + **MiMo Plan** + **MiMo API**。MiMo Plan base_url = `https://token-plan-cn.xiaomimimo.com/v1`,MiMo API base_url = `https://api.xiaomimimo.com/v1`。新增 `LlmClient` trait 抽象,委员会编排器只依赖 trait,不绑死任何 Provider。 |
| **D2** | A 股宏观替代 | **不再依赖 yfinance / VIX / TNX**。改用 Tushare 计算 A 股本地化指标:HV20/HV60 比率(恐慌代理)、10 年国债 ETF (511260) 收益率代理、DR007、北向资金、两融余额、涨跌停家数广度。Crash regime 定义改为「连续 5 日累计跌幅 >8% 且跌停占比 >5%」。 |
| **D3** | SQLite 事务 | 使用 SQLite **WAL + `BEGIN IMMEDIATE`** 包裹 `record_external_trade`(holdings + cash + history.jsonl)。跨文件原子性靠「DB 先提交,history.jsonl 后追加,失败用 reconciliation 补偿」。 |
| **D4** | 双数据库隔离 | 委员会数据 `~/.claw-go/invest/invest.db`,与 ClawGO 主 `memory.db` **物理隔离**。`domain_insights` 表只走 invest.db。 |
| **D5** | 任务丢失策略 | **接受丢失**:Tauri 后台任务在 app 关闭期间不补跑。下次启动只跑当下符合 cron 的一次。不做持久化队列。 |
| **D6** | 代理复用 | 直接复用 ClawGO 在 Settings 配的 HTTP 代理(`UserSettings.proxy`),Rust HTTP client 启动时读取,无需独立配置。 |
| **D7** | 辩论轮数 | UI 用下拉框 6 档:1/2/3/4/6/8 轮,默认 4 轮。1 轮 = 不辩论(旧行为),8 轮 = 实验档。 |
| **D8** | LLM 并发 | tokio `Semaphore`,**per-provider 各 8**(DeepSeek / MiMo Plan / MiMo API 各自独立 8 个许可)。5 资产 × 3 角色 = 15 并发请求会被压到 8,剩余排队,避免 429。 |
| **D9** | 输出长度约束 | 每个角色 system prompt 注入分级硬约束:**辩论轮(Quant/Risk)≤ 200 汉字**、**Macro 首轮 ≤ 400 汉字**、**CIO 终局 ≤ 300 汉字**。后端解析时按角色 + 轮次硬截断兜底。 |
| **D10** | Dreaming 双路径快照 | 路径 A(user memory: global/project)+ 路径 B(domain_insights: invest)都做快照,允许独立回滚。每次 Dreaming 前先 `INSERT INTO snapshots ... SELECT * FROM memories WHERE scope IN (...)`。 |
| **D11** | 委员会输出流式 | 直接做 SSE streaming(via Tauri event channel),不走「先收集再展示」的伪流式。前端 `<DebateBlock>` 边收边渲染。 |

### 11.2 委员会并发模型 `[已确认]`

- **资产级别并发**:`pipeline.run_all_assets()` 用 `tokio::spawn` 同时跑 5 个 asset
- **资产内 LLM 串行**:每个 asset 内部 Macro→Quant R1→Risk R1→Wealth R1→Quant R2→Risk R2→CIO **顺序执行**,不并发
- **理由**:Round 2 需要 Round 1 输出作为 cross-challenge 输入,无法并发;同 asset 内并发也无收益(全是同一 Provider)。
- **峰值并发**:5 资产 × 1 LLM 调用 = 5 个并发请求(同一 Provider),被 per-provider `Semaphore(8)` 容纳,留 3 余量(D8)

### 11.3 HOLD vs WATCH 双类资产分类 `[已确认]`

- **HOLD**:用户真实买入,占用现金,真实 PnL
- **WATCH**:AI 推荐观望,**不占用现金**,虚拟 PnL,但**进入命中率统计**(避免幸存者偏差)
- **存储**:`holdings` 表主键改为 `(symbol, currency, kind)`,`kind IN ('hold','watch')`
- **转换**:WATCH → HOLD 通过专用对话框,以「真实买入价/数量」覆写,补一条 `history.jsonl` 记录
- **HOLD → WATCH**:允许「平仓但保留观察」,清零数量并打 `kind='watch'`
- **命中率统计**:`verdict_review` 同时回顾 HOLD 和 WATCH 两类资产的当时裁决

### 11.4 Dashboard KPI + 现金管理 `[已确认]`

- **KPI 5 卡布局**:总资产 / 持仓市值(HOLD)/ **可用现金(CNY,带编辑按钮 ✎)** / 总收益率 / 持仓数量(HOLD n + WATCH m)
- **现金编辑入口**:在 KPI 卡右上角 ✎ 按钮,点开弹出对话框「修改可用现金」,字段:当前余额(只读)、新余额、原因(可选)。提交后写入 `cash[CNY]` + 追加 `history.jsonl` `{type: 'cash_adjust', delta, reason}`
- **理由**:用户可能从外部券商划入/划出资金,委员会要看得到真实可用现金

### 11.5 数据刷新策略 `[已确认]`

- **交易时段(9:30–11:30 / 13:00–15:00)**:Dashboard 每 **60 秒**轮询(Tushare 收费 500/min 充裕)
- **盘前/盘后**:**只做定时拉取**,每天 3 次(9:00 / 12:30 / 15:30),不轮询
- **委员会运行时**:立即触发一次行情拉取,确保用最新数据
- **未来转 miniqmt**:改为推送订阅,无限流风险
- **快照保留**:`pnl_snapshot` 工作日 4 次贴盘前/盘中/盘后(`30 9,11 * * 1-5` + `0 13,15 * * 1-5`)— 已对齐 RFC §4.6

### 11.6 决策回放 UI 简化 `[已确认]`

- **取消时间轴 + 倍速控制**:回放是事后查看,不需要 1x/2x/4x/8x 拖拽
- **保留**:PipelineFlow 7 节点静态图(全部 done 态)+ 下方角色输出卡片
- **节点顺序**:Macro → Quant R1 → Risk R1 → Wealth R1 → Quant R2 → Risk R2 → CIO(水平 7 节点串行)
- **直播模式**(改动 9b)与回放模式共用 PipelineFlow 组件,差异只在节点状态(active/pending vs done)

### 11.7 Provider 配置 UI `[已确认]`

- **手动输入 3 行矩阵**:DeepSeek / MiMo Plan / MiMo API 各一行,字段:启用 ☑ + base_url + api_key + model 输入框
- **「应用并保存」按钮**:统一保存到 `~/.claw-go/invest/llm_config.json`,触发 Rust 端 reload
- **不用下拉**:因为同一 Provider 用户可能用不同 model(默认 vs 备选),手动输入更灵活

### 11.8 系统二级页结构(参照 invest-gui System.tsx) `[已确认]`

`/invest` 顶部 Tab 新增「系统」,内嵌 **7 个二级 Tab**(去掉 invest-gui 原有的「LLM 成本」):

| 二级 Tab | 来源(invest-gui) | 功能 |
|---------|------------------|------|
| Cron Jobs | `JobsTab` | 复用改动 5b 内容,合并到此处 |
| 市场 Regime | `RegimeTab` | 资产输入框 + regime 分类(uptrend/downtrend/range_bound/crash/recovery)+ brief + 原始 metrics |
| 事件 (Events) | `system/EventsTab.tsx` | 改动 4 内容**迁移到此**:时窗筛选(24h/48h/7d)+ 严重度筛选 + 立即扫描按钮 + counts chips + 事件列表 |
| 数据源 | `system/DataSourcesTab.tsx` | 10 数据源健康表(Tushare API、SQLite、PnL JSONL 等),状态 ✓/⚠、最后成功时间、采样值,60s 刷新 |
| PnL 历史 | `PnLTab` | 原始 2h 快照表(ts、total_pnl_pct、csi300_pnl_pct),倒序展示最近 80 条 |
| 长期模式 | `InsightsTab` | Dreaming 沉淀的 insights(slug + confidence + count + 完整 body 折叠) |
| Dreams | `DreamsTab` | 最近 events 表(时间/阶段/资产/verdict/conf)+ short-term + candidates(JSON 折叠) |

**去掉「LLM 成本」**:用户决定不需要此 Tab。

### 11.9 决议归档 UI(参照 committee/HistoryTab.tsx) `[已确认]`

- **左右双栏布局**:
  - **左侧**:`.committee/<date>/<symbol>.md` 列表,显示日期 + 股票 + verdict badge + 置信度 + 配比
  - **右侧详情**:顶部 verdict landmark 卡片 + `PERSONAL_NOTE` 卡片 + `EXECUTION_PLAN` / `RISK_PLAN` 折叠 + 4 角色 brief + 完整辩论 transcript 折叠 + 原始 markdown
- **筛选**:按日期范围、verdict 类型(BUY/ACCUMULATE/HOLD/TRIM/SELL)、置信度区间
- **导出**:支持单条 / 批量导出 markdown

### 11.10 设置 — 用户档案 `[已确认]`

参照 invest-gui `Settings.tsx` 的 `wealth_context` 表单,新增「用户档案」二级页(`/settings/profile`):

- **可编辑字段**:
  - `emergency_buffer_cny`(应急储备金,元)
  - `family_backup_available`(家庭后备资源 toggle)
  - `account_purpose`(账户用途 radio:零花钱 / 长期投资 / 退休金 / 教育金 / 其他)
  - `lifestyle_notes`(生活方式备注,textarea)
- **只读概要**:`display_name`、`risk_tolerance`、`exchange_buffer_cny`(从 ClawGO `UserSettings` 同步)
- **作用**:`wealth_context` 在每次委员会运行时注入 Wealth Context Officer 的 system prompt,决定 SOLVENCY_BUFFER_LEVEL

### 11.11 Phase 3 拆分 `[已确认]`

原 Phase 3 拆为 3a / 3b / 3c(对应 RFC):

- **Phase 3a — LLM 核心 + 委员会批处理**:`LlmClient` trait、`Semaphore` Governor、5 角色 prompt、串行/并发编排、`record_external_trade` 事务、verdict_review 归档
- **Phase 3b — Streaming + UI**:SSE streaming、PipelineFlow 实时动画、`<DebateBlock>` 流式渲染、Live Tab、决策回放、决议归档(HistoryTab)
- **Phase 3c — Event Watch**:Tushare + RSS 多源新闻、LLM 归一化、事件流 UI、触发对话框、迁移到「系统/事件」二级 Tab

### 11.12 交易日历守卫 + 启动迁移 toast `[已确认]`

- **交易日历守卫**:所有 cron 任务调用 `is_trading_day(date)` (查询 Tushare `trade_cal`),非交易日跳过,UI 显示「已跳过 N 个非交易日任务」灰色 toast
- **启动迁移 toast**:首次启动 invest.db 时,检测旧版 JSON 数据 → 一键迁移按钮 → 显示 `已迁移 X 条持仓 / Y 条交易` toast

---

## 十二、实施 Phases

### Phase 1：数据层 + 侧边栏入口 + 记忆管理重构 ✅ (v3.1.0, 2026-05-29)

> 实施计划：`[done] 2026-05-29-openinvest-phase1-impl.md`，18 commits (c5bbf71..27a2914)，含 9-angle code review + 8 findings fix。

- [x] 新增 `/invest` 路由 + 侧边栏入口（6 Tab skeleton）
- [x] 新增 `/memory-mgmt` 独立路由 + 侧边栏入口（2 Tab: 用户记忆 + 提取配置）
- [ ] 移除角色记忆：`/memory-mgmt` 只保留用户记忆，AiCharacter 库移除记忆入口 — **延后到 Phase 3+**
- [x] `memories` 表新增 `scope` + `project_id` 字段，去掉 approve 流程（pending→approved 简化为 active）
- [x] `storage/portfolio.rs`、`storage/trades.rs`、`storage/strategy.rs` — **strategy.rs Phase 2 完成**
- [x] 对应 Tauri commands（含 `list_memories(scope_filter)` 等，20+ commands）
- [x] `/memory-mgmt` 页面（用户记忆 + Scope 筛选 + 提取配置）— **Dreaming 控制区延后到 Phase 4**
- [ ] Memory Extraction 添加「应用并重载」按钮 — **延后到 Phase 3+**
- [x] 标题栏 `[···]` 下拉菜单（含 Doctor 入口 + Release Notes + All Settings）
- [ ] 设置页移除旧 `memory` Tab — **延后到 Phase 3+**
- [x] i18n 更新（14 keys: en + zh-CN）

### Phase 2：Dashboard + 持仓 + 交易 + PnL 快照 ✅ (v3.2.0, 2026-05-29)

> 实施计划：`[done] 2026-05-29-phase2-dashboard-portfolio-pnl.md`，22 commits，含 13 项审查修复。
> 设计文档：`[done] 2026-05-29-phase2-dashboard-portfolio-pnl-design.md`

- [x] `/invest` Dashboard 页面(KPI 5 卡:总资产/持仓市值/**可用现金(带 ✎ 编辑入口)**/总收益率/持仓数量、持仓+观望表格、最新裁决、PnL 趋势图)
- [x] **HOLD / WATCH 双类资产**(主键 `(symbol, currency, kind)`,`kind IN ('hold','watch')`)
  - WATCH 不占现金,只追踪虚拟 PnL,但进入命中率统计
  - WATCH ↔ HOLD 转换对话框(写 history.jsonl)
- [x] **现金编辑对话框**:写入 `cash[CNY]` + 追加 `history.jsonl` `{type: 'cash_adjust', delta, reason}`
- [x] 持仓管理:调整持仓(买入/卖出/修改),加权平均成本计算
- [x] 买进/卖出流水(`record_external_trade` 原子事务,WAL + `BEGIN IMMEDIATE`,`history.jsonl` 审计)
- [x] 策略配置 CRUD
- [x] PnL 快照定时任务(工作日 4 次,贴盘前/盘中/盘后,Tauri background task)
  - `30 9,11 * * 1-5` + `0 13,15 * * 1-5`(北京时间 9:30 / 11:00 / 13:00 / 15:00)— 对齐 RFC §4.6
  - `is_trading_day` 守卫,非交易日跳过
  - 存储:invest.db `pnl_snapshots` 表(upsert 去重)
  - Dashboard 渲染 PnL 趋势折线图(Chart.js)
- [x] Tushare HTTP Client(Rust,reqwest,自定义代理 `http://101.35.233.113:8020/`)
- [ ] **A 股本地化宏观数据层**(替换 yfinance/VIX/TNX):HV20/HV60、10Y 国债 ETF、DR007、北向、两融、涨跌停广度 — **延后到 Phase 3a**
- [ ] **数据刷新策略**:交易时段 60s 轮询 + 盘前/盘后定时 3x/天 + 委员会运行时立即拉取 — **延后到 Phase 3a**（前端定时轮询组件待实现）
- [x] **交易日历守卫**:`is_trading_day(date)` (Tushare `trade_cal`),非交易日跳过 cron
- [x] **启动迁移 toast**:首次启动 invest.db 时检测旧 JSON 数据 → 一键迁移

### Phase 3a：LLM 核心 + 委员会批处理 ✅ (2026-05-29)

> 实施计划：`[done] 2026-05-29-openinvest-phase3a-llm-committee.md`，含 14 项审查修复。
> 配套 RFC：`[done] 2026-05-29-committee-engineering-rfc.md`

- [x] **`LlmClient` trait 抽象**(只支持 OpenAI 兼容协议),实现 DeepSeek + MiMo Plan + MiMo API
- [x] **`LlmGovernor`(per-provider 各 `Semaphore(8)`,DeepSeek / MiMo Plan / MiMo API 独立计数)** + Provider 配置手动输入矩阵(3 行 + 应用并保存)
- [x] 5 角色 prompt 存储 + 编辑 UI(`~/.claw-go/invest/prompts/{role}.md`)
- [x] 5 角色 system prompt 注入分级长度硬约束(辩论 ≤ 200 / Macro ≤ 400 / CIO ≤ 300 汉字)+ 后端按角色 + 轮次硬截断兜底
- [x] 委员会编排器(Rust,严格还原 openInvest 算法)
- [x] **资产级别并发(5 资产 tokio::spawn)+ 资产内 LLM 串行**(Macro→Q1→R1→W1→Q2→R2→CIO)
- [x] 辩论轮数下拉(1/2/3/4/6/8,默认 4)
- [x] SENTINEL 覆写 + CIO Sanity Check 3 Gates
- [x] 收敛检测
- [x] 决策归档(`.committee/<date>/<symbol>.md` + `events.jsonl`)

**审查修复(14 项,2026-05-29):**
1. `CommitteeResult`/`RoundOutputSummary` 缺 `serde(rename_all = "camelCase")` → 前端收不到字段
2. `SentinelOverride`/`SanityCheckResult` 缺 `serde(rename_all = "camelCase")`
3. `check_convergence`: `None == None` 被视为一致 → 误收敛
4. `check_convergence`: strength 缺失时用 0.0 → 误收敛
5. `check_sentinel`: R1 数据缺失时默认 0.0 → 误触发
6. `check_sentinel`: 只比首尾 → 遗漏中间最大偏移
7. Gate 3 dry_powder: 缺失数据强制 HOLD → 应跳过
8. `ToolCallDelta` ID 为空时丢弃 chunk → 流式 tool call 参数丢失
9. `run_committee_batch` 第一个失败就短路 → 部分成功被丢弃
10. `debate_rounds` 未从 config 传入 → UI 下拉不生效
11. `daily()` 返回升序未 reverse → 价格/百分位计算反向
12. `archive_decision_full` 未接入编排器 → 无 markdown+events.jsonl 归档
13. `expandedRound` 用 `result.rounds.length` → 多资产展开状态互相覆盖
14. 测试数据用简写字符串 → 与 prompt 模板不一致

### Phase 3b：Streaming + UI ✅ (2026-05-29)

- [x] **SSE streaming**(via Tauri event channel),`<DebateBlock>` 流式渲染
- [x] PipelineFlow 动画(Svelte transitions 重写,7 角色颜色,7 节点串行)
- [x] 委员会**直播 Tab**(多资产并发概览 + 单资产 Tab 切换 + 资产内详情卡)
- [x] 委员会**决策回放 Tab**(无时间轴,只 PipelineFlow + 角色输出卡)
- [x] **决议归档 Tab**:左列表 + 右详情(markdown 内容)
- [x] 角色配置 Tab(独立 prompt 编辑)
- [x] 命中率 Tab + Tool 调用 Tab(Phase 4 占位)

**Phase 3b 代码审查修复**(2026-05-29):

1. **serde stepIndex 命名**(`events.rs`):`RoleStart`/`RoleComplete` 添加 `#[serde(rename_all = "camelCase")]`,修复 `step_index` → `stepIndex` 序列化不匹配
2. **Error 事件发射**(`orchestrator.rs`):`run_committee_batch_stream` 在符号失败和 task join 失败时发射 `CommitteeEvent::Error`,并用 `(symbol, handle)` 元组追踪符号名
3. **死三元表达式**(`CommitteeLiveTab.svelte`):`'done' : 'done'` → `'active' : 'done'`,流式期间最后完成的 round 现在正确显示 active spinner
4. **_unlisten 竞态**(`invest-committee-store.svelte.ts`):`runCommittee()` 入口先捕获并调用旧 listener,防止并发调用覆盖
5. **error handler 缺少 activeStep**(`invest-committee-store.svelte.ts`):错误处理添加 `activeStep: -1`,PipelineFlow 可正确显示 error 状态

### Phase 3c：Event Watch `[完成]`

- [x] Event Watch 新闻扫描(Tushare major_news + anns_d + LLM 归一化)
- [x] 事件触发确认对话框(EventTriggerDialog — 委员会启动 + 事件标记)
- [x] 事件监控 Tab(EventWatchTab — 时窗/严重度/搜索筛选 + 立即扫描)
- [x] 时窗筛选(24h/48h/7d)+ 严重度筛选 + 立即扫描按钮 + counts chips
- [x] 后台 cron 工作日 8-22 每 30 分钟 / 周末 9:00+18:00
- [x] 8 项审查修复:runCommittee/triggerCommittee 静默捕获、listen() try/finally、LOW 严重度穿透、convertWatchToHold 回滚完整性、|| vs ??、get_event_stats 错误吞没

实施计划:`[done] 2026-05-29-event-watch-impl.md`

### Phase 4a：Scheduler + Verdict Review + Dreaming + FTS5 + Archived (完成)

实施计划:`[done] 2026-05-30-phase4a-scheduler-review-dreaming.md`

- [x] Scheduler Framework — 后端类型 + Config 持久化
- [x] Scheduler Runner + Tauri Commands + SchedulerTab UI
- [x] 历史命中率 Tab(AccuracyTab:verdict_review 事后回顾,1d/7d/30d 窗口)
  - **HOLD + WATCH 都进入统计**(避免幸存者偏差)
- [x] Dreaming 统计管道(Rust 移植,`domain_insights` 表 → invest.db,3 阶段:Light→REM→Deep)
- [x] Dreaming 开关 + cron 配置 + DreamConfig 双文件持久化
- [x] Dreaming 快照 + 独立回滚(状态一致性校验)
- [x] Dream Trace 审计记录(dream_snapshots 表)
- [x] FTS5 全文检索(domain_insights 升级为 FTS5 虚拟表,unicode61 tokenizer,BM25 排序)
- [x] **已归档记忆视图**(`/memory-mgmt` 增加「已归档」Tab,支持恢复/删除)
- [x] i18n + InsightsFeed 搜索 + Pipeline 通知
- [x] 14 项代码审查修复(3 CRITICAL + 2 HIGH + 4 MEDIUM + 5 LOW)

### Phase 4b：系统页 + 用户档案 + 每日报告 (已完成, 2026-05-30)

- [x] **系统二级页**(`/invest` 顶部新增「系统」Tab,内嵌 7 个二级 Tab,无 LLM 成本):
  - Cron Jobs(复用改动 5b) — `<SchedulerTab />`
  - 市场 Regime — `<SystemRegimeTab />`(新组件,`get_regime_classification` 命令)
  - 事件(从改动 4 迁移) — `<EventWatchTab />`
  - 数据源 — `<SystemDatasourceTab />`(新组件,`get_datasource_health` 命令)
  - PnL 历史 — `<SystemPnlHistoryTab />`(新组件)
  - 长期模式(Insights) — `<InsightsFeed />`
  - Dreams(短期 + candidates) — `<SystemDreamsTab />`(新组件)
- [x] **用户档案页**(`/settings/profile`,参照 invest-gui `Settings.tsx`):
  - 可编辑:`emergency_buffer_cny`、`family_backup_available`、`account_purpose`(5 选 1 radio)、`lifestyle_notes`
  - 只读:`display_name`、`risk_tolerance`、`exchange_buffer_cny`(从 ClawGO `UserSettings` 同步)
  - 注入 Wealth Context Officer system prompt
  - `user_profile.rs` 单行表模式 + `Option<UserProfile>` 返回(避免默认值覆盖)
  - NaN 前端校验 + `$derived` i18n 响应式
- [x] 每日报告定时任务
  - `daily_report.rs` — 非 async,`ON CONFLICT DO UPDATE` 正确 upsert
  - Scheduler 注册: `0 22 * * 1-5`, `requires_trading_day: true`

**Phase 4b 代码审查修复(9 项):**
1. 实现缺失的 `get_regime_classification` 命令(Tushare 日线 + MA20/MA60/volatility)
2. 实现缺失的 `get_datasource_health` 命令(连通性检查 + DB 健康)
3. `get_profile()` 返回 `Option<UserProfile>` 避免默认值覆盖 llm_config
4. `INSERT OR REPLACE` → `ON CONFLICT DO UPDATE` 保留 AUTOINCREMENT id
5. `emergencyBufferCny` NaN 前端校验
6. `generate_daily_report` 移除多余 `async`(纯阻塞 I/O)
7. 新增 `invest_loading` i18n key
8. `SystemDreamsTab` 展开时隐藏 truncated shortTerm
9. `accountPurposeOptions` → `$derived` i18n 响应式

---

## 技术栈确认

零外部依赖新增:
- 数据源:Tushare MCP(已有) + **A 股本地化宏观替代**(HV20/HV60、10Y 国债 ETF、DR007、北向、两融、涨跌停广度) + 未来 miniqmt 推送
- 存储:ClawGO storage 模式(JSON/JSONL) + **SQLite WAL + `BEGIN IMMEDIATE`**(invest.db 与 memory.db 物理隔离)
- 编排:独立委员会编排器(Rust),不复用 Group Chat orchestrator;**资产级并发(`tokio::spawn`)+ 资产内 LLM 串行**
- LLM:**`LlmClient` trait 抽象(只支持 OpenAI 兼容协议),只保留 DeepSeek + MiMo Plan + MiMo API**,UI 手动输入 3 行矩阵;per-provider 各 `Semaphore(8)` 并发限制
- 前端:Svelte 5 + 现有组件库 + Svelte transitions(替代 Framer Motion);**SSE streaming via Tauri event channel**
- 调度:Tauri background task + **交易日历守卫**(`is_trading_day` via Tushare `trade_cal`)
- RSS:feedparser(Rust crate 或 JS lib)
- 代理:**复用 ClawGO `UserSettings.proxy`**,无需独立配置

## 依赖项

- [x] ClawGO 记忆系统更新完毕（v3.0.0 SQLite FTS5 已完成）
- [x] 确认 Tushare MCP 覆盖范围满足需求（ETF 数据、板块数据等）— Phase 2 Tushare HTTP Client 已验证 A 股日线、股票搜索、交易日历
- [x] 用户确认 UI 方案（侧边栏布局、记忆管理独立路由、openInvest 页面结构）
- [x] 用户确认委员会架构（独立 prompt 注入、不进 AiCharacter 库、与 Group Chat 隔离）
- [x] 用户确认 LLM 配置（Provider 下拉选择、角色级覆盖）
- [x] 用户确认 Dreaming 整合方案（统计管道 + FTS5 存储）
- [x] 用户确认记忆三 Scope 模型（global/project/invest 隔离 + 去掉 approve）
- [x] 用户确认数据准备（Rust 预处理 + MCP 实时 + 未来 miniqmt）

## 关键源文件参考

### openInvest
- `core/committee.py` — 委员会编排引擎（850 行）：多轮辩论、收敛检测、sanity check、SENTINEL
- `core/committee_runner.py` — 统一会话编排器 `run_committee_session()`
- `core/portfolio_manager.py` — 持仓管理，multi-currency cash，`with_portfolio_tx()` 原子事务
- `core/schemas.py` — Pydantic 数据模型
- `core/regime.py` — 市场 regime 分类（uptrend/downtrend/range_bound/crash/recovery）
- `agents/skills/macro_strategist/SKILL.md` — Macro prompt（SIGNAL: risk_on|risk_off|neutral）
- `agents/skills/quant/SKILL.md` — Quant R1 prompt（REGIME 约束）
- `agents/skills/quant/SKILL_rebuttal.md` — Quant R2 prompt（REGIME 硬保护）
- `agents/skills/risk_officer/SKILL.md` — Risk R1 prompt（CONCENTRATION_PCT, DRY_POWDER_CNY）
- `agents/skills/risk_officer/SKILL_rebuttal.md` — Risk R2 prompt（仅 2 条合法升级规则）
- `agents/skills/wealth_context_officer/SKILL.md` — Wealth Context prompt（SOLVENCY_BUFFER_LEVEL）
- `agents/skills/cio/SKILL.md` — CIO prompt（temperature=0.1）
- `jobs/dreaming.py` — Dreaming 3 阶段统计管道
- `db/insights_db.py` — insights SQLite schema
- `jobs/event_watch.py` — Event Watch 调度
- `services/news_sources/` — 新闻抓取（3 源）
- `services/event_normalizer.py` — LLM 归一化

### invest-gui
- `src/routes/Dashboard.tsx` — 主面板
- `src/routes/Strategy.tsx` — 策略管理
- `src/routes/History.tsx` — 交易历史
- `src/routes/Settings.tsx` — wealth_context 表单
- `src/routes/committee/` — 委员会 7 tab
- `src/components/PipelineFlow.tsx` — PipelineFlow 动画（242 行）
- `src/lib/format.ts` — 数字格式化
- `src/lib/swr-keys.ts` — API 端点常量（60+）

### ClawGO（需要改动的文件）
- `src/routes/+layout.svelte` — 侧边栏 navItems（第 457-465 行）
- `src/routes/settings/+page.svelte` — 设置页 Tab 定义（第 42-94 行）+ Memory Extraction（第 3496-3557 行）
- `src/routes/settings/characters/+page.svelte` — 角色管理
- `src/lib/components/UserMemoryPanel.svelte` — 用户记忆面板
- `src/lib/components/CharacterMemoryPanel.svelte` — 角色记忆面板
- `src/lib/commands.ts` — CommandPalette 命令定义
- `messages/en.json` + `messages/zh-CN.json` — i18n
