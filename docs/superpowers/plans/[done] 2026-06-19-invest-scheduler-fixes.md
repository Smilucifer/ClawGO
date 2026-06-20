# 投资委员会定时任务修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 invest scheduler 的 cron 解析根因 bug、双重调度、漏执行与防御性缺陷，并清理已污染的空转快照数据。

**Architecture:** 调度器由 `src-tauri/src/invest/scheduler/{config,runner,mod}.rs` 三个文件构成（config 负责默认任务+磁盘覆盖+next_run 计算，runner 负责主循环+dedicated 循环+dispatch，mod 定义 CronJob+default_jobs）。本计划集中修这三个文件，外加删除 `lib.rs` 里两段与 scheduler 重复的独立循环，并在 `storage/invest/dream_snapshots.rs` 加一个保留式清理函数。所有改动 Windows-first，不引入新依赖（`once_cell`、`tokio-util`、`cron` 均已在 Cargo.toml）。

**Tech Stack:** Rust + Tauri + rusqlite（SQLite）+ `cron` crate（要求 6 字段表达式）+ `chrono` + `tokio` + `tokio_util::sync::CancellationToken`。

## Global Constraints

- **Rust 单测运行时崩溃（本机）：** CLAUDE.md §11 — 本机 `cargo test` 会 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`。验证以 `cargo check --manifest-path src-tauri/Cargo.toml` 和 `cargo clippy ... -- -D warnings` 为准；纯逻辑单测照常写入（供 CI/干净环境运行），但本机不强求 `cargo test` 通过。
- **不新增依赖：** 仅用 Cargo.toml 已声明的 crate。
- **时区语义：** A 股相关时间一律按北京时间（UTC+8）。现有 `is_a_share_market_open()`（`storage/invest/scheduler.rs:110`）用 `Utc::now() + 8h` 是正确范式，需对齐。
- **Conventional Commits：** 每个 commit 用 `fix:` / `chore:` / `refactor:` 前缀。
- **i18n：** 本计划不涉及新 UI 文案；若某步新增面向用户的字符串，需同时改 `messages/en.json` 与 `zh-CN.json`。
- **cron 字段数：** `cron` crate 要求 6 字段（秒 分 时 日 月 周）。`normalize_cron_6field`（config.rs:163）把 5 字段补 `0 ` 前缀。

## 背景：已验证的根因（来自源码 + 运行数据 invest.db 6.1 万行）

- **根因 #13：** `config.rs::load_jobs_base()`（行 58-80）读取磁盘覆盖时**不调用** `normalize_cron_6field`（归一化只在写入路径 `update_cron`/`save_dream_config` 做）。任何被持久化成 5 字段的 cron（当前 `scheduler.json` 里 `dream_invest` = `"0 3 * * *"`）→ `compute_next_run_for_job`（行 106）`Schedule::from_str` 失败 → 返回 `None` → 主循环 `runner.rs:169-174` 的 `None => true` → 每个 tick（~60s）触发。实测 dream_invest 12377 条日志、9521 条空转快照。
- **根因 #14：** `requires_trading_day` 任务非交易日被 skip（`runner.rs:176-185`）时**不更新 next_run**，下一 tick 继续判定应触发；与 #13 叠加表现为每分钟刷 `skipped` 日志。
- **#1/#2 双重调度：** `lib.rs:686-749`（pnl_snapshot 独立循环）与 `lib.rs:755 spawn_event_scanner_cron()`（event_scan 独立循环）和 scheduler 的同名任务并存，导致重复执行 + status 值不一致（`success` vs `ok`）。
- **#3/#4/#8 防御性：** 主循环串行 `await` 慢任务阻塞其它任务；spawn task 无 panic 兜底、不接 CancellationToken、`RUNNING` 永不复位；手动 `trigger_cron_job` 与主循环可并发同一任务。
- **#7：** `save_jobs`（config.rs:147）用 `std::fs::write` 非原子覆盖且无锁，lastRun 持久化已被观测到失效。
- **#5/#6：** 交易日在日历缺失时回退 `is_weekday`（节假日误跑）；`lib.rs` 的"Beijing time"注释实际用 `chrono::Local`。
- **数据清理：** `dream_snapshots` 9521 条全 `invest` 类型、绝大多数空转。需在止血后保留式清理。

---

## 任务索引（按依赖与优先级排序）

1. **Task 1** — 修 cron 读取归一化（根因 #13）：`load_jobs_base` 读取时归一化 + `compute_next_run_for_job` 容错日志
2. **Task 2** — 修 skip 不推进 next_run（#14）：skip 分支回写 next_run
3. **Task 3** — 删除 `lib.rs` pnl_snapshot 独立循环（#1），修正 scheduler pnl cron 为 4 时刻
4. **Task 4** — 删除 `lib.rs` event_scan 独立循环（#2）
5. **Task 5** — `save_jobs` 原子写 + load/save 串行化锁（#7）
6. **Task 6** — 主循环防御：per-job 互斥 + panic 兜底 + 接 CancellationToken（#3/#4/#8）
7. **Task 7** — 交易日判定收紧 + 时区对齐北京时间（#5/#6）
8. **Task 8** — `dream_snapshots` 保留式清理函数 + 启动时一次性清理（数据治理）
9. **Task 9** — scheduler_logs 保留清理（#11，卫生）

> 严格 TDD：先写失败测试再实现。本机用 `cargo check` 代替 `cargo test` 验证编译（见 Global Constraints）。
> 各任务详情见下方分节（占位，后续逐段填充）。

---

### Task 1: cron 读取归一化（根因 #13）

**Goal**: `load_jobs_base()` 在把磁盘 override 的 5 字段 cron 写入 `job.cron_expr` 时漏掉了 `normalize_cron_6field`，导致 `compute_next_run_for_job` 里 `Schedule::from_str` 失败、`.ok()?` 静默返回 `None`，job 永远没有 `next_run`，runner 又把 `None` 当作"该触发"，每分钟循环空转。本任务把归一化下沉到读路径，并给 `compute_next_run_for_job` 加 `log::warn!` 防御静默失败。

**Files**:
- `src-tauri/src/invest/scheduler/config.rs`

**Interfaces**:
- Consumes: `super::CronJob`（`scheduler/mod.rs` 已有 struct，不改字段）、`super::default_jobs`、磁盘文件 `~/.claw-go/invest/scheduler.json`、`cron::Schedule`、`log::warn!`
- Produces: 修改后的 `pub(crate) fn load_jobs_base() -> Vec<CronJob>`、修改后的 `pub fn compute_next_run_for_job(job: &CronJob) -> Option<String>`、新增 `#[cfg(test)] mod tests` 单测块

**Semantics note**: `normalize_cron_6field` 仍是私有 fn，仅在读 / 写路径（`load_jobs_base`、`update_cron`、`save_dream_config`）内部使用；外部无 API 变化。`compute_next_run_for_job` 行为不变（仍然返回 `Option<String>`），只是解析失败时多一条 warn 日志，便于将来再出 bug 时不至于完全静默。

- [ ] **Step 1: 在 `config.rs` 末尾新增 `#[cfg(test)] mod tests` 失败测试块**

  在文件末尾（第 273 行 `}` 之后）追加：

  ```rust

  #[cfg(test)]
  mod tests {
      use super::*;

      fn make_job(cron_expr: impl Into<String>) -> CronJob {
          CronJob {
              id: "test_job".into(),
              name: "Test Job".into(),
              cron_expr: cron_expr.into(),
              interval_min: None,
              enabled: true,
              requires_trading_day: false,
              last_run: None,
              next_run: None,
              last_status: None,
              description: String::new(),
              dedicated: false,
          }
      }

      #[test]
      fn normalize_5field_prepends_seconds() {
          assert_eq!(normalize_cron_6field("0 3 * * *"), "0 0 3 * * *");
          assert_eq!(normalize_cron_6field("*/15 * * * *"), "0 */15 * * * *");
      }

      #[test]
      fn normalize_6field_unchanged() {
          assert_eq!(normalize_cron_6field("0 0 3 * * *"), "0 0 3 * * *");
          assert_eq!(
              normalize_cron_6field("0 30 9,11 * * 1-5"),
              "0 30 9,11 * * 1-5"
          );
      }

      #[test]
      fn compute_next_run_returns_none_for_unnormalized_5field_cron() {
          // Documents the bug: a raw 5-field cron is unparseable by the
          // `cron` crate, so without normalization on load we silently get None.
          let job = make_job("0 3 * * *");
          assert!(compute_next_run_for_job(&job).is_none());
      }

      #[test]
      fn compute_next_run_returns_some_for_5field_cron_after_normalize() {
          // Simulates what load_jobs_base now writes when a user override
          // contains a 5-field cron string.
          let mut job = make_job("");
          job.cron_expr = normalize_cron_6field("0 3 * * *");
          assert_eq!(job.cron_expr, "0 0 3 * * *");
          assert!(
              compute_next_run_for_job(&job).is_some(),
              "expected Some next_run for normalized cron '{}'",
              job.cron_expr
          );
      }

      #[test]
      fn compute_next_run_some_for_6field_cron() {
          let job = make_job("0 30 9,11 * * 1-5");
          assert!(compute_next_run_for_job(&job).is_some());
      }
  }
  ```

