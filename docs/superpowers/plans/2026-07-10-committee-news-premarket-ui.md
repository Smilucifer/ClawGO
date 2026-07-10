# 委员会「新闻/舆论」+ 盘前观察优化 — Plan 1 (UI 组) 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付 spec 的 UI 组四项——盘前 01 模块自适应布局(A3)、快讯/新闻 7 天定时清理(A4)、盘前报告改前一晚生成(A6)、委员会「新闻/舆论」双列视图(A1)——每项独立可验收、独立可回滚。

**Architecture:** 前端 SvelteKit(Svelte 5 runes)+ Rust/Tauri 后端,store-centric。A3 纯前端 CSS/派生;A4/A6 纯后端(storage + scheduler cron);A1 跨栈(新 Tauri 命令 + store 方法 + 类型 + 组件重构 + i18n)。四项彼此近乎独立,可按任意顺序做;本计划排序 A3 → A4 → A6 → A1,即"最自包含、最低风险"在前,"最大跨栈重构"在后。

**Tech Stack:** SvelteKit + Svelte 5 runes、TypeScript、Rust、rusqlite(SQLite)、tokio-cron-scheduler、Tauri IPC、扁平 JSON i18n(`messages/en.json` + `messages/zh-CN.json`)。

## Global Constraints

- **红涨绿跌**: `--up` = 红(涨/偏多),`--down` = 绿(跌/偏空)。stance 上色一律 bullish→`--up`、bearish→`--down`,不要反。
- **i18n 双文件同步**: 任何 UI 文案必须同时改 `messages/en.json` 与 `messages/zh-CN.json`,键集必须完全一致;每个改 i18n 的任务末尾跑 `npm run i18n:check` 必须 PASS。
- **Windows-first**: 无 WSL/mac/Linux 假设。
- **本机 Rust 单测限制**: 裸 `cargo test`(Git Bash)会挂 `0xc0000139`;单测用 `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- <filter> --nocapture"` 或 `npm run rust:test`。快速校验用 `cargo check`。
- **created_at 格式**: `events` 与 `sentiment_items` 的 `created_at` 由 `datetime('now')` 写入,格式为 `"YYYY-MM-DD HH:MM:SS"`(UTC、空格、无 `T`/`Z`);任何 cutoff 字符串必须用 `%Y-%m-%d %H:%M:%S` 格式化,不得用 rfc3339。
- **Cron 表达式为 6 字段含秒**: `sec min hour day month dow`。
- **验证基线(每个前端任务)**: `npm run check` + 视情况 `npm run build`;后端任务: `cargo check --manifest-path src-tauri/Cargo.toml` + `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`。
- **Conventional Commits**: `feat:`/`fix:`/`refactor:`/`chore:`。
- **提交范围**: 只 `git add` 本任务涉及的文件,绝不 `git add .`;不提交密钥/本地设置/生成态。

---

### Task A3: 盘前 01 模块 theme-wall 自适应布局

**Files:**
- Modify `src/lib/components/invest/PremarketReportTab.svelte`
  - script: 在 `commentary` `$derived` 附近（~line 156）新增 `hasRisk` / `wallClass` 派生
  - markup: lines 451-466（`.theme-wall` 块）
  - CSS: line 858-859（`.theme-wall` 规则）

**规则回顾（供后续自查，不写入代码注释）：**
- `n ≤ 4` → `repeat(n, 1fr)`（用 `wall-n1..wall-n4`）
- `n === 5 && !hasRisk` → 1+4 布局（`wall-1plus4`：4 列，第一张 `.ttc-first` 整行）
- `n ≥ 6` 或 `hasRisk` → 3 列（`wall-3col`）；风险卡沿用原有 inline `grid-column: 1 / -1;` 整行占位。

**Steps:**

- [ ] 1. 在 `PremarketReportTab.svelte` 的 script 区、`commentary` `$derived` 声明之后，新增两个派生（保持 Svelte 5 runes 风格；`commentary` 类型为 `AiCommentary | null`）：

```ts
  const hasRisk = $derived(
    !!commentary && commentary.sectors.some((s) => s.tag.includes('风险'))
  );
  const wallClass = $derived.by(() => {
    if (!commentary) return 'wall-3col';
    const n = commentary.sectors.length;
    if (hasRisk) return 'wall-3col';
    if (n <= 4) return `wall-n${n}`;
    if (n === 5) return 'wall-1plus4';
    return 'wall-3col';
  });
```

- [ ] 2. 更新 markup（lines 451-466）。

Before:

```svelte
        {#if commentary && commentary.sectors.length > 0}
          <div class="theme-wall">
            {#each commentary.sectors as sec, i}
              <div
                class="theme-tag-card"
                style={sec.tag.includes('风险') ? 'grid-column: 1 / -1;' : ''}
              >
                <div class="ttc-head">
                  <span class="ttc-name">{sec.name}</span>
                  <span class="eval-tag {evalClass(sec.tag)}">{sec.tag}</span>
                  <span class="ttc-count">{sec.count} {t('invest_premarket_news_count_unit')}</span>
                </div>
                <div class="ttc-desc">{sec.note}</div>
              </div>
            {/each}
          </div>
```

After:

```svelte
        {#if commentary && commentary.sectors.length > 0}
          <div class="theme-wall {wallClass}">
            {#each commentary.sectors as sec, i}
              <div
                class="theme-tag-card"
                class:ttc-first={i === 0 && wallClass === 'wall-1plus4'}
                style={sec.tag.includes('风险') ? 'grid-column: 1 / -1;' : ''}
              >
                <div class="ttc-head">
                  <span class="ttc-name">{sec.name}</span>
                  <span class="eval-tag {evalClass(sec.tag)}">{sec.tag}</span>
                  <span class="ttc-count">{sec.count} {t('invest_premarket_news_count_unit')}</span>
                </div>
                <div class="ttc-desc">{sec.note}</div>
              </div>
            {/each}
          </div>
```

（注意：`sec.tag.includes('风险')` inline `style` 保留不动；`class:ttc-first` 只在 `wallClass === 'wall-1plus4'` 时对 `i === 0` 生效——该分支下必然 `hasRisk === false`，与风险卡整行不冲突。）

- [ ] 3. 更新 CSS。

Before (line 858-859):

```css
  /* 01 舆情标签墙 */
  .theme-wall { display: grid; grid-template-columns: repeat(4, 1fr); gap: var(--space-2); }
```

After:

```css
  /* 01 舆情标签墙 */
  .theme-wall { display: grid; gap: var(--space-2); grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-n1 { grid-template-columns: 1fr; }
  .theme-wall.wall-n2 { grid-template-columns: repeat(2, 1fr); }
  .theme-wall.wall-n3 { grid-template-columns: repeat(3, 1fr); }
  .theme-wall.wall-n4 { grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-1plus4 { grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-3col { grid-template-columns: repeat(3, 1fr); }
  .theme-tag-card.ttc-first { grid-column: 1 / -1; }
```

- [ ] 4. 运行 `npm run check`，预期 PASS（新增派生的类型推断应无告警；`commentary.sectors` 已由 `AiCommentary` 类型收窄）。

- [ ] 5. 运行 `npm run build`，预期 PASS（SvelteKit adapter-static 构建通过）。

- [ ] 6. 视觉自查（无自动化测试）：加载盘前报告，分别构造 1/2/3/4/5/6+ 卡片以及包含"风险"标签的情形，确认：
  - `n ≤ 4`：单行 n 等分。
  - `n === 5` 且无风险卡：第一张整行，其余 4 张一行 4 列。
  - `n ≥ 6` 或存在风险卡：3 列流式，风险卡整行占位。

- [ ] 7. 提交：

```
feat(premarket): theme-wall 按卡片数自适应布局 (1+4 / 3col / 风险卡整行)
```


---

### Task A4: 快讯/新闻 7 天定时清理

