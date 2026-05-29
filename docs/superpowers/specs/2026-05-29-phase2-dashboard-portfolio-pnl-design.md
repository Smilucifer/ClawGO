# Phase 2: Dashboard + Portfolio + Trading + PnL Design

> 状态: 已确认 (2026-05-29)
> 配套: `docs/superpowers/plans/[wip] 2026-05-28-openinvest-investgui-port.md` §Phase 2

## 背景

Phase 1 完成了 invest.db 数据层（10 表 + 19 Tauri 命令）和 /invest 路由骨架（6 Tab placeholder）。Phase 2 要把 Dashboard、持仓管理、交易记录、策略配置、PnL 快照从 placeholder 变成可用功能。

## 技术决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 图表库 | Chart.js | 用户选择。开箱即用的 tooltip/hover/动画。PnL 折线图 + 沪深300 基准线。 |
| Tushare 数据获取 | Rust 直调 HTTP API | 独立于 MCP 服务进程，稳定可控。reqwest + Tushare Pro API。 |
| 实施策略 | 单次全量 | 用户确认。所有子任务在同一版本发布。 |

## 架构

```
前端 (Svelte 5)
├── src/lib/types/invest.ts          — TS 接口 (Holding, Trade, PnlSnapshot, Verdict)
├── src/lib/stores/invest-store.svelte.ts — 主 store ($state runes, 封装 19 个 invoke)
├── src/routes/invest/+page.svelte   — Dashboard Tab 替换 placeholder
├── src/lib/components/invest/       — 新组件目录
│   ├── KpiCard.svelte               — KPI 数字卡片
│   ├── HoldingsTable.svelte         — 持仓表格 (HOLD + WATCH 分组)
│   ├── TradeDialog.svelte           — 买入/卖出/现金编辑对话框
│   ├── TradeLogTab.svelte           — 交易记录 Tab
│   ├── StrategyTab.svelte           — 策略配置 Tab
│   └── PnlChart.svelte              — Chart.js 折线图
└── messages/en.json + zh-CN.json    — i18n 新增 keys

后端 (Rust)
├── src-tauri/src/tushare/client.rs  — Tushare HTTP API client
├── src-tauri/src/tushare/mod.rs     — 模块导出
├── src-tauri/src/storage/invest/strategy.rs — 策略存储 (新增)
├── src-tauri/src/commands/invest.rs — 新增 save_strategy, get_strategy
└── src-tauri/src/commands/          — 新增 sync_trade_calendar, migrate_legacy_portfolio
```

## 1. 前端类型 (`src/lib/types/invest.ts`)

与 Rust struct camelCase 对齐：

```ts
export interface Holding {
  symbol: string;       // '600519.SH'
  currency: string;     // 'CNY'
  kind: 'hold' | 'watch';
  name: string;
  notional: number;
  avgCost: number | null;
  shares: number | null;
  entryDate: string | null;
  linkedVerdictId: string | null;
  notes: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Trade {
  id: string;
  symbol: string;
  currency: string;
  kind: string;
  action: string;       // 'buy' | 'sell' | 'convert' | 'cash_adjust' | 'cost_edit'
  shares: number | null;
  price: number | null;
  amount: number | null;
  notes: string | null;
  createdAt: string;
}

export interface PnlSnapshot {
  id: number;
  snapshotDate: string;
  totalValue: number;
  cash: number;
  holdingsValue: number;
  dailyPnl: number | null;
  dailyPnlPct: number | null;
}

export interface Verdict {
  id: string;
  symbol: string;
  verdict: string;
  confidence: number | null;
  macroSignal: string | null;
  model: string | null;
  provider: string | null;
  tokensUsed: number | null;
  latencyMs: number | null;
  notes: string | null;
  createdAt: string;
}

export interface CashBalance {
  available: number;    // CNY
}
```

## 2. Store (`src/lib/stores/invest-store.svelte.ts`)

Svelte 5 runes 模式：

```ts
// $state: holdings, trades, cash, verdicts, pnlSnapshots, loading, error
// $derived: totalAssets, holdingsMarketValue, totalReturnPct, holdCount, watchCount
// 方法: loadAll(), refreshPrices(), buyStock(), sellStock(), updateCash(), ...
```

所有 Tauri 调用通过 `import { invoke } from '$lib/transport'`。

## 3. Tushare Client (`src-tauri/src/tushare/`)

```rust
pub struct TushareClient {
    token: String,
    client: reqwest::Client,
}

impl TushareClient {
    pub fn new(token: String) -> Self;
    pub async fn daily(&self, ts_code: &str, start: &str, end: &str) -> Result<Vec<DailyBar>>;
    pub async fn daily_basic(&self, ts_code: &str, trade_date: &str) -> Result<Vec<DailyBasic>>;
    pub async fn stock_basic(&self, name: Option<&str>) -> Result<Vec<StockBasic>>;
    pub async fn get_latest_price(&self, ts_code: &str) -> Result<f64>;
    pub async fn trade_cal(&self, exchange: &str, start: &str, end: &str) -> Result<Vec<TradeCal>>;
}
```

- Token 从 `UserSettings.tushare_token` 读取（需在 settings 表新增字段，设置页新增输入框）。MCP 服务器配置是外部管理的，Rust 代码无法直接读取，因此需要独立配置。
- 请求格式: `POST https://api.tushare.pro` with `{ api_name, token, params, fields }`
- 超时: 30s，重试: 2 次（429/5xx）

## 4. Dashboard 页面

### 4.1 KPI 5 卡布局

