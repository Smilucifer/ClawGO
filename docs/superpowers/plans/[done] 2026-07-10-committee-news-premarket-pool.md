# 委员会「新闻/舆论」+ 盘前观察优化 — Plan 2 (选池组) 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重构盘前观察选池模型——从"绝对分阈值凑档"改为"全市场 top20 名次切档"(B4),候选池多信号化(B1),五因子固定权重(B2),新增东财板块强度因子(B5),AI 终选证伪复核(B3)——让观察池="市场上最值得追的 20 只"。

**Architecture:** 后端为主(Rust/Tauri + Python akshare 桥),盘后 `premarket_cache` cron 用 `build_cache` 算好全部因子存 `premarket_factor_cache`,盘前报告只读缓存做线性合成 + AI 复核。选池五块**必须串行**(都改 `scoring.rs`/`cache_builder.rs`/`report.rs`,相互叠加)。

**Tech Stack:** Rust、rusqlite(SQLite)、Python akshare(东财板块数据)、Tauri IPC、tokio-cron-scheduler、SvelteKit(Svelte 5 runes)前端展示。

## Global Constraints

- **实施顺序(硬性,串行)**: B4 → B1 → B2 → B5 → B3。B4 最独立、最低风险(拿现有 `SymbolScore.total` 即可名次切档),最先做;B1/B2/B5/B3 在已改成名次切档的稳定视图上迭代打分逻辑。B5 工作量最大(东财数据源 + 一对多映射表 + 低频 cron)。B3 最后(AI 复核依赖前四步的量化结果)。
- **量化 vs AI 隔离(硬性)**: `SymbolScore.total` 与档位**永远只由量化决定**;AI 结果走独立可选字段 `ai_review: Option<AiReview>`,绝不回写 `total`。
- **不做 regime 动态权重**: 路线乙(大盘/广度 regime 权重矩阵、EMA、`weight_state.rs`、crash 旁路)全部不做;市况维度改由 B5 板块强度因子从个股级表达。
- **本机 Rust 单测限制**: 裸 `cargo test`(Git Bash)挂 `0xc0000139`;用 `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- <filter> --nocapture"` 或 `npm run rust:test`。快速校验用 `cargo check`。
- **验证基线**: 后端 `cargo check --manifest-path src-tauri/Cargo.toml` + `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`;前端 `npm run check` + `npm run build`;改 i18n 加 `npm run i18n:check`(en/zh 键集必须一致)。
- **Cron 表达式为 6 字段含秒**: `sec min hour day month dow`。
- **关键 API 事实(子 agent 已核实,勿臆造)**: AI CLI 复用 `event_analyzer::cli_complete(system_prompt, user_message)`(内部经 `macro_verdict::resolve_settings_path()` 路由 provider/model);**`cli_complete_with_settings` 不存在**。缓存函数名是 `save_factor_cache`/`load_latest_cache`(非 upsert/load)。`select_candidates` 返回 `Vec<(String,f64,f64)>` = (ts_code, pct_chg, amount)。刷新模板是 `refresh_stock_industry`(非 `refresh_stock_basic`)。`scoring.rs` 原本无 `threshold_c`/`weight_source`/`enable_ai_review`/`ai_review`,`FactorBreakdown` 原为 4 字段。`premarket_cache.rs` 无 `ensure_column` 助手,需自建幂等 `ALTER TABLE`。
- **sections_status 为绿地新增**: 现无任何 `sections_status`/`sectionsStatus` 机制;B3 从零加到报告 JSON。
- **KPI 非阻塞**: 选池效果 KPI(日换血率、S/A 档次日命中率、追高股占比、板块映射覆盖率 >80%)是发版后跟踪/验收打印项,**不阻塞 plan close**——多交易日样本 + 当前无历史基线。
- **Conventional Commits**;只 `git add` 本任务文件,不提交密钥/本地设置/生成态。

---

### Task B4: SABC 观察池改「先选池→打分排序→按名次切档」

**目标**：把 SABC 从"绝对阈值分桶+每桶取 3"改为"全市场按 `total` 排序取 top20、按名次切档(1-5=S / 6-10=A / 11-15=B / 16-20=C)"。SABC 由"绝对质量档"退化为"精英 20 只内部的相对名次标签"。

**范围**：只改档位分配算法，沿用当前 4 因子 `total`。B1/B2/B3/B5 不在本任务内。B3 未落地前 pool 始终为完整候选(~200),此处永远拿到 top20;但新 helper 需 gracefully 处理 pool<20(按序 S→A→B→C 填,末档不满 5 不补更差票),便于 B3 后组合。

**保留**：`grade_of()` 仍被 `report::build_themes` 用作板块内档位标签,不删。`assign_grades_by_rank` 是新增(additive)函数,不影响现有单测。

---

#### Step 1 — 写失败测试(TDD)

- [ ] 在 `src-tauri/src/invest/premarket/scoring.rs` 的 `#[cfg(test)] mod tests` 中追加下列测试(不要改动已有 4 个测试),预期 `cargo check` 失败(函数尚未定义):

```rust
    fn mk(symbol: &str, total: f64) -> SymbolScore {
        // 构造一个 total 精确等于入参的 SymbolScore(绕开 score() 里的四舍五入),
        // 让排序断言可预期。grade 用什么无所谓——assign_grades_by_rank 会覆写。
        SymbolScore {
            symbol: symbol.to_string(),
            name: symbol.to_string(),
            total,
            grade: Grade::C,
            factors: FactorBreakdown {
                sentiment: 0.0,
                capital: 0.0,
                technical: 0.0,
                catalyst: 0.0,
            },
            missing_factors: vec![],
        }
    }

    #[test]
    fn test_assign_grades_by_rank_25_stocks_takes_top20_and_cuts_5_per_bucket() {
        // 25 只,total 从 100 递减到 76(step 1)
        let input: Vec<SymbolScore> = (0..25)
            .map(|i| mk(&format!("S{:02}", i), 100.0 - i as f64))
            .collect();
        let out = assign_grades_by_rank(input);
        assert_eq!(out.len(), 20, "应截断到 top20");
        // 降序:第 0 名 total=100,第 19 名 total=81
        assert!((out[0].total - 100.0).abs() < 1e-9);
        assert!((out[19].total - 81.0).abs() < 1e-9);
        // 排序单调递减
        for i in 1..out.len() {
            assert!(out[i - 1].total >= out[i].total, "非降序 at {i}");
        }
        // 档位切法:1-5=S,6-10=A,11-15=B,16-20=C
        for i in 0..5 {
            assert_eq!(out[i].grade, Grade::S, "rank {} 应为 S", i + 1);
        }
        for i in 5..10 {
            assert_eq!(out[i].grade, Grade::A, "rank {} 应为 A", i + 1);
        }
        for i in 10..15 {
            assert_eq!(out[i].grade, Grade::B, "rank {} 应为 B", i + 1);
        }
        for i in 15..20 {
            assert_eq!(out[i].grade, Grade::C, "rank {} 应为 C", i + 1);
        }
    }

    #[test]
    fn test_assign_grades_by_rank_12_stocks_last_bucket_underfilled() {
        // 12 只 → S=5, A=5, B=2, C=0(末档不补更差的票,因为没有更差的了)
        let input: Vec<SymbolScore> = (0..12)
            .map(|i| mk(&format!("S{:02}", i), 100.0 - i as f64))
            .collect();
        let out = assign_grades_by_rank(input);
        assert_eq!(out.len(), 12);
        let count = |g: Grade| out.iter().filter(|s| s.grade == g).count();
        assert_eq!(count(Grade::S), 5, "S 应满 5");
        assert_eq!(count(Grade::A), 5, "A 应满 5");
        assert_eq!(count(Grade::B), 2, "B 只剩 2 只");
        assert_eq!(count(Grade::C), 0, "C 无票不补");
        // 顺序仍为 S..A..B
        for i in 0..5 {
            assert_eq!(out[i].grade, Grade::S);
        }
        for i in 5..10 {
            assert_eq!(out[i].grade, Grade::A);
        }
        for i in 10..12 {
            assert_eq!(out[i].grade, Grade::B);
        }
    }

    #[test]
    fn test_assign_grades_by_rank_unsorted_input_gets_sorted_desc() {
        // 打乱顺序输入,验证函数内部会按 total 降序排
        let input = vec![
            mk("low", 50.0),
            mk("high", 90.0),
            mk("mid", 70.0),
        ];
        let out = assign_grades_by_rank(input);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].symbol, "high");
        assert_eq!(out[1].symbol, "mid");
        assert_eq!(out[2].symbol, "low");
        // 3 只全走 S 桶
        assert_eq!(out[0].grade, Grade::S);
        assert_eq!(out[1].grade, Grade::S);
        assert_eq!(out[2].grade, Grade::S);
    }
```

#### Step 2 — 确认失败

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` — 应报 `assign_grades_by_rank` 未定义(或测试直接编不过)。

#### Step 3 — 实现 `assign_grades_by_rank`

- [ ] 在 `scoring.rs` 的 `grade_of` 与 `score` 之间(约 L70 之后、L72 之前)插入下列函数,`grade_of` 保留不动:

```rust
/// 按名次切档:入参已按 total 降序。前 20 名每档 5 只(1-5=S/6-10=A/11-15=B/16-20=C);
/// 不足 20 只时按序填 S→A→B→C,末档不补更差的票。就地改写每只的 grade,返回 top-20(或全部,若 <20)。
pub fn assign_grades_by_rank(mut scores: Vec<SymbolScore>) -> Vec<SymbolScore> {
    scores.sort_by(|a, b| b.total.partial_cmp(&a.total).unwrap_or(std::cmp::Ordering::Equal));
    scores.truncate(20);
    for (i, s) in scores.iter_mut().enumerate() {
        s.grade = match i {
            0..=4 => Grade::S,
            5..=9 => Grade::A,
            10..=14 => Grade::B,
            _ => Grade::C,
        };
    }
    scores
}
```

#### Step 4 — 跑测试

- [ ] 本机 Rust 单测直接 `cargo test` 会挂 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`,用 cmd.exe:
  ```bash
  cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- assign_grades_by_rank --nocapture"
  ```
  三条新测试应全部 PASS。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` PASS。

#### Step 5 — 接入 `report.rs`

- [ ] 打开 `src-tauri/src/invest/premarket/report.rs`,在 `generate_premarket_report` 函数体内 L614(即 `let scores: Vec<SymbolScore> = collect_scores_from_cache(&cfg).await;`)之后紧接插入下面这一段(shadow `scores`,让下游 `render_scores_md` / `build_themes` / JSON `"scores"` 都拿到 rank-cut 后的 top20):

```rust
    // B4: 名次切档——取全市场总分 top20,按名次(而非绝对阈值)分 S/A/B/C。
    // 下游 render_scores_md / build_themes / JSON.scores 都基于本次赋档后的集合。
    let scores = crate::invest::premarket::scoring::assign_grades_by_rank(scores);