**目标**: `events` 与 `sentiment_items` 两张表当前无清理机制、会无限增长。加一个每日 03:30 的 cron 任务，删除 7 天前的过期数据；`events` 保留已触发（`triggered = 1`）与高价值（`severity = 'high'`）历史；`sentiment_items` 硬删除。7 天为硬编码常量（YAGNI）。

**Files:**
- 新建 `src-tauri/src/storage/invest/news_cleanup.rs`
- 修改 `src-tauri/src/storage/invest/mod.rs`（挂 `pub mod news_cleanup;`）
- 修改 `src-tauri/src/invest/scheduler/mod.rs`（`default_jobs()` 里加 CronJob）
- 修改 `src-tauri/src/invest/scheduler/runner.rs`（`dispatch_job` 加 match arm）

**关键约束（不要走错的坑）:**
- `created_at` 由 `datetime('now')` 写入，格式是 `"YYYY-MM-DD HH:MM:SS"`（UTC、空格分隔、无 `T` 无 `Z`）。cutoff 字符串必须用 `"%Y-%m-%d %H:%M:%S"` 格式化，**不要**用 `to_rfc3339()`，否则字符串比较会全错。
- 纯函数 `prune_events_on(&Connection, &str)` / `prune_sentiment_on(&Connection, &str)` 便于用 `Connection::open_in_memory()` 单测；对外的包装函数再从 `with_conn_mut` 拿连接。
- 本机 `cargo test` 直接跑会挂 `0xc0000139`，通过 `cmd.exe /c` 走 CC6 runtime，或用 `npm run rust:test`。

---

**Steps:**

- [ ] **1. 新建 `src-tauri/src/storage/invest/news_cleanup.rs`（含实现 + 单测，TDD 一次落地）**。下面是文件的完整最终内容——先写 `#[cfg(test)] mod tests`(第 2 步 `cargo check` 前它引用的函数还未挂进 crate,靠本步同文件实现补齐);函数体即最终实现,SQL 见第 3 步的精确约束。文件完整内容：

```rust
use rusqlite::{params, Connection};

use super::with_conn_mut;

/// 删除 events 表中过期的低价值未触发事件。
/// 保留：`triggered = 1` 或 `severity = 'high'` 的历史记录。
pub fn prune_events_on(conn: &Connection, cutoff: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "DELETE FROM events WHERE created_at < ?1 AND triggered = 0 AND severity != 'high'",
            params![cutoff],
        )
        .map_err(|e| format!("prune_events: {}", e))?;
    Ok(n)
}

/// 删除 sentiment_items 表中过期的舆情条目（硬清理）。
pub fn prune_sentiment_on(conn: &Connection, cutoff: &str) -> Result<usize, String> {
    let n = conn
        .execute(
            "DELETE FROM sentiment_items WHERE created_at < ?1",
            params![cutoff],
        )
        .map_err(|e| format!("prune_sentiment: {}", e))?;
    Ok(n)
}

fn cutoff_string(keep_days: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::days(keep_days))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

/// 生产入口：清理 events 表。
pub fn prune_events(keep_days: i64) -> Result<usize, String> {
    let cutoff = cutoff_string(keep_days);
    with_conn_mut(|conn| prune_events_on(conn, &cutoff))
}

/// 生产入口：清理 sentiment_items 表。
pub fn prune_sentiment_items(keep_days: i64) -> Result<usize, String> {
    let cutoff = cutoff_string(keep_days);
    with_conn_mut(|conn| prune_sentiment_on(conn, &cutoff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open_in_memory");
        conn.execute_batch(
            r#"
            CREATE TABLE events (
                id TEXT PRIMARY KEY,
                severity TEXT NOT NULL DEFAULT 'info',
                triggered INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE TABLE sentiment_items (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL
            );
            "#,
        )
        .expect("create tables");
        conn
    }

    fn insert_event(conn: &Connection, id: &str, severity: &str, triggered: i64, created_at: &str) {
        conn.execute(
            "INSERT INTO events (id, severity, triggered, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, severity, triggered, created_at],
        )
        .expect("insert event");
    }

    fn insert_sentiment(conn: &Connection, id: &str, created_at: &str) {
        conn.execute(
            "INSERT INTO sentiment_items (id, created_at) VALUES (?1, ?2)",
            params![id, created_at],
        )
        .expect("insert sentiment");
    }

    #[test]
    fn prune_events_keeps_triggered_and_high_severity() {
        let conn = setup_db();
        // 过期 + info + 未触发 -> 应删
        insert_event(&conn, "old-info-untriggered", "info", 0, "2026-07-01 08:00:00");
        // 过期 + 已触发 -> 保留
        insert_event(&conn, "old-info-triggered", "info", 1, "2026-07-01 08:00:00");
        // 过期 + high -> 保留
        insert_event(&conn, "old-high-untriggered", "high", 0, "2026-07-01 08:00:00");
        // 未过期 + info + 未触发 -> 保留
        insert_event(&conn, "recent-info-untriggered", "info", 0, "2026-07-09 08:00:00");

        let cutoff = "2026-07-03 00:00:00";
        let n = prune_events_on(&conn, cutoff).expect("prune_events_on");
        assert_eq!(n, 1, "should delete exactly the old untriggered non-high row");

        let remaining: Vec<String> = conn
            .prepare("SELECT id FROM events ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            remaining,
            vec![
                "old-high-untriggered".to_string(),
                "old-info-triggered".to_string(),
                "recent-info-untriggered".to_string(),
            ]
        );
    }

    #[test]
    fn prune_sentiment_hard_cutoff() {
        let conn = setup_db();
        insert_sentiment(&conn, "old-1", "2026-07-01 08:00:00");
        insert_sentiment(&conn, "old-2", "2026-07-02 23:59:59");
        insert_sentiment(&conn, "recent-1", "2026-07-05 00:00:00");

        let cutoff = "2026-07-03 00:00:00";
        let n = prune_sentiment_on(&conn, cutoff).expect("prune_sentiment_on");
        assert_eq!(n, 2);

        let remaining: Vec<String> = conn
            .prepare("SELECT id FROM sentiment_items ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(remaining, vec!["recent-1".to_string()]);
    }

    #[test]
    fn cutoff_format_matches_sqlite_datetime_now() {
        // 格式必须是 "YYYY-MM-DD HH:MM:SS"（空格、无 T、无 Z），与 datetime('now') 一致。
        let s = cutoff_string(7);
        assert_eq!(s.len(), 19);
        assert_eq!(s.chars().nth(10), Some(' '));
        assert!(!s.contains('T'));
        assert!(!s.contains('Z'));
    }
}
```

- [ ] **2. 挂模块**：在 `src-tauri/src/storage/invest/mod.rs` 里，紧挨已有的 `pub mod sentiment;` 那一段 sibling 声明，追加一行：

```rust
pub mod news_cleanup;
```

然后先跑一次 `cargo check`（此时函数已经实现，应通过；如果第 1 步用 `todo!()` 占位则这里也会 check pass，只有 test 失败）：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **3. 实现函数体**：第 1 步已经给出完整实现（`prune_events_on` / `prune_sentiment_on` / `cutoff_string` / `prune_events` / `prune_sentiment_items`），如果之前用 `todo!()` 占位现在替换成完整版本。SQL 必须精确匹配：
  - events: `DELETE FROM events WHERE created_at < ?1 AND triggered = 0 AND severity != 'high'`
  - sentiment: `DELETE FROM sentiment_items WHERE created_at < ?1`

- [ ] **4. 跑测试（本机走 cmd.exe 绕过 0xc0000139）**：

```bash
cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- news_cleanup --nocapture"
```

期望：3 个测试全 PASS。再跑：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

期望 PASS。

- [ ] **5. 注册 CronJob**：在 `src-tauri/src/invest/scheduler/mod.rs` 的 `default_jobs()` 返回的 `Vec<CronJob>` 里追加一个条目（与其它 job 同风格）：

```rust
        CronJob {
            id: "news_cleanup".into(),
            name: "快讯清理".into(),
            cron_expr: "0 30 3 * * *".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: false,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "清理 7 天前的过期快讯/舆情（保留已触发/高价值事件）".into(),
            dedicated: false,
        },
```