- [ ] **Step 2: 编译失败测试块（运行时崩溃，本机不跑测试）**

  本机 Rust 单测运行时会 panic 于 `STATUS_ENTRYPOINT_NOT_FOUND` (CLAUDE.md §11)，因此 TDD 的"运行确认失败"环节降级为"编译通过 + 源码静态确认现状"。

  命令：
  ```
  cargo check --manifest-path src-tauri/Cargo.toml --tests
  ```
  期望输出：以 `Finished ... profile [unoptimized + debuginfo] target(s) in ...s` 结尾，无 `error[E...]`。

  静态确认现状：第 60-62 行 `if let Some(c) = ov.cron_expr { job.cron_expr = c; }` 把 5 字段字符串原样赋值；第 106 行 `Schedule::from_str(&job.cron_expr).ok()?` 对 5 字段返回 `Err`，被 `.ok()?` 吞成 `None`。

- [ ] **Step 3: 修复 `load_jobs_base` 在写入 cron_expr 时归一化**

  把 `config.rs` 第 60-62 行：
  ```rust
              if let Some(c) = ov.cron_expr {
                  job.cron_expr = c;
              }
  ```
  替换为：
  ```rust
              if let Some(c) = ov.cron_expr {
                  job.cron_expr = normalize_cron_6field(&c);
              }
  ```

  其它分支（interval_min / enabled / requires_trading_day / last_run / last_status）保持不变。

- [ ] **Step 4: 给 `compute_next_run_for_job` 的 cron 解析失败加 `log::warn!`**

  把 `config.rs` 第 106 行：
  ```rust
      let schedule = Schedule::from_str(&job.cron_expr).ok()?;
  ```
  替换为：
  ```rust
      let schedule = match Schedule::from_str(&job.cron_expr) {
          Ok(s) => s,
          Err(e) => {
              log::warn!(
                  "compute_next_run_for_job: failed to parse cron '{}' for job '{}': {e}",
                  job.cron_expr,
                  job.id
              );
              return None;
          }
      };
  ```

  `log` crate 已在 `runner.rs` 等处使用，`log::warn!` 为完全限定调用，无需在 `config.rs` 顶部新增 `use`。