```

- [ ] 不需要新增 `use`(用完整路径调用即可)。`build_themes(&scores, &cfg, 5)` 签名未变,只是现在只在 top20 上聚合板块——这与 B4 语义一致(板块热度反映精英池)。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` PASS。
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` PASS。

#### Step 6 — 前端 `grouped` 上限 3 → 5

- [ ] 打开 `src/lib/components/invest/PremarketReportTab.svelte`,把 L158-L171 的 `grouped` 派生整块替换。

  **Before**(现状,`slice(0, 3)`,注释 "cap 3 per bucket"):

  ```ts
    // Grouped by grade — cap 3 per bucket
    const grouped = $derived.by(() => {
      const buckets: Record<Grade, SymbolScore[]> = { S: [], A: [], B: [], C: [] };
      for (const s of scores) buckets[s.grade].push(s);
      (['S', 'A', 'B', 'C'] as const).forEach((g) =>
        buckets[g].sort((a, b) => b.total - a.total),
      );
      return {
        S: buckets.S.slice(0, 3),
        A: buckets.A.slice(0, 3),
        B: buckets.B.slice(0, 3),
        C: buckets.C.slice(0, 3),
      };
    });
  ```

  **After**(名次切档,后端已在 top20 内赋档,前端每档最多显示 5):

  ```ts
    // Grouped by grade — server assigns grade by rank (top20, 5 per bucket). Cap 5 for safety.
    const grouped = $derived.by(() => {
      const buckets: Record<Grade, SymbolScore[]> = { S: [], A: [], B: [], C: [] };
      for (const s of scores) buckets[s.grade].push(s);
      (['S', 'A', 'B', 'C'] as const).forEach((g) =>
        buckets[g].sort((a, b) => b.total - a.total),
      );
      return {
        S: buckets.S.slice(0, 5),
        A: buckets.A.slice(0, 5),
        B: buckets.B.slice(0, 5),
        C: buckets.C.slice(0, 5),
      };
    });
  ```

- [ ] SABC 段落 markup(L657-709)不动:它循环 `{S,A,B,C}` 迭代 `grouped.X` 渲染 `.stock-row`,自动适配 5 行。`.pool-box` 无 `max-height`,行数从 3 增到 5 后会更高,视觉可接受(不加滚动条)。
- [ ] `npm run check` PASS。
- [ ] `npm run build` PASS。

#### Step 7 — 提交

- [ ] `git add src-tauri/src/invest/premarket/scoring.rs src-tauri/src/invest/premarket/report.rs src/lib/components/invest/PremarketReportTab.svelte`
- [ ] Commit:
  ```
  feat(premarket): SABC 改名次切档 (全市场 top20, 每档 5 只)

  - scoring: 新增 assign_grades_by_rank,按 total 降序取 top20 后 1-5=S/6-10=A/11-15=B/16-20=C;pool<20 时末档不补更差票
  - report: generate_premarket_report 在收池后调用新 helper,shadow scores 让 md/themes/json 全部基于 rank-cut 结果
  - frontend PremarketReportTab: grouped 每档 slice(0,3) → slice(0,5),注释更新为「服务端按名次赋档」
  - grade_of 保留,build_themes 内部板块档位标签仍用绝对阈值
  ```


---

### Task B1: 候选池多信号化 (S1 舆情 ∪ S2 主力净流入 Top60 ∪ S7 涨幅兜底)

**Problem**: `select_candidates` 在 `src-tauri/src/invest/premarket/cache_builder.rs` 目前只用两级筛选：舆情命中全保留，剩余 slot 纯按 `pct_chg` 降序填到 `CANDIDATE_CAP=200`。这会导致系统性追涨偏差——没有主力资金信号参与候选池组装，涨幅兜底反而成了主策略。

**Fix (P1, 零新接口)**: 改成多信号并集打分：
- **S1 舆情命中**：仍然全部保留（现有行为）。
- **S2 主力净流入 Top60**：直接复用 `build_cache` 里已经在内存中构建好的 `net_map: HashMap<String, Option<f64>>`（来自 `moneyflow_dc_market`），按 `net_amount` 降序取前 60，并入候选。**零新接口**。
- **S7 涨幅兜底**：从主策略降级为兜底，只用于把候选池填到 `CANDIDATE_CAP=200`。
- **合并/去重排序**：先按"命中信号数"降序，同分再按 `net_amount` 降序（缺失排最后），最后剩余槽位由 S7 按 `pct_chg` 降序补齐。
- **S2 降级**：当 `net_map` 为空（如接口失败）时，S2 贡献 0 只股票，静默退化为"S1 + S7"（相当于旧行为）。sections_status 可观测性由后续 B3 任务负责，本任务只做优雅降级。
- `CANDIDATE_CAP` 保持 200。

**当前状态 (已核对)**：
- `src-tauri/src/invest/premarket/cache_builder.rs` L14-38 是 `select_candidates` 现在的实现，签名 `pub fn select_candidates(daily: &[DailyBar], sentiment_symbols: &HashSet<String>, cap: usize) -> Vec<(String, f64, f64)>`，返回 `(ts_code, pct_chg, amount)` 元组。
- L51 `const CANDIDATE_CAP: usize = 200;`
- `build_cache` L77-162：L84-93 已经构建 `net_map`（6 位代码 → `Option<f64>`），来自 `moneyflow_dc_market`；L109 调用 `select_candidates`。顺序 OK，`net_map` 在候选调用之前就绪。
- `DailyBar` 字段使用：`ts_code`（如 `600000.SH`）、`pct_chg`、`amount`。测试 fixture L175-181 展示了完整字段（`trade_date`、`open/high/low/close/pre_close`、`change`、`vol` 都要给），L184 是需要更新的老测试。
- 下游依赖：`build_cache` 用返回的完整 `ts_code` 去查 `tech_map`，用 `code6(ts)` 去查其他 6 位代码 map，所以**返回元组第一位必须保持完整 `ts_code` 而非 6 位**。

**Steps**:

- [ ] 1. **TDD**: 先重写 `src-tauri/src/invest/premarket/cache_builder.rs` 里 L171-192 的测试模块（保留 `capital_score_maps_net_to_range` 不动，只改 `select_prioritizes_sentiment_hits_then_fills_by_pct` 并新增 3 个用例覆盖 S2/多信号排序/S2 降级）。整块 `tests` 模块替换成：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn bar(ts: &str, pct: f64) -> DailyBar {
        DailyBar {
            ts_code: ts.into(), trade_date: "20260708".into(),
            open: 0.0, high: 0.0, low: 0.0, close: 0.0, pre_close: 0.0,
            change: 0.0, pct_chg: pct, vol: 0.0, amount: 0.0,
        }
    }

    #[test]
    fn s1_sentiment_hit_always_kept_even_low_pct() {
        // 舆情命中即便涨幅垫底也必须保留 (S1)
        let daily = vec![bar("600000.SH", 1.0), bar("600001.SH", 9.0), bar("600002.SH", 5.0)];
        let mut hits = HashSet::new();
        hits.insert("600000".to_string());
        let net_map: HashMap<String, Option<f64>> = HashMap::new(); // S2 空 → 只走 S1+S7
        let out = select_candidates(&daily, &hits, 2, &net_map);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "600000.SH");     // S1 命中优先
        assert_eq!(out[1].0, "600001.SH");     // S7 兜底按 pct_chg 降序 → 9.0 先
    }

    #[test]
    fn s2_top60_stock_pulled_in_even_low_pct() {
        // 涨幅低但主力净流入高 → S2 必须把它拉进候选池
        let daily = vec![bar("600000.SH", 0.5), bar("600001.SH", 8.0), bar("600002.SH", 4.0)];
        let hits: HashSet<String> = HashSet::new(); // S1 空
        let mut net_map: HashMap<String, Option<f64>> = HashMap::new();
        net_map.insert("600000".to_string(), Some(5.0e5));   // 强净流入
        net_map.insert("600002".to_string(), Some(-1.0e4));  // 净流出，进不了 S2
        let out = select_candidates(&daily, &hits, 2, &net_map);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "600000.SH", "S2 命中(高净额)应排首位，即便涨幅只有 0.5%");
        assert_eq!(out[1].0, "600001.SH", "剩余槽位 S7 兜底按涨幅 → 8.0 先");
    }

    #[test]
    fn multi_signal_ranks_above_single_signal() {
        // S1∩S2 (2 信号) 排在只有 S1 或只有 S2 之前
        let daily = vec![
            bar("600000.SH", 1.0),  // S1 only
            bar("600001.SH", 2.0),  // S1 ∩ S2 (2 信号)
            bar("600002.SH", 3.0),  // S2 only
            bar("600003.SH", 9.0),  // 无信号，S7 兜底
        ];
        let mut hits = HashSet::new();
        hits.insert("600000".to_string());
        hits.insert("600001".to_string());
        let mut net_map: HashMap<String, Option<f64>> = HashMap::new();
        net_map.insert("600001".to_string(), Some(9.0e5));   // 强净流入
        net_map.insert("600002".to_string(), Some(3.0e5));   // 中净流入
        let out = select_candidates(&daily, &hits, 4, &net_map);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].0, "600001.SH", "S1∩S2 双命中排首位");
        // 单信号内按 net_amount 降序：S2-only(600002 net=3e5) > S1-only(600000 net=None → 排最后)
        assert_eq!(out[1].0, "600002.SH");
        assert_eq!(out[2].0, "600000.SH");
        assert_eq!(out[3].0, "600003.SH", "无信号被 S7 兜底填入");
    }

    #[test]
    fn empty_net_map_degrades_to_s1_plus_s7() {
        // net_map 为空(接口失败) → S2 贡献 0，静默降级为 S1+S7，即旧行为
        let daily = vec![bar("600000.SH", 1.0), bar("600001.SH", 9.0), bar("600002.SH", 5.0)];
        let mut hits = HashSet::new();
        hits.insert("600000".to_string());
        let net_map: HashMap<String, Option<f64>> = HashMap::new();
        let out = select_candidates(&daily, &hits, 3, &net_map);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, "600000.SH");     // S1 命中
        assert_eq!(out[1].0, "600001.SH");     // S7 pct 9.0
        assert_eq!(out[2].0, "600002.SH");     // S7 pct 5.0
    }

    #[test]
    fn capital_score_maps_net_to_range() {
        assert_eq!(capital_score_from_net(None), 50.0);
        assert!((capital_score_from_net(Some(0.0)) - 50.0).abs() < 0.01);
        assert!(capital_score_from_net(Some(1.0e5)) > 85.0);   // 10亿单日 → ~88
        assert!(capital_score_from_net(Some(-1.0e5)) < 15.0);  // 对称
    }
}
```

- [ ] 2. 运行 `cargo check --manifest-path src-tauri/Cargo.toml` — 预期 **FAIL**：`select_candidates` 只接受 3 个参数、测试传了 4 个 (arity mismatch)，且 L109 老调用点仍传 3 个也不匹配新签名。

- [ ] 3. 重写 `select_candidates` (替换 L14-38 整个函数) 并更新调用点 L109。

  **3a. 新签名 + 完整实现**（替换 L14-38）：

```rust
/// 候选池多信号化：S1 舆情命中 ∪ S2 主力净流入 Top60 ∪ S7 涨幅兜底。
///
/// - S1: `sentiment_symbols` 内所有股票（全保留）。
/// - S2: `net_map` 中 `net_amount` 前 60 的股票并入候选。空 map → S2 贡献 0，静默降级。
/// - 排序：先按命中信号数（0/1/2）降序，同分按 `net_amount` 降序，缺失 net 排最后。
/// - S7: 剩余槽位按 `pct_chg` 降序补齐到 `cap`。
///
/// 返回 `(完整 ts_code, pct_chg, amount)`。**必须保留完整 ts_code**——build_cache
/// 下游用 ts_code 查 tech_map，用 code6(ts) 查其他 6 位 map。
pub fn select_candidates(
    daily: &[DailyBar],
    sentiment_symbols: &HashSet<String>,
    cap: usize,
    net_map: &std::collections::HashMap<String, Option<f64>>,
) -> Vec<(String, f64, f64)> {
    let code6 = |ts: &str| ts.split('.').next().unwrap_or(ts).to_string();

    // S2: 从 net_map 里挑出 net_amount 前 60 的 6 位代码。空 map → 空集，静默降级。
    let mut flow_pairs: Vec<(String, f64)> = net_map
        .iter()
        .filter_map(|(c6, n)| n.map(|v| (c6.clone(), v)))
        .collect();
    flow_pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let s2_top: HashSet<String> = flow_pairs.into_iter().take(60).map(|(c, _)| c).collect();

    // 为每一根 daily bar 计算 (signal_count, net_for_tiebreak)。
    // signal_count = S1 命中 + S2 命中(0/1/2)。net_for_tiebreak: 缺失时用 None 排最后。
    struct Scored<'a> {
        bar: &'a DailyBar,
        signal_count: u32,
        net: Option<f64>,
    }
    let mut scored: Vec<Scored> = Vec::with_capacity(daily.len());
    for b in daily {
        let c6 = code6(&b.ts_code);
        let s1 = sentiment_symbols.contains(&c6);
        let s2 = s2_top.contains(&c6);
        let count = (s1 as u32) + (s2 as u32);
        let net = net_map.get(&c6).and_then(|o| *o);
        scored.push(Scored { bar: b, signal_count: count, net });
    }

    // 拆两桶：有信号(≥1) 走多信号排序；无信号走 S7 兜底 pct 降序。
    let (mut signaled, mut fallback): (Vec<Scored>, Vec<Scored>) =
        scored.into_iter().partition(|s| s.signal_count >= 1);

    // 多信号排序：信号数降序 → net 降序（None 最后） → 稳定即可。
    signaled.sort_by(|a, b| {
        b.signal_count.cmp(&a.signal_count).then_with(|| {
            match (a.net, b.net) {
                (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            }
        })
    });

    // S7 兜底：pct_chg 降序。
    fallback.sort_by(|a, b| {
        b.bar.pct_chg.partial_cmp(&a.bar.pct_chg).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out: Vec<(String, f64, f64)> = Vec::with_capacity(cap);
    for s in signaled.iter().chain(fallback.iter()) {
        if out.len() >= cap { break; }
        out.push((s.bar.ts_code.clone(), s.bar.pct_chg, s.bar.amount));
    }
    out
}
```

  **3b. 更新调用点** — `src-tauri/src/invest/premarket/cache_builder.rs` L109：

```rust
    let candidates = select_candidates(&daily, &sentiment_symbols, CANDIDATE_CAP, &net_map);
```

  （`net_map` 在 L84-93 已构建，在 L109 之前就绪，顺序 OK。前向 hook：B3 未来会在 `net_map` 为空时给 `sections_status` 打降级标记，本任务不动。）

- [ ] 4. 因本机 Rust 测试跑不起来 (STATUS_ENTRYPOINT_NOT_FOUND)，用 cmd.exe 跑：

```bash
cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- invest::premarket::cache_builder::tests --nocapture"
```

  预期 5 个测试全部 PASS（S1/S2/多信号/降级/capital_score）。再跑 `cargo check --manifest-path src-tauri/Cargo.toml` 确认全项目编译。

- [ ] 5. `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` — 预期无 warning。若 `Scored` 结构 clippy 抱怨字段未用（`bar` 只读取内部字段），保留即可（有用途）；若报 `dead_code` 加 `#[allow(dead_code)]` 不必要——三个字段都有读取。

- [ ] 6. 提交：

```bash
git add src-tauri/src/invest/premarket/cache_builder.rs
git commit -m "feat(premarket): 候选池多信号化 (舆情∪主力净流入Top60∪涨幅兜底)"
```

**Forward hook (deferred to B3)**：当 `net_map` 空导致 S2 贡献 0 时，B3 需要在 `sections_status` 里记录 `moneyflow: degraded`，让前端可以显示降级徽标。B1 本身只做静默降级，不动 sections_status。


---

### Task B2: 五因子固定权重 (加 sector 因子槽位 + weight_source, 不做 regime 动态权重)

**依赖**: B1 已完成。**注意**: B2 只加 `sector` 因子槽位 + 权重管道 + `weight_source`;真正的 `sector_strength` 计算与 cache 列由 B5 落地。B2 时把 `sector_strength` 喂 `50.0` 中性占位,保证编译 + 测试绿。

**本机 Rust 测试注意**: 裸 `cargo test` 会挂 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)`;用 `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- <filter> --nocapture"` 或 `npm run rust:test`。

---