Cron 表达式 `0 30 3 * * *` = 每天 03:30:00（6 字段含秒）。`requires_trading_day: false` — 清理不受交易日限制。

- [ ] **6. Runner 分发**：在 `src-tauri/src/invest/scheduler/runner.rs` 的 `dispatch_job` 大 `match id` 里，紧挨其它 arm 之前、`_ => Err(...)` 之上，加：

```rust
        "news_cleanup" => {
            let ev = crate::storage::invest::news_cleanup::prune_events(7)?;
            let se = crate::storage::invest::news_cleanup::prune_sentiment_items(7)?;
            Ok(format!("快讯清理: events {} 条, sentiment {} 条", ev, se))
        }
```

- [ ] **7. 静态检查全绿**：

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

两条都必须 PASS。有 clippy warning 就修掉再进下一步。

- [ ] **8. 提交**：

```bash
git add src-tauri/src/storage/invest/news_cleanup.rs \
        src-tauri/src/storage/invest/mod.rs \
        src-tauri/src/invest/scheduler/mod.rs \
        src-tauri/src/invest/scheduler/runner.rs
git commit -m "feat(invest): 快讯/新闻 7 天定时清理 cron (保留已触发/高价值)"
```

**验收**:
- `news_cleanup.rs` 3 个单测通过。
- `cargo check` + `cargo clippy -D warnings` 通过。
- `default_jobs()` 输出包含 `news_cleanup`；runner 能匹配、执行不 panic。
- 现网首日运行后 events / sentiment_items 行数不再单调增长，7 天以内的最近数据以及历史上已触发/high 事件保留完整。


---

### Task A6: 盘前报告改前一晚生成（按下一个交易日标注）

**目标**：把盘前观察报告的生成从"交易日当天早上 09:00"改到"交易日前一晚 21:00"，报告日期标注为它服务的**下一个交易日**（用交易日历判断），这样用户早上打开就能看到已经生成好的报告。

**范围收敛**：post-close 因子缓存 cron（`premarket_cache`）在 16:30 已经正确，本任务**不**改它。A6 只改：
1. `premarket_report` cron 触发时间 + `requires_trading_day` 标志。
2. 报告入口的日期派生逻辑。
3. 新增 `next_trading_day(date) -> Result<String, String>` 辅助函数。

**关键决策：为什么 `requires_trading_day` 必须翻为 `false`**

主调度循环会在 `requires_trading_day == true` 时用当日日期做交易日门禁，非交易日直接跳过。改到 21:00 后，生成夜（例如周日晚）**本身不是交易日**——如果保留 `true`，周日晚上会被门禁直接跳过，永远生不出周一的盘前报告。所以：
- cron 覆盖到"任何可能是下一个交易日前一晚"的日子：`0 0 21 * * 0-4`（周日到周四晚 21:00）。
- `requires_trading_day` 翻为 `false`，把"是否存在下一个交易日"的判断下沉到 `next_trading_day`（用交易日历查询）。节假日前一晚（例如国庆前的周五晚是 5，本表达式不覆盖；正常节前如清明前一晚若正好落在 0-4，`next_trading_day` 会跳过整个假期返回下一个真实交易日，日期依旧正确）。

**Files:**
- Modify `src-tauri/src/storage/invest/scheduler.rs` — 新增 `next_trading_day` 辅助函数。
- Modify `src-tauri/src/invest/premarket/report.rs`（第 599 行附近）— 报告日期改用"北京今天 + 下一个交易日"。
- Modify `src-tauri/src/invest/scheduler/mod.rs`（第 151-163 行）— `premarket_report` CronJob 的 `cron_expr`/`requires_trading_day`/`description`。

**Steps:**

- [ ] **1. 在 `src-tauri/src/storage/invest/scheduler.rs` 追加 `next_trading_day` 辅助函数**

  在文件末尾（或紧挨 `is_trading_day` / `is_weekday` 之后的位置）追加：

  ```rust
  /// 返回给定日期**之后**的下一个交易日（YYYY-MM-DD）。
  /// 优先查交易日历（is_open=1 且 cal_date > date 的最小值）；
  /// 日历无覆盖时回退为"下一个工作日"（跳过周末），最多前探 10 天。
  pub fn next_trading_day(date: &str) -> Result<String, String> {
      // 1) 交易日历优先
      let from_cal: Option<String> = with_conn(|conn| {
          conn.query_row(
              "SELECT MIN(cal_date) FROM trade_calendar WHERE is_open = 1 AND cal_date > ?1",
              params![date],
              |row| row.get::<_, Option<String>>(0),
          )
          .map_err(|e| format!("next_trading_day query: {}", e))
      })?;
      if let Some(d) = from_cal {
          return Ok(d);
      }
      // 2) 回退：从次日起找第一个工作日
      let base = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
          .map_err(|e| format!("parse date: {}", e))?;
      for i in 1..=10 {
          let cand = base + chrono::Duration::days(i);
          let s = cand.format("%Y-%m-%d").to_string();
          if is_weekday(&s) {
              return Ok(s);
          }
      }
      Err(format!("next_trading_day: 未找到 {} 之后的交易日", date))
  }
  ```

  说明：`with_conn`、`rusqlite::params`、`chrono` 在文件顶部已经引入（同文件其他函数已使用），无需新增 `use`。

  运行：
  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml
  ```
  预期：通过（新函数暂未被调用，允许 dead_code 因为 `pub`）。

- [ ] **2. 修改 `src-tauri/src/invest/premarket/report.rs` 的日期派生（第 598-599 行）**

  Before:
  ```rust
  pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
      let date = crate::invest::date_utils::get_invest_date();
  ```

  After:
  ```rust
  pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
      // A6: 盘前报告在交易日前一晚生成，日期标注为它服务的"下一个交易日"。
      // 不能用 date_utils::get_invest_date()（05:00 分割线、非交易日历感知）。
      let today = crate::storage::invest::scheduler::beijing_today();
      let date = crate::storage::invest::scheduler::next_trading_day(&today)?;
  ```

  下游对 `date` 的使用保持不变：
  - md 头 ``# 盘前观察 {date}``（第 649 行）
  - 文件名 `premarket_{date}.md`（第 666 行）、`premarket_{date}.json`（第 669 行）
  - JSON `"date"` 字段（第 671 行）

  现在这些字段全都天然携带"目标交易日"，无需其他改动。

  运行：
  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml
  ```
  预期：通过。

- [ ] **3. 修改 `src-tauri/src/invest/scheduler/mod.rs` 第 151-163 行的 `premarket_report` CronJob**

  Before:
  ```rust
          CronJob {
              id: "premarket_report".into(),
              name: "盘前观察报告".into(),
              cron_expr: "0 0 9 * * 1-5".into(),
              interval_min: None,
              enabled: true,
              requires_trading_day: true,
              last_run: None,
              next_run: None,
              last_status: None,
              description: "盘前生成观察报告（舆情+SABC+拥挤度+AI点评）".into(),
              dedicated: false,
          },
  ```

  After:
  ```rust
          CronJob {
              id: "premarket_report".into(),
              name: "盘前观察报告".into(),
              // A6: 交易日前一晚 21:00 生成；周日-周四覆盖"下一个可能是交易日"的前夜。
              // requires_trading_day 必须为 false：生成夜本身不是交易日，
              // 若为 true 会被主循环的交易日门禁跳过。日历判定下沉到 next_trading_day。
              cron_expr: "0 0 21 * * 0-4".into(),
              interval_min: None,
              enabled: true,
              requires_trading_day: false,
              last_run: None,
              next_run: None,
              last_status: None,
              description: "交易日前一晚 21:00 生成下一交易日的盘前观察报告（舆情+SABC+拥挤度+AI点评）".into(),
              dedicated: false,
          },
  ```

- [ ] **4. 编译 + Clippy 全清零**

  ```bash
  cargo check --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  ```
  预期：均通过。