| 卡片 | 计算 | 格式 |
|------|------|------|
| 总资产 | cash + Σ(hold.shares × latest_price) | ¥xxx,xxx.xx |
| 持仓市值 | Σ(hold kind × shares × price) | ¥xxx,xxx.xx |
| 可用现金 | `get_cash` | ¥xxx,xxx.xx + ✎ |
| 总收益率 | (total - cost_basis) / cost_basis | ±x.xx% |
| 持仓数量 | count(hold) + count(watch) | HOLD n + WATCH m |

### 4.2 持仓表格

- HOLD 分组在上，WATCH 分组在下
- 列: 股票名 / 代码 / 持仓量 / 成本价 / 现价 / 盈亏% / 最新裁决 badge
- HOLD 行操作: 卖出 / 编辑成本价
- WATCH 行操作: 转为持仓（弹出买入对话框，预填股票信息）
- 空状态: 「暂无持仓，点击买入添加」

### 4.3 对话框

**买入对话框：**
- 股票搜索: 输入代码或名称，调 Tushare `stock_basic(name=...)` 模糊匹配，下拉选择
- 数量: 100 的整数倍（A 股最小交易单位）
- 价格: 手动输入 或 「市价」按钮（调 `get_latest_price`）
- 确认: 前端 store 调用 3 个 Tauri 命令序列 —
  1. `record_trade(action='buy')` — 写审计记录
  2. `add_holding(kind='hold', shares=qty, avg_cost=price)` — upsert 持仓（加权平均：如果已存在则 `(old_avg × old_qty + price × qty) / (old_qty + qty)`）
  3. `update_cash(current_cash - qty × price)` — 扣减现金

**卖出对话框：**
- 从已有 HOLD 持仓下拉选择
- 数量 + 价格
- 确认: 前端 store 调用 3 个 Tauri 命令序列 —
  1. `record_trade(action='sell')` — 写审计记录
  2. `update_holding(shares=old_shares - qty)` — 减少持仓（clamp to 0，归零后 `delete_holding`）
  3. `update_cash(current_cash + qty × price)` — 增加现金

**WATCH → HOLD 转换：**
- 弹出买入对话框（预填股票代码和名称，kind 改为 'hold'）
- 确认后: `delete_holding(kind='watch')` + `add_holding(kind='hold', shares, avg_cost)` + `record_trade(action='convert')`

**现金编辑对话框：**
- 当前余额（只读）
- 新余额（输入）
- 原因（可选 textarea）
- 确认: 调 `update_cash` + `record_trade(action='cash_adjust')`

### 4.4 PnL 趋势图

- Chart.js line chart
- 两条线: 总资产 PnL% + 沪深300 PnL%（基准对比）
- 视图切换: 日 / 周 / 月
- 数据源: `get_pnl_snapshots(limit=80)`
- 空状态: 「暂无 PnL 数据，首个快照将在下一个交易日生成」

## 5. 交易记录 Tab

- 表格: 日期 / 股票 / 方向(买/卖/调整) / 数量 / 价格 / 金额 / 备注
- 筛选栏: 日期范围 picker + 股票代码输入 + 方向下拉
- 分页: 默认 50 条，「加载更多」按钮
- 导出 CSV 按钮
- 数据源: `get_trades(symbol?, limit?)`

## 6. 策略配置 Tab

- 目标资产列表: 股票代码 + 目标比例(%) + 添加/删除
- 约束: 最大单股集中度(%) / 最小现金比例(%)
- 保存按钮 → `save_strategy` / `get_strategy`
- 需新增: `storage/invest/strategy.rs` + `strategy` 表 + 2 个 Tauri 命令

## 7. PnL 快照定时任务

- Tauri background task，启动时注册
- Cron: `30 9,11 * * 1-5` + `0 13,15 * * 1-5`（北京时间 9:30/11:00/13:00/15:00）
- 守卫: `is_trading_day(date)` — 非交易日跳过
- 流程: 遍历 HOLD holdings → `get_latest_price` → 计算总值/PnL → `save_pnl_snapshot`
- 存储: `pnl_snapshots` 表

## 8. 数据刷新策略

| 时段 | 策略 |
|------|------|
| 交易时段 (9:30–11:30 / 13:00–15:00) | Dashboard 每 60s 轮询 `get_latest_price` |
| 盘前/盘后 | 每天 3 次定时拉取 (9:00 / 12:30 / 15:30) |
| 委员会运行时 | 立即触发（Phase 3 预留接口） |

实现: 前端 `setInterval` + visibility API（页面不可见时暂停轮询）。

## 9. 交易日历守卫

- `is_trading_day(date)` 已实现
- 新增启动同步: 应用启动时检查 `trade_calendar` 最大日期，不足 90 天则调 Tushare `trade_cal` 拉取未来 2 年
- 失败不阻塞启动，退化为周一到周五判定 + toast 警告

## 10. Legacy JSON 迁移

- 启动时检测 `~/.claw-go/invest/portfolio.json` 是否存在
- 存在 → 显示 toast「检测到旧版投资数据，点击迁移」
- 点击 → 读取 JSON → 逐条 `add_holding` + `update_cash` → 重命名 `.legacy`
- 失败不删除旧文件，保留原始数据

## 11. i18n 新增 keys

Dashboard、持仓、交易、策略、PnL 相关 UI 文本，en + zh-CN 双语。

## 12. 不在 Phase 2 范围

- 委员会编排（Phase 3a）
- Streaming（Phase 3b）
- Event Watch（Phase 3c）
- 历史命中率 / Dreaming（Phase 4）
- 系统二级页（Phase 4）
- 用户档案（Phase 4）

## 测试策略

- Frontend: Vitest 单元测试 for invest-store（mock invoke）
- Rust: `cargo test invest::` for Tushare client、strategy storage、PnL snapshot logic
- 手动: Dashboard 价格刷新、买入/卖出对话框、PnL 图表渲染、交易日历守卫
- 构建: `npm run build` + `cargo check`