- [ ] 1. **先改测试** (TDD 失败步): 覆写 `src-tauri/src/invest/premarket/scoring.rs` 底部整个 `#[cfg(test)] mod tests` 块 (L121-181 范围) 为下面完整代码。改后编译会因 `FactorBreakdown` 字段不足 + `PremarketConfig` 缺 `weight_sector`/`weight_source` 而失败,这就是失败的红。

  完整替换后的测试模块:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      fn cfg_5() -> PremarketConfig {
          PremarketConfig {
              weight_sentiment: 0.25,
              weight_capital: 0.25,
              weight_technical: 0.20,
              weight_catalyst: 0.15,
              weight_sector: 0.15,
              weight_source: "auto".into(),
              threshold_s: 78.0,
              threshold_a: 62.0,
              threshold_b: 45.0,
          }
      }

      #[test]
      fn score_weights_five_factors() {
          let cfg = cfg_5();
          let f = FactorBreakdown {
              sentiment: 80.0,
              capital: 60.0,
              technical: 70.0,
              catalyst: 50.0,
              sector_strength: 50.0,
          };
          let s = score("000001", "平安银行", f, vec![], &cfg);
          // 80*0.25 + 60*0.25 + 70*0.20 + 50*0.15 + 50*0.15 = 20 + 15 + 14 + 7.5 + 7.5 = 64.0
          assert!((s.total - 64.0).abs() < 1e-6, "total={}", s.total);
      }

      #[test]
      fn score_sector_contributes_to_total() {
          let cfg = cfg_5();
          let low = FactorBreakdown {
              sentiment: 50.0,
              capital: 50.0,
              technical: 50.0,
              catalyst: 50.0,
              sector_strength: 0.0,
          };
          let high = FactorBreakdown {
              sentiment: 50.0,
              capital: 50.0,
              technical: 50.0,
              catalyst: 50.0,
              sector_strength: 100.0,
          };
          let s_low = score("X", "X", low, vec![], &cfg);
          let s_high = score("X", "X", high, vec![], &cfg);
          assert!(s_high.total > s_low.total, "sector should lift total");
          assert!((s_high.total - s_low.total - 15.0).abs() < 1e-6);
      }

      #[test]
      fn grade_of_thresholds() {
          let cfg = cfg_5();
          assert_eq!(grade_of(80.0, &cfg), "S");
          assert_eq!(grade_of(65.0, &cfg), "A");
          assert_eq!(grade_of(50.0, &cfg), "B");
          assert_eq!(grade_of(30.0, &cfg), "C");
      }

      #[test]
      fn default_config_sums_to_one() {
          let cfg = PremarketConfig::default();
          let sum = cfg.weight_sentiment
              + cfg.weight_capital
              + cfg.weight_technical
              + cfg.weight_catalyst
              + cfg.weight_sector;
          assert!((sum - 1.0).abs() < 1e-9, "default weights must sum to 1.0, got {}", sum);
          assert_eq!(cfg.weight_source, "auto");
      }
  }
  ```

- [ ] 2. 跑 `cargo check --manifest-path src-tauri/Cargo.toml` → **FAIL** (预期,证明红)。

- [ ] 3. 修 `src-tauri/src/invest/premarket/scoring.rs`,给出完整替换的三块:

  **PremarketConfig 结构 (替换 L5-15)**:

  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct PremarketConfig {
      pub weight_sentiment: f64,
      pub weight_capital: f64,
      pub weight_technical: f64,
      pub weight_catalyst: f64,
      pub weight_sector: f64,
      /// "auto" = 使用固定 baseline 权重; "manual" = 使用用户自定义权重
      pub weight_source: String,
      pub threshold_s: f64,
      pub threshold_a: f64,
      pub threshold_b: f64,
  }
  ```

  **Default impl (替换 L17-29)**:

  ```rust
  impl Default for PremarketConfig {
      fn default() -> Self {
          Self {
              weight_sentiment: 0.25,
              weight_capital: 0.25,
              weight_technical: 0.20,
              weight_catalyst: 0.15,
              weight_sector: 0.15,
              weight_source: "auto".to_string(),
              threshold_s: 78.0,
              threshold_a: 62.0,
              threshold_b: 45.0,
          }
      }
  }
  ```

  **FactorBreakdown (替换 L39-46)**:

  ```rust
  #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct FactorBreakdown {
      pub sentiment: f64,
      pub capital: f64,
      pub technical: f64,
      pub catalyst: f64,
      pub sector_strength: f64,
  }
  ```

  **score() 内部加权求和 (替换 L72-92 里 `let total = ...` 那段,保持函数签名与返回不变)**:

  ```rust
  pub fn score(
      symbol: &str,
      name: &str,
      factors: FactorBreakdown,
      missing: Vec<String>,
      cfg: &PremarketConfig,
  ) -> SymbolScore {
      let total = factors.sentiment * cfg.weight_sentiment
          + factors.capital * cfg.weight_capital
          + factors.technical * cfg.weight_technical
          + factors.catalyst * cfg.weight_catalyst
          + factors.sector_strength * cfg.weight_sector;
      let grade = grade_of(total, cfg).to_string();
      SymbolScore {
          symbol: symbol.to_string(),
          name: name.to_string(),
          factors,
          total,
          grade,
          missing,
      }
  }
  ```

- [ ] 4. 修 `src-tauri/src/invest/premarket/report.rs` `collect_scores_from_cache` (L26-60 里的 FactorBreakdown mapping):

  **Before**:

  ```rust
  let factors = FactorBreakdown {
      sentiment: c.sentiment,
      capital: c.capital,
      technical: c.technical,
      catalyst: c.catalyst,
  };
  ```

  **After**:

  ```rust
  let factors = FactorBreakdown {
      sentiment: c.sentiment,
      capital: c.capital,
      technical: c.technical,
      catalyst: c.catalyst,
      // B2: 中性占位, B5 接入真实板块强度后改为 c.sector_strength
      sector_strength: 50.0,
  };
  ```

- [ ] 5. 验证 Rust 侧:

  ```bash
  cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- invest::premarket::scoring --nocapture"
  cargo check  --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  ```

  三者必须全绿 (scoring 4 个测试全过, check 无错, clippy 零 warning)。

- [ ] 6. 前端 `src/lib/components/invest/PremarketReportTab.svelte` 改动 (逐块给出完整替换):

  **6a. inline `FactorBreakdown` (替换 L23-28)**:

  ```ts
  type FactorBreakdown = {
      sentiment: number;
      capital: number;
      technical: number;
      catalyst: number;
      sector_strength: number;
  };
  ```

  **6b. inline `PremarketConfig` (替换 L61-69)**:

  ```ts
  type PremarketConfig = {
      weight_sentiment: number;
      weight_capital: number;
      weight_technical: number;
      weight_catalyst: number;
      weight_sector: number;
      weight_source: 'auto' | 'manual';
      threshold_s: number;
      threshold_a: number;
      threshold_b: number;
  };
  ```

  **6c. `cfg` 默认值 (替换 L133-141)**:

  ```ts
  let cfg = $state<PremarketConfig>({
      weight_sentiment: 0.25,
      weight_capital: 0.25,
      weight_technical: 0.20,
      weight_catalyst: 0.15,
      weight_sector: 0.15,
      weight_source: 'auto',
      threshold_s: 78,
      threshold_a: 62,
      threshold_b: 45,
  });
  ```

  **6d. `weightSum` / `weightSumOk` / `cfgValid` (替换 L146-151)**:

  ```ts
  const weightSum = $derived(
      cfg.weight_sentiment
          + cfg.weight_capital
          + cfg.weight_technical
          + cfg.weight_catalyst
          + cfg.weight_sector,
  );
  // auto 模式:后端用 baseline,前端和值免检
  const weightSumOk = $derived(cfg.weight_source === 'auto' || Math.abs(weightSum - 1) < 0.001);
  const thresholdsOk = $derived(cfg.threshold_s > cfg.threshold_a && cfg.threshold_a > cfg.threshold_b);
  const cfgValid = $derived(weightSumOk && thresholdsOk);
  ```

  > 依赖提示:如果 B4 已经从这份文件里删掉 `thresholdsOk` 和 threshold 输入,那这里只保留 `weightSumOk` 那一行,`cfgValid = weightSumOk`;否则按上面写。

  **6e. Settings 面板 (在原有 4 个 weight 输入之后追加第 5 个 + 顶部加 auto/manual 切换,镜像已有 `.settings-item` 模式)**:

  在权重区块 (L358 附近 `<h4>权重</h4>` 之后, 4 个 weight input 之前) 插入 auto/manual 切换:

  ```svelte
  <div class="settings-item settings-item--full">
      <label for="cfg-weight-source">权重来源</label>
      <select
          id="cfg-weight-source"
          bind:value={cfg.weight_source}
      >
          <option value="auto">自动 (固定 baseline 0.25/0.25/0.20/0.15/0.15)</option>
          <option value="manual">手动</option>
      </select>
  </div>
  ```

  4 个已有 weight input 各加 `disabled={cfg.weight_source === 'auto'}` (镜像现有 L365-380 pattern),并在其后追加第 5 个:

  ```svelte
  <div class="settings-item">
      <label for="cfg-weight-sector">板块 (sector)</label>
      <input
          id="cfg-weight-sector"
          type="number"
          step="0.05"
          min="0"
          max="1"
          bind:value={cfg.weight_sector}
          disabled={cfg.weight_source === 'auto'}
      />
  </div>
  ```

  weightSum 提示 (L395-401) 改成:

  ```svelte
  {#if cfg.weight_source === 'manual'}
      <p class="hint" class:hint--error={!weightSumOk}>
          权重和: {weightSum.toFixed(2)} {weightSumOk ? '' : '(需为 1.00)'}
      </p>
  {:else}
      <p class="hint">自动模式:后端使用固定 baseline 权重,输入框已禁用</p>
  {/if}
  ```

  > CSS `.settings-grid { grid-template-columns: repeat(4, 1fr); }` (L782) 保持不变;第 5 个输入会自然换行,可接受。

- [ ] 7. 前端验证:

  ```bash
  npm run check
  npm run build
  ```

  两者全绿。

- [ ] 8. Commit:

  ```
  feat(premarket): 五因子固定权重 (加 sector 槽位 0.15 + weight_source auto/manual)
  ```


---

### Task B5a: akshare_sector.py 新增东财板块成分 + 强度端点

**Files**

Modify: `src-tauri/python-runtime/scripts/providers/akshare_sector.py` — add two top-level fns after existing `sector_fund_flow` (currently at L242).

No `server.py` change needed — dispatcher does `getattr(module, func_name)`.

**Steps**

- [ ] Append the following two top-level functions to `src-tauri/python-runtime/scripts/providers/akshare_sector.py` (place after `sector_fund_flow` definition, before the module-end):

```python
def board_cons_em(board_type=None):
    """
    东财板块成分（industry + concept）。
    board_type: None=both, "industry", "concept"（分批便于恢复/日志）。
    返回 [{"ts_code": "600519", "board_name": "白酒", "board_type": "industry"}, ...]
    单板块失败仅 _warn 后继续，不中断整体。
    """
    try:
        import akshare as ak
    except Exception as e:
        _warn(f"akshare_sector.board_cons_em: akshare 导入失败 {e}")
        return []

    def _pick_code_col(df):
        for cand in ("代码", "成分券代码", "股票代码", "证券代码", "code"):
            if cand in df.columns:
                return cand
        for c in df.columns:
            cs = str(c)
            if "代码" in cs or cs.lower() == "code":
                return c
        return None

    def _six(x):
        s = str(x).strip()
        if "." in s:
            s = s.split(".")[0]
        s = "".join(ch for ch in s if ch.isdigit())
        return s.zfill(6)[-6:] if s else ""

    rows = []

    if board_type in (None, "industry"):
        try:
            name_df = ak.stock_board_industry_name_em()
            name_col = None
            for cand in ("板块名称", "名称", "name"):
                if cand in name_df.columns:
                    name_col = cand
                    break
            if name_col is None:
                _warn(f"board_cons_em: 行业板块名称列缺失, cols={list(name_df.columns)}")
            else:
                names = [str(x) for x in name_df[name_col].tolist()]
                for board_name in names:
                    try:
                        cons_df = ak.stock_board_industry_cons_em(symbol=board_name)
                        col = _pick_code_col(cons_df)
                        if col is None:
                            _warn(f"board_cons_em industry '{board_name}' 代码列缺失, cols={list(cons_df.columns)}")
                            continue
                        for v in cons_df[col].tolist():
                            code = _six(v)
                            if code:
                                rows.append({
                                    "ts_code": code,
                                    "board_name": board_name,
                                    "board_type": "industry",
                                })
                    except Exception as e:
                        _warn(f"board_cons_em industry '{board_name}' 失败: {e}")
                        continue
        except Exception as e:
            _warn(f"board_cons_em industry name_em 失败: {e}")

    if board_type in (None, "concept"):
        try:
            name_df = ak.stock_board_concept_name_em()
            name_col = None
            for cand in ("板块名称", "名称", "name"):
                if cand in name_df.columns:
                    name_col = cand
                    break
            if name_col is None:
                _warn(f"board_cons_em: 概念板块名称列缺失, cols={list(name_df.columns)}")
            else:
                names = [str(x) for x in name_df[name_col].tolist()]
                for board_name in names:
                    try:
                        cons_df = ak.stock_board_concept_cons_em(symbol=board_name)
                        col = _pick_code_col(cons_df)
                        if col is None:
                            _warn(f"board_cons_em concept '{board_name}' 代码列缺失, cols={list(cons_df.columns)}")
                            continue
                        for v in cons_df[col].tolist():
                            code = _six(v)
                            if code:
                                rows.append({
                                    "ts_code": code,
                                    "board_name": board_name,
                                    "board_type": "concept",
                                })
                    except Exception as e:
                        _warn(f"board_cons_em concept '{board_name}' 失败: {e}")
                        continue
        except Exception as e:
            _warn(f"board_cons_em concept name_em 失败: {e}")

    return rows


def sector_strength_em():
    """
    东财板块强度（今日 change_pct + 主力净流入净额）。
    industry + concept 合并返回:
      [{"board_name": "...", "board_type": "industry"|"concept",
        "change_pct": float|None, "net_amount": float|None (亿元)}]
    """
    try:
        import akshare as ak
    except Exception as e:
        _warn(f"akshare_sector.sector_strength_em: akshare 导入失败 {e}")
        return []

    def _pick(df, cands):
        for c in cands:
            if c in df.columns:
                return c
        return None

    def _to_float(v):
        try:
            if v is None:
                return None
            f = float(v)
            if f != f:  # NaN
                return None
            return f
        except Exception:
            return None

    def _yuan_to_yi(v):
        f = _to_float(v)
        if f is None:
            return None
        # akshare 该接口净额单位为「元」; 归一到「亿元」
        return f / 1e8

    out = []
    for stype, label in (("行业资金流", "industry"), ("概念资金流", "concept")):
        try:
            df = ak.stock_sector_fund_flow_rank(indicator="今日", sector_type=stype)
            name_col = _pick(df, ["名称", "板块名称", "行业名称", "概念名称"])
            pct_col = _pick(df, ["今日涨跌幅", "涨跌幅"])
            net_col = _pick(df, ["今日主力净流入-净额", "主力净流入-净额", "主力净流入净额"])
            if name_col is None:
                _warn(f"sector_strength_em {label}: 名称列缺失, cols={list(df.columns)}")
                continue
            for _, row in df.iterrows():
                bname = str(row.get(name_col, "")).strip()
                if not bname:
                    continue
                out.append({
                    "board_name": bname,
                    "board_type": label,
                    "change_pct": _to_float(row.get(pct_col)) if pct_col else None,
                    "net_amount": _yuan_to_yi(row.get(net_col)) if net_col else None,
                })
        except Exception as e:
            _warn(f"sector_strength_em {label} 失败: {e}")
            continue
    return out
```

- [ ] Manual smoke (no pytest infra in python-runtime):
  ```powershell
  D:\ClaudeWorkspace\Code\ClawGO\src-tauri\python-runtime\python.exe -c "import sys; sys.path.insert(0, r'D:\ClaudeWorkspace\Code\ClawGO\src-tauri\python-runtime\scripts\providers'); import akshare_sector as m; s = m.sector_strength_em(); print('strength rows:', len(s), s[:2]); c = m.board_cons_em(board_type='industry'); print('industry cons rows:', len(c), c[:2])"
  ```
  Expected: `strength rows: >0`, first item has non-null `change_pct`; `industry cons rows: 数千`, first item has 6-digit `ts_code`. Full B5b Rust integration will re-verify.

**Verification**

`python -c "..."` smoke above returns non-empty lists. No pytest infra to run.

**Commit**

`feat(premarket): akshare 东财板块成分+强度端点 (B5 数据源)`

---

### Task B5b: Rust 封装 B5 东财取数

**Files**

Create: `src-tauri/src/invest/premarket/sector_em.rs`
Modify: `src-tauri/src/invest/premarket/mod.rs` — register `pub mod sector_em;`

**Steps**