- [ ] **5. 运行期验证说明（不在本地跑单测——本机 Rust 单测环境不可用）**

  `next_trading_day` 的日历分支依赖共享 SQLite 连接，无 temp-DB 隔离，不适合在本机加 `#[cfg(test)]`。回退分支的核心 `is_weekday` 已在同文件其他路径中被使用，编译期由 `cargo check` 保证签名正确。

  行为层验证放在集成运行时：
  - 周日晚 21:00 触发一次 → 应生成 `premarket_2026-07-06.md`（示例：假设那周一为 07-06）。
  - 报告 md header、`.md` / `.json` 文件名、JSON `date` 字段应统一为 next trading day。
  - 前端 `list_premarket_reports` 按文件名 DESC 排序取最新，无需前端改动。

- [ ] **6. 提交**

  ```bash
  git add src-tauri/src/storage/invest/scheduler.rs \
          src-tauri/src/invest/premarket/report.rs \
          src-tauri/src/invest/scheduler/mod.rs
  git commit -m "fix(invest): 盘前报告改交易日前一晚生成, 按下一个交易日标注"
  ```


---

### Task A1a: 新增 `get_sentiment_items` 命令读取 sentiment_items 表

**Goal**: expose the existing `storage::invest::sentiment::list_recent_sentiment` helper to the frontend as a Tauri command so the news/opinion view can render 雪球舆情 rows. Backend-only; no UI touched here.

**Files**
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src-tauri\src\commands\invest.rs` (imports lines 1-11; command block near line 173).
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src-tauri\src\lib.rs` (`generate_handler!` list, after line 486).

**Steps**

- [ ] **Add `sentiment` to the `use crate::storage::invest::{...}` import block** at the top of `src-tauri/src/commands/invest.rs`. Replace the current block:
    ```rust
    use crate::storage::invest::{
        events::{self, Event},
        portfolio::{self, Holding, Trade},
        scheduler,
        strategy,
        verdicts::{self, PnlSnapshot, Verdict},
    };
    ```
    with:
    ```rust
    use crate::storage::invest::{
        events::{self, Event},
        portfolio::{self, Holding, Trade},
        scheduler,
        sentiment::{self, SentimentItem},
        strategy,
        verdicts::{self, PnlSnapshot, Verdict},
    };
    ```

- [ ] **Append the new `get_sentiment_items` command** in `src-tauri/src/commands/invest.rs`, immediately after the existing `get_events` command (around line 176):
    ```rust
    #[tauri::command]
    pub fn get_sentiment_items(limit: Option<i64>) -> Result<Vec<SentimentItem>, String> {
        // `list_recent_sentiment` requires a `since` cutoff; passing epoch effectively means
        // "give me the latest N ordered by created_at DESC" which is what the news column wants.
        sentiment::list_recent_sentiment("1970-01-01 00:00:00", limit.unwrap_or(200))
    }
    ```

- [ ] **Register the command in `src-tauri/src/lib.rs`** by inserting a new line immediately after line 486 (`commands::invest::collect_sentiment,`):
    ```rust
    commands::invest::get_sentiment_items,
    ```
    (Placement between `collect_sentiment` and the next entry keeps sentiment-related handlers grouped.)

- [ ] **Verify the backend compiles**:
    ```bash
    cargo check --manifest-path src-tauri/Cargo.toml
    ```
    Expected: `Finished ... dev [unoptimized + debuginfo]` with no errors. Warnings about unused imports elsewhere are fine; there must be **no** errors mentioning `sentiment`, `SentimentItem`, or `get_sentiment_items`.

- [ ] **Verify clippy stays clean on the touched files** (fast smoke — the full clippy is part of `npm run verify`):
    ```bash
    cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
    ```
    Expected: no warnings/errors in `commands/invest.rs` or `lib.rs`.

- [ ] **Commit**:
    ```bash
    git add src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
    git commit -m "feat(invest): 新增 get_sentiment_items 命令读取舆情条目"
    ```

---

### Task A1b: 前端 `SentimentItem` 类型 + `store.fetchSentimentItems`

**Goal**: mirror the backend contract in TS and add the store slice the news view will consume. Also patch the pre-existing `sectors` gap on `InvestEvent` (Rust already returns it — frontend was missing the field).

**Files**
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\types.ts` (`InvestEvent` at lines 1886-1901; new interface after `EventFilter` at lines 1918-1922).
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\stores\invest-store.svelte.ts` (invest-type import at top; event state near lines 76-85; `fetchEvents` around lines 595-605).

**Steps**

- [ ] **Patch `InvestEvent` to include `sectors`** in `src/lib/types.ts`. Replace:
    ```typescript
    export interface InvestEvent {
      id: string;
      source: string;
      eventType: string;
      title: string;
      body: string | null;
      symbols: string | null;
      severity: string;
      stance: string;
      triggered: boolean;
      triggerVerdictId: string | null;
      createdAt: string;
      analyzed: boolean;
      analyzedAt: string | null;
      channels: string;
    }
    ```
    with:
    ```typescript
    export interface InvestEvent {
      id: string;
      source: string;
      eventType: string;
      title: string;
      body: string | null;
      symbols: string | null;
      sectors: string | null;
      severity: string;
      stance: string;
      triggered: boolean;
      triggerVerdictId: string | null;
      createdAt: string;
      analyzed: boolean;
      analyzedAt: string | null;
      channels: string;
    }
    ```

- [ ] **Add the `SentimentItem` interface** in `src/lib/types.ts` immediately after the existing `EventFilter` interface (line ~1922):
    ```typescript
    export interface SentimentItem {
      id: string;
      provider: string;
      symbol: string | null;
      title: string;
      summary: string | null;
      url: string | null;
      publishedAt: string | null;
      readCount: number | null;
      commentCount: number | null;
      sourceType: string;
      sentimentHint: number | null;
      affectedSymbols: string | null;
      sectors: string | null;
      topics: string | null;
      stance: string;
      severity: string;
      analyzed: boolean;
      createdAt: string;
    }
    ```

- [ ] **Import `SentimentItem` in the store**. Open `src/lib/stores/invest-store.svelte.ts` and add `SentimentItem` to the type import that already brings in `InvestEvent`, `EventFilter`, etc. from `$lib/types` (the imports live near the top of the file). Example — if the import currently reads:
    ```typescript
    import type {
      InvestEvent,
      EventFilter,
      ScanStatus,
      ScanResult,
      // ...other invest types
    } from "$lib/types";
    ```
    change it to include `SentimentItem`:
    ```typescript
    import type {
      InvestEvent,
      EventFilter,
      ScanStatus,
      ScanResult,
      SentimentItem,
      // ...other invest types
    } from "$lib/types";
    ```

- [ ] **Add `sentimentItems` state** to the Event Watch State block (around line 85, immediately after `lastScanResult`):
    ```typescript
      // ── Event Watch State ───────────────────────────────────────────────
      events = $state<InvestEvent[]>([]);
      eventFilter = $state<EventFilter>({ timeWindow: "24h", severity: "all", search: "" });
      scanStatus = $state<ScanStatus | null>(null);
      isScanning = $state<boolean>(false);
      lastScanResult = $state<ScanResult | null>(null);
      sentimentItems = $state<SentimentItem[]>([]);
    ```

- [ ] **Add `fetchSentimentItems`** as a sibling to `fetchEvents`. Locate `fetchEvents` (around lines 595-605); immediately after its closing brace insert:
    ```typescript
      async fetchSentimentItems(): Promise<void> {
        try {
          const items = await invoke<SentimentItem[]>("get_sentiment_items", { limit: 200 });
          this.sentimentItems = items;
        } catch (e) {
          console.error("Failed to fetch sentiment items:", e);
        }
      }
    ```

- [ ] **Verify the frontend type-checks**:
    ```bash
    npm run check
    ```
    Expected: `svelte-check` reports **0 errors, 0 warnings** on the touched files. If a warning appears in an unrelated file, ensure it existed before this task (compare with `git stash && npm run check`).