- [ ] **Step 5: 编译 + clippy 校验通过**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml --tests
  cargo clippy --manifest-path src-tauri/Cargo.toml --tests -- -D warnings
  ```
  期望两者均以 `Finished ...` 结尾，无 `error` 或 `warning` 输出。

- [ ] **Step 6: 提交**

  ```bash
  git add src-tauri/src/invest/scheduler/config.rs
  git commit -m "fix(invest/scheduler): normalize 5-field cron on disk override load"
  ```

---

### Task 2: skip 不推进 next_run（#14）

**Goal**: 主循环里 `requires_trading_day=true` 的 job 在非交易日只写 `skipped` 日志却没有更新 `next_run`，下一 tick 仍然判定 due，整个非交易日每 60 秒刷一条 skipped 日志。修复：(1) 把 due 判断从 `.filter()` 闭包里拆出来做成可复用的纯函数 `should_fire(job, now)`，(2) skip 分支里同时调用 `compute_next_run_for_job` + 标 dirty，循环末尾一次性 `save_jobs`，避免 filter 闭包里塞持久化副作用。

**Files**:
- `src-tauri/src/invest/scheduler/config.rs`（新增 `pub(crate) fn should_fire` + 测试）
- `src-tauri/src/invest/scheduler/runner.rs`（重写 `to_fire` 收集块，约第 159-187 行）

**Interfaces**:
- Consumes: `super::CronJob`、`config::compute_next_run_for_job`、`config::save_jobs`、`is_trading_day`、`log_task_start` / `log_task_end`、`chrono::NaiveDateTime` / `chrono::Local`
- Produces: 新公开 `pub(crate) fn should_fire(job: &CronJob, now: chrono::NaiveDateTime) -> bool`、runner 主循环 `to_fire` 收集块的非副作用化重写

**None 语义最终决议**:
- 首次 `next_run == None`（job 创建后还没被调度过）→ `should_fire` 返回 `true`，允许触发一次。
- 一旦触发或被 skip → 必写回 `next_run = compute_next_run_for_job(job)`，杜绝"持续 None"循环。
- `next_run` 字符串无法解析（损坏数据）→ `should_fire` 返回 `true`（与原逻辑 `unwrap_or(true)` 一致），下一次触发或 skip 会重新计算并写回正常值，自愈。
- `enabled=false` 或 `dedicated=true` → `should_fire` 永远返回 `false`，由专用循环处理。

- [ ] **Step 1: 在 `config.rs` 新增 `should_fire` 纯函数**

  在 `config.rs` 第 109 行（`compute_next_run_for_job` 闭合的 `}` 之后、第 111 行 `pub fn save_jobs` 之前）插入：

  ```rust

  /// Pure predicate: should the main scheduler loop fire `job` at `now`?
  ///
  /// Semantics:
  /// - `enabled == false` or `dedicated == true` → never fire from the main loop.
  /// - `next_run == None` (never scheduled) → fire once; the caller MUST write
  ///   back `next_run` after firing or skipping so subsequent ticks fall into
  ///   the parsed branch.
  /// - `next_run == Some(parseable)` → fire when `now >= parsed`.
  /// - `next_run == Some(garbage)` → fire (self-heals on next write-back).
  pub(crate) fn should_fire(job: &CronJob, now: chrono::NaiveDateTime) -> bool {
      if !job.enabled || job.dedicated {
          return false;
      }
      match &job.next_run {
          Some(next) => chrono::NaiveDateTime::parse_from_str(next, "%Y-%m-%dT%H:%M:%S")
              .map(|dt| now >= dt)
              .unwrap_or(true),
          None => true,
      }
  }
  ```

- [ ] **Step 2: 在 Task 1 已有的 `mod tests` 末尾追加 `should_fire` 单测**

  在 `compute_next_run_some_for_6field_cron` 测试之后、`mod tests` 闭合 `}` 之前追加：

  ```rust

      fn at(s: &str) -> chrono::NaiveDateTime {
          chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").unwrap()
      }

      #[test]
      fn should_fire_disabled_returns_false() {
          let mut job = make_job("0 0 3 * * *");
          job.enabled = false;
          job.next_run = None;
          assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
      }

      #[test]
      fn should_fire_dedicated_returns_false() {
          let mut job = make_job("0 0 3 * * *");
          job.dedicated = true;
          job.next_run = None;
          assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
      }

      #[test]
      fn should_fire_none_next_run_returns_true() {
          let job = make_job("0 0 3 * * *");
          assert!(should_fire(&job, at("2026-06-19T12:00:00")));
      }

      #[test]
      fn should_fire_past_next_run_returns_true() {
          let mut job = make_job("0 0 3 * * *");
          job.next_run = Some("2000-01-01T00:00:00".into());
          assert!(should_fire(&job, at("2026-06-19T12:00:00")));
      }

      #[test]
      fn should_fire_future_next_run_returns_false() {
          let mut job = make_job("0 0 3 * * *");
          job.next_run = Some("2099-01-01T00:00:00".into());
          assert!(!should_fire(&job, at("2026-06-19T12:00:00")));
      }

      #[test]
      fn should_fire_unparseable_next_run_returns_true() {
          let mut job = make_job("0 0 3 * * *");
          job.next_run = Some("not-a-timestamp".into());
          assert!(should_fire(&job, at("2026-06-19T12:00:00")));
      }
  ```

  命令：
  ```
  cargo check --manifest-path src-tauri/Cargo.toml --tests
  ```
  期望以 `Finished ...` 结尾，无 `error`、无 `warning`。

- [ ] **Step 3: 重写 `runner.rs` 的 `to_fire` 收集块**

  把 `runner.rs` 第 159-187 行（含 `loop {` 到 `.collect();` 这一整段，**不**含其后第 189 行起的 `for job_id in to_fire` 循环）：

  ```rust
          loop {
              let mut jobs = config::load_jobs();
              let today = crate::invest::date_utils::get_invest_date();

              // Collect IDs of enabled, non-dedicated jobs that should fire
              let to_fire: Vec<String> = jobs
                  .iter()
                  .filter(|j| j.enabled)
                  .filter(|j| !j.dedicated)
                  .filter(|j| {
                      match &j.next_run {
                          Some(next) => chrono::NaiveDateTime::parse_from_str(next, "%Y-%m-%dT%H:%M:%S")
                              .map(|dt| chrono::Local::now().naive_local() >= dt)
                              .unwrap_or(true),
                          None => true,
                      }
                  })
                  .filter(|j| {
                      if j.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                          if let Ok(id) = log_task_start(&j.id) {
                              let _ = log_task_end(id, "skipped", Some("non-trading day"));
                          }
                          false
                      } else {
                          true
                      }
                  })
                  .map(|j| j.id.clone())
                  .collect();
  ```

  替换为：

  ```rust
          loop {
              let mut jobs = config::load_jobs();
              let today = crate::invest::date_utils::get_invest_date();
              let now_naive = chrono::Local::now().naive_local();

              // First pass: pure due-check + non-trading-day skip handling.
              // Skip side-effects (log + advance next_run) live here, NOT inside
              // a .filter() closure, so we can persist once at the end and so
              // the predicate stays a pure function (`config::should_fire`).
              let mut to_fire: Vec<String> = Vec::new();
              let mut dirty = false;
              for job in jobs.iter_mut() {
                  if !config::should_fire(job, now_naive) {
                      continue;
                  }
                  if job.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                      if let Ok(log_id) = log_task_start(&job.id) {
                          let _ = log_task_end(log_id, "skipped", Some("non-trading day"));
                      }
                      // BUG #14 fix: advance next_run so we don't re-skip every
                      // tick for the rest of the non-trading day.
                      job.next_run = config::compute_next_run_for_job(job);
                      dirty = true;
                      continue;
                  }
                  to_fire.push(job.id.clone());
              }

              if dirty {
                  if let Err(e) = config::save_jobs(&jobs) {
                      log::error!("Failed to persist skipped-job next_run: {e}");
                  }
              }
  ```

  其后第 189-204 行 `for job_id in to_fire { ... }` 火焰循环及其内部的 `last_run` / `next_run` 写回 + `save_jobs` 保持不变；第 206 行 `sleep(Duration::from_secs(60)).await;` 保持不变。

- [ ] **Step 4: 编译 + clippy 校验通过**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml --tests
  cargo clippy --manifest-path src-tauri/Cargo.toml --tests -- -D warnings
  ```
  期望两者均以 `Finished ...` 结尾，无 `error` 或 `warning`。

- [ ] **Step 5: 代码审查清单（人工，无单测覆盖主循环）**

  1. `jobs.iter_mut()` 借用在 for 循环结束后释放，第 197 行 `jobs.iter_mut().find(|j| j.id == job_id)` 不会和上面的可变借用冲突。
  2. `dirty` 仅由 skip 分支置 `true`；fire 分支已有自己的 `save_jobs`（runner.rs 第 200 行附近），两次写都是先序列化全量再原子覆盖文件。
  3. skip 分支调用 `compute_next_run_for_job(job)` 时 `job.enabled` 仍为 `true`，函数会走 cron 分支；非交易日下次 cron 时间落到下一天对应时刻，下一 tick `should_fire` 立刻返回 `false`，每天最多再触发一次 skip 日志。
  4. 当 `compute_next_run_for_job` 因 cron 不可解析返回 `None`，下一 tick 仍可能再 skip——这是已接受的退化路径；正常 6 字段 cron 不会触发，且 Task 1 的 warn 日志会暴露根因。

- [ ] **Step 6: 提交**

  ```bash
  git add src-tauri/src/invest/scheduler/config.rs src-tauri/src/invest/scheduler/runner.rs
  git commit -m "fix(invest/scheduler): advance next_run when skipping non-trading-day jobs"
  ```

---

### Task 3: 删除 lib.rs pnl_snapshot 独立循环 + 修正 scheduler pnl cron（#1）

**Files**:
- Modify: `src-tauri/src/lib.rs`（删除行 684-750 的硬编码 pnl 调度块）
- Modify: `src-tauri/src/invest/scheduler/mod.rs`（修改 default_jobs 中 pnl_snapshot 的 cron_expr）

**Interfaces**:
- Consumes: `crate::run_pnl_snapshot()` — 仍由 `dispatch_job("pnl_snapshot")` 调用，保留 lib.rs 行 75-139；`invest::scheduler::runner::start(...)`（lib.rs:556-558）保留为唯一驱动。
- Produces: `pnl_snapshot.cron_expr` 默认值变更为 `"0 30 9,11,13,15 * * 1-5"`（覆盖每个交易日 9:30 / 11:30 / 13:30 / 15:30 京时，与原硬编码 4 时刻语义对齐）；`scheduler_logs` 表 `pnl_snapshot` 的 status 只剩 `"ok"` / `"error"`，不再出现 `"success"`（lib.rs 副本独有的字符串）。

- [ ] **Step 1: 修改 default_jobs pnl cron**

  `src-tauri/src/invest/scheduler/mod.rs` 中 `default_jobs()` 的 pnl_snapshot 条目，把 `cron_expr: "0 30 9,11 * * 1-5".into()` 改为 `cron_expr: "0 30 9,11,13,15 * * 1-5".into()`。其它字段保持不变。

- [ ] **Step 2: 删除 lib.rs pnl 独立循环**

  删除 `src-tauri/src/lib.rs` 中「Start PnL snapshot cron job.」注释及其后的 `{ tauri::async_runtime::spawn(async { ... }) }` 块（当前位于行 684-750，含 `target_times: [(u32, u32); 4]`、`run_pnl_snapshot().await`、`log_task_start("pnl_snapshot")`、`log_task_end(.., "success", ..)`）。

- [ ] **Step 3: 保留依赖**

  不要触碰行 75-139 的 `pub async fn run_pnl_snapshot()`，也不要触碰行 556-558 的 `invest::scheduler::runner::start(...)`。

- [ ] **Step 4: 新增 default_jobs 单测**

  在 `src-tauri/src/invest/scheduler/mod.rs` 的 `#[cfg(test)] mod tests` 末尾（若不存在则新建）追加：
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn pnl_snapshot_default_cron_covers_four_intraday_slots() {
          let jobs = default_jobs();
          let pnl = jobs.iter().find(|j| j.id == "pnl_snapshot").expect("pnl_snapshot job present");
          assert_eq!(pnl.cron_expr, "0 30 9,11,13,15 * * 1-5");
          assert!(pnl.enabled);
          assert!(pnl.requires_trading_day);
          assert!(!pnl.dedicated);
      }
  }
  ```

- [ ] **Step 5: 验证（命令式）**

  - `grep -n "Beijing time" src-tauri/src/lib.rs` —— 应无输出。
  - `grep -n "target_times" src-tauri/src/lib.rs` —— 应无输出。
  - `grep -n 'log_task_end.*"success"' src-tauri/src/lib.rs` —— 应无输出。
  - `grep -rn "0 30 9,11 \* \* 1-5" src-tauri/src` —— 应无输出（旧 cron 已不存在）。
  - `cargo check --manifest-path src-tauri/Cargo.toml --tests` —— 通过。
  - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` —— 通过。

- [ ] **Step 6: 提交**

  ```bash
  git add src-tauri/src/lib.rs src-tauri/src/invest/scheduler/mod.rs
  git commit -m "fix(invest): drop duplicated lib.rs pnl_snapshot loop, align scheduler cron to 4 intraday slots"
  ```

---

### Task 4: 删除 lib.rs event_scan 独立循环（#2）

**Files**:
- Modify: `src-tauri/src/lib.rs`（删除 `spawn_event_scanner_cron`、`run_event_scan_once`，及调用点）