- [ ] Create `src-tauri/src/invest/premarket/sector_em.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardMembership {
    pub ts_code: String,
    pub board_name: String,
    pub board_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardStrength {
    pub board_name: String,
    pub board_type: String,
    #[serde(default)]
    pub change_pct: Option<f64>,
    #[serde(default)]
    pub net_amount: Option<f64>,
}

/// 拉取东财板块成分。
/// board_type: None=industry+concept 全量; Some("industry")/Some("concept") 分批。
pub async fn fetch_board_membership(
    board_type: Option<&str>,
) -> Result<Vec<BoardMembership>, String> {
    let runtime = crate::python::require()?;
    let params = match board_type {
        Some(t) => serde_json::json!({ "board_type": t }),
        None => serde_json::json!({ "board_type": null }),
    };
    let value = runtime
        .call("akshare_sector.board_cons_em", params)
        .await?;
    serde_json::from_value::<Vec<BoardMembership>>(value)
        .map_err(|e| format!("parse akshare_sector.board_cons_em: {e}"))
}

/// 拉取东财板块今日强度（industry+concept 合并）。
pub async fn fetch_sector_strength_em() -> Result<Vec<BoardStrength>, String> {
    let runtime = crate::python::require()?;
    let value = runtime
        .call("akshare_sector.sector_strength_em", serde_json::json!({}))
        .await?;
    serde_json::from_value::<Vec<BoardStrength>>(value)
        .map_err(|e| format!("parse akshare_sector.sector_strength_em: {e}"))
}
```

- [ ] Edit `src-tauri/src/invest/premarket/mod.rs` — add `pub mod sector_em;` next to the existing module registrations.

- [ ] Run:
  ```
  cargo check --manifest-path src-tauri/Cargo.toml
  ```
  Expected: clean build, no warnings for the new module.

**Verification**

`cargo check` passes.

**Commit**

`feat(premarket): Rust 封装东财板块成分/强度取数`

---

### Task B5c: stock_board_map 表 + 模块

**Files**

Create: `src-tauri/src/storage/invest/stock_board_map.rs`
Modify: `src-tauri/src/storage/invest/mod.rs` — register `pub mod stock_board_map;` and add DDL to init around L371.

**Steps**

- [ ] Create `src-tauri/src/storage/invest/stock_board_map.rs`:

```rust
use crate::storage::invest::{with_conn, with_conn_mut};
use rusqlite::{params, Connection};
use std::collections::HashMap;

pub const CREATE_STOCK_BOARD_MAP_TABLE: &str = "\
CREATE TABLE IF NOT EXISTS stock_board_map (\
    ts_code TEXT NOT NULL,\
    board_name TEXT NOT NULL,\
    board_type TEXT NOT NULL,\
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),\
    PRIMARY KEY (ts_code, board_type, board_name)\
);\
CREATE INDEX IF NOT EXISTS idx_board_map_name ON stock_board_map(board_name);";

// ---------- pure connection helpers (testable) ----------

pub fn replace_board_type_on(
    conn: &mut Connection,
    board_type: &str,
    rows: &[(String, String)],
) -> Result<usize, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM stock_board_map WHERE board_type = ?1",
        params![board_type],
    )
    .map_err(|e| e.to_string())?;
    let mut n = 0usize;
    {
        let mut stmt = tx
            .prepare(
                "INSERT OR REPLACE INTO stock_board_map \
                 (ts_code, board_name, board_type, updated_at) \
                 VALUES (?1, ?2, ?3, datetime('now'))",
            )
            .map_err(|e| e.to_string())?;
        for (ts_code, board_name) in rows {
            if ts_code.is_empty() || board_name.is_empty() {
                continue;
            }
            stmt.execute(params![ts_code, board_name, board_type])
                .map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n)
}

pub fn boards_of_on(
    conn: &Connection,
    code6: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT board_name, board_type FROM stock_board_map \
             WHERE ts_code = ?1 ORDER BY board_type, board_name",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![code6], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

pub fn all_board_maps_on(
    conn: &Connection,
) -> Result<HashMap<String, Vec<(String, String)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT ts_code, board_name, board_type FROM stock_board_map \
             ORDER BY ts_code",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut map: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for r in rows {
        let (ts, bname, btype) = r.map_err(|e| e.to_string())?;
        map.entry(ts).or_default().push((bname, btype));
    }
    Ok(map)
}

pub fn board_map_count_on(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT COUNT(*) FROM stock_board_map", [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

// ---------- with_conn / with_conn_mut wrappers ----------

pub fn replace_board_type(
    board_type: &str,
    rows: &[(String, String)],
) -> Result<usize, String> {
    with_conn_mut(|conn| replace_board_type_on(conn, board_type, rows))
}

pub fn boards_of(code6: &str) -> Result<Vec<(String, String)>, String> {
    with_conn(|conn| boards_of_on(conn, code6))
}

pub fn all_board_maps() -> Result<HashMap<String, Vec<(String, String)>>, String> {
    with_conn(|conn| all_board_maps_on(conn))
}

pub fn board_map_count() -> Result<i64, String> {
    with_conn(|conn| board_map_count_on(conn))
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open memory");
        conn.execute_batch(CREATE_STOCK_BOARD_MAP_TABLE).unwrap();
        conn
    }

    #[test]
    fn boards_of_after_insert_returns_all_types() {
        let mut conn = setup();
        let industry = vec![
            ("600519".to_string(), "白酒".to_string()),
            ("600519".to_string(), "食品饮料".to_string()),
        ];
        let concept = vec![
            ("600519".to_string(), "消费龙头".to_string()),
            ("600519".to_string(), "MSCI".to_string()),
        ];
        assert_eq!(replace_board_type_on(&mut conn, "industry", &industry).unwrap(), 2);
        assert_eq!(replace_board_type_on(&mut conn, "concept", &concept).unwrap(), 2);
        let got = boards_of_on(&conn, "600519").unwrap();
        assert_eq!(got.len(), 4);
        assert!(got.iter().any(|(n, t)| n == "白酒" && t == "industry"));
        assert!(got.iter().any(|(n, t)| n == "MSCI" && t == "concept"));
    }

    #[test]
    fn replace_board_type_leaves_other_type_intact() {
        let mut conn = setup();
        let industry = vec![("600519".to_string(), "白酒".to_string())];
        let concept = vec![("600519".to_string(), "消费龙头".to_string())];
        replace_board_type_on(&mut conn, "industry", &industry).unwrap();
        replace_board_type_on(&mut conn, "concept", &concept).unwrap();
        // rewrite concept only
        let concept_new = vec![("600519".to_string(), "白马股".to_string())];
        replace_board_type_on(&mut conn, "concept", &concept_new).unwrap();
        let got = boards_of_on(&conn, "600519").unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|(n, t)| n == "白酒" && t == "industry"));
        assert!(got.iter().any(|(n, t)| n == "白马股" && t == "concept"));
        assert!(!got.iter().any(|(n, _)| n == "消费龙头"));
    }

    #[test]
    fn all_board_maps_groups_by_code() {
        let mut conn = setup();
        let industry = vec![
            ("600519".to_string(), "白酒".to_string()),
            ("000001".to_string(), "银行".to_string()),
        ];
        replace_board_type_on(&mut conn, "industry", &industry).unwrap();
        let map = all_board_maps_on(&conn).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("600519").unwrap().len(), 1);
        assert_eq!(board_map_count_on(&conn).unwrap(), 2);
    }
}
```

- [ ] Edit `src-tauri/src/storage/invest/mod.rs`:
  - Near L11 (module list): add `pub mod stock_board_map;`
  - Inside the init block near L371 (right after the existing `conn.execute_batch(stock_industry::CREATE_STOCK_INDUSTRY_TABLE)` call), add:
    ```rust
    conn.execute_batch(stock_board_map::CREATE_STOCK_BOARD_MAP_TABLE)
        .map_err(|e| e.to_string())?;
    ```
    (match the surrounding error-handling style — mirror the stock_industry call exactly.)

- [ ] Run tests via cmd.exe (see machine note):
  ```
  cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- storage::invest::stock_board_map:: --nocapture"
  ```
  Expected: 3 tests pass.

**Verification**

`cargo test` on `storage::invest::stock_board_map` passes; `cargo check --manifest-path src-tauri/Cargo.toml` clean.

**Commit**

`feat(invest): stock_board_map 一对多板块映射表 + CRUD`

---

### Task B5d: 刷新命令 + 每周 cron

**Files**

Create: `src-tauri/src/invest/premarket/board_refresh.rs`
Modify: `src-tauri/src/invest/premarket/mod.rs` (register), `src-tauri/src/commands/invest.rs` (command wrapper), `src-tauri/src/lib.rs` (register command), `src-tauri/src/invest/scheduler/mod.rs` (default_jobs), `src-tauri/src/invest/scheduler/runner.rs` (dispatch arm).

**Steps**

- [ ] Create `src-tauri/src/invest/premarket/board_refresh.rs`:

```rust
use crate::invest::premarket::sector_em;
use crate::storage::invest::stock_board_map;
use std::collections::HashSet;

/// 刷新东财板块映射（industry + concept 分批 replace）。
/// 返回覆盖的 distinct ts_code 数。
pub async fn refresh_stock_board_map() -> Result<usize, String> {
    let mut all_codes: HashSet<String> = HashSet::new();

    for board_type in ["industry", "concept"] {
        let memberships = sector_em::fetch_board_membership(Some(board_type)).await?;
        let mut rows: Vec<(String, String)> = Vec::with_capacity(memberships.len());
        for m in &memberships {
            if m.ts_code.is_empty() || m.board_name.is_empty() {
                continue;
            }
            all_codes.insert(m.ts_code.clone());
            rows.push((m.ts_code.clone(), m.board_name.clone()));
        }
        let n = stock_board_map::replace_board_type(board_type, &rows)?;
        tracing::info!(
            board_type = board_type,
            written = n,
            "stock_board_map refreshed"
        );
    }

    let total = all_codes.len();
    tracing::info!(coverage_codes = total, "stock_board_map coverage");
    Ok(total)
}
```

- [ ] Edit `src-tauri/src/invest/premarket/mod.rs`: add `pub mod board_refresh;` alongside `sector_em`.

- [ ] Edit `src-tauri/src/commands/invest.rs` — add near the existing `refresh_stock_industry_cmd` (L1327):

```rust
#[tauri::command]
pub async fn refresh_stock_board_map_cmd() -> Result<usize, String> {
    crate::invest::premarket::board_refresh::refresh_stock_board_map().await
}
```

- [ ] Edit `src-tauri/src/lib.rs` — in the `invoke_handler`/`generate_handler!` list near L485, add `commands::invest::refresh_stock_board_map_cmd,` next to `refresh_stock_industry_cmd`.

- [ ] Edit `src-tauri/src/invest/scheduler/mod.rs` `default_jobs()` — append this CronJob literal (match the surrounding CronJob struct field order):

```rust
CronJob {
    id: "stock_board_map_refresh".to_string(),
    name: "板块映射刷新".to_string(),
    cron_expr: "0 0 4 * * 1".to_string(), // 每周一 04:00:00 (sec min hour day month dow)
    interval_min: None,
    enabled: true,
    requires_trading_day: false,
    last_run: None,
    next_run: None,
    last_status: None,
    description: "每周刷新东财 industry+concept 板块成分映射表 stock_board_map".to_string(),
    dedicated: false,
},
```
(If any listed field name here does not match a struct field literally present in `CronJob`, drop that field to match the surrounding entries — do NOT invent fields. The `id`/`name`/`cron_expr`/`enabled`/`requires_trading_day`/`description` names are the required core; mirror the file for the rest.)

- [ ] Edit `src-tauri/src/invest/scheduler/runner.rs` `dispatch_job` — add before the `_ => Err(...)` arm:

```rust
"stock_board_map_refresh" => {
    let n = crate::invest::premarket::board_refresh::refresh_stock_board_map().await?;
    Ok(format!("板块映射刷新: {} 只", n))
}
```

- [ ] Run:
  ```
  cargo check --manifest-path src-tauri/Cargo.toml
  cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
  ```
  Expected: clean.

**Verification**

`cargo check` + `cargo clippy` clean. Manual dispatch smoke can be triggered later via the scheduler UI or by calling `refresh_stock_board_map_cmd` from frontend.

**Commit**

`feat(invest): 板块映射每周刷新 cron + 手动命令`

---

### Task B5e: build_cache 算 sector_strength + 缓存列迁移

**Files**

Modify:
1. `src-tauri/src/storage/invest/premarket_cache.rs`
2. `src-tauri/src/invest/premarket/cache_builder.rs`
3. `src-tauri/src/invest/premarket/report.rs`

**Steps — Part 1: premarket_cache.rs migration + I/O**

- [ ] Add field to `CachedFactor` (L21-32). Before:

```rust
#[derive(Debug, Clone)]
pub struct CachedFactor {
    pub symbol: String,
    pub name: Option<String>,
    pub change_pct: f64,
    pub amount: f64,
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
    pub missing: Vec<String>,
}
```
After:

```rust
#[derive(Debug, Clone)]
pub struct CachedFactor {
    pub symbol: String,
    pub name: Option<String>,
    pub change_pct: f64,
    pub amount: f64,
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
    pub sector_strength: f64,
    pub missing: Vec<String>,
}
```

- [ ] Add idempotent column migration helper (place above `create_table`):

```rust
fn ensure_sector_strength_column(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(premarket_factor_cache)")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?;
    let mut has_col = false;
    for r in rows {
        if r.map_err(|e| e.to_string())? == "sector_strength" {
            has_col = true;
            break;
        }
    }
    if !has_col {
        conn.execute(
            "ALTER TABLE premarket_factor_cache ADD COLUMN sector_strength REAL NOT NULL DEFAULT 50",
            [],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

- [ ] Update `create_table` (L34-37). Before:

```rust
pub fn create_table() -> Result<(), String> {
    with_conn(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL).map_err(|e| e.to_string())?;
        Ok(())
    })
}
```
After:

```rust
pub fn create_table() -> Result<(), String> {
    with_conn(|conn| {
        conn.execute_batch(CREATE_TABLE_SQL).map_err(|e| e.to_string())?;
        ensure_sector_strength_column(conn)?;
        Ok(())
    })
}
```

- [ ] Update `save_factor_cache` (L53-81). The INSERT SQL and params must be extended.

Before (INSERT statement + params):
```rust
"INSERT INTO premarket_factor_cache
   (trade_date, symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing, cached_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
 ON CONFLICT(trade_date, symbol) DO UPDATE SET
    name = excluded.name,
    change_pct = excluded.change_pct,
    amount = excluded.amount,
    sentiment = excluded.sentiment,
    capital = excluded.capital,
    technical = excluded.technical,
    catalyst = excluded.catalyst,
    missing = excluded.missing,
    cached_at = excluded.cached_at",
params![trade_date, r.symbol, r.name, r.change_pct, r.amount, r.sentiment, r.capital, r.technical, r.catalyst, r.missing.join(",")]
```

After:
```rust
"INSERT INTO premarket_factor_cache
   (trade_date, symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing, sector_strength, cached_at)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, datetime('now'))
 ON CONFLICT(trade_date, symbol) DO UPDATE SET
    name = excluded.name,
    change_pct = excluded.change_pct,
    amount = excluded.amount,
    sentiment = excluded.sentiment,
    capital = excluded.capital,
    technical = excluded.technical,
    catalyst = excluded.catalyst,
    missing = excluded.missing,
    sector_strength = excluded.sector_strength,
    cached_at = excluded.cached_at",