- [ ] **Commit**:
    ```bash
    git add src/lib/types.ts src/lib/stores/invest-store.svelte.ts
    git commit -m "feat(invest): 前端 SentimentItem 类型 + store.fetchSentimentItems"
    ```

---

### Task A1c: 事件视图从「系统」迁移到「委员会/新闻/舆论」子标签

**Goal**: physically relocate the events sub-tab. The `EventWatchTab` component keeps its **existing** single-column implementation for this step (the two-column rebuild lands in A1d) — this task is purely the wiring / navigation move and its i18n handles.

**Files**
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src\routes\invest\+page.svelte` (types at lines 30-31; sub-tab arrays lines 45-63; render branches lines 227-239 and 301-302).
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\messages\zh-CN.json` (line 327 `invest_system_sub_events`; committee sub-keys near lines 570-575).
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\messages\en.json` (mirror keys).

**Steps**

- [ ] **Update the sub-tab type unions** at the top of `src/routes/invest/+page.svelte`. Replace:
    ```svelte
    type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'roles' | 'accuracy' | 'premarket';
    type SystemSubTab = 'cron' | 'events' | 'datasource' | 'pnl_history' | 'insights' | 'dreams' | 'profile' | 'cleanup';
    ```
    with:
    ```svelte
    type CommitteeSubTab = 'live' | 'replay' | 'archive' | 'news' | 'roles' | 'accuracy' | 'premarket';
    type SystemSubTab = 'cron' | 'datasource' | 'pnl_history' | 'insights' | 'dreams' | 'profile' | 'cleanup';
    ```

- [ ] **Insert the `news` entry in `committeeSubTabs`** (between `archive` and `roles`, per spec 「位置置于 archive 之后、premarket 之前」— placing it right after `archive` satisfies both constraints):
    ```svelte
    { id: 'news', label: t('invest_committee_sub_news') },
    ```

- [ ] **Remove the `events` entry from `systemSubTabs`**. Delete this line (currently line 47):
    ```svelte
    { id: 'events', label: t('invest_system_sub_events') },
    ```

- [ ] **Add a committee render branch for `news`**. In the committee render block (lines 227-239), insert a new branch after the `archive` branch and before `roles`:
    ```svelte
          {:else if committeeSubTab === 'archive'}
            <CommitteeArchiveTab />
          {:else if committeeSubTab === 'news'}
            <EventWatchTab onNavigateToCommittee={() => { committeeSubTab = 'live'; }} />
          {:else if committeeSubTab === 'roles'}
            <CommitteeRolesTab />
    ```
    Rationale for the callback: the existing `EventWatchTab` uses `onNavigateToCommittee` after a successful trigger to jump into the live committee run. We're already inside the committee tab, so switching just the sub-tab to `'live'` is the natural equivalent.

- [ ] **Remove the events branch from the system render block** (lines 301-302). Delete:
    ```svelte
          {:else if systemSubTab === 'events'}
            <EventWatchTab onNavigateToCommittee={() => { activeTab = 'committee'; committeeSubTab = 'live'; }} />
    ```
    Leave the `import EventWatchTab from '$lib/components/invest/EventWatchTab.svelte';` at line 19 as-is — it's now used by the committee branch.

- [ ] **Add `invest_committee_sub_news` to `messages/zh-CN.json`**, adjacent to the other `invest_committee_sub_*` keys (near line 570-575). Example placement — insert immediately after `invest_committee_sub_archive`:
    ```json
      "invest_committee_sub_archive": "档案",
      "invest_committee_sub_news": "新闻/舆论",
      "invest_committee_sub_roles": "角色",
    ```
    (Keep whatever exact values already exist for `archive`/`roles` — only the new `news` line is added.)

- [ ] **Remove `invest_system_sub_events` from `messages/zh-CN.json`** (currently line 327). Delete the line:
    ```json
      "invest_system_sub_events": "事件",
    ```
    Make sure the surrounding JSON commas are still valid (the previous line should keep its trailing comma, and the removed line's trailing comma disappears with it).

- [ ] **Mirror both changes in `messages/en.json`**. Insert `"invest_committee_sub_news": "News / Opinion",` in the same relative position (after `invest_committee_sub_archive`) and delete the `invest_system_sub_events` entry.

- [ ] **Verify no stray references to the removed key**:
    ```bash
    git grep "invest_system_sub_events"
    ```
    Expected: no matches.

- [ ] **Type-check and i18n-check**:
    ```bash
    npm run check
    npm run i18n:check
    ```
    Expected: both pass. `i18n:check` should confirm en/zh key sets are identical (both got `invest_committee_sub_news` added, both lost `invest_system_sub_events`).

- [ ] **Manual smoke** (dev server):
    ```bash
    npm run tauri dev
    ```
    Navigate to `/invest` → 委员会 sub-tabs — the new `新闻/舆论` chip appears between `档案` and `角色`, clicking it renders the existing (still-single-column) `EventWatchTab`. Navigate to 系统 sub-tabs — the `事件` chip is gone.

- [ ] **Commit**:
    ```bash
    git add src/routes/invest/+page.svelte messages/zh-CN.json messages/en.json
    git commit -m "refactor(invest): 事件视图迁移至委员会「新闻/舆论」子标签"
    ```

---

### Task A1d: `EventWatchTab` 双列重构 + 子组件 + 内容 i18n 键

**Goal**: rebuild `EventWatchTab` as a two-column container. LEFT = 金十快讯 (`source === 'jinshi_flash'`) with 全部/仅高/偏多/偏空 chips. RIGHT = merged 公告/个股新闻/雪球舆情 with 全部/公告/个股新闻/雪球舆情 chips. Each column scrolls independently. Below 900px the layout stacks. Trigger dialog stays at the container level; only the left column emits triggers.

**Files**
- Create: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\components\invest\news-helpers.ts`
- Create: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\components\invest\NewsFlashColumn.svelte`
- Create: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\components\invest\NewsDigestColumn.svelte`
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\src\lib\components\invest\EventWatchTab.svelte` (full rewrite, ~242 lines → new container)
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\messages\zh-CN.json` (add `invest.news.*` block)
- Modify: `D:\ClaudeWorkspace\Code\ClawGO\messages\en.json` (mirror block)

**Steps**

- [ ] **Create the shared helpers** at `src/lib/components/invest/news-helpers.ts`:
    ```typescript
    // Shared visual helpers for the 新闻/舆论 columns.
    // These duplicate the small mapping fns that used to live inside EventWatchTab.svelte;
    // extracting them keeps NewsFlashColumn and NewsDigestColumn DRY without pulling
    // in a wider utility barrel.

    export type Stance = "bullish" | "bearish" | "neutral" | string;
    export type Severity = "high" | "medium" | "low" | string;

    /** Background token for a severity chip. 红涨绿跌 is applied by stanceColor(), not here. */
    export function severityBadgeBg(severity: Severity): string {
      switch (severity) {
        case "high":
          return "var(--color-error-bg)";
        case "medium":
          return "var(--color-warning-bg)";
        default:
          return "var(--bg-hover)";
      }
    }

    /** Foreground color for a stance chip. 偏多=RED(--up), 偏空=GREEN(--down). */
    export function stanceColor(stance: Stance): string {
      switch (stance) {
        case "bullish":
          return "var(--up)";
        case "bearish":
          return "var(--down)";
        default:
          return "var(--text-tertiary)";
      }
    }

    export function severityLabel(
      severity: Severity,
      t: (key: string) => string,
    ): string {
      switch (severity) {
        case "high":
          return t("invest.eventWatch.filterHigh");
        case "medium":
          return t("invest.eventWatch.filterMedium");
        case "low":
          return t("invest.eventWatch.filterLow");
        default:
          return severity;
      }
    }

    export function stanceLabel(
      stance: Stance,
      t: (key: string) => string,
    ): string {
      switch (stance) {
        case "bullish":
          return t("invest.eventWatch.stanceBullish");
        case "bearish":
          return t("invest.eventWatch.stanceBearish");
        case "neutral":
          return t("invest.eventWatch.stanceNeutral");
        default:
          return stance;
      }
    }

    /** Compact relative time — s / m / h / d. Falls back to the raw string on parse failure. */
    export function formatRelativeTime(iso: string): string {
      const then = new Date(iso).getTime();
      if (Number.isNaN(then)) return iso;
      const diffMs = Date.now() - then;
      if (diffMs < 0) return "0s";
      const sec = Math.floor(diffMs / 1000);
      if (sec < 60) return `${sec}s`;
      const min = Math.floor(sec / 60);
      if (min < 60) return `${min}m`;
      const hr = Math.floor(min / 60);
      if (hr < 24) return `${hr}h`;
      const day = Math.floor(hr / 24);
      return `${day}d`;
    }

    /** Split comma-separated sector string into a clean array. */
    export function splitCsv(input: string | null | undefined): string[] {
      if (!input) return [];
      return input
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s.length > 0);
    }
    ```