**Interfaces**:
- Consumes: `crate::commands::invest::build_scan_clients()` — runner.rs:19 dispatch_job("event_scan") 仍调用，**保留勿删**；`crate::invest::event_scanner::{scan_events, ScanResult, DEFAULT_LANGUAGE}` 保留；`default_jobs()` 的 `event_scan` 条目（cron `"0 */30 8-22 * * 1-5"`，requires_trading_day:false）保持不变。
- Produces: `lib.rs` 不再出现 `spawn_event_scanner_cron` 与 `run_event_scan_once` 符号；event_scan 改由 scheduler 主循环独家驱动。

- [ ] **Step 1: 删除两个函数 + 调用点**

  在 `src-tauri/src/lib.rs` 删除：
  - 行 141-168：`fn spawn_event_scanner_cron()` 整个函数（含上方 `/// Spawn the event scanner background cron job.` doc comment）。
  - 行 170-175：`async fn run_event_scan_once()` 整个函数（含上方 `/// Run a single event scan: ...` doc comment）。
  - 行 752-755 的注释块 `// Start event scanner cron job. ...` 与紧随的 `spawn_event_scanner_cron();` 调用，整段删除。

- [ ] **Step 2: 确认 build_scan_clients 保留**

  不要删除或修改 `crate::commands::invest::build_scan_clients()`。`grep -rn "build_scan_clients" src-tauri/src` 复核 dispatch_job 路径仍引用。

- [ ] **Step 3: 验证**

  - `grep -n "spawn_event_scanner_cron\|run_event_scan_once" src-tauri/src/lib.rs` —— 应无输出。
  - `grep -rn "spawn_event_scanner_cron\|run_event_scan_once" src-tauri/src` —— 应无输出。
  - `grep -n "build_scan_clients" src-tauri/src/invest/scheduler/runner.rs` —— 应仍命中至少 1 行。
  - `cargo check --manifest-path src-tauri/Cargo.toml` —— 通过（无 unused 警告）。
  - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` —— 通过。

- [ ] **Step 4: 提交**

  ```bash
  git add src-tauri/src/lib.rs
  git commit -m "refactor(invest): drop duplicate event_scan loop in lib.rs, scheduler is now sole driver"
  ```

---

### Task 5: save_jobs 原子写 + 串行化锁（#7）

**Files**:
- Modify: `src-tauri/src/invest/scheduler/config.rs`（改写 `save_jobs`，新增模块级文件锁，新增 roundtrip 单测）
- Possibly modify: `src-tauri/Cargo.toml`（若 `tempfile` 不在 `[dev-dependencies]`）

**Interfaces**:
- Consumes: `super::CronJob`, `super::default_jobs()`；`once_cell::sync::Lazy`（已在依赖中）；`std::sync::Mutex`、`std::time::{SystemTime, UNIX_EPOCH, Duration}`、`std::process::id()`、`std::fs`。
- Produces: `pub fn save_jobs(jobs: &[CronJob]) -> Result<(), String>` 改为 tmp+rename 原子写 + PermissionDenied 重试 3 次（沿用 `committee/queue.rs::save_queue` 模式）+ 入口取串行化锁；`pub(crate) fn load_jobs_base()` 入口取同一锁；新增模块私有 `static SCHEDULER_FILE_LOCK: Lazy<Mutex<()>>`。

**并发设计**: `toggle_job` / `update_cron` / `save_dream_config` 都是 `load_jobs(); ...; save_jobs(&jobs)` —— 锁在 `load_jobs_base` 和 `save_jobs` 入口分别获取/释放，两次之间无锁，同一线程不会重入死锁。串行化目标是写入端原子性 + 读端不读半截文件，而非整个 read-modify-write 的线性一致性。

- [ ] **Step 1: 顶部 use + 模块级锁声明**

  在 `config.rs` 顶部 `use` 区追加：
  ```rust
  use once_cell::sync::Lazy;
  use std::fs;
  use std::sync::Mutex;
  use std::time::{Duration, SystemTime, UNIX_EPOCH};
  ```
  在 `fn config_path()` 之上声明：
  ```rust
  /// Serialize concurrent reads/writes to scheduler.json so callers never see
  /// half-written content. `load_jobs_base` and `save_jobs` each take this
  /// lock independently; toggle_job / update_cron / save_dream_config call
  /// load → save sequentially with the lock released in between, so no
  /// re-entrance occurs on the same thread.
  static SCHEDULER_FILE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
  ```

- [ ] **Step 2: load_jobs_base 入口取锁**

  在 `pub(crate) fn load_jobs_base() -> Vec<CronJob>` 函数体首行加：
  ```rust
      let _guard = SCHEDULER_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  ```
  其余逻辑不变。注意：Task 1 已在该函数内加了 `normalize_cron_6field` 归一化，保留。

- [ ] **Step 3: 用原子写完整替换 save_jobs**

  用以下实现替换现有 `pub fn save_jobs(...)`（行 112-148）：
  ```rust
  /// Save user overrides (only changed fields) to scheduler.json.
  ///
  /// Atomic write (tmp + rename), retrying up to 3 times on PermissionDenied.
  /// Mirrors `crate::invest::committee::queue::save_queue`.
  pub fn save_jobs(jobs: &[CronJob]) -> Result<(), String> {
      let _guard = SCHEDULER_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

      let defaults = super::default_jobs();
      let overrides: Vec<JobOverride> = jobs
          .iter()
          .filter_map(|job| {
              let def = defaults.iter().find(|d| d.id == job.id);
              let changed = def.map_or(true, |d| {
                  d.cron_expr != job.cron_expr
                      || d.interval_min != job.interval_min
                      || d.enabled != job.enabled
                      || d.requires_trading_day != job.requires_trading_day
                      || d.last_run != job.last_run
                      || d.last_status != job.last_status
              });
              if changed {
                  Some(JobOverride {
                      id: job.id.clone(),
                      cron_expr: Some(job.cron_expr.clone()),
                      interval_min: job.interval_min,
                      enabled: Some(job.enabled),
                      requires_trading_day: Some(job.requires_trading_day),
                      last_run: job.last_run.clone(),
                      last_status: job.last_status.clone(),
                  })
              } else {
                  None
              }
          })
          .collect();

      let config = SchedulerConfig { jobs: overrides };
      let json = serde_json::to_string_pretty(&config).map_err(|e| format!("{e}"))?;

      let path = config_path();
      let dir = path
          .parent()
          .ok_or_else(|| "scheduler.json path has no parent".to_string())?;
      fs::create_dir_all(dir).map_err(|e| format!("create_dir_all: {e}"))?;

      let nanos = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .unwrap_or_default()
          .as_nanos();
      let tmp = dir.join(format!("scheduler.json.{}.{}.tmp", std::process::id(), nanos));

      fs::write(&tmp, &json).map_err(|e| format!("write tmp: {e}"))?;

      for attempt in 0..3u8 {
          match fs::rename(&tmp, &path) {
              Ok(()) => return Ok(()),
              Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && attempt < 2 => {
                  std::thread::sleep(Duration::from_millis(50));
              }
              Err(e) => {
                  let _ = fs::remove_file(&tmp);
                  return Err(format!("rename: {e}"));
              }
          }
      }
      let _ = fs::remove_file(&tmp);
      Err("rename: PermissionDenied after 3 retries".to_string())
  }
  ```

- [ ] **Step 4: 不动 toggle_job/update_cron/save_dream_config**

  它们调用 `load_jobs()` 与 `save_jobs(...)` 的顺序不变，锁分别获取/释放。`grep` 复核没有谁在持锁状态又调对方。

- [ ] **Step 5: 新增 roundtrip 单测**

  在 Task 1 创建的 `#[cfg(test)] mod tests` 内追加（需要 `tempfile` dev-dep；先 `grep -n "tempfile" src-tauri/Cargo.toml` 确认，缺失则在 `[dev-dependencies]` 加 `tempfile = "3"`）：
  ```rust

      use std::sync::Mutex as StdMutex;
      static TEST_ENV_LOCK: StdMutex<()> = StdMutex::new(());

      #[test]
      fn save_then_load_base_roundtrips_cron_override() {
          let _t = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
          let tmp = tempfile::tempdir().expect("tempdir");
          let prev_userprofile = std::env::var_os("USERPROFILE");
          let prev_home = std::env::var_os("HOME");
          std::env::set_var("USERPROFILE", tmp.path());
          std::env::set_var("HOME", tmp.path());

          let mut jobs = super::default_jobs();
          let pnl = jobs.iter_mut().find(|j| j.id == "pnl_snapshot").expect("pnl present");
          pnl.cron_expr = "0 0 10,14 * * 1-5".to_string();
          pnl.enabled = false;
          save_jobs(&jobs).expect("save_jobs ok");

          let reloaded = load_jobs_base();
          let rp = reloaded.iter().find(|j| j.id == "pnl_snapshot").expect("pnl after reload");
          assert_eq!(rp.cron_expr, "0 0 10,14 * * 1-5");
          assert!(!rp.enabled);

          // restore env
          match prev_userprofile {
              Some(v) => std::env::set_var("USERPROFILE", v),
              None => std::env::remove_var("USERPROFILE"),
          }
          match prev_home {
              Some(v) => std::env::set_var("HOME", v),
              None => std::env::remove_var("HOME"),
          }
      }
  ```