params![
    trade_date,
    r.symbol,
    r.name,
    r.change_pct,
    r.amount,
    r.sentiment,
    r.capital,
    r.technical,
    r.catalyst,
    r.missing.join(","),
    r.sector_strength,
]
```

- [ ] Update `load_latest_cache` (L84-125) SELECT list + row mapper.

Before:
```rust
"SELECT symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing
   FROM premarket_factor_cache
  WHERE trade_date = ?1
  ORDER BY change_pct DESC"
```
```rust
Ok(CachedFactor {
    symbol: row.get(0)?,
    name: row.get(1)?,
    change_pct: row.get(2)?,
    amount: row.get(3)?,
    sentiment: row.get(4)?,
    capital: row.get(5)?,
    technical: row.get(6)?,
    catalyst: row.get(7)?,
    missing: row.get::<_, String>(8)?
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect(),
})
```

After:
```rust
"SELECT symbol, name, change_pct, amount, sentiment, capital, technical, catalyst, missing, sector_strength
   FROM premarket_factor_cache
  WHERE trade_date = ?1
  ORDER BY change_pct DESC"
```
```rust
Ok(CachedFactor {
    symbol: row.get(0)?,
    name: row.get(1)?,
    change_pct: row.get(2)?,
    amount: row.get(3)?,
    sentiment: row.get(4)?,
    capital: row.get(5)?,
    technical: row.get(6)?,
    catalyst: row.get(7)?,
    missing: row.get::<_, String>(8)?
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect(),
    sector_strength: row.get(9)?,
})
```

**Steps — Part 2: cache_builder.rs — compute sector_strength**

- [ ] Add imports at top of `src-tauri/src/invest/premarket/cache_builder.rs` (alongside existing L1-8):

```rust
use crate::invest::premarket::sector_em;
use crate::storage::invest::stock_board_map;
use std::collections::HashMap;
```

- [ ] Add helper `percentile_rank` + `median` near the top of the file (below imports, above `build_cache`):

```rust
/// 数组已升序排列, 返回 v 在数组中的百分位 0..=100。
/// 若数组为空, 返回 50 (中性)。
/// 采用 `<= v` 的数量 / 总数 * 100（含并列）。
pub(crate) fn percentile_rank(sorted_asc: &[f64], v: f64) -> f64 {
    if sorted_asc.is_empty() {
        return 50.0;
    }
    let n = sorted_asc.len();
    // partition_point 需要 f64 全序 (无 NaN); 调用方需过滤 NaN。
    let count_le = sorted_asc.partition_point(|x| *x <= v);
    (count_le as f64) / (n as f64) * 100.0
}