- [ ] **Create `NewsFlashColumn.svelte`** at `src/lib/components/invest/NewsFlashColumn.svelte`:
    ```svelte
    <script lang="ts">
      import { t } from "$lib/i18n/index.svelte";
      import { investStore } from "$lib/stores/invest-store.svelte";
      import type { InvestEvent } from "$lib/types";
      import {
        severityBadgeBg,
        stanceColor,
        severityLabel,
        stanceLabel,
        formatRelativeTime,
        splitCsv,
      } from "./news-helpers";

      type FlashFilter = "all" | "high" | "bull" | "bear";

      let { onTrigger }: { onTrigger: (event: InvestEvent) => void } = $props();

      let filter = $state<FlashFilter>("all");

      // Left column is jinshi_flash only — always source-scoped, then user filter on top.
      const flashEvents = $derived(
        investStore.events.filter((e) => e.source === "jinshi_flash"),
      );

      const filtered = $derived(
        flashEvents.filter((e) => {
          if (filter === "high") return e.severity === "high";
          if (filter === "bull") return e.stance === "bullish";
          if (filter === "bear") return e.stance === "bearish";
          return true;
        }),
      );

      const chips: { id: FlashFilter; labelKey: string }[] = [
        { id: "all", labelKey: "invest.news.filterAll" },
        { id: "high", labelKey: "invest.news.filterHighOnly" },
        { id: "bull", labelKey: "invest.news.filterBull" },
        { id: "bear", labelKey: "invest.news.filterBear" },
      ];
    </script>

    <section class="flash-col">
      <header class="col-header">
        <h3 class="col-title">{t("invest.news.flashTitle")}</h3>
        <div class="chips" role="tablist">
          {#each chips as chip}
            <button
              type="button"
              role="tab"
              aria-selected={filter === chip.id}
              class="chip"
              class:chip-active={filter === chip.id}
              onclick={() => (filter = chip.id)}
            >
              {t(chip.labelKey)}
            </button>
          {/each}
        </div>
      </header>

      <div class="scroll">
        {#if filtered.length === 0}
          <div class="empty">{t("invest.news.flashEmpty")}</div>
        {:else}
          <ul class="rows">
            {#each filtered as event (event.id)}
              {@const sectors = splitCsv(event.sectors)}
              <li class="row">
                <div class="row-head">
                  <span
                    class="sev-badge"
                    style="background: {severityBadgeBg(event.severity)};"
                  >
                    {severityLabel(event.severity, t)}
                  </span>
                  <span
                    class="stance"
                    style="color: {stanceColor(event.stance)};"
                  >
                    {stanceLabel(event.stance, t)}
                  </span>
                  <span class="ts">{formatRelativeTime(event.createdAt)}</span>
                </div>
                <div class="body">
                  {event.body ?? event.title}
                </div>
                {#if sectors.length > 0}
                  <div class="sectors">
                    {#each sectors as sec}
                      <span class="sector-chip">{sec}</span>
                    {/each}
                  </div>
                {/if}
                {#if event.severity === "high" && !event.triggered}
                  <div class="actions">
                    <button
                      type="button"
                      class="trigger-btn"
                      onclick={() => onTrigger(event)}
                    >
                      {t("invest.eventWatch.triggerCommittee")}
                    </button>
                  </div>
                {:else if event.triggered}
                  <div class="triggered-note">
                    {t("invest.eventWatch.triggered")}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </section>

    <style>
      .flash-col {
        display: flex;
        flex-direction: column;
        min-height: 0;
        background: var(--bg-input);
        border-radius: var(--radius-md);
      }
      .col-header {
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
        padding: var(--space-3) var(--space-4);
        border-bottom: 1px solid var(--bg-hover);
      }
      .col-title {
        font-size: 0.95rem;
        font-weight: 600;
        color: var(--text-primary);
        margin: 0;
      }
      .chips {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-2);
      }
      .chip {
        border: 1px solid transparent;
        background: var(--bg-hover);
        color: var(--text-secondary);
        padding: 2px var(--space-3);
        border-radius: var(--radius-full);
        font-size: 0.75rem;
        cursor: pointer;
        line-height: 1.4;
      }
      .chip:hover {
        color: var(--text-primary);
      }
      .chip-active {
        background: var(--accent);
        color: #fff;
      }
      .scroll {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        padding: var(--space-3) var(--space-4);
      }
      .rows {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
      }
      .row {
        padding: var(--space-3);
        background: var(--bg-hover);
        border-radius: var(--radius-md);
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
      }
      .row-head {
        display: flex;
        align-items: center;
        gap: var(--space-2);
        font-size: 0.72rem;
      }
      .sev-badge {
        padding: 1px var(--space-2);
        border-radius: var(--radius-full);
        color: var(--text-primary);
        font-weight: 600;
      }
      .stance {
        font-weight: 600;
      }
      .ts {
        margin-left: auto;
        color: var(--text-tertiary);
      }
      .body {
        font-size: 0.85rem;
        line-height: 1.5;
        color: var(--text-primary);
        white-space: pre-wrap;
        word-break: break-word;
      }
      .sectors {
        display: flex;
        flex-wrap: wrap;
        gap: 4px;
      }
      .sector-chip {
        font-size: 0.7rem;
        padding: 1px var(--space-2);
        border-radius: var(--radius-full);
        background: var(--bg-input);
        color: var(--text-secondary);
      }
      .actions {
        display: flex;
        justify-content: flex-end;
      }
      .trigger-btn {
        font-size: 0.75rem;
        padding: 2px var(--space-3);
        background: var(--accent);
        color: #fff;
        border: none;
        border-radius: var(--radius-full);
        cursor: pointer;
      }
      .trigger-btn:hover {
        filter: brightness(1.1);
      }
      .triggered-note {
        font-size: 0.72rem;
        color: var(--text-tertiary);
        text-align: right;
      }
      .empty {
        color: var(--text-tertiary);
        font-size: 0.85rem;
        text-align: center;
        padding: var(--space-6) 0;
      }
    </style>
    ```