- [ ] **Step 6: 验证**

  - `cargo check --manifest-path src-tauri/Cargo.toml --tests` —— 通过。
  - `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` —— 通过。
  - `grep -n "SCHEDULER_FILE_LOCK" src-tauri/src/invest/scheduler/config.rs` —— 应命中至少 3 处。
  - `grep -n "fs::write(&path, json)" src-tauri/src/invest/scheduler/config.rs` —— 应仅命中 `save_dream_config` 的 dream_config.json 那行，不再命中 scheduler.json。

- [ ] **Step 7: 提交**

  ```bash
  git add src-tauri/src/invest/scheduler/config.rs src-tauri/Cargo.toml
  git commit -m "fix(invest): atomic write + serialization lock for scheduler.json"
  ```

---

### Task 6: 主循环防御 — per-job 互斥 + panic 兜底 + 接 CancellationToken（#3/#4/#8）

**Files**:
- Modify: `src-tauri/src/invest/scheduler/runner.rs`
- Modify: `src-tauri/src/commands/invest.rs`（`trigger_cron_job`）
- Modify: `src-tauri/src/lib.rs`（`runner::start` 调用点）

**Interfaces**:
- Consumes: `tokio_util::sync::CancellationToken`（已 manage 到 app state，lib.rs:552）；`once_cell::sync::Lazy`、`std::sync::Mutex`、`std::collections::HashSet`；`tokio::spawn` / `tokio::task::JoinError`；`tokio::select!`、`tokio::time::sleep`；模块内既有 `dispatch_job`、`persist_job_status`、`execute_and_log`、`RUNNING`。
- Produces: `pub fn try_acquire_job(id: &str) -> bool`、`pub fn release_job(id: &str)`、`pub struct JobGuard(pub String)`（Drop 自动 release）、`async fn run_dispatch_with_panic_catch`、`async fn run_job_guarded`、`pub fn start<F,Fut>(dispatch: F, cancel: CancellationToken)`（新签名）、`fn start_dedicated_loop(..., cancel: CancellationToken)`（新签名）、改写后的 `trigger_cron_job`。

**设计取舍**: 主循环对 `to_fire` 仍**顺序** await `run_job_guarded`，不并行 spawn 全部任务——dream_invest/verdict_review/event_scan 会同时打 Tushare 与 LLM API，并行会瞬时打爆配额；顺序 + 单任务 panic 隔离已覆盖"一只老鼠坏一锅汤"的风险。互斥用 `JobGuard` 的 Drop 释放，即使任务 panic 栈展开也会释放，不会"job 永久占位无法再触发"。`start` 接 CancellationToken，所有 `sleep` 换 `tokio::select!` 与 `cancel.cancelled()` 二选一；cancel 后 break + `RUNNING.store(false)` 复位。lib.rs:552 的 `cancel` 已被 team_watcher 拿走一份，再 `clone()` 一份给 scheduler（共享同一通知源）。

- [ ] **Step 1: 扩充 runner.rs imports**

  ```rust
  use super::config;
  use crate::storage::invest::scheduler::{is_trading_day, log_task_end, log_task_start};
  use crate::tushare::client::TushareClient;
  use once_cell::sync::Lazy;
  use std::collections::HashSet;
  use std::sync::atomic::{AtomicBool, Ordering};
  use std::sync::{Arc, Mutex};
  use tokio::time::{sleep, Duration, Instant};
  use tokio_util::sync::CancellationToken;
  ```

- [ ] **Step 2: 新增 per-job 互斥与 RAII guard（在 `static RUNNING` 之后）**

  ```rust
  /// Set of currently-executing job ids. Used so the main loop, dedicated loops
  /// and the manual `trigger_cron_job` command never concurrently run the same id.
  static RUNNING_JOBS: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

  /// Try to claim exclusive execution of `id`. Returns `true` iff this caller
  /// won the race and is now responsible for calling `release_job` (typically
  /// via the `JobGuard` RAII helper).
  pub fn try_acquire_job(id: &str) -> bool {
      let mut set = match RUNNING_JOBS.lock() {
          Ok(g) => g,
          Err(poisoned) => poisoned.into_inner(),
      };
      set.insert(id.to_string())
  }

  /// Release a job slot. Idempotent: removing an absent id is a no-op.
  pub fn release_job(id: &str) {
      let mut set = match RUNNING_JOBS.lock() {
          Ok(g) => g,
          Err(poisoned) => poisoned.into_inner(),
      };
      set.remove(id);
  }

  /// RAII guard that releases a job slot on drop, including unwind from a panic.
  pub struct JobGuard(pub String);

  impl Drop for JobGuard {
      fn drop(&mut self) {
          release_job(&self.0);
      }
  }
  ```

- [ ] **Step 3: panic 兜底入口 + run_job_guarded（在 `execute_and_log` 之后）**

  ```rust
  /// Run a dispatch future under a tokio task so any panic is captured by the
  /// JoinHandle rather than aborting the surrounding loop. Outcome is funneled
  /// through `execute_and_log` so success / failure / panic all leave a row in
  /// `scheduler_logs` and update `last_run`/`last_status` consistently.
  async fn run_dispatch_with_panic_catch<Fut>(job_id: &str, fut: Fut, compute_next: bool)
  where
      Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
  {
      let id_owned = job_id.to_string();
      let handle = tokio::spawn(fut);
      match handle.await {
          Ok(result) => execute_and_log(&id_owned, result, compute_next).await,
          Err(join_err) => {
              log::error!("[scheduler] job {id_owned} panicked: {join_err}");
              execute_and_log(&id_owned, Err(format!("panic: {join_err}")), compute_next).await;
          }
      }
  }

  /// Acquire the per-job mutex, build the dispatch future, run it under panic
  /// protection. If the slot is already held (manual trigger or a still-running
  /// previous tick), this tick is skipped with a warn log — never blocked.
  async fn run_job_guarded<F, Fut>(dispatch: Arc<F>, job_id: String, compute_next: bool)
  where
      F: Fn(String) -> Fut + Send + Sync + 'static,
      Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
  {
      if !try_acquire_job(&job_id) {
          log::warn!("[scheduler] job {job_id} already running, skipping this tick");
          return;
      }
      let _guard = JobGuard(job_id.clone());
      let fut = (dispatch)(job_id.clone());
      run_dispatch_with_panic_catch(&job_id, fut, compute_next).await;
  }
  ```