pub(crate) fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 50.0;
    }
    let mut v: Vec<f64> = values.iter().copied().filter(|x| !x.is_nan()).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n == 0 {
        50.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests_sector_math {
    use super::*;

    #[test]
    fn percentile_rank_empty_neutral() {
        assert!((percentile_rank(&[], 5.0) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn percentile_rank_monotone() {
        let v = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let p_lo = percentile_rank(&v, 1.0);
        let p_mid = percentile_rank(&v, 3.0);
        let p_hi = percentile_rank(&v, 5.0);
        assert!(p_lo < p_mid);
        assert!(p_mid < p_hi);
        assert!((p_hi - 100.0).abs() < 1e-9);
    }

    #[test]
    fn median_odd_even() {
        assert!((median(&[1.0, 3.0, 2.0]) - 2.0).abs() < 1e-9);
        assert!((median(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
        assert!((median(&[]) - 50.0).abs() < 1e-9);
    }
}
```

- [ ] In `build_cache` (L77-162), BEFORE the candidate loop at L132, insert the board-strength computation. Replace the existing pre-loop preamble section (right after `net_map` is built and `daily` is available) with the following additions (keep everything else — the candidate loop, `code6` closure — intact):

```rust
    // ---------- B5: 计算每板块强度 + 每股 sector_strength ----------
    // 1) 拉板块 today 强度 (industry+concept 合并)。取数失败降级为空。
    let strengths = sector_em::fetch_sector_strength_em()
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "sector_strength_em failed, sector factor will fallback");
            Vec::new()
        });

    // 2) 板块映射: ts_code(6) -> Vec<(board_name, board_type)>
    let board_map: HashMap<String, Vec<(String, String)>> =
        stock_board_map::all_board_maps().unwrap_or_default();

    // 3) 全市场 pct_chg 数组 (用于板块内相对强度需要 daily 支持)
    //    以及 ts_code(6) -> pct_chg 索引
    let mut daily_pct: HashMap<String, f64> = HashMap::new();
    for d in &daily {
        let c6 = d.ts_code.split('.').next().unwrap_or(&d.ts_code).to_string();
        if !c6.is_empty() && !d.pct_chg.is_nan() {
            daily_pct.insert(c6, d.pct_chg);
        }
    }

    // 4) 板块 -> members: Vec<(ts_code6, pct_chg)>
    //    反向索引 board_map。
    let mut board_members: HashMap<(String, String), Vec<(String, f64)>> = HashMap::new(); // (name,type) -> [(code, pct)]
    for (code, boards) in &board_map {
        if let Some(&pct) = daily_pct.get(code) {
            for (bname, btype) in boards {
                board_members
                    .entry((bname.clone(), btype.clone()))
                    .or_default()
                    .push((code.clone(), pct));
            }
        }
    }

    // 5) 每板块的 change_pct / net_amount 数组 -> 板块层截面分位
    //    (industry 与 concept 独立各自 rank, 避免行业 vs 概念口径混淆)
    let mut strength_by_key: HashMap<(String, String), (Option<f64>, Option<f64>)> = HashMap::new();
    for s in &strengths {
        strength_by_key.insert(
            (s.board_name.clone(), s.board_type.clone()),
            (s.change_pct, s.net_amount),
        );
    }

    let build_sorted = |vals: Vec<f64>| -> Vec<f64> {
        let mut v: Vec<f64> = vals.into_iter().filter(|x| !x.is_nan()).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    };

    let mut industry_pct_sorted: Vec<f64> = Vec::new();
    let mut industry_net_sorted: Vec<f64> = Vec::new();
    let mut concept_pct_sorted: Vec<f64> = Vec::new();
    let mut concept_net_sorted: Vec<f64> = Vec::new();
    for s in &strengths {
        if s.board_type == "industry" {
            if let Some(v) = s.change_pct { industry_pct_sorted.push(v); }
            if let Some(v) = s.net_amount { industry_net_sorted.push(v); }
        } else if s.board_type == "concept" {
            if let Some(v) = s.change_pct { concept_pct_sorted.push(v); }
            if let Some(v) = s.net_amount { concept_net_sorted.push(v); }
        }
    }
    let industry_pct_sorted = build_sorted(industry_pct_sorted);
    let industry_net_sorted = build_sorted(industry_net_sorted);
    let concept_pct_sorted = build_sorted(concept_pct_sorted);
    let concept_net_sorted = build_sorted(concept_net_sorted);

    // board_key -> (pct_percentile, net_percentile) 0..=100
    let mut board_percentiles: HashMap<(String, String), (f64, f64)> = HashMap::new();
    for (key, (pct, net)) in &strength_by_key {
        let (pct_sorted, net_sorted) = match key.1.as_str() {
            "industry" => (&industry_pct_sorted, &industry_net_sorted),
            "concept" => (&concept_pct_sorted, &concept_net_sorted),
            _ => continue,
        };
        let p_pct = pct.map(|v| percentile_rank(pct_sorted, v)).unwrap_or(50.0);
        let p_net = net.map(|v| percentile_rank(net_sorted, v)).unwrap_or(50.0);
        board_percentiles.insert(key.clone(), (p_pct, p_net));
    }

    // 计算「板块 ex-self」强度: 用板块内 pct 均值 (剔除该股) 做近似截面分位。
    // 说明: 真正的 ex-self net_amount 需要个股→板块净流入贡献, 数据源未提供;
    // 此处以 pct 均值 ex-self 做代理, 保留正交性 (不用该股自身 pct)。
    let ex_self_score = |board_key: &(String, String), self_pct: Option<f64>| -> f64 {
        let members = match board_members.get(board_key) {
            Some(v) => v,
            None => return board_percentiles.get(board_key).map(|(a, b)| (a + b) / 2.0).unwrap_or(50.0),
        };
        if members.len() <= 1 {
            return board_percentiles.get(board_key).map(|(a, b)| (a + b) / 2.0).unwrap_or(50.0);
        }
        let (sum, cnt) = members.iter().fold((0.0f64, 0usize), |(s, c), (_, p)| (s + p, c + 1));
        let (adj_sum, adj_cnt) = if let Some(sp) = self_pct {
            (sum - sp, cnt - 1)
        } else {
            (sum, cnt)
        };
        let mean_ex = if adj_cnt > 0 { adj_sum / (adj_cnt as f64) } else { 0.0 };
        // 拿到 ex-self 均值在同类型板块 pct 分布中的分位; net 分位不动 (无法 ex-self)。
        let pct_sorted = match board_key.1.as_str() {
            "industry" => &industry_pct_sorted,
            "concept" => &concept_pct_sorted,
            _ => return 50.0,
        };
        let p_pct_ex = percentile_rank(pct_sorted, mean_ex);
        let p_net = board_percentiles.get(board_key).map(|(_, b)| *b).unwrap_or(50.0);
        (p_pct_ex + p_net) / 2.0
    };

    // 计算每股 sector_strength
    let compute_sector_strength = |code6: &str, self_pct: Option<f64>| -> Option<f64> {
        let boards = board_map.get(code6)?;
        if boards.is_empty() {
            return None;
        }
        let mut scores: Vec<f64> = Vec::new();
        for bkey in boards {
            let key = (bkey.0.clone(), bkey.1.clone());
            let members = match board_members.get(&key) {
                Some(v) if !v.is_empty() => v,
                _ => continue,
            };
            // 板块内相对强度: 该股 self_pct 在板块内 pct 分布的分位
            let sp = match self_pct {
                Some(v) => v,
                None => continue,
            };
            let mut inner: Vec<f64> = members.iter().map(|(_, p)| *p).collect();
            inner.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let inner_rel = percentile_rank(&inner, sp);
            // 板块 ex-self 强度
            let ex = ex_self_score(&key, Some(sp));
            let single = inner_rel * ex / 100.0;
            scores.push(single);
        }
        if scores.is_empty() {
            return None;
        }
        scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let combined = if scores.len() == 1 {
            scores[0]
        } else {
            0.7 * scores[0] + 0.3 * scores[1]
        };
        Some(combined.clamp(0.0, 100.0))
    };

    // ---------- B5 end ----------
```

- [ ] Inside the candidate loop (L132-157), when constructing `CachedFactor`, compute + set the new field. Just before the `CachedFactor { ... }` literal, insert:

```rust
        let code6_val = code6(&candidate_ts_code);
        let self_pct = daily_pct.get(&code6_val).copied();
        let sector_strength_val = compute_sector_strength(&code6_val, self_pct);
        let sector_strength_final = sector_strength_val.unwrap_or_else(|| {
            // fallback: whole-market median of computed sector_strengths this run
            // computed lazily on first miss
            0.0 // placeholder replaced below via post-loop pass
        });
```

Because a true whole-market median must be computed AFTER a first pass, use a two-pass approach. Replace the previous single-loop cache assembly with:

```rust
    // First pass: build CachedFactor with sector_strength = Option<f64>
    let mut staging: Vec<(CachedFactor, Option<f64>)> = Vec::new();
    // (…existing candidate iteration; wherever a CachedFactor was pushed to `cached`,
    //  now compute sector_strength_val via compute_sector_strength(…) and push
    //  (CachedFactor { sector_strength: 0.0, .. }, sector_strength_val) into staging.
    //  The 0.0 is a temp; second pass overrides it.)
    // …
    // Second pass: compute dynamic fallback = median of Some(v) values, apply.
    let computed_vals: Vec<f64> = staging
        .iter()
        .filter_map(|(_, v)| *v)
        .collect();
    let fallback_median = median(&computed_vals);
    let coverage = if staging.is_empty() {
        0.0
    } else {
        (computed_vals.len() as f64) / (staging.len() as f64) * 100.0
    };
    tracing::info!(
        coverage_pct = format!("{coverage:.1}"),
        boards_industry = industry_pct_sorted.len(),
        boards_concept = concept_pct_sorted.len(),
        fallback_median = fallback_median,
        "sector_strength coverage"
    );
    let cached: Vec<CachedFactor> = staging
        .into_iter()
        .map(|(mut f, v)| {
            f.sector_strength = v.unwrap_or(fallback_median);
            f
        })
        .collect();
```

Note to implementer: the exact wiring depends on how `cache_builder.rs` currently builds its `cached: Vec<CachedFactor>`. The intent is:
1. During the candidate loop, compute `sector_strength_val: Option<f64>` per candidate and stash it alongside the CachedFactor whose `sector_strength` starts as any placeholder.
2. After the loop, replace placeholders using `fallback_median` for candidates whose `sector_strength_val` is None.
Preserve every existing field/computation; only add the new field's population + the post-loop finalize + the tracing::info log.

**Steps — Part 3: report.rs**

- [ ] In `src-tauri/src/invest/premarket/report.rs` `collect_scores_from_cache` (L26-60), change the ONE line:

Before:
```rust
sector_strength: 50.0,
```
After:
```rust
sector_strength: c.sector_strength,
```

**Verification**

```
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- invest::premarket::cache_builder::tests_sector_math --nocapture"
```

Expected: check + clippy clean; 3 sector_math unit tests pass. Also run a dev-only build_cache dispatch (via scheduler manual trigger) and confirm the `sector_strength coverage` tracing log fires with `coverage_pct` >= 60 on a real trading day (acceptance target is 80% but not a hard gate — flag if <60%).

**Commit**

`feat(premarket): 盘后缓存计算个股板块强度因子 + 缓存列迁移`

---

### Task B5f: 前端「板块强」chip

**Files**

Modify:
- `src/lib/components/invest/PremarketReportTab.svelte`
- `messages/zh-CN.json`
- `messages/en.json`

**Steps**

- [ ] Edit `src/lib/components/invest/PremarketReportTab.svelte` chips block (L687-700). Locate the technical chip (last existing chip with `>=60` gate) and add immediately after it:

```svelte
{#if s.factors.sector_strength >= 60}
    <span class="stk-tag">{t('invest_premarket_tag_sector')}</span>
{/if}
```

Full block (for reference — preserve every other existing chip):
```svelte
{#if s.factors.capital >= 60}
    <span class="stk-tag money">{t('invest_premarket_tag_capital')}</span>
{/if}
{#if s.factors.sentiment >= 60}
    <span class="stk-tag mood">{t('invest_premarket_tag_sentiment')}</span>
{/if}
{#if s.factors.catalyst >= 60}
    <span class="stk-tag">{t('invest_premarket_tag_catalyst')}</span>
{/if}
{#if s.factors.technical >= 60}
    <span class="stk-tag">{t('invest_premarket_tag_technical')}</span>
{/if}
{#if s.factors.sector_strength >= 60}
    <span class="stk-tag">{t('invest_premarket_tag_sector')}</span>
{/if}
```

- [ ] Edit `messages/zh-CN.json` — add near the other `invest_premarket_tag_*` keys:

```json
"invest_premarket_tag_sector": "板块强",
```

- [ ] Edit `messages/en.json` — add near the other `invest_premarket_tag_*` keys:

```json
"invest_premarket_tag_sector": "Strong sector",
```

- [ ] Run:
  ```
  npm run i18n:check
  npm run check
  npm run build
  ```
  Expected: `i18n:check` clean (both locales in sync), `check` clean, `build` succeeds.

**Verification**

`npm run check` + `npm run build` + `npm run i18n:check` all pass. Manual: open `/invest` premarket report on a stock with `factors.sector_strength >= 60` — the new「板块强」chip appears.

**Commit**

`feat(premarket): 04 模块新增「板块强」因子 chip`


---

### Task B3a: AiReview 结构 + `enable_ai_review` 配置项

**目标**：新增 `AiReview` 结构、给 `SymbolScore` 挂 `Option<AiReview>`、给 `PremarketConfig` 加 `enable_ai_review` 开关。仅结构与默认值，不接执行路径。

**架构硬约束**（贯穿 B3 全流程）：
- `SymbolScore.total` 与 `SymbolScore.grade` 永远由纯量化产出（B1+B2+B5 打分 + B4 名次切档），AI 结果绝不写回。
- `ai_review` 是 `SymbolScore` 上的可选旁路字段，`serde` 走 `skip_serializing_if = "Option::is_none"`——AI 关闭或未跑时不进 JSON。
- 结构体外壳保持 `#[serde(rename_all = "camelCase")]`，所以 wire 上 `ai_review`→`aiReview`、`risk_flag`→`riskFlag`。

**Files**：
- `src-tauri/src/invest/premarket/scoring.rs`
- `src/lib/components/invest/PremarketReportTab.svelte`

---

#### Step 1 — 写失败测试（TDD）

- [ ] 在 `scoring.rs` 的 `#[cfg(test)] mod tests` 里追加下面这条测试，验证：`SymbolScore` 序列化时若 `ai_review = None` 不产生 `aiReview` 键；若 `Some(...)` 则 `risk_flag` 字段在 wire 上叫 `riskFlag`。预期编译失败（`AiReview`/`ai_review` 尚不存在）。

```rust
    #[test]
    fn test_ai_review_serde_camelcase_and_skip_none() {
        // None → 序列化不含 aiReview 键
        let s_none = SymbolScore {
            symbol: "600519.SH".into(),
            name: "茅台".into(),
            total: 80.0,
            grade: Grade::S,
            factors: FactorBreakdown {
                sentiment: 80.0,
                capital: 80.0,
                technical: 80.0,
                catalyst: 80.0,
                sector_strength: 80.0,
            },
            missing_factors: vec![],
            ai_review: None,
        };
        let j = serde_json::to_string(&s_none).unwrap();
        assert!(!j.contains("aiReview"), "None 时不应出现 aiReview: {j}");

        // Some → 走 camelCase：risk_flag → riskFlag
        let s_some = SymbolScore {
            ai_review: Some(AiReview {
                action: "drop".into(),
                reason: "监管收紧".into(),
                risk_flag: "regulatory".into(),
            }),
            ..s_none
        };
        let j = serde_json::to_string(&s_some).unwrap();
        assert!(j.contains("\"aiReview\""), "应含 aiReview: {j}");
        assert!(j.contains("\"riskFlag\":\"regulatory\""), "应 camelCase: {j}");
        assert!(!j.contains("risk_flag"), "不应保留 snake_case: {j}");
    }

    #[test]
    fn test_premarket_config_default_enables_ai_review() {
        let cfg = PremarketConfig::default();
        assert!(cfg.enable_ai_review, "默认应开 AI 复核");
    }
```

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 应报 `AiReview` / `ai_review` / `enable_ai_review` 未定义。

---

#### Step 2 — 后端结构改动

- [ ] 打开 `src-tauri/src/invest/premarket/scoring.rs`，在 `SymbolScore` 定义正上方（L47 之前）插入 `AiReview` 结构：

```rust
/// AI 终选复核结果（B3）。挂在 `SymbolScore.ai_review` 上，仅展示/持久化用，
/// 绝不参与 total/grade 计算。
///
/// - `action`: "keep" | "drop"（其它值容错为 keep）
/// - `reason`: ≤30 汉字
/// - `risk_flag`: "none" | "regulatory" | "sentiment_only" | "weak_fundamental" | "other"
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiReview {
    pub action: String,
    pub reason: String,
    pub risk_flag: String,
}
```

- [ ] 把 `SymbolScore` 定义（L48-57）替换为：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolScore {
    pub symbol: String,
    pub name: String,
    pub total: f64,
    pub grade: Grade,
    pub factors: FactorBreakdown,
    pub missing_factors: Vec<String>,
    /// B3 AI 终选复核结果；None = 未跑/关闭/降级/熔断/未在本次输入内。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_review: Option<AiReview>,
}
```

- [ ] 把 `PremarketConfig` 加一个字段 `enable_ai_review: bool`，同时在 `Default` 里补默认 `true`。假设 B2 后 `PremarketConfig` 已含 `weight_sector` 与 `weight_source`，则调整后的 struct + Default 大致如下（**只关注新增行**，B1/B2/B5 已有字段维持不动）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremarketConfig {
    pub weight_sentiment: f64,
    pub weight_capital: f64,
    pub weight_technical: f64,
    pub weight_catalyst: f64,
    pub weight_sector: f64,   // B2
    pub weight_source: f64,   // B2
    pub threshold_s: f64,
    pub threshold_a: f64,
    pub threshold_b: f64,
    /// B3 AI 终选复核开关；false 时报告跳过 AI，纯量化 top20。
    pub enable_ai_review: bool,
}

impl Default for PremarketConfig {
    fn default() -> Self {
        Self {
            weight_sentiment: 0.30,
            weight_capital: 0.30,
            weight_technical: 0.25,
            weight_catalyst: 0.15,
            weight_sector: 0.10,   // B2 已落地值
            weight_source: 0.10,   // B2 已落地值
            threshold_s: 78.0,
            threshold_a: 62.0,
            threshold_b: 45.0,
            enable_ai_review: true,
        }
    }
}
```

- [ ] 打开 `scoring.rs::score()`（L72 起），把内部构造 `SymbolScore { ... }` 的字面量补一行 `ai_review: None,`：

```rust
    SymbolScore {
        symbol: symbol.to_string(),
        name: name.to_string(),
        total: (total * 100.0).round() / 100.0,
        grade,
        factors,
        missing_factors: missing,
        ai_review: None,
    }
```

- [ ] 已有单测 `test_grade_thresholds` / `test_grade_c_low` / `test_weighted_sum` / `test_missing_factor_recorded` 都是通过 `score()` 得到 `SymbolScore` 的，不用改。B4 的三条新测（`test_assign_grades_by_rank_*`）里手工 `mk()` 构造 `SymbolScore` 的辅助函数需要加 `ai_review: None`：

```rust
    fn mk(symbol: &str, total: f64) -> SymbolScore {
        SymbolScore {
            symbol: symbol.to_string(),
            name: symbol.to_string(),
            total,
            grade: Grade::C,
            factors: FactorBreakdown {
                sentiment: 0.0,
                capital: 0.0,
                technical: 0.0,
                catalyst: 0.0,
                sector_strength: 0.0, // B2
            },
            missing_factors: vec![],
            ai_review: None, // B3
        }
    }
```

（若 B3 在 B4 之后落地，改的就是 `test_assign_grades_by_rank_*` 那三条测试里的 `mk()`；同一份代码，别再另造。）

---

#### Step 3 — 跑测试

- [ ] 本机 Rust 单测走 cmd.exe：
  ```bash
  cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- premarket::scoring --nocapture"
  ```
  两条新测试 + 之前的旧测试 + B4 名次切档测试全部 PASS。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` PASS。
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` PASS。

---

#### Step 4 — 前端结构对齐

- [ ] 打开 `src/lib/components/invest/PremarketReportTab.svelte`。把 inline `SymbolScore`（L29-36）替换为：

```ts
  interface AiReview {
    action: 'keep' | 'drop';
    reason: string;
    riskFlag: 'none' | 'regulatory' | 'sentiment_only' | 'weak_fundamental' | 'other';
  }
  interface SymbolScore {
    symbol: string;
    name: string;
    total: number;
    grade: Grade;
    factors: FactorBreakdown;
    missingFactors: string[];
    aiReview?: AiReview;
  }
```

- [ ] `PremarketConfig` interface（L61-69）补一行 `enable_ai_review: boolean;`（沿用 snake_case 与后端 wire 一致——注意：`PremarketConfig` 结构体本身也带 `#[serde(rename_all = "camelCase")]`，所以 wire 实际是 `enableAiReview`。**统一使用 `enableAiReview`**：）

```ts
  interface PremarketConfig {
    weight_sentiment: number;
    weight_capital: number;
    weight_technical: number;
    weight_catalyst: number;
    weight_sector: number;   // B2
    weight_source: number;   // B2
    threshold_s: number;
    threshold_a: number;
    threshold_b: number;
    enableAiReview: boolean; // B3
  }
```

**重要**：`PremarketConfig` 后端加了 `#[serde(rename_all = "camelCase")]`，所以 wire 上 snake_case 字段（`weight_sentiment` 等）本应是 `weightSentiment`。但前端组件里现存字段一直沿用 snake_case，说明后端 struct 上其实并未加 `rename_all`（**校对 scoring.rs 现状**：确实 L6 有 `#[serde(rename_all = "camelCase")]` 但字段是 `weight_sentiment`——那么 wire 实际是 `weightSentiment`，前端 snake_case 早已是 bug/兼容层）。为不扩大改动面，B3 只保证新增字段 `enable_ai_review` 遵循**同一套**规范：**跟随现有前端命名**（若前端沿用 snake_case，就用 `enable_ai_review`；若前端本轮统一改 camelCase，就用 `enableAiReview`）。

**执行动作**：先 `grep -n weight_sentiment` 看前端实际拿到的字段名——按结果二选一，B3a/B3b/B3c 三处保持一致即可。以下代码块假设**保留 snake_case**（现状）：

```ts
  interface PremarketConfig {
    weight_sentiment: number;
    weight_capital: number;
    weight_technical: number;
    weight_catalyst: number;
    weight_sector: number;
    weight_source: number;
    threshold_s: number;
    threshold_a: number;
    threshold_b: number;
    enable_ai_review: boolean; // B3
  }
```

- [ ] `cfg` 默认值（L133-141 附近）补 `enable_ai_review: true,`。
- [ ] `npm run check` PASS。

---

#### Step 5 — 提交

- [ ] `git add src-tauri/src/invest/premarket/scoring.rs src/lib/components/invest/PremarketReportTab.svelte`
- [ ] Commit:
  ```
  feat(premarket): AiReview 结构 + enable_ai_review 开关 (B3a)

  - scoring: 新增 AiReview {action,reason,risk_flag} (camelCase wire: riskFlag)
  - SymbolScore 增可选 ai_review (skip_serializing_if None); score() 构造补 ai_review: None
  - PremarketConfig 增 enable_ai_review: bool, 默认 true
  - frontend: 对齐 SymbolScore.aiReview? 与 cfg.enable_ai_review
  - 硬约束: total/grade 永远由 quant 产出, AI 不写回
  ```

---

### Task B3b: `report.rs` AI 复核 pass + 熔断降级 + `sections_status`

**目标**：在 `generate_premarket_report` 里插入 AI 终选复核：全池 → 排序 → top25 → AI keep/drop → kept 前 20 → B4 名次切档 → 报告写档。降级/熔断分三层，任一失败退化为纯量化 top20。

**执行路径决策**：
- 直接复用 `crate::invest::event_analyzer::cli_complete(system_prompt, user_message)`（`src-tauri/src/invest/event_analyzer.rs:27`）。**Spec 里提到的 `cli_complete_with_settings` 不存在**，别用。
- `cli_complete` 内部走 180s 默认超时。若确要 60s 自定义（本任务采纳），改为直调：
  ```rust
  let cli = crate::invest::committee::cli_executor::CliCommitteeExecutor::global()
      .ok_or("claude CLI not available")?;
  let settings = crate::invest::macro_verdict::resolve_settings_path();
  let raw = cli.run_role(&sys, &user, 60, settings.as_deref(), None).await?;
  ```
  这条路径已在 `macro_verdict::run_macro_verdict` L134-140 验证过。`resolve_settings_path` 是 `pub(crate)`（`macro_verdict.rs:65`），从 `report.rs` 可用完整路径调。
- 无 provider/model 显式参数：`CliCommitteeExecutor` 内部按 `platform_credentials + CommitteeTuning` 解析（与 macro_verdict、committee 同一套）。

**Files**：
- `src-tauri/src/invest/premarket/report.rs`

**熔断/降级三层**（严格实现）：
1. `cfg.enable_ai_review == false` → 跳过 AI，`ai_review_status = Disabled`，所有 `ai_review = None`。
2. CLI 超时/网络错/JSON 解析失败 → `Failed`，全体 `ai_review = None`，报告底部注记「AI 精筛失败(不影响选池)」。
3. 熔断：`drop_count >= 13`（top_k=25，>=52%）或全部 drop → `CircuitBroken`，全体 `ai_review = None`，`log::warn!` 记录 drop 数与首个 reason 便于调查。

**LLM 异常处理**：
- 输入外的 `ts_code` → 忽略。
- 缺席 ts_code（未在 decisions 里）→ 默认 keep，`ai_review = None`。
- `action` 非 `keep|drop` → keep。
- `risk_flag` 非枚举值 → 覆写为 `"other"`。

---

#### Step 1 — 写失败测试（TDD，纯函数）

- [ ] 在 `report.rs` 的（若已存在则追加，若无则新建）`#[cfg(test)] mod tests` 里添加 5 条测试。测试对象是 pure fn `apply_ai_decisions(top25, decisions) -> (kept, dropped, status)`，无需真的 CLI。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::premarket::scoring::{FactorBreakdown, Grade, SymbolScore};

    fn mk(sym: &str, total: f64) -> SymbolScore {
        SymbolScore {
            symbol: sym.into(),
            name: sym.into(),
            total,
            grade: Grade::C,
            factors: FactorBreakdown {
                sentiment: 0.0,
                capital: 0.0,
                technical: 0.0,
                catalyst: 0.0,
                sector_strength: 0.0,
            },
            missing_factors: vec![],
            ai_review: None,
        }
    }

    fn dec(sym: &str, action: &str, risk: &str) -> AiDecision {
        AiDecision {
            symbol: sym.into(),
            action: action.into(),
            reason: "test".into(),
            risk_flag: risk.into(),
        }
    }

    #[test]
    fn apply_normal_drop_produces_kept_and_dropped_lists() {
        let top25: Vec<SymbolScore> = (0..25)
            .map(|i| mk(&format!("S{i:02}"), 100.0 - i as f64))
            .collect();
        let decisions = vec![
            dec("S00", "drop", "regulatory"),
            dec("S05", "drop", "sentiment_only"),
        ];
        let (kept, dropped, status) = apply_ai_decisions(top25.clone(), decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(kept.len(), 23);
        assert_eq!(dropped.len(), 2);
        // dropped 都带 ai_review = Some(drop)
        assert!(dropped.iter().all(|s| s
            .ai_review
            .as_ref()
            .map(|r| r.action == "drop")
            .unwrap_or(false)));
        // kept 里没漏没多
        assert!(kept.iter().any(|s| s.symbol == "S01"));
        assert!(!kept.iter().any(|s| s.symbol == "S00" || s.symbol == "S05"));
    }

    #[test]
    fn apply_circuit_breaker_when_drop_ge_13_returns_empty_ai() {
        let top25: Vec<SymbolScore> = (0..25)
            .map(|i| mk(&format!("S{i:02}"), 100.0 - i as f64))
            .collect();
        // 13 只 drop → 熔断
        let decisions: Vec<AiDecision> = (0..13)
            .map(|i| dec(&format!("S{i:02}"), "drop", "other"))
            .collect();
        let (kept, dropped, status) = apply_ai_decisions(top25, decisions);
        assert_eq!(status, AiReviewStatus::CircuitBroken);
        assert_eq!(kept.len(), 25, "熔断后 kept 应还是全部 25");
        assert!(dropped.is_empty(), "熔断后 dropped 清空");
        assert!(
            kept.iter().all(|s| s.ai_review.is_none()),
            "熔断后所有 ai_review 应 None"
        );
    }

    #[test]
    fn apply_partial_return_missing_symbols_default_to_keep() {
        let top25: Vec<SymbolScore> = (0..25)
            .map(|i| mk(&format!("S{i:02}"), 100.0 - i as f64))
            .collect();
        // 只对 S00 有决策,其余 24 只应默认 keep + ai_review None
        let decisions = vec![dec("S00", "drop", "none")];
        let (kept, dropped, status) = apply_ai_decisions(top25, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(kept.len(), 24);
        assert_eq!(dropped.len(), 1);
        let missing_reviews = kept.iter().filter(|s| s.ai_review.is_none()).count();
        assert_eq!(missing_reviews, 24, "24 只未返回的应 ai_review=None 且 keep");
    }

    #[test]
    fn apply_unknown_ts_code_is_ignored() {
        let top25: Vec<SymbolScore> = (0..3)
            .map(|i| mk(&format!("S{i:02}"), 100.0 - i as f64))
            .collect();
        let decisions = vec![
            dec("XXX.SH", "drop", "regulatory"), // 不在输入内
            dec("S01", "drop", "regulatory"),
        ];
        let (kept, dropped, status) = apply_ai_decisions(top25, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].symbol, "S01");
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn apply_bad_risk_flag_and_bad_action_are_coerced() {
        let top25 = vec![mk("S00", 100.0), mk("S01", 99.0)];
        let decisions = vec![
            dec("S00", "drop", "not_a_valid_flag"), // → 覆写 other
            dec("S01", "promote", "none"),          // action 非 keep/drop → keep
        ];
        let (kept, dropped, status) = apply_ai_decisions(top25, decisions);
        assert_eq!(status, AiReviewStatus::Ok);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].ai_review.as_ref().unwrap().risk_flag, "other");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].symbol, "S01");
        assert_eq!(
            kept[0].ai_review.as_ref().unwrap().action,
            "keep",
            "非法 action 应容错为 keep"
        );
    }
}
```

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` 应报 `apply_ai_decisions` / `AiDecision` / `AiReviewStatus` 未定义。

---

#### Step 2 — 实现纯函数 + AI pass

- [ ] 在 `report.rs` 文件顶部 `use` 区确认已有 `serde::Deserialize`（用完整路径调用即可，不用新增 use）。
- [ ] 在 `ai_commentary` 函数（L248-276）**下方**、`generate_premarket_report` **上方**插入以下代码块：

```rust
// ============================================================
// B3: AI 终选复核
// ============================================================

/// LLM 返回结构。仅在 report.rs 内部使用。
#[derive(Debug, Clone, serde::Deserialize)]
struct AiDecisions {
    #[serde(default)]
    decisions: Vec<AiDecision>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct AiDecision {
    symbol: String,
    action: String,
    #[serde(default)]
    reason: String,
    #[serde(default, rename = "risk_flag")]
    risk_flag: String,
}

/// B3 AI 复核状态。写进 sectionsStatus.aiReview。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiReviewStatus {
    /// AI 正常应用
    Ok,
    /// cfg.enable_ai_review == false，未跑
    Disabled,
    /// CLI/解析失败，退化纯量化
    Failed,
    /// drop 数 >= 13 或全 drop，熔断
    CircuitBroken,
}

impl AiReviewStatus {
    fn as_wire(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
            Self::CircuitBroken => "circuit_broken",
        }
    }
}

const AI_REVIEW_TOP_K: usize = 25;
const AI_REVIEW_DROP_CIRCUIT: usize = 13;
const AI_REVIEW_TIMEOUT_SECS: u64 = 60;

/// 纯函数：把 LLM decisions 应用到 top25 上。抽离出来专供单测，无 IO。
///
/// 返回 (kept_scores, dropped_scores, status)。
/// - kept_scores：保留者（含未在 decisions 里的默认 keep 者），保持原有相对顺序。
/// - dropped_scores：被 AI drop 的股票，每只带 `ai_review = Some { action: "drop", .. }`。
/// - status：熔断/正常/异常。
///
/// 输入外 ts_code 忽略；缺席默认 keep（`ai_review=None`）；action 非 keep/drop 容错为 keep；
/// risk_flag 非枚举 → "other"。drop 数 >= 13 或全 drop → 熔断，全部退回 kept 且 `ai_review=None`。
fn apply_ai_decisions(
    top_k: Vec<SymbolScore>,
    decisions: Vec<AiDecision>,
) -> (Vec<SymbolScore>, Vec<SymbolScore>, AiReviewStatus) {
    use std::collections::HashMap;
    let valid_flags = ["none", "regulatory", "sentiment_only", "weak_fundamental", "other"];
    // 索引 symbol -> decision（后到覆盖先到，避免重复 ts_code 出问题）
    let mut by_symbol: HashMap<String, AiDecision> = HashMap::new();
    for d in decisions.into_iter() {
        by_symbol.insert(d.symbol.clone(), d);
    }

    // 先统计 drop（只统计输入内、action=drop 的）
    let drop_count = top_k
        .iter()
        .filter(|s| {
            by_symbol
                .get(&s.symbol)
                .map(|d| d.action == "drop")
                .unwrap_or(false)
        })
        .count();

    // 熔断
    if drop_count >= AI_REVIEW_DROP_CIRCUIT || (drop_count > 0 && drop_count == top_k.len()) {
        log::warn!(
            "[premarket] AI review circuit-broken: drop_count={} of {}",
            drop_count,
            top_k.len()
        );
        // 全部退回 kept，ai_review 清空
        let cleared: Vec<SymbolScore> = top_k
            .into_iter()
            .map(|mut s| {
                s.ai_review = None;
                s
            })
            .collect();
        return (cleared, vec![], AiReviewStatus::CircuitBroken);
    }

    // 正常分派
    let mut kept: Vec<SymbolScore> = Vec::with_capacity(top_k.len());
    let mut dropped: Vec<SymbolScore> = Vec::new();
    for mut s in top_k.into_iter() {
        match by_symbol.get(&s.symbol) {
            Some(d) => {
                // 归一化 risk_flag
                let risk = if valid_flags.contains(&d.risk_flag.as_str()) {
                    d.risk_flag.clone()
                } else {
                    "other".to_string()
                };
                // action 容错
                let action = if d.action == "drop" { "drop" } else { "keep" };
                s.ai_review = Some(crate::invest::premarket::scoring::AiReview {
                    action: action.to_string(),
                    reason: d.reason.chars().take(60).collect(), // ≤30 汉字兜底
                    risk_flag: risk,
                });
                if action == "drop" {
                    dropped.push(s);
                } else {
                    kept.push(s);
                }
            }
            None => {
                // 未返回 → 默认 keep，ai_review = None
                s.ai_review = None;
                kept.push(s);
            }
        }
    }
    (kept, dropped, AiReviewStatus::Ok)
}

/// 从最近 sentiment_items 里，为每个 symbol 统计近 3 日命中次数（简化：用 tickers 字段包含匹配）。
/// 避免新增 API；读取本地已缓存的 sentiment。
fn sentiment_hit_map(symbols: &[String]) -> std::collections::HashMap<String, u32> {
    use std::collections::HashMap;
    let mut out: HashMap<String, u32> = symbols.iter().map(|s| (s.clone(), 0u32)).collect();
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let items =
        crate::storage::invest::sentiment::list_recent_sentiment(&since, 500).unwrap_or_default();
    for it in items.iter() {
        // it.tickers 是 Vec<String> 或 Option<Vec<String>>；用 to_string debug 命中作最宽兜底
        let raw = format!("{:?}", it);
        for sym in symbols.iter() {
            if raw.contains(sym) {
                *out.entry(sym.clone()).or_insert(0) += 1;
            }
        }
    }
    out
}

/// 拼给 LLM 的用户消息：每只股票一行摘要。上限 25 只 × 约 180 tokens ≈ 4.5k。
fn build_ai_review_prompt(top_k: &[SymbolScore]) -> String {
    let symbols: Vec<String> = top_k.iter().map(|s| s.symbol.clone()).collect();
    let hits = sentiment_hit_map(&symbols);
    let mut lines: Vec<String> = Vec::with_capacity(top_k.len());
    for s in top_k.iter() {
        let hit = hits.get(&s.symbol).copied().unwrap_or(0);
        lines.push(format!(
            "{} {} 情绪{:.0} 资金{:.0} 技术{:.0} 催化{:.0} 板块{:.0} 近3日舆情命中{}次",
            s.symbol,
            s.name,
            s.factors.sentiment,
            s.factors.capital,
            s.factors.technical,
            s.factors.catalyst,
            s.factors.sector_strength,
            hit
        ));
    }
    format!(
        "以下是量化模型筛出的 {} 只候选股。你只做「证伪」：\
         对每只股票判定 keep 或 drop。drop 只在以下情形使用：\
         1) 明确利空/监管处罚/退市风险；2) 纯情绪炒作、基本面完全不支撑；3) 基本面明显恶化。\
         没有明确利空时一律 keep。理由每只 ≤30 汉字。\
         输出严格 JSON：{{\"decisions\":[{{\"symbol\":\"ts_code\",\"action\":\"keep|drop\",\
         \"reason\":\"...\",\"risk_flag\":\"none|regulatory|sentiment_only|weak_fundamental|other\"}}, ...]}}\
         只输出 JSON，不要围栏、不要额外文字。\n\n{}",
        top_k.len(),
        lines.join("\n")
    )
}

/// 组装、调 CLI、解析、应用决策。
///
/// 返回 (kept, dropped, status)。任何 IO/解析异常 → 退化 Failed（kept=原样, dropped=空, ai_review=None）。
async fn run_ai_review(top_k: Vec<SymbolScore>) -> (Vec<SymbolScore>, Vec<SymbolScore>, AiReviewStatus) {
    if top_k.is_empty() {
        return (top_k, vec![], AiReviewStatus::Ok);
    }
    let sys = "你是A股盘前风控分析师，只负责证伪，只输出JSON。".to_string();
    let user = build_ai_review_prompt(&top_k);

    // 直调 CliCommitteeExecutor 为了 60s 自定义超时。
    let cli = match crate::invest::committee::cli_executor::CliCommitteeExecutor::global() {
        Some(c) => c,
        None => {
            log::warn!("[premarket] AI review skipped: claude CLI not available");
            let cleared: Vec<SymbolScore> = top_k
                .into_iter()
                .map(|mut s| {
                    s.ai_review = None;
                    s
                })
                .collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };
    let settings = crate::invest::macro_verdict::resolve_settings_path();
    let raw = match cli
        .run_role(&sys, &user, AI_REVIEW_TIMEOUT_SECS, settings.as_deref(), None)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[premarket] AI review CLI failed: {e}");
            let cleared: Vec<SymbolScore> = top_k
                .into_iter()
                .map(|mut s| {
                    s.ai_review = None;
                    s
                })
                .collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };
    let cleaned = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: AiDecisions = match serde_json::from_str::<AiDecisions>(cleaned) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "[premarket] AI review JSON parse failed: {e}; cleaned_len={}",
                cleaned.len()
            );
            let cleared: Vec<SymbolScore> = top_k
                .into_iter()
                .map(|mut s| {
                    s.ai_review = None;
                    s
                })
                .collect();
            return (cleared, vec![], AiReviewStatus::Failed);
        }
    };
    apply_ai_decisions(top_k, parsed.decisions)
}
```

- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` PASS，`AiReview` 引用应能解析到 `scoring.rs`。

---

#### Step 3 — 改写 `generate_premarket_report` 选池顺序

- [ ] 打开 `report.rs`，把 L613-614 附近的**旧片段**：

```rust
    // 3. 股票池 SABC 打分（读盘后缓存,兜底现场构建）
    let cfg: PremarketConfig = get_premarket_config();
    let scores: Vec<SymbolScore> = collect_scores_from_cache(&cfg).await;
```

以及紧随其后的 B4 单行（示例）：

```rust
    let scores = crate::invest::premarket::scoring::assign_grades_by_rank(scores);
```

**整段替换**为：

```rust
    // 3. 股票池打分（读盘后缓存,兜底现场构建）
    let cfg: PremarketConfig = get_premarket_config();
    let full_pool: Vec<SymbolScore> = collect_scores_from_cache(&cfg).await;

    // 3.1 全池按 total 降序 → 取 top25 → B3 AI 终选复核 → kept 前 20 → B4 名次切档
    let mut sorted_pool = full_pool;
    sorted_pool.sort_by(|a, b| {
        b.total
            .partial_cmp(&a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_k: Vec<SymbolScore> = sorted_pool.into_iter().take(AI_REVIEW_TOP_K).collect();

    let (kept_after_ai, ai_dropped, ai_status) = if cfg.enable_ai_review {
        run_ai_review(top_k).await
    } else {
        // 显式关闭：直通,全部 ai_review = None
        let cleared: Vec<SymbolScore> = top_k
            .into_iter()
            .map(|mut s| {
                s.ai_review = None;
                s
            })
            .collect();
        (cleared, vec![], AiReviewStatus::Disabled)
    };

    // kept_after_ai 已按 total 降序（apply_ai_decisions 保序 & 熔断时保序）。
    // 现在取前 20 交给 B4 名次切档；名次切档 helper 已在 <=20 时正确处理。
    let scores: Vec<SymbolScore> = crate::invest::premarket::scoring::assign_grades_by_rank(
        kept_after_ai.into_iter().take(20).collect(),
    );
```

- [ ] 打开 L666-681 的 `serde_json::json!({...})`，把整段替换为：

```rust
    let json = serde_json::json!({
        "date": date,
        "macro": macro_snapshot,
        "sectorFlows": sector_flows_entries,
        "themes": themes,
        "scores": scores,
        "aiDropped": ai_dropped,
        "config": cfg,
        "aiCommentary": ai,
        "sectionsStatus": {
            "capitalFlow": if sector_flows_entries.is_empty() { "unavailable" } else { "ok" },
            "aiReview": ai_status.as_wire(),
        },
    });
```

- [ ] 把 L659-660 附近的 md 尾注补一行：AI 失败时告诉用户（防止用户以为 AI 已跑）。在原 `md.push_str("\n## AI 点评\n\n"); md.push_str(&ai_md);` **之前**插入：

```rust
    if matches!(ai_status, AiReviewStatus::Failed) {
        md.push_str("\n> AI 精筛失败(不影响选池)。\n");
    } else if matches!(ai_status, AiReviewStatus::CircuitBroken) {
        md.push_str(&format!(
            "\n> AI 精筛熔断（drop 数 ≥ {}），已回退纯量化 top20。\n",
            AI_REVIEW_DROP_CIRCUIT
        ));
    }
```

- [ ] 若原代码里没有 `use crate::invest::premarket::scoring::AiReview;` 也没关系——上面所有引用都用完整路径。
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml` PASS。
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings` PASS（若报 unused import 或未 export `AI_REVIEW_TOP_K`，就地 fix）。

---

#### Step 4 — 跑测试

- [ ] 单测走 cmd.exe：
  ```bash
  cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib -- premarket::report::tests --nocapture"
  ```
  5 条新测试全部 PASS。
- [ ] 现有 `premarket::scoring` 单测继续 PASS。
- [ ] `cargo check` PASS。

---

#### Step 5 — 提交

- [ ] `git add src-tauri/src/invest/premarket/report.rs`
- [ ] Commit:
  ```
  feat(premarket): AI 终选复核 pass + 熔断降级 + sectionsStatus (B3b)

  - 新增 apply_ai_decisions/run_ai_review/build_ai_review_prompt/sentiment_hit_map
  - 直调 CliCommitteeExecutor::run_role 走 60s 超时(参考 macro_verdict::run_macro_verdict)
  - generate_premarket_report 重排: 全池排序 -> top25 -> AI keep/drop -> kept top20 -> B4 rank-cut
  - 三层降级: cfg 关闭 disabled / CLI 或解析失败 failed / drop>=13 熔断 circuit_broken; 均退化纯量化 top20 且 ai_review 清空
  - LLM 异常兜底: 未返回 -> keep+None, 未知 ts_code 忽略, 非法 action -> keep, 非法 risk_flag -> other
  - JSON 新增 aiDropped[] 与 sectionsStatus{capitalFlow, aiReview}
  - md 尾注在 Failed/CircuitBroken 时提示用户
  - 硬约束: total/grade 仅由量化产出, AI 结果只挂 ai_review 旁路
  ```

---

### Task B3c: 前端 AI 剔除区 + `enable_ai_review` toggle

**目标**：报告面板加 AI 开关；报告 04 段（观察池）下方加一个折叠式「AI 剔除」区，展示被 AI drop 的股票 + 理由 + 风险标签。关闭 AI 时不消费 `aiReview`，不显示剔除区，纯净 top20。

**Files**：
- `src/lib/components/invest/PremarketReportTab.svelte`
- `src-tauri/messages/en.json`
- `src-tauri/messages/zh-CN.json`

---

#### Step 1 — 扩展 `ReportPayload.json` 类型 + i18n 键

- [ ] 打开 `PremarketReportTab.svelte`，找到 `ReportPayload.json` 类型（L99-111 附近），把 `json:{...}` 部分整段替换为：

```ts
    json: {
      date: string;
      macro: MacroSnapshot | null;
      sectorFlows?: SectorFlowEntry[];
      themes?: ThemeEntry[];
      scores: SymbolScore[];
      aiDropped?: SymbolScore[]; // B3
      config: PremarketConfig;
      aiCommentary: AiCommentary | null;
      sectionsStatus?: {
        capitalFlow?: 'ok' | 'unavailable';
        aiReview?: 'ok' | 'failed' | 'circuit_broken' | 'disabled';
      };
    };
```

- [ ] 打开 `src-tauri/messages/zh-CN.json`，在 `invest_premarket_*` 组下追加：

```json
    "invest_premarket_ai_review_toggle": "AI 终选复核",
    "invest_premarket_ai_review_hint": "量化选出 25 只后, 由 AI 剔除利空/纯情绪炒作/基本面不支撑的股票",
    "invest_premarket_ai_dropped_title": "AI 剔除",
    "invest_premarket_ai_dropped_empty": "本次 AI 未剔除任何标的",
    "invest_premarket_ai_status_failed": "AI 精筛失败 (不影响选池)",
    "invest_premarket_ai_status_circuit_broken": "AI 精筛熔断 (已回退纯量化)",
    "invest_premarket_ai_status_disabled": "AI 精筛已关闭",
    "invest_premarket_risk_none": "无",
    "invest_premarket_risk_regulatory": "监管",
    "invest_premarket_risk_sentiment_only": "纯情绪",
    "invest_premarket_risk_weak_fundamental": "基本面弱",
    "invest_premarket_risk_other": "其他",
```

- [ ] 打开 `src-tauri/messages/en.json`，同一组追加：

```json
    "invest_premarket_ai_review_toggle": "AI final review",
    "invest_premarket_ai_review_hint": "After quant picks top 25, AI drops stocks with regulatory risk / pure sentiment / weak fundamentals",
    "invest_premarket_ai_dropped_title": "AI Dropped",
    "invest_premarket_ai_dropped_empty": "AI dropped no stocks in this run",
    "invest_premarket_ai_status_failed": "AI review failed (pool unaffected)",
    "invest_premarket_ai_status_circuit_broken": "AI review circuit-broken (pure-quant fallback)",
    "invest_premarket_ai_status_disabled": "AI review disabled",
    "invest_premarket_risk_none": "None",
    "invest_premarket_risk_regulatory": "Regulatory",
    "invest_premarket_risk_sentiment_only": "Sentiment-only",
    "invest_premarket_risk_weak_fundamental": "Weak fundamentals",
    "invest_premarket_risk_other": "Other",
```

---

#### Step 2 — 在设置面板加 toggle

- [ ] 打开 `PremarketReportTab.svelte`，找到 Settings panel（约 L358-417 区域，`.settings-item` 那一堆）。参考 B2 的 `weight_sector` 项样式（若结构不同，就近对齐现有 `.settings-item`），在最后一个权重项**之后**、门槛项**之前**插入：

```svelte
        <label class="settings-item settings-item--toggle">
          <span class="settings-label">{$t('invest_premarket_ai_review_toggle')}</span>
          <input
            type="checkbox"
            bind:checked={cfg.enable_ai_review}
          />
          <span class="settings-hint">{$t('invest_premarket_ai_review_hint')}</span>
        </label>
```

- [ ] 若组件内没有 `.settings-item--toggle` 样式，直接沿用 `.settings-item` + inline flex；具体样式可加在 `<style>` 末尾：

```css
  .settings-item--toggle {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .settings-item--toggle .settings-hint {
    color: var(--text-2, #999);
    font-size: 11px;
    margin-left: 4px;
  }
```

---

#### Step 3 — AI 剔除区（04 段之后）

- [ ] 在 `<script>` 顶部工具区（`grouped` 派生附近）加两条 derived + 一个 helper：

```ts
  const aiDropped = $derived<SymbolScore[]>(report?.json?.aiDropped ?? []);
  const aiStatus = $derived<'ok' | 'failed' | 'circuit_broken' | 'disabled' | undefined>(
    report?.json?.sectionsStatus?.aiReview,
  );

  function riskFlagLabel(flag: string | undefined): string {
    switch (flag) {
      case 'regulatory':
        return $t('invest_premarket_risk_regulatory');
      case 'sentiment_only':
        return $t('invest_premarket_risk_sentiment_only');
      case 'weak_fundamental':
        return $t('invest_premarket_risk_weak_fundamental');
      case 'other':
        return $t('invest_premarket_risk_other');
      default:
        return $t('invest_premarket_risk_none');
    }
  }
  function riskFlagCls(flag: string | undefined): string {
    if (!flag || flag === 'none') return 'risk-none';
    if (flag === 'regulatory' || flag === 'weak_fundamental') return 'risk-hard';
    return 'risk-soft';
  }
```

- [ ] 找到 04 段观察池 `.pool-grid` 收尾 `</div>` 处（约 L708-709），在其**外面紧随其后**插入 AI 剔除区块。使用 `<details>` 原生折叠：

```svelte
      {#if cfg.enable_ai_review && (aiDropped.length > 0 || aiStatus === 'failed' || aiStatus === 'circuit_broken')}
        <details class="ai-dropped" open={aiDropped.length > 0}>
          <summary>
            <span class="ai-dropped-title">{$t('invest_premarket_ai_dropped_title')}</span>
            <span class="ai-dropped-count">({aiDropped.length})</span>
            {#if aiStatus === 'failed'}
              <span class="ai-status-tag ai-status-failed">{$t('invest_premarket_ai_status_failed')}</span>
            {:else if aiStatus === 'circuit_broken'}
              <span class="ai-status-tag ai-status-circuit">{$t('invest_premarket_ai_status_circuit_broken')}</span>
            {/if}
          </summary>
          {#if aiDropped.length === 0}
            <div class="ai-dropped-empty">{$t('invest_premarket_ai_dropped_empty')}</div>
          {:else}
            <ul class="ai-dropped-list">
              {#each aiDropped as d (d.symbol)}
                <li class="ai-dropped-item">
                  <span class="ai-dropped-name">{d.name}</span>
                  <span class="ai-dropped-code">{d.symbol}</span>
                  <span class="ai-dropped-reason" title={d.aiReview?.reason ?? ''}>
                    {d.aiReview?.reason ?? ''}
                  </span>
                  <span class="risk-badge {riskFlagCls(d.aiReview?.riskFlag)}">
                    {riskFlagLabel(d.aiReview?.riskFlag)}
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        </details>
      {/if}
```

- [ ] 在 `<style>` 末尾加样式（含 reason 单行 truncate 与三档 risk 色）：

```css
  .ai-dropped {
    margin-top: 12px;
    padding: 8px 12px;
    border: 1px dashed var(--border, #d0d0d0);
    border-radius: 6px;
    background: var(--bg-2, #fafafa);
  }
  .ai-dropped > summary {
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    user-select: none;
  }
  .ai-dropped-title {
    color: var(--text-1);
  }
  .ai-dropped-count {
    color: var(--text-2, #999);
    font-weight: 400;
  }
  .ai-status-tag {
    margin-left: auto;
    font-size: 11px;
    padding: 2px 6px;
    border-radius: 4px;
    font-weight: 400;
  }
  .ai-status-failed {
    background: rgba(255, 152, 0, 0.15);
    color: #ff9800;
  }
  .ai-status-circuit {
    background: rgba(244, 67, 54, 0.15);
    color: #f44336;
  }
  .ai-dropped-empty {
    padding: 8px 0;
    color: var(--text-2, #999);
    font-size: 12px;
  }
  .ai-dropped-list {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ai-dropped-item {
    display: grid;
    grid-template-columns: 80px 80px 1fr auto;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    padding: 4px 6px;
    border-radius: 4px;
  }
  .ai-dropped-item:hover {
    background: var(--bg-3, #f0f0f0);
  }
  .ai-dropped-name {
    font-weight: 500;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ai-dropped-code {
    color: var(--text-2, #999);
    font-family: var(--font-mono, monospace);
  }
  .ai-dropped-reason {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .risk-badge {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 10px;
    white-space: nowrap;
  }
  .risk-badge.risk-none {
    background: rgba(158, 158, 158, 0.15);
    color: #757575;
  }
  .risk-badge.risk-soft {
    background: rgba(255, 152, 0, 0.15);
    color: #ff9800;
  }
  .risk-badge.risk-hard {
    background: rgba(244, 67, 54, 0.18);
    color: #f44336;
  }
```

---

#### Step 4 — 校验

- [ ] `npm run check` PASS（关注 `aiReview` / `sectionsStatus` / `aiDropped` 类型错误）。
- [ ] `npm run i18n:check` PASS（zh 与 en 键对齐）。
- [ ] `npm run build` PASS。
- [ ] （可选）`npm run tauri dev`，`/invest` → 盘前观察 → 关掉 toggle：报告应无剔除区，`aiReview` 完全未消费；打开 toggle 生成新报告后 04 段下方有折叠区。

---

#### Step 5 — 提交

- [ ] `git add src/lib/components/invest/PremarketReportTab.svelte src-tauri/messages/en.json src-tauri/messages/zh-CN.json`
- [ ] Commit:
  ```
  feat(premarket): 前端 AI 剔除区 + 复核开关 (B3c)

  - PremarketReportTab: ReportPayload.json 增 aiDropped[] 与 sectionsStatus{capitalFlow,aiReview}
  - 设置面板加 enable_ai_review 复选框 + hint
  - 04 段观察池下方新增可折叠「AI 剔除」区: 名称 + 代码 + 理由 (truncate) + risk 三档色 badge
  - 状态标签: failed / circuit_broken 显式提示; disabled 不显示剔除区
  - 关闭 AI 时不消费 aiReview, 保证纯净 top20 观感
  - i18n: 补 ai_review_toggle/hint, ai_dropped_title/empty, ai_status_*, risk_flag 5 档
  ```