- [ ] **Create `NewsDigestColumn.svelte`** at `src/lib/components/invest/NewsDigestColumn.svelte`:
    ```svelte
    <script lang="ts">
      import { t } from "$lib/i18n/index.svelte";
      import { investStore } from "$lib/stores/invest-store.svelte";
      import {
        stanceColor,
        stanceLabel,
        formatRelativeTime,
      } from "./news-helpers";

      type DigestFilter = "all" | "anns" | "stock" | "xueqiu";
      type DigestKind = "anns" | "stock" | "xueqiu";

      interface DigestRow {
        id: string;
        kind: DigestKind;
        title: string;
        summary: string | null;
        stance: string;
        symbol: string | null;
        ts: string;
      }

      let filter = $state<DigestFilter>("all");

      const annsRows = $derived<DigestRow[]>(
        investStore.events
          .filter((e) => e.source === "tushare_anns_d")
          .map((e) => ({
            id: `anns:${e.id}`,
            kind: "anns" as const,
            title: e.title,
            summary: e.body,
            stance: e.stance,
            symbol: e.symbols,
            ts: e.createdAt,
          })),
      );

      const stockRows = $derived<DigestRow[]>(
        investStore.events
          .filter((e) => e.source.startsWith("akshare:"))
          .map((e) => ({
            id: `stock:${e.id}`,
            kind: "stock" as const,
            title: e.title,
            summary: e.body,
            stance: e.stance,
            symbol: e.symbols,
            ts: e.createdAt,
          })),
      );

      const xueqiuRows = $derived<DigestRow[]>(
        investStore.sentimentItems.map((s) => ({
          id: `xueqiu:${s.id}`,
          kind: "xueqiu" as const,
          title: s.title,
          summary: s.summary,
          stance: s.stance,
          symbol: s.symbol ?? s.affectedSymbols,
          ts: s.publishedAt ?? s.createdAt,
        })),
      );

      const merged = $derived<DigestRow[]>(
        [...annsRows, ...stockRows, ...xueqiuRows].sort((a, b) =>
          a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0,
        ),
      );

      const filtered = $derived(
        merged.filter((r) => {
          if (filter === "anns") return r.kind === "anns";
          if (filter === "stock") return r.kind === "stock";
          if (filter === "xueqiu") return r.kind === "xueqiu";
          return true;
        }),
      );

      const chips: { id: DigestFilter; labelKey: string }[] = [
        { id: "all", labelKey: "invest.news.filterAll" },
        { id: "anns", labelKey: "invest.news.filterAnns" },
        { id: "stock", labelKey: "invest.news.filterStock" },
        { id: "xueqiu", labelKey: "invest.news.filterXueqiu" },
      ];

      function sourceTagLabel(kind: DigestKind): string {
        switch (kind) {
          case "anns":
            return t("invest.news.srcAnns");
          case "stock":
            return t("invest.news.srcStock");
          case "xueqiu":
            return t("invest.news.srcXueqiu");
        }
      }

      function sourceTagClass(kind: DigestKind): string {
        return `src-tag src-${kind}`;
      }
    </script>

    <section class="digest-col">
      <header class="col-header">
        <h3 class="col-title">{t("invest.news.digestTitle")}</h3>
        <div class="chips" role="tablist">
          {#each chips as chip}
            <button
              type="button"
              role="tab"
              aria-selected={filter === chip.id}
              class="chip"
              class:chip-active={filter === chip.id}
              onclick={() => (filter = chip.id)}
            >
              {t(chip.labelKey)}
            </button>
          {/each}
        </div>
      </header>

      <div class="scroll">
        {#if filtered.length === 0}
          <div class="empty">{t("invest.news.digestEmpty")}</div>
        {:else}
          <ul class="rows">
            {#each filtered as row (row.id)}
              <li class="row">
                <div class="row-head">
                  <span class={sourceTagClass(row.kind)}>
                    {sourceTagLabel(row.kind)}
                  </span>
                  <span
                    class="stance"
                    style="color: {stanceColor(row.stance)};"
                  >
                    {stanceLabel(row.stance, t)}
                  </span>
                  {#if row.symbol}
                    <span class="sym">{row.symbol}</span>
                  {/if}
                  <span class="ts">{formatRelativeTime(row.ts)}</span>
                </div>
                <div class="title">{row.title}</div>
                {#if row.summary}
                  <div class="summary">{row.summary}</div>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </section>

    <style>
      .digest-col {
        display: flex;
        flex-direction: column;
        min-height: 0;
        background: var(--bg-input);
        border-radius: var(--radius-md);
      }
      .col-header {
        display: flex;
        flex-direction: column;
        gap: var(--space-2);
        padding: var(--space-3) var(--space-4);
        border-bottom: 1px solid var(--bg-hover);
      }
      .col-title {
        font-size: 0.95rem;
        font-weight: 600;
        color: var(--text-primary);
        margin: 0;
      }
      .chips {
        display: flex;
        flex-wrap: wrap;
        gap: var(--space-2);
      }
      .chip {
        border: 1px solid transparent;
        background: var(--bg-hover);
        color: var(--text-secondary);
        padding: 2px var(--space-3);
        border-radius: var(--radius-full);
        font-size: 0.75rem;
        cursor: pointer;
        line-height: 1.4;
      }
      .chip:hover {
        color: var(--text-primary);
      }
      .chip-active {
        background: var(--accent);
        color: #fff;
      }
      .scroll {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        padding: var(--space-3) var(--space-4);
      }
      .rows {
        list-style: none;
        margin: 0;
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
      }
      .row {
        padding: var(--space-3);
        background: var(--bg-hover);
        border-radius: var(--radius-md);
        display: flex;
        flex-direction: column;
        gap: 4px;
      }
      .row-head {
        display: flex;
        align-items: center;
        gap: var(--space-2);
        font-size: 0.72rem;
      }
      .src-tag {
        padding: 1px var(--space-2);
        border-radius: var(--radius-full);
        font-weight: 600;
        font-size: 0.7rem;
      }
      .src-anns {
        background: var(--accent);
        color: #fff;
      }
      .src-stock {
        background: var(--bg-input);
        color: var(--text-secondary);
      }
      .src-xueqiu {
        background: #7c94a8;
        color: #fff;
      }
      .stance {
        font-weight: 600;
      }
      .sym {
        color: var(--text-secondary);
        font-family: var(--font-mono, monospace);
      }
      .ts {
        margin-left: auto;
        color: var(--text-tertiary);
      }
      .title {
        font-size: 0.85rem;
        line-height: 1.4;
        color: var(--text-primary);
        font-weight: 500;
        white-space: pre-wrap;
        word-break: break-word;
      }
      .summary {
        font-size: 0.78rem;
        line-height: 1.4;
        color: var(--text-tertiary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }
      .empty {
        color: var(--text-tertiary);
        font-size: 0.85rem;
        text-align: center;
        padding: var(--space-6) 0;
      }
    </style>
    ```