- [ ] **Step 4: 用下面整段替换 `pub fn start<F, Fut>(...)`**

  注意：这一段已经把 Task 2 的 skip 推进 next_run 逻辑合并进来（skip 分支记 `skipped` 日志，但 next_run 的推进由后续 fire 循环的 dirty/save_jobs 统一处理——这里 skip 的 job 不进 to_fire，其 next_run 在下一轮 load 时若仍 due 会再 skip；**若已执行 Task 2，runner.rs 的收集块已是 for 循环形态，本步在其基础上加 cancel/guard 即可，不要重复 skip 逻辑**）。

  ```rust
  /// Start the scheduler loop and dedicated timers. Call once from lib.rs setup.
  ///
  /// `cancel` is the app-wide shutdown token. When tripped, every loop exits
  /// cleanly and `RUNNING` is reset so a future `start()` can re-arm.
  pub fn start<F, Fut>(dispatch: F, cancel: CancellationToken)
  where
      F: Fn(String) -> Fut + Send + Sync + 'static,
      Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
  {
      if RUNNING.swap(true, Ordering::SeqCst) {
          return; // already running
      }

      start_dedicated_loop("jin10_collector", Duration::from_secs(10), Duration::from_secs(15), cancel.clone());
      start_dedicated_loop("event_analyzer", Duration::from_secs(30), Duration::from_secs(10 * 60), cancel.clone());

      let dispatch = Arc::new(dispatch);
      let cancel_main = cancel.clone();
      tauri::async_runtime::spawn(async move {
          tokio::select! {
              _ = sleep(Duration::from_secs(10)) => {}
              _ = cancel_main.cancelled() => {
                  RUNNING.store(false, Ordering::SeqCst);
                  log::info!("[scheduler] main loop cancelled before startup");
                  return;
              }
          }

          loop {
              let mut jobs = config::load_jobs();
              let today = crate::storage::invest::scheduler::beijing_today();
              let now_naive = chrono::Local::now().naive_local();

              // First pass: due-check (Task 2 should_fire) + non-trading-day skip.
              let mut to_fire: Vec<String> = Vec::new();
              let mut dirty = false;
              for job in jobs.iter_mut() {
                  if !config::should_fire(job, now_naive) {
                      continue;
                  }
                  if job.requires_trading_day && !is_trading_day(&today).unwrap_or(false) {
                      if let Ok(log_id) = log_task_start(&job.id) {
                          let _ = log_task_end(log_id, "skipped", Some("non-trading day"));
                      }
                      job.next_run = config::compute_next_run_for_job(job);
                      dirty = true;
                      continue;
                  }
                  to_fire.push(job.id.clone());
              }

              // Sequential execution: shared LLM + Tushare quotas would burst
              // under parallel fan-out. Per-job mutex + panic catch isolate failures.
              for job_id in to_fire {
                  run_job_guarded(dispatch.clone(), job_id.clone(), true).await;
                  let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
                  if let Some(j) = jobs.iter_mut().find(|j| j.id == job_id) {
                      j.last_run = Some(now);
                      j.next_run = config::compute_next_run_for_job(j);
                      dirty = true;
                  }
              }

              if dirty {
                  if let Err(e) = config::save_jobs(&jobs) {
                      log::error!("Failed to save job state: {e}");
                  }
              }

              tokio::select! {
                  _ = sleep(Duration::from_secs(60)) => {}
                  _ = cancel_main.cancelled() => break,
              }
          }

          RUNNING.store(false, Ordering::SeqCst);
          log::info!("[scheduler] main loop exited (cancelled)");
      });
  }
  ```

- [ ] **Step 5: 用下面整段替换 `start_dedicated_loop`**

  ```rust
  /// Spawn a dedicated timer loop for a high-frequency job.
  /// `cancel` lets app shutdown break the loop without leaving an orphaned task.
  fn start_dedicated_loop(
      job_id: &'static str,
      initial_delay: Duration,
      interval: Duration,
      cancel: CancellationToken,
  ) {
      tauri::async_runtime::spawn(async move {
          tokio::select! {
              _ = sleep(initial_delay) => {}
              _ = cancel.cancelled() => {
                  log::info!("[{job_id}] dedicated timer cancelled before start");
                  return;
              }
          }
          log::info!("[{job_id}] dedicated timer started ({}s interval)", interval.as_secs());

          loop {
              let start = Instant::now();

              if try_acquire_job(job_id) {
                  let _guard = JobGuard(job_id.to_string());
                  let fut = dispatch_job(job_id);
                  run_dispatch_with_panic_catch(job_id, fut, false).await;
              } else {
                  log::warn!("[scheduler] dedicated job {job_id} already running, skipping tick");
              }

              let elapsed = start.elapsed();
              let to_sleep = interval.saturating_sub(elapsed);
              tokio::select! {
                  _ = sleep(to_sleep) => {}
                  _ = cancel.cancelled() => break,
              }
          }

          log::info!("[{job_id}] dedicated timer exited (cancelled)");
      });
  }
  ```

- [ ] **Step 6: 改 lib.rs:552-558 调用点**

  ```rust
              // Start team file watcher for ~/.claude/teams/ and ~/.claude/tasks/
              let cancel = app.state::<CancellationToken>().inner().clone();
              let cancel_for_scheduler = cancel.clone();
              hooks::team_watcher::start_team_watcher(app.handle().clone(), cancel);

              // Start invest scheduler runner (background cron loop)
              invest::scheduler::runner::start(
                  |job_id| async move {
                      invest::scheduler::runner::dispatch_job(&job_id).await
                  },
                  cancel_for_scheduler,
              );
  ```

- [ ] **Step 7: 改 `commands/invest.rs::trigger_cron_job` 走互斥**

  ```rust
  #[tauri::command]
  pub async fn trigger_cron_job(id: String) -> Result<String, String> {
      use crate::invest::scheduler::runner::{try_acquire_job, JobGuard};
      use crate::storage::invest::scheduler::{log_task_end, log_task_start};

      if !try_acquire_job(&id) {
          return Err(format!("job {id} already running"));
      }
      let _guard = JobGuard(id.clone());

      let log_id = log_task_start(&id)?;
      let result = crate::invest::scheduler::runner::dispatch_job(&id).await;

      let status = if result.is_ok() { "ok" } else { "error" };
      let msg = match &result {
          Ok(m) => Some(m.as_str()),
          Err(e) => Some(e.as_str()),
      };
      let _ = log_task_end(log_id, status, msg);
      result
  }
  ```

- [ ] **Step 8: 在 runner.rs 末尾追加纯单测**

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn try_acquire_release_cycle_is_exclusive() {
          let id = "scheduler_runner_test_acquire_release";
          release_job(id); // defensive cleanup from any leaked prior run

          assert!(try_acquire_job(id), "first acquire should succeed");
          assert!(!try_acquire_job(id), "second acquire while held should fail");

          release_job(id);
          assert!(try_acquire_job(id), "acquire after release should succeed again");
          release_job(id);
      }

      #[test]
      fn job_guard_releases_on_drop() {
          let id = "scheduler_runner_test_guard_drop";
          release_job(id); // ensure clean slot

          {
              let _guard = JobGuard(id.to_string());
              assert!(try_acquire_job(id), "slot free, acquire inside guard scope");
              // guard's id matches what we acquired; on drop it releases the slot
          }
          // After guard drop, slot must be free again.
          assert!(try_acquire_job(id), "guard drop should have released slot");
          release_job(id);
      }
  }
  ```

- [ ] **Step 9: 验证**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  cargo fmt    --manifest-path src-tauri/Cargo.toml --check
  ```
  CI / 干净 VM 上：`cargo test --manifest-path src-tauri/Cargo.toml invest::scheduler::runner::tests:: -- --nocapture`

- [ ] **Step 10: 提交**

  ```bash
  git add src-tauri/src/invest/scheduler/runner.rs src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
  git commit -m "fix(scheduler): per-job mutex, panic isolation, cancellation propagation"
  ```

---

### Task 7: 交易日判定收紧 + 时区对齐北京时间（#5/#6）

**Files**:
- Modify: `src-tauri/src/storage/invest/scheduler.rs`（`is_trading_day` 加 warn，新增 `beijing_today`，扩 tests）
- Modify: `src-tauri/src/invest/scheduler/runner.rs`（主循环 `today` 改 `beijing_today()`——Task 6 Step 4 已采用该写法，本任务确保 `beijing_today` 存在）

**Interfaces**:
- Consumes: `chrono::{Utc, Duration, NaiveDate}`、`rusqlite::Error::QueryReturnedNoRows`、既有 `with_conn`/`is_weekday`。
- Produces: `pub fn beijing_today() -> String`（`%Y-%m-%d`，北京时间日历日，无 05:00 cutoff）；`is_trading_day` 在 DB miss 时 `log::warn!` 并保持 weekday 回退。

**设计取舍**:
- **不切到"日历缺失就保守跳过"**：未导入 trade_calendar（首次启动/未配 token）会让所有 requires_trading_day 任务哑火，体感"调度器全坏"，比偶尔节假日误跑更糟。改为保留 weekday 回退 + warn 日志，让回退可观测。strict-mode 列为 future option 不实施。
- **today 语义切换**：原 runner.rs:161 用 `get_invest_date()`（Local + 05:00 cutoff），把凌晨 05:00 前算前一天——这是 PnL 业务日语义，用于"今天是不是交易日"是错的（周一凌晨 02:00 会被推回周日）。且 Local 受宿主机时区影响。新增 `beijing_today()` 用 `Utc::now() + 8h` 锁死北京时间，与 `is_a_share_market_open` 同口径。`get_invest_date` 在 pnl_snapshot 业务日逻辑里保留不动。

- [ ] **Step 1: is_trading_day DB miss 分支加 warn**

  ```rust
  pub fn is_trading_day(date: &str) -> Result<bool, String> {
      with_conn(|conn| {
          let result = conn.query_row(
              "SELECT is_open FROM trade_calendar WHERE cal_date = ?1",
              params![date],
              |row| row.get::<_, i32>(0),
          );
          match result {
              Ok(v) => Ok(v != 0),
              Err(rusqlite::Error::QueryReturnedNoRows) => {
                  // No calendar row: either not synced yet or no Tushare token.
                  // Fall back to weekday heuristic so the scheduler keeps firing
                  // — skipping every requires_trading_day job on miss would
                  // silently stop dream/verdict/pnl for token-less users, which
                  // is worse than the occasional holiday misfire. The warn makes
                  // the fallback observable so operators can backfill.
                  // Future option (NOT implemented): strict-mode skip on miss.
                  log::warn!("[trade_calendar] miss for {date}, falling back to weekday heuristic");
                  Ok(is_weekday(date))
              }
              Err(e) => Err(format!("check trading day: {}", e)),
          }
      })
  }
  ```

- [ ] **Step 2: 新增 beijing_today（在 is_a_share_market_open 之上）**

  ```rust
  /// 返回北京时间当天的日历日期（`%Y-%m-%d`）。
  ///
  /// 这是日历日，与 `crate::invest::date_utils::get_invest_date` 不同：
  /// - get_invest_date 含 05:00 cutoff（业务日），用于 PnL 按交易日聚合；
  /// - beijing_today 不含 cutoff，用于"今天是不是 A 股交易日"，并锁死东八区，
  ///   使行为不依赖宿主机本地时区（与 is_a_share_market_open 同口径）。
  pub fn beijing_today() -> String {
      let cst = chrono::Utc::now() + chrono::Duration::hours(8);
      cst.format("%Y-%m-%d").to_string()
  }
  ```

- [ ] **Step 3: 新增 beijing_today 格式单测（scheduler.rs 末尾 mod tests，若已有则追加）**

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use chrono::NaiveDate;

      #[test]
      fn beijing_today_returns_parseable_yyyy_mm_dd() {
          let s = beijing_today();
          assert_eq!(s.len(), 10, "expected YYYY-MM-DD length, got {s:?}");
          assert!(NaiveDate::parse_from_str(&s, "%Y-%m-%d").is_ok());
      }

      #[test]
      fn beijing_today_is_within_one_day_of_utc() {
          let utc = chrono::Utc::now();
          let utc_date = utc.format("%Y-%m-%d").to_string();
          let utc_plus1 = (utc + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
          let s = beijing_today();
          assert!(s == utc_date || s == utc_plus1, "beijing_today {s} vs {utc_date}/{utc_plus1}");
      }
  }
  ```

- [ ] **Step 4: 确认 runner.rs 主循环用 beijing_today**

  Task 6 Step 4 的 `start` 已写 `let today = crate::storage::invest::scheduler::beijing_today();`。若 Task 6 尚未执行（任务乱序），则把 runner.rs 主循环中 `let today = crate::invest::date_utils::get_invest_date();` 替换为 `let today = crate::storage::invest::scheduler::beijing_today();`。`get_invest_date` 在 dispatch_job → pnl_snapshot 路径仍在用，不动。

- [ ] **Step 5: 验证**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  cargo fmt    --manifest-path src-tauri/Cargo.toml --check
  ```
  人肉抽测：清空 trade_calendar 启动 dev，确认 `[trade_calendar] miss` warn 出现且任务仍按 weekday 触发；恢复 is_open=1 后 warn 消失。

- [ ] **Step 6: 提交**

  ```bash
  git add src-tauri/src/storage/invest/scheduler.rs src-tauri/src/invest/scheduler/runner.rs
  git commit -m "fix(scheduler): tighten trading-day detection and pin to Beijing time"
  ```

---

### Task 8: dream_snapshots 保留式清理（数据治理，依赖 Task 1+2）

**Background**: `dream_snapshots` 累计 9521 条，几乎全部 `dream_type='invest'`，绝大多数是 #13 dream 每分钟空触发产生的空转快照。Task 1/2 已修复触发源；本任务回收存量并提供未来可复用的保留函数。委员会记忆闭环 session 已约定不碰该表，由本计划负责。

**Files**:
- Modify: `src-tauri/src/storage/invest/dream_snapshots.rs`（新增 `prune_keep_recent`）
- Modify: `src-tauri/src/lib.rs`（init_db 之后插入一次性启动清理）

**Interfaces**:
- Consumes: `crate::storage::invest::with_conn_mut`、`rusqlite::params!`、`log`。SQLite ≥ 3.25（rusqlite 自带，支持窗口函数）。
- Produces: `pub fn dream_snapshots::prune_keep_recent(keep_per_type: i64) -> Result<usize, String>`（按 dream_type 分区保留最近 keep_per_type 条，返回删除行数）；lib.rs 启动期 `log::info!`。

**设计取舍**: 单条 SQL + `ROW_NUMBER() OVER (PARTITION BY dream_type ORDER BY created_at DESC, id DESC)` 一次完成所有类型截断。**不特殊保护 `rollback_ready=1`**——`insert_complete` 写入时硬编码 `rollback_ready=1`，加 `AND rollback_ready=0` 会删空全表；保留最近 N 条已隐含保护最近可回滚样本。启动时同步调用（单 SQL，9k 行毫秒级），放在 init_db 之后、scheduler start 之前。**执行顺序依赖**：本任务在 Task 1/2 之后，否则清理完又被新空转填回。

- [ ] **Step 1: 新增 prune_keep_recent（dream_snapshots.rs 末尾）**

  ```rust
  /// Retention pruning: for each distinct `dream_type`, keep the `keep_per_type`
  /// most recent snapshots (ordered by created_at DESC, id DESC) and delete the
  /// rest. Returns the number of rows deleted.
  ///
  /// Tradeoff: snapshots with rollback_ready = 1 are NOT specially protected.
  /// insert_complete always writes rollback_ready = 1, so AND rollback_ready = 0
  /// would erase the whole table. The retention window is the protection.
  pub fn prune_keep_recent(keep_per_type: i64) -> Result<usize, String> {
      if keep_per_type < 0 {
          return Err(format!("keep_per_type must be >= 0, got {keep_per_type}"));
      }
      with_conn_mut(|conn| {
          let deleted = conn
              .execute(
                  "DELETE FROM dream_snapshots
                   WHERE id NOT IN (
                       SELECT id FROM (
                           SELECT id,
                                  ROW_NUMBER() OVER (
                                      PARTITION BY dream_type
                                      ORDER BY created_at DESC, id DESC
                                  ) AS rn
                           FROM dream_snapshots
                       ) WHERE rn <= ?1
                   )",
                  rusqlite::params![keep_per_type],
              )
              .map_err(|e| format!("prune dream_snapshots: {e}"))?;
          Ok(deleted)
      })
  }
  ```

- [ ] **Step 2: 启动一次性清理（lib.rs，init_db 之后）**

  找到 lib.rs 中 `crate::storage::invest::init_db(&data_dir)` 调用块，紧随其后、trade_calendar spawn 之前插入：
  ```rust
      // One-shot startup cleanup: bound dream_snapshots growth.
      // Each dream_type retains its 20 most recent snapshots; older rows deleted.
      match crate::storage::invest::dream_snapshots::prune_keep_recent(20) {
          Ok(0) => log::debug!("[invest] dream_snapshots within retention bound, nothing to prune"),
          Ok(n) => log::info!("[invest] pruned {} stale dream_snapshots (kept latest 20 per type)", n),
          Err(e) => log::warn!("[invest] dream_snapshots prune failed: {}", e),
      }
  ```