- [ ] **Full-rewrite `EventWatchTab.svelte`** at `src/lib/components/invest/EventWatchTab.svelte` — replace the existing 242-line implementation with the new container. Preserve the top status bar behavior (scan status counts + 立即扫描 button + error) and keep the trigger dialog + its callback wiring:
    ```svelte
    <script lang="ts">
      import { onMount } from "svelte";
      import { t } from "$lib/i18n/index.svelte";
      import { fmtRelative } from "$lib/i18n/format";
      import { investStore } from "$lib/stores/invest-store.svelte";
      import type { InvestEvent } from "$lib/types";
      import NewsFlashColumn from "./NewsFlashColumn.svelte";
      import NewsDigestColumn from "./NewsDigestColumn.svelte";
      import EventTriggerDialog from "./EventTriggerDialog.svelte";

      let { onNavigateToCommittee }: { onNavigateToCommittee?: () => void } =
        $props();

      let triggerTarget = $state<InvestEvent | null>(null);

      onMount(() => {
        investStore.fetchEvents();
        investStore.fetchScanStatus();
        investStore.fetchSentimentItems();
      });

      // `triggerScan()` is the existing store method; it internally re-fetches events
      // + scan status. We additionally refresh sentiment items so the right column updates.
      async function runScan(): Promise<void> {
        await investStore.triggerScan();
        await investStore.fetchSentimentItems();
      }

      function openTrigger(event: InvestEvent): void {
        triggerTarget = event;
      }

      function closeTrigger(): void {
        triggerTarget = null;
      }

      function onTriggerSuccess(): void {
        triggerTarget = null;
        onNavigateToCommittee?.();
      }
    </script>

    <div class="news-tab">
      <header class="status-bar">
        <div class="status-summary">
          {#if investStore.scanStatus}
            <span class="stat">
              <span class="stat-label">{t("invest.eventWatch.events")}</span>
              <span class="stat-value">{investStore.scanStatus.totalEvents}</span>
            </span>
            <span class="stat">
              <span class="stat-label">{t("invest.eventWatch.high")}</span>
              <span class="stat-value">{investStore.scanStatus.highCount}</span>
            </span>
            <span class="stat">
              <span class="stat-label">{t("invest.eventWatch.untriggered")}</span>
              <span class="stat-value"
                >{investStore.scanStatus.untriggeredHigh}</span
              >
            </span>
            {#if investStore.scanStatus.lastEventAt}
              <span class="stat stat-muted">
                {t("invest.eventWatch.last")}
                {fmtRelative(investStore.scanStatus.lastEventAt)}
              </span>
            {/if}
          {:else}
            <span class="stat stat-muted"
              >{t("invest.eventWatch.noScanData")}</span
            >
          {/if}
        </div>
        <div class="status-actions">
          <button
            type="button"
            class="scan-btn"
            disabled={investStore.isScanning}
            onclick={runScan}
          >
            {investStore.isScanning
              ? t("invest.eventWatch.scanning")
              : t("invest.eventWatch.scanNow")}
          </button>
        </div>
      </header>

      <div class="two-col">
        <NewsFlashColumn onTrigger={openTrigger} />
        <NewsDigestColumn />
      </div>

      {#if triggerTarget}
        <EventTriggerDialog
          event={triggerTarget}
          onClose={closeTrigger}
          onTriggered={onTriggerSuccess}
        />
      {/if}
    </div>

    <style>
      .news-tab {
        display: flex;
        flex-direction: column;
        gap: var(--space-3);
        height: 100%;
        min-height: 0;
        padding: var(--space-3) var(--space-4);
      }
      .status-bar {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--space-3);
        padding: var(--space-2) var(--space-3);
        background: var(--bg-input);
        border-radius: var(--radius-md);
        flex-wrap: wrap;
      }
      .status-summary {
        display: flex;
        gap: var(--space-4);
        flex-wrap: wrap;
        align-items: center;
      }
      .stat {
        display: inline-flex;
        gap: 4px;
        font-size: 0.8rem;
        align-items: baseline;
      }
      .stat-label {
        color: var(--text-tertiary);
      }
      .stat-value {
        color: var(--text-primary);
        font-weight: 600;
      }
      .stat-muted {
        color: var(--text-tertiary);
      }
      .scan-btn {
        font-size: 0.8rem;
        padding: 4px var(--space-3);
        background: var(--accent);
        color: #fff;
        border: none;
        border-radius: var(--radius-full);
        cursor: pointer;
      }
      .scan-btn:disabled {
        opacity: 0.6;
        cursor: not-allowed;
      }
      .scan-btn:not(:disabled):hover {
        filter: brightness(1.1);
      }
      .two-col {
        flex: 1 1 auto;
        min-height: 0;
        display: grid;
        grid-template-columns: 1fr 1fr;
        gap: var(--space-3);
      }
      /* Below 900px viewport width, the columns stack and each keeps its own scroll region. */
      @media (max-width: 900px) {
        .two-col {
          grid-template-columns: 1fr;
          grid-auto-rows: minmax(0, 1fr);
        }
      }
    </style>
    ```
    **Notes on preserved behavior**
    - The scan status field names are the VERIFIED `ScanStatus` shape (`src/lib/types.ts:1903-1908`): `totalEvents`, `highCount`, `untriggeredHigh`, `lastEventAt` (all non-optional; `lastEventAt: string | null`). Do not use `total`/`high`/`untriggered`/`lastScanAt` — those do not exist.
    - `investStore.triggerScan()` (VERIFIED at `invest-store.svelte.ts:616`) is the existing scan method — it internally refreshes events + scan status. Do not invent `runEventScan`.
    - `t` is imported from `$lib/i18n/index.svelte` and `fmtRelative` from `$lib/i18n/format` — the exact paths the old `EventWatchTab` used (verified). Do not import from a bare `$lib/i18n`.
    - `EventTriggerDialog` (`src/lib/components/invest/EventTriggerDialog.svelte`) has the VERIFIED prop contract `{ event, onClose, onTriggered }` (`$props()` at its line 8-12). Use `onTriggered`, NOT `onSuccess`.

- [ ] **Add the `invest.news.*` content keys to `messages/zh-CN.json`**. Append this block inside the top-level object (alongside the existing `invest.eventWatch.*` block near lines 882-909 — keep JSON valid, don't drop the closing brace/comma of the neighbor):
    ```json
      "invest.news.flashTitle": "金十快讯",
      "invest.news.digestTitle": "新闻/舆论",
      "invest.news.filterAll": "全部",
      "invest.news.filterHighOnly": "仅高",
      "invest.news.filterBull": "偏多",
      "invest.news.filterBear": "偏空",
      "invest.news.filterAnns": "公告",
      "invest.news.filterStock": "个股新闻",
      "invest.news.filterXueqiu": "雪球舆情",
      "invest.news.srcAnns": "东财公告",
      "invest.news.srcStock": "个股",
      "invest.news.srcXueqiu": "雪球",
      "invest.news.flashEmpty": "暂无快讯",
      "invest.news.digestEmpty": "暂无新闻/舆情",
    ```

- [ ] **Mirror the block in `messages/en.json`** at the equivalent position:
    ```json
      "invest.news.flashTitle": "Jinshi Flash",
      "invest.news.digestTitle": "News / Opinion",
      "invest.news.filterAll": "All",
      "invest.news.filterHighOnly": "High only",
      "invest.news.filterBull": "Bullish",
      "invest.news.filterBear": "Bearish",
      "invest.news.filterAnns": "Announcements",
      "invest.news.filterStock": "Stock news",
      "invest.news.filterXueqiu": "Xueqiu sentiment",
      "invest.news.srcAnns": "DFCF ann.",
      "invest.news.srcStock": "Stock",
      "invest.news.srcXueqiu": "Xueqiu",
      "invest.news.flashEmpty": "No flash items yet",
      "invest.news.digestEmpty": "No news / sentiment items yet",
    ```

- [ ] **Type-check, i18n-check, and build**:
    ```bash
    npm run check
    npm run i18n:check
    npm run build
    ```
    Expected — all three pass. `i18n:check` should still report en/zh in sync (both got the same 14 new keys). If `npm run build` fails, fix before proceeding — an untyped prop or missing import in one of the three new/rewritten Svelte files is the most likely culprit.

- [ ] **Manual visual verification** (dev app):
    ```bash
    npm run tauri dev
    ```
    - Navigate `/invest` → 委员会 → 新闻/舆论. Confirm two side-by-side columns, each with its own header + chip row + independently scrolling body.
    - Left column: only `jinshi_flash` events; chips 全部/仅高/偏多/偏空 filter without touching the right column. 偏多 rows show stance text in RED (`--up`); 偏空 rows show it in GREEN (`--down`).
    - Right column: 公告 rows show a **gold** `东财公告` tag, 个股 rows a **grey** `个股` tag, 雪球 rows a **cyan (#7c94a8)** `雪球` tag. Chips 全部/公告/个股新闻/雪球舆情 filter without touching the left column.
    - Resize the window below 900px width: layout stacks into two rows, and each row still scrolls independently.
    - Click 立即扫描 in the top status bar: scan status counts refresh, and both columns pick up new items after the scan (the container calls `fetchEvents` + `fetchSentimentItems`).
    - On a `high` `jinshi_flash` row with `triggered=false`, click 触发委员会 → dialog opens; on successful trigger the dialog closes and the sub-tab switches to 委员会/进行中 via `onNavigateToCommittee`.

- [ ] **Commit**:
    ```bash
    git add \
      src/lib/components/invest/news-helpers.ts \
      src/lib/components/invest/NewsFlashColumn.svelte \
      src/lib/components/invest/NewsDigestColumn.svelte \
      src/lib/components/invest/EventWatchTab.svelte \
      messages/zh-CN.json \
      messages/en.json
    git commit -m "feat(invest): 新闻/舆论双列视图 (金十快讯 + 新闻/舆情, ≤900px 堆叠)"
    ```