- [ ] **Step 3: 编译验证**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  cargo fmt    --manifest-path src-tauri/Cargo.toml --check
  ```

- [ ] **Step 4: 手动验证（DB 集成，本机用 Python dry-run）**

  重启 ClawGO 之前记录基线 + 预测删除数：
  ```python
  import sqlite3, pathlib
  db = pathlib.Path.home() / ".claw-go" / "invest" / "invest.db"
  conn = sqlite3.connect(db)
  print("total:", conn.execute("SELECT COUNT(*) FROM dream_snapshots").fetchone()[0])
  predicted = conn.execute(
      """SELECT COUNT(*) FROM dream_snapshots WHERE id NOT IN (
           SELECT id FROM (SELECT id, ROW_NUMBER() OVER
             (PARTITION BY dream_type ORDER BY created_at DESC, id DESC) AS rn
             FROM dream_snapshots) WHERE rn <= 20)""").fetchone()[0]
  print("predicted delete:", predicted)
  ```
  启动后查应用日志，应见 `[invest] pruned <predicted> stale dream_snapshots`；再跑脚本确认每个 dream_type ≤ 20。

- [ ] **Step 5: 提交**

  ```bash
  git add src-tauri/src/storage/invest/dream_snapshots.rs src-tauri/src/lib.rs
  git commit -m "chore(invest): bound dream_snapshots to 20 most recent per type"
  ```

---

### Task 9: scheduler_logs 保留清理（#11，卫生）

**Background**: `scheduler_logs` 当前 6.1 万行，jin10_collector 每 15s 一条占 45330 条，从未清理。

**Files**:
- Modify: `src-tauri/src/storage/invest/scheduler.rs`（新增 `prune_scheduler_logs`）
- Modify: `src-tauri/src/lib.rs`（Task 8 prune 之后插入第二个一次性清理）

**Interfaces**:
- Consumes: `crate::storage::invest::with_conn_mut`、`chrono::{Utc, Duration, SecondsFormat}`、`rusqlite::params!`、`log`。
- Produces: `pub fn scheduler::prune_scheduler_logs(keep_days: i64) -> Result<usize, String>`（删除 started_at 早于 now-keep_days 的记录，返回删除行数）；lib.rs 启动期 `log::info!`。

**设计取舍**: `started_at` 由 log_task_start 写为 `Utc::now().to_rfc3339_opts(Millis, true)`（形如 `2026-06-19T08:42:13.456Z`），全是 UTC、毫秒精度、带 Z。等长同时区固定格式的 rfc3339 字符串字典序 = 时间序，故 `WHERE started_at < ?1` 用同格式 cutoff 是合法范围比较，无需 julianday 解析。30 天保留：jin10 每天 5760 条，上限约 17 万行可接受。

- [ ] **Step 1: 新增 prune_scheduler_logs（scheduler.rs 末尾）**

  ```rust
  /// Retention pruning: delete scheduler_logs rows whose started_at is older
  /// than keep_days days. Returns the number of rows deleted.
  ///
  /// started_at is a UTC rfc3339 string with millisecond precision and a
  /// trailing Z (written by log_task_start). UTC rfc3339 strings of identical
  /// shape sort lexicographically the same as chronologically, so the string
  /// filter is sound as long as the cutoff uses the same to_rfc3339_opts call.
  pub fn prune_scheduler_logs(keep_days: i64) -> Result<usize, String> {
      if keep_days < 0 {
          return Err(format!("keep_days must be >= 0, got {keep_days}"));
      }
      with_conn_mut(|conn| {
          let cutoff = (chrono::Utc::now() - chrono::Duration::days(keep_days))
              .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
          let deleted = conn
              .execute(
                  "DELETE FROM scheduler_logs WHERE started_at < ?1",
                  params![cutoff],
              )
              .map_err(|e| format!("prune scheduler_logs: {}", e))?;
          Ok(deleted)
      })
  }
  ```

- [ ] **Step 2: 启动一次性清理（lib.rs，Task 8 prune 块之后）**

  ```rust
      // One-shot startup cleanup: drop scheduler_logs older than 30 days.
      // jin10_collector writes ~5760 rows/day, so 30 days caps at ~170k rows.
      match crate::storage::invest::scheduler::prune_scheduler_logs(30) {
          Ok(0) => log::debug!("[invest] scheduler_logs within retention window, nothing to prune"),
          Ok(n) => log::info!("[invest] pruned {} scheduler_logs older than 30 days", n),
          Err(e) => log::warn!("[invest] scheduler_logs prune failed: {}", e),
      }
  ```

- [ ] **Step 3: 编译验证**

  ```
  cargo check  --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  cargo fmt    --manifest-path src-tauri/Cargo.toml --check
  ```

- [ ] **Step 4: 手动验证（Python dry-run）**

  ```python
  import sqlite3, pathlib, datetime
  db = pathlib.Path.home() / ".claw-go" / "invest" / "invest.db"
  conn = sqlite3.connect(db)
  cutoff = (datetime.datetime.utcnow() - datetime.timedelta(days=30)).strftime("%Y-%m-%dT%H:%M:%S") + ".000Z"
  print("cutoff:", cutoff)
  print("total:", conn.execute("SELECT COUNT(*) FROM scheduler_logs").fetchone()[0])
  predicted = conn.execute("SELECT COUNT(*) FROM scheduler_logs WHERE started_at < ?", (cutoff,)).fetchone()[0]
  print("predicted delete:", predicted)
  ```
  启动后查日志应见 `[invest] pruned <predicted> scheduler_logs older than 30 days`。注：cutoff 用 `.000Z` 固定 3 位毫秒，勿用微秒位破坏字典序。

- [ ] **Step 5: 提交**

  ```bash
  git add src-tauri/src/storage/invest/scheduler.rs src-tauri/src/lib.rs
  git commit -m "chore(invest): retain scheduler_logs for 30 days, prune on startup"
  ```

---

## 与并行 session 的边界（committee 记忆闭环）

- **归本计划（scheduler session）**：#13/#14（dream 空转根因 + skip 推进 next_run，Task 1/2）、`dream_snapshots` 表清理（Task 8）。
- **归 committee session**（`docs/superpowers/specs/2026-06-19-committee-memory-loop-design.md`，本计划不碰）：委员会命中率记忆注入、结构化点位、高信念裁决通道、CLI prompt 工具噪声清理、L4 死字段移除——全在 `invest/committee/` 内，与本计划无文件重叠。
- `dream_snapshots` 表只由本计划 Task 8 触碰，committee session 不动。

## Self-Review

**1. Spec coverage（11 个问题 → 任务映射）**：
- #13 cron 读取归一化 → Task 1 ✓
- #14 skip 推进 next_run → Task 2 ✓
- #1 pnl 双重调度 + cron 修正 → Task 3 ✓
- #2 event_scan 双重调度 → Task 4 ✓
- #7 save_jobs 原子写 + 锁 → Task 5 ✓
- #3/#4/#8 串行 panic 兜底 + cancel + per-job 互斥 → Task 6 ✓
- #5/#6 交易日收紧 + 时区 → Task 7 ✓
- dream_snapshots 数据清理 → Task 8 ✓
- #11 scheduler_logs 清理 → Task 9 ✓
- #9（dedicated cron 字段被忽略）、#10（update_cron 字符过滤剥合法字符）：**未单列任务**——#9 是 UI 层显示问题（dedicated 任务隐藏 cron 编辑），#10 风险低（`?LW#` 极少用）。两者列为 follow-up，不阻塞本批修复。若需纳入，建议合入 Task 5（同改 config.rs）。

**2. Type consistency**：
- `should_fire(job, now_naive)`：Task 2 定义、Task 6 主循环调用 ✓
- `beijing_today()`：Task 7 定义、Task 6 主循环 + Task 7 runner 调用 ✓（Task 6/7 互相引用，Step 注明任一先执行都可）
- `try_acquire_job`/`release_job`/`JobGuard`：Task 6 定义、Task 6 trigger_cron_job 复用 ✓
- `prune_keep_recent`/`prune_scheduler_logs`：Task 8/9 定义并在 lib.rs 调用 ✓
- `normalize_cron_6field`：Task 1 在 load_jobs_base 调用（已存在的私有 fn）✓

**3. 任务顺序约束**：Task 1 → Task 2（共享 mod tests）；Task 6 与 Task 7 互相引用 `beijing_today`（Step 注明任一先行均可）；Task 8 依赖 Task 1/2（先止血再清理）；Task 5 与 Task 6 都改 config.rs/runner.rs，建议按 1→2→3→4→5→6→7→8→9 顺序执行避免 rebase 冲突。

**已知风险**：Task 6 重写 `start` 与 Task 2 重写 `to_fire` 收集块有重叠——若按顺序执行，Task 6 Step 4 的 `start` 已合并 Task 2 的 skip 逻辑，实施时以 Task 6 的最终形态为准，不要重复施加 Task 2 的 runner.rs 改动（config.rs 的 should_fire 仍来自 Task 2）。
