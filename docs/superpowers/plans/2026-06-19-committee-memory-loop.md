# 委员会自我迭代优化 (v1) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 打通委员会"决策 → 复盘 → 记忆回流"闭环,补齐过度 HOLD 的高信念主动裁决通道、两条无代码的 hard rule 与 Gate1 confidence 瑕疵,并新增 holding/research 双分析模式。

**Architecture:** 全部改动落在 `src-tauri/src/invest/committee/` 与 `src-tauri/src/storage/invest/` 的现有文件内,无新增模块文件。命中率记忆走纯读 SQL 聚合 + prompt append 注入;高信念/hard rule/Gate1 走 `analysis.rs` + `orchestrator.rs` 后处理;分析模式以 `Mode` 枚举从 IPC 经 `run_committee` → `run_role_phase` 逐层透传。

**Tech Stack:** Rust(rusqlite 同步 SQLite、tokio、serde)、Claude CLI 子进程执行。前端 Svelte 5 runes(仅 Block 5 涉及少量 store/IPC 适配,本计划聚焦后端)。

## Global Constraints

以下为 spec 的项目级约束,每个任务都隐含适用:

- **Rust test 运行时问题**(CLAUDE.md §11):本机 Rust 单测会因 `STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139)` 在运行期失败。**每个任务的"运行测试"步骤一律先以 `cargo check` 保证编译通过;能跑单测则跑,跑不起来不阻塞**。验证命令统一:`cargo check --manifest-path src-tauri/Cargo.toml`。
- **错误类型惯例**:storage 层所有 pub 函数返回 `Result<T, String>`,错误用 `.map_err(|e| format!("…: {}", e))`。
- **连接获取**:`storage/invest/` 用全局 `Mutex<Connection>`,纯读走 `super::with_conn(|conn| {...})`,读写走 `with_conn_mut`。
- **裁决词集合**:`BUY | ACCUMULATE | HOLD | TRIM | SELL`(parser 已归一化大小写与中英文)。
- **macro_signal 取值**:`risk_on | risk_off | neutral`(LLM 自由输出,代码不做枚举校验,需对其他值容错)。
- **向后兼容**:任何新增持久化字段(QueueItem、CommitteeConfig)一律用裸 `#[serde(default)]`,读旧 JSON 不得报错。
- **i18n**:涉及前端 UI 文案时同步改 `messages/en.json` 与 `messages/zh-CN.json`(本计划后端为主,Block 5 若动前端再补)。
- **提交规范**:Conventional Commits(`feat:`/`fix:`/`chore:`)。

---

## 关键设计决策(实现前必读)

**决策 1 — regime 切聚合用 JOIN**:`verdict_reviews` 表无 regime/macro_signal 列。`verdict_reviews.verdict_id` 的值等于 `verdicts.id`(TEXT 主键,格式 `{symbol}_{YYYYMMDDHHMMSS.sss}`)。按 regime 切聚合时 `JOIN verdicts v ON vr.verdict_id = v.id`,regime 取 `v.macro_signal`。

**决策 2 — mode 透传不走 QueueItem 回读**:`QueueItem`(`queue.rs`)是前端独占的持久化壳,后端从不回读它来驱动管线。后端入口 `run_committee_batch_stream` 接收 `&[String]` symbols + `HashMap<String, CancellationToken>`。因此 `Mode` 以 `HashMap<String, Mode>`(symbol → mode)形式显式传入 IPC 命令,与 cancel token map 完全同构。QueueItem 上仍加 `mode` 字段供前端持久化与展示,但它不是后端的 mode 来源。这满足 spec"同一批可混合各走各逻辑"。

**决策 3 — 止损价数字与自由文本并存**:CIO prompt 现有 `止损条件: <文本>`(roles.rs L536,parser 当前未抽取)。新增 `止损价: <数字>`(parser 抽取为 `stop_loss_price`)。两者并存:数字供复盘,文本供人读。

**决策 4 — `cio_sanity_check` 最终签名提前约定(避免跨任务回归)**:`cio_sanity_check` 的最终签名是 6 参数:`(cio_parsed, round_outputs, macro_signal, macro_strength, mode)` 之外再加 `mode: Mode`(Task 11 引入,research 时跳过 Gate2)。**Task 7、Task 9 虽然在 Task 10/11 之前编号,但它们新增的单测一律按最终签名写**,即调用时显式传 `Mode::Holding`(例:`cio_sanity_check(&cio, &outputs, "risk_off", None, Mode::Holding)`),并在测试文件顶部 `use crate::invest::committee::orchestrator::Mode;`。这样无论按编号顺序还是乱序执行,Task 7/9 实现完成后其测试都不会因 Task 11 改签名而编译失败。**推荐执行顺序:先做 Task 10(引入 Mode 枚举与透传),再做 Task 7/9/11**——若严格按编号顺序执行,则在 Task 7 实现时先临时用 5 参数(不含 mode),并在 Task 11 Step 4 统一补 mode;但提前约定 6 参数签名是更省事的做法。

---

## 文件结构(改动地图)

| 文件 | 职责 | 涉及 Block |
|---|---|---|
| `storage/invest/verdict_reviews.rs` | 新增纯读聚合函数 `aggregate_hit_rates` | 1 |
| `invest/committee/cli_executor.rs` | 注入命中率;Quant R2/CIO/Risk prompt 的 mode 分支;清理 strip_tool_section 残留工具引用 | 1,4,5 |
| `invest/committee/parser.rs` | 移除 7 个 L4 字段;新增 3 个价格字段 | 2,4,6 |
| `invest/committee/roles.rs` | Quant R1 加进场价/目标价;CIO 加止损价;删 L4 输出项;清理正文工具引用 | 2,4 |
| `invest/committee/analysis.rs` | Gate1 压低 confidence;高信念主动裁决通道;research 模式 Gate2 跳过 | 3,5 |
| `invest/committee/orchestrator.rs` | hard rule A/B clamp;Mode 透传到 run_role_phase;research 模式成本切换 | 3,5 |
| `invest/committee/queue.rs` | QueueItem 加 `mode` 字段(前端持久化用) | 5 |
| `commands/invest.rs` + `lib.rs` | IPC 命令加 mode map 参数 | 5 |

---

## 任务总览与依赖

按依赖顺序排列,每个任务结束都有可独立测试的交付物:

1. **Task 1** — parser 移除 L4 死字段(Block 4+6 删除部分)
2. **Task 2** — parser 新增 3 个价格字段(Block 2+6 新增部分)
3. **Task 3** — roles.rs prompt 输出契约改造(Block 2 加价格 + Block 4 删 L4 输出项)
4. **Task 4** — cli_executor 工具引用噪声清理(Block 4)
5. **Task 5** — verdict_reviews 命中率聚合函数(Block 1 核心)
6. **Task 6** — 命中率注入 Quant R2 + CIO prompt(Block 1 注入)
7. **Task 7** — Gate1 触发时压低 confidence(Block 3 子项)
8. **Task 8** — hard rule A/B clamp 落地 orchestrator(Block 3 子项)
9. **Task 9** — 高信念主动裁决通道(Block 3 核心)
10. **Task 10** — Mode 枚举 + 全链路透传(Block 5 基础设施)
11. **Task 11** — research 模式成本来源切换 + Gate2 跳过(Block 5 逻辑)
12. **Task 12** — research 模式 Risk/CIO prompt 分支(Block 5 prompt)

依赖关系:Task 2→3(prompt 要输出 parser 新字段);Task 5→6(注入依赖聚合函数);Task 7/8/9 同改 post-analysis,顺序执行;Task 10→11→12(模式基础设施先行)。Task 1/4/5 相互独立,可任意先做。

---

### Task 1: parser 移除 L4 死字段

移除 7 个零消费的 L4 字段:struct 定义、解析逻辑、相关单测一并清除。`l4_execution_checks_passed` 是 Rust 端从 3 个 bool 计算的,一并删除其计算块。

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(struct L64-70/98-104、parse_risk L465-475、parse_cio L514-532、相关测试 L803-822)

**Interfaces:**
- Consumes: 无(纯删除)
- Produces: `ParsedFields` 不再含 `l4_veto`/`l4_veto_reason`/`l4_veto_r2`/`l4_check_stop_loss`/`l4_check_position`/`l4_check_buy_point`/`l4_execution_checks_passed`。其余字段(`verdict`/`confidence`/`suggested_alloc_cny`/`first_tranche_cny`/`is_tier1`/`tier1_watch_hours` 等)签名不变。

- [ ] **Step 1: 先确认现有 L4 测试存在(基线)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::parser::tests::test_parse_cio_new_fields 2>&1 | head -20`
说明:本机单测运行期可能失败(§11),只要 `cargo check` 能编过即可;此步仅为定位 `test_parse_cio_new_fields`(parser.rs L803-815)和 `test_cio_execution_checks_all_pass`(L818-822)两个引用 L4 字段的测试。

- [ ] **Step 2: 从 `ParsedFields` struct 删除 Risk L4 字段(parser.rs L62-70)**

删除以下三段(连同其上方注释):
```rust
    // -- Risk-specific (new fields) --
    /// L4 否决: true=卫语句触发
    pub l4_veto: Option<bool>,
    /// 否决原因
    pub l4_veto_reason: Option<String>,
    /// 标的风险综合
    pub stock_risk_summary: Option<String>,
    /// Risk R2: L4 否决复检结果
    pub l4_veto_r2: Option<bool>,
```
改为(保留 `stock_risk_summary`,它不是 L4 字段,有独立语义):
```rust
    // -- Risk-specific (new fields) --
    /// 标的风险综合
    pub stock_risk_summary: Option<String>,
```

- [ ] **Step 3: 从 `ParsedFields` struct 删除 CIO L4 字段(parser.rs L98-104)**

删除以下四段:
```rust
    /// CIO 检查: 止损明确
    pub l4_check_stop_loss: Option<bool>,
    /// CIO 检查: 仓位合理
    pub l4_check_position: Option<bool>,
    /// CIO 检查: 买点合理
    pub l4_check_buy_point: Option<bool>,
    /// 执行检查通过数 (0-3)，Rust 端计算
    pub l4_execution_checks_passed: Option<f64>,
```
(直接删除这 8 行,前后字段 `risk_plan`/`catalyst_tier`...`is_tier1` 保持不变。)

- [ ] **Step 4: 从 `parse_risk` 删除 L4 抽取(parser.rs L464-475)**

删除:
```rust
    // L4 否决: true=卫语句触发
    parsed.l4_veto = extract_bool_any(text, &["L4否决", "L4_VETO"]);
    // 否决原因
    parsed.l4_veto_reason = extract_field_any(text, &["否决原因", "L4_VETO_REASON"]);
```
以及 R2 段的:
```rust
    // R2: L4 否决复检结果
    parsed.l4_veto_r2 = extract_bool_any(text, &["L4否决复检", "L4_VETO_R2"]);
```
保留 `parsed.stock_risk_summary = extract_field_any(text, &["标的风险", "STOCK_RISK"]);`(L469)。

- [ ] **Step 5: 从 `parse_cio` 删除 L4 抽取与计算块(parser.rs L513-532)**

删除以下整段:
```rust
    // L4 检查: 止损明确
    parsed.l4_check_stop_loss =
        extract_bool_any(text, &["STOP_LOSS_CLEAR", "止损明确"]);
    // L4 检查: 仓位合理
    parsed.l4_check_position = extract_bool_any(text, &["POSITION_OK", "仓位合理"]);
    // L4 检查: 买点合理
    parsed.l4_check_buy_point =
        extract_bool_any(text, &["BUY_POINT_OK", "买点合理"]);
```
以及函数末尾的计算块:
```rust
    // 执行检查通过数 (0-3)，Rust 端计算
    let checks = [
        parsed.l4_check_stop_loss.unwrap_or(false),
        parsed.l4_check_position.unwrap_or(false),
        parsed.l4_check_buy_point.unwrap_or(false),
    ];
    let passed = checks.iter().filter(|&&b| b).count() as f64;
    parsed.l4_execution_checks_passed = Some(passed);
```
保留 `is_tier1`(L522)、`tier1_watch_hours`(L524)的抽取不动。

- [ ] **Step 6: 修正引用 L4 字段的单测(parser.rs L803-822)**

将 `test_parse_cio_new_fields`(L804-815)中删除 L4 断言,改为:
```rust
    #[test]
    fn test_parse_cio_new_fields() {
        let text = "VERDICT: ACCUMULATE\nCONFIDENCE: 0.75\nCATALYST_TIER: Tier1\nCATALYST_SUMMARY: 政策利好\nIS_TIER1: true\nTIER1_WATCH_HOURS: 48";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.catalyst_tier.as_deref(), Some("Tier1"));
        assert_eq!(parsed.catalyst_summary.as_deref(), Some("政策利好"));
        assert_eq!(parsed.is_tier1, Some(true));
        assert_eq!(parsed.tier1_watch_hours, Some(48.0));
    }
```
完全删除 `test_cio_execution_checks_all_pass`(L818-822,整个函数),因为它只断言 `l4_execution_checks_passed`。

- [ ] **Step 7: 验证编译通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: 编译成功,无 `l4_` 相关字段未定义错误。若有其他文件(如 archive.rs / 前端类型 .ts)引用了被删字段,会在此暴露——逐个移除引用(grep 确认:`grep -rn "l4_veto\|l4_check\|l4_execution_checks_passed" src-tauri/src`,预期 0 处剩余)。

- [ ] **Step 8: 确认无残留引用**

Run: `grep -rn "l4_veto\|l4_check_stop_loss\|l4_check_position\|l4_check_buy_point\|l4_execution_checks_passed" src-tauri/src/`
Expected: 无输出(全部清除)。

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "refactor(committee): 移除零消费的 L4 死字段"
```

---

### Task 2: parser 新增结构化点位字段

新增 3 个价格字段供未来精细复盘。`entry_price`/`target_price` 来自 Quant R1,`stop_loss_price` 来自 CIO。与现有自由文本止损并存(决策 3)。

**Files:**
- Modify: `src-tauri/src/invest/committee/parser.rs`(struct 新增字段、parse_quant 新增抽取、parse_cio 新增抽取、新增单测)

**Interfaces:**
- Consumes: `extract_f64_any(text: &str, keys: &[&str]) -> Option<f64>`(parser.rs 现有辅助函数)
- Produces: `ParsedFields` 新增 `entry_price: Option<f64>`、`target_price: Option<f64>`、`stop_loss_price: Option<f64>`,供 Task 3 的 prompt 输出契约与未来 verdict_review 消费。

- [ ] **Step 1: 写失败测试(parser.rs tests mod 内,紧接 test_parse_quant_r1 之后)**

```rust
    #[test]
    fn test_parse_quant_r1_prices() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7\n进场价: 12.50\n目标价: 15.80\nONE_LINER: 技术面偏多";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.entry_price, Some(12.50));
        assert_eq!(parsed.target_price, Some(15.80));
    }

    #[test]
    fn test_parse_cio_stop_loss_price() {
        let text = "VERDICT: BUY\nCONFIDENCE: 0.7\n止损价: 11.20\n止损条件: 跌破20日线";
        let parsed = parse_role_output(CommitteeRole::Cio, text, false);
        assert_eq!(parsed.stop_loss_price, Some(11.20));
    }

    #[test]
    fn test_parse_prices_absent_is_none() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7";
        let parsed = parse_role_output(CommitteeRole::Quant, text, false);
        assert_eq!(parsed.entry_price, None);
        assert_eq!(parsed.target_price, None);
        assert_eq!(parsed.stop_loss_price, None);
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -i "entry_price\|target_price\|stop_loss_price" | head`
Expected: 编译失败,报 `no field entry_price on type ParsedFields` 等(字段尚未定义)。

- [ ] **Step 3: 在 `ParsedFields` struct 新增字段(parser.rs Quant-specific 区块内,buy_point_assessment 之后 L58 附近)**

在 `pub buy_point_assessment: Option<String>,` 之后追加:
```rust
    /// Quant R1: 进场价(结构化点位,供复盘)
    pub entry_price: Option<f64>,
    /// Quant R1: 目标价(结构化点位,供复盘)
    pub target_price: Option<f64>,
```
在 CIO-specific 区块内,`pub risk_plan: Option<String>,`(L92)之后追加:
```rust
    /// CIO: 止损价(结构化数字,与自由文本 adjusted_stop_loss 并存)
    pub stop_loss_price: Option<f64>,
```

- [ ] **Step 4: 在 `parse_quant` 新增抽取**

找到 `parse_quant` 函数中抽取 `buy_point_assessment` 的行,其后追加:
```rust
    parsed.entry_price = extract_f64_any(text, &["进场价", "ENTRY_PRICE"]);
    parsed.target_price = extract_f64_any(text, &["目标价", "TARGET_PRICE"]);
```

- [ ] **Step 5: 在 `parse_cio` 新增抽取(parser.rs，risk_plan 抽取行之后)**

在 `parsed.risk_plan = extract_field_any(text, &["RISK_PLAN", "风控计划"]);`(L508)之后追加:
```rust
    parsed.stop_loss_price = extract_f64_any(text, &["止损价", "STOP_LOSS_PRICE"]);
```

- [ ] **Step 6: 运行测试确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。能跑单测则 `cargo test --manifest-path src-tauri/Cargo.toml committee::parser::tests::test_parse_quant_r1_prices` 应 PASS。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/invest/committee/parser.rs
git commit -m "feat(committee): parser 新增进场价/目标价/止损价结构化字段"
```

---

### Task 3: roles.rs prompt 输出契约改造

让 Quant R1 输出进场价/目标价,CIO 输出止损价(供 Task 2 的 parser 抽取),同时删除 Risk/CIO prompt 里的 L4 输出项(对应 Task 1 已移除的字段)。改的是 in-source const(`QUANT_PROMPT`/`RISK_PROMPT`/`RISK_R2_PROMPT`/`CIO_PROMPT`);若用户已在 `~/.claw-go/invest/prompts/*.txt` 落了 override 文件,override 优先,需在 Step 7 提示用户同步(本计划只改源码 const)。

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`(QUANT_PROMPT L360-371、RISK_PROMPT L467-475、RISK_R2_PROMPT L510-512、CIO_PROMPT L528-542)

**Interfaces:**
- Consumes: Task 2 新增的 parser key —— Quant 抽 `进场价`/`目标价`,CIO 抽 `止损价`。prompt 输出标签必须与 parser 抽取关键字逐字一致。
- Produces: prompt 输出契约。无 Rust 签名变化,纯字符串常量改写。

- [ ] **Step 1: QUANT_PROMPT R1 输出格式段加进场价/目标价(roles.rs L360-371)**

在现有输出格式段的 `买点评估:` 行(L370)之后、`一句话:` 行(L371)之前插入两行:
```
进场价: <建议进场价格数字，无则填 N/A>
目标价: <目标价格数字，无则填 N/A>
```
插入后该段顺序为:`...买点评估 → 进场价 → 目标价 → 一句话`。

- [ ] **Step 2: CIO_PROMPT 输出格式段加止损价(roles.rs L536 附近)**

在现有 `止损条件: <具体条件>`(L536)行之后插入一行:
```
止损价: <止损价格数字，无则填 N/A>
```
保留 `止损条件` 自由文本行不动(决策 3:数字与文本并存)。

- [ ] **Step 3: CIO_PROMPT 删除 L4 检查输出项(roles.rs L537-539)**

删除以下三行(对应 Task 1 移除的 `l4_check_*` 字段):
```
止损明确: yes | no
仓位合理: yes | no
买点合理: yes | no
```
保留 `is_tier1`(L540)、`tier1_watch_hours`(L541)、`个人备注`(L542)等其余行不动。

- [ ] **Step 4: CIO_PROMPT Hard Rules 文字改为"已生效"口径(roles.rs L553-555)**

现有 L554-555 写"系统会自动降级/clamp"(承诺但 Task 8 才真正落地代码)。Task 8 落地后这两条成立,文字保留即可,但为避免歧义,将 L554-555 改为更明确的:
```
- confidence ≥ 0.95 + verdict=BUY → 系统自动降级到 ACCUMULATE(已实现，无需你额外处理)
- |SUGGESTED_ALLOC_CNY| > 100000 → 系统自动 clamp 到 ±100000(已实现)
```
(若 Task 8 尚未完成,此步可暂缓到 Task 8 后再改;两者文字-代码一致即可。)

- [ ] **Step 5: RISK_PROMPT R1 删除 L4 否决输出项(roles.rs L474-475)**

删除以下两行(对应 Task 1 移除的 `l4_veto`/`l4_veto_reason`):
```
L4否决: true | false
否决原因: <卫语句触发条件，未触发写 N/A>
```
保留 `标的风险:`(L473)行不动(对应保留字段 `stock_risk_summary`)。

- [ ] **Step 6: 确认 RISK_R2_PROMPT 无 L4 字段需删(roles.rs L510-512)**

RISK_R2_PROMPT 仅含 `调整风险信号`/`调整止损`/`推理` 三行,**无 L4 输出项**(探查已确认),此步仅为核对,不改动。

- [ ] **Step 7: 验证编译 + 检查 prompt override**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功(纯字符串改动不影响类型)。
人工提示:若 `~/.claw-go/invest/prompts/` 下存在 `quant_r1.txt`/`cio.txt`/`risk_r1.txt` override 文件,它们会覆盖源码 const,需手动同步同样改动(本计划不自动改用户运行时文件)。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "feat(committee): prompt 输出进场价/目标价/止损价并删除 L4 输出项"
```

---

### Task 4: cli_executor 工具引用噪声清理

`strip_tool_section` 只删了 `**你有工具可调用**` 标题块,但 prompt 正文里散落的具体工具名引用(如 Quant 提 `get_recent_committee_verdicts`、Macro 提 `get_recent_events()`)在 CLI 模式(`--max-turns 1`,不能调工具)下是误导且白占 token。这些引用在 `roles.rs` 的 prompt const 正文里(非工具标题块内),`strip_tool_section` 删不掉,需直接从 const 文本删除。

**Files:**
- Modify: `src-tauri/src/invest/committee/roles.rs`(各 prompt const 正文中的工具名引用)

**Interfaces:**
- Consumes: 无
- Produces: 无签名变化,纯文本清理。

- [ ] **Step 1: 定位所有散落工具引用**

Run: `grep -n "get_recent_committee_verdicts\|get_recent_events\|query_dreaming_insights\|工具调用\|可调用工具\|调用.*工具\|tool_call" src-tauri/src/invest/committee/roles.rs`
Expected: 列出 prompt const 正文里残留的工具名/工具调用提法及行号(`**你有工具可调用**` 标题块除外——那个由 strip_tool_section 运行期删)。

- [ ] **Step 2: 逐条删除正文工具引用**

对 Step 1 列出的每一处(不在 `**你有工具可调用**` 标题块内的),删除该句中关于"调用某工具获取数据"的引导文字。原则:CLI 模式数据已预取注入,prompt 不应再提"调用工具"。例如将
```
可通过 get_recent_committee_verdicts 查询近期裁决
```
这类句子整句删除(因为近期裁决已由 `format_recent_verdicts_for_prompt` 注入)。
注意:**只删工具引用句**,不动该角色的分析职责、输出格式、判定规则等正文。

- [ ] **Step 3: 验证 strip_tool_section 标题块仍正常工作**

确认未误删 `**你有工具可调用**` 标题(它仍需由 `strip_tool_section` 在运行期删,以兼容非 CLI 路径)。
Run: `grep -n "你有工具可调用" src-tauri/src/invest/committee/roles.rs`
Expected: 各角色 prompt 的工具标题块仍在(本任务不动它)。

- [ ] **Step 4: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/roles.rs
git commit -m "chore(committee): 清理 CLI 模式下 prompt 正文的工具引用噪声"
```

---

### Task 5: verdict_reviews 命中率聚合函数

新增纯读聚合函数,实时 `GROUP BY` 算每类裁决的 1d/7d/30d 命中率(全局 + 按 regime)。`verdict_reviews` 用 `window_days` 列区分窗口(同一 verdict 的 1/7/30 天回顾是 3 行,非 3 列),`hit` 是单 bool 列。regime 切聚合需 JOIN `verdicts.macro_signal`(决策 1)。

**Files:**
- Modify: `src-tauri/src/storage/invest/verdict_reviews.rs`(新增 struct + 聚合函数 + 单测)

**Interfaces:**
- Consumes: `super::with_conn(|conn| {...})`(纯读);`verdict_reviews` 列 `verdict_type`/`window_days`/`hit`/`verdict_id`/`price_after`;`verdicts` 列 `id`/`macro_signal`。
- Produces:
  ```rust
  pub struct HitRateRow { pub verdict_type: String, pub window_days: i64, pub hits: i64, pub total: i64, pub matured: bool }
  pub struct HitRateAgg { pub global: Vec<HitRateRow>, pub by_regime: Vec<(String, Vec<HitRateRow>)> }
  pub fn aggregate_hit_rates(min_samples: i64) -> Result<HitRateAgg, String>
  ```
  Task 6 消费 `aggregate_hit_rates` 渲染注入文本。

- [ ] **Step 1: 写失败测试(verdict_reviews.rs 文件末尾 #[cfg(test)] mod 内;若无则新建)**

```rust
#[cfg(test)]
mod agg_tests {
    use super::*;

    // 本机单测运行期可能失败(§11)，该测试主要保证编译；
    // SQL 逻辑正确性以评审为准。min_samples 过滤是纯函数式的。
    #[test]
    fn hit_rate_row_filters_below_min_samples() {
        let rows = vec![
            HitRateRow { verdict_type: "ACCUMULATE".into(), window_days: 1, hits: 2, total: 8, matured: true },
            HitRateRow { verdict_type: "TRIM".into(), window_days: 1, hits: 1, total: 3, matured: true },
        ];
        let filtered: Vec<_> = rows.into_iter().filter(|r| r.total >= 5).collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].verdict_type, "ACCUMULATE");
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -i "HitRateRow\|aggregate_hit_rates" | head`
Expected: 编译失败,`cannot find type HitRateRow`(类型未定义)。

- [ ] **Step 3: 定义聚合结果 struct(verdict_reviews.rs,紧接现有 VerdictReviewEntry 之后)**

```rust
/// 单行命中率聚合：某 verdict_type × window_days 的命中数/样本数。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitRateRow {
    pub verdict_type: String,
    pub window_days: i64,
    pub hits: i64,
    pub total: i64,
    /// 30 天窗口在历史足够长前命中普遍为 0；matured=false 表示样本未到期，
    /// 不应解读为"0% 命中"。
    pub matured: bool,
}

/// 命中率聚合结果：全局 + 按 regime(verdicts.macro_signal)切分。
#[derive(Debug, Clone, serde::Serialize)]
pub struct HitRateAgg {
    pub global: Vec<HitRateRow>,
    pub by_regime: Vec<(String, Vec<HitRateRow>)>,
}
```

- [ ] **Step 4: 实现 `aggregate_hit_rates`(verdict_reviews.rs)**

注意:SQL 字符串里的 `'unknown'` 等字面量是 SQL 单引号,Rust 端用普通双引号字符串包裹即可。

```rust
/// 实时聚合 verdict_reviews 命中率。纯读,无缓存。
/// - 全局:按 verdict_type × window_days 聚合。
/// - 按 regime:JOIN verdicts.macro_signal 再切一层(决策 1)。
/// - min_samples:total < min_samples 的行被剔除(HAVING)。
/// - matured:window_days>=30 且该组无任何 price_after(全 NULL)时 matured=false。
pub fn aggregate_hit_rates(min_samples: i64) -> Result<HitRateAgg, String> {
    with_conn(|conn| {
        // 全局聚合
        let mut stmt = conn
            .prepare(
                "SELECT verdict_type, window_days, \
                        SUM(CASE WHEN hit != 0 THEN 1 ELSE 0 END) AS hits, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN price_after IS NOT NULL THEN 1 ELSE 0 END) AS matured_cnt \
                 FROM verdict_reviews \
                 GROUP BY verdict_type, window_days \
                 HAVING total >= ?1",
            )
            .map_err(|e| format!("prepare global agg: {}", e))?;
        let global: Vec<HitRateRow> = stmt
            .query_map(params![min_samples], |row| {
                let window_days: i64 = row.get(1)?;
                let total: i64 = row.get(3)?;
                let matured_cnt: i64 = row.get(4)?;
                Ok(HitRateRow {
                    verdict_type: row.get(0)?,
                    window_days,
                    hits: row.get(2)?,
                    total,
                    matured: !(window_days >= 30 && matured_cnt == 0),
                })
            })
            .map_err(|e| format!("query global agg: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect global agg: {}", e))?;

        // 按 regime 聚合(JOIN verdicts.macro_signal)
        let mut stmt2 = conn
            .prepare(
                "SELECT COALESCE(v.macro_signal, 'unknown') AS regime, \
                        vr.verdict_type, vr.window_days, \
                        SUM(CASE WHEN vr.hit != 0 THEN 1 ELSE 0 END) AS hits, \
                        COUNT(*) AS total, \
                        SUM(CASE WHEN vr.price_after IS NOT NULL THEN 1 ELSE 0 END) AS matured_cnt \
                 FROM verdict_reviews vr \
                 JOIN verdicts v ON vr.verdict_id = v.id \
                 GROUP BY regime, vr.verdict_type, vr.window_days \
                 HAVING total >= ?1 \
                 ORDER BY regime",
            )
            .map_err(|e| format!("prepare regime agg: {}", e))?;
        let flat: Vec<(String, HitRateRow)> = stmt2
            .query_map(params![min_samples], |row| {
                let window_days: i64 = row.get(2)?;
                let total: i64 = row.get(4)?;
                let matured_cnt: i64 = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    HitRateRow {
                        verdict_type: row.get(1)?,
                        window_days,
                        hits: row.get(3)?,
                        total,
                        matured: !(window_days >= 30 && matured_cnt == 0),
                    },
                ))
            })
            .map_err(|e| format!("query regime agg: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect regime agg: {}", e))?;

        // 折叠成 Vec<(regime, Vec<HitRateRow>)>,保持 regime 顺序(SQL 已 ORDER BY regime)
        let mut by_regime: Vec<(String, Vec<HitRateRow>)> = Vec::new();
        for (regime, row) in flat {
            match by_regime.last_mut() {
                Some((r, rows)) if *r == regime => rows.push(row),
                _ => by_regime.push((regime, vec![row])),
            }
        }

        Ok(HitRateAgg { global, by_regime })
    })
}
```

- [ ] **Step 5: 运行确认编译通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。能跑则 `cargo test --manifest-path src-tauri/Cargo.toml verdict_reviews::agg_tests` PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/storage/invest/verdict_reviews.rs
git commit -m "feat(invest): verdict_reviews 命中率实时聚合函数"
```

---

### Task 6: 命中率注入 Quant R2 + CIO prompt

把 Task 5 的聚合渲染成软提示文本块,append 到 Quant R2 与 CIO 的 system prompt。纯参考、不引入硬规则;聚合为空时整块不注入(对齐 memory_injection.rs 的"空则不注入"惯例)。

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`(新增 `render_hit_rates` + `format_hit_rates_for_prompt`;在 `build_cli_quant_r2_prompt`、`build_cli_cio_prompt` 内 append)

**Interfaces:**
- Consumes: `crate::storage::invest::verdict_reviews::{aggregate_hit_rates, HitRateAgg, HitRateRow}`(Task 5);`build_cli_quant_r2_prompt`/`build_cli_cio_prompt` 的 `round_outputs: &[RoundOutput]` 参数(取 Macro signal 作 regime)。
- Produces: `fn render_hit_rates(agg: &HitRateAgg, current_regime: &str) -> String`(空聚合返回 `""`);`fn format_hit_rates_for_prompt(current_regime: &str) -> String`(读库+渲染,失败/空返回 `""`)。

- [ ] **Step 1: 写失败测试(cli_executor.rs tests mod 内)**

```rust
    #[test]
    fn hit_rates_empty_returns_blank() {
        let agg = crate::storage::invest::verdict_reviews::HitRateAgg {
            global: vec![],
            by_regime: vec![],
        };
        assert!(render_hit_rates(&agg, "neutral").is_empty());
    }

    #[test]
    fn hit_rates_renders_global_rows() {
        use crate::storage::invest::verdict_reviews::{HitRateAgg, HitRateRow};
        let agg = HitRateAgg {
            global: vec![HitRateRow {
                verdict_type: "ACCUMULATE".into(),
                window_days: 1,
                hits: 10,
                total: 21,
                matured: true,
            }],
            by_regime: vec![],
        };
        let out = render_hit_rates(&agg, "risk_off");
        assert!(out.contains("历史命中率参考"));
        assert!(out.contains("ACCUMULATE"));
        assert!(out.contains("n=21"));
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -i "render_hit_rates" | head`
Expected: 编译失败,`cannot find function render_hit_rates`。

- [ ] **Step 3: 实现纯渲染函数 `render_hit_rates`(cli_executor.rs)**

```rust
/// 把命中率聚合渲染成软提示文本块。空聚合返回 ""。
/// current_regime 用于高亮"当前市场状态"下的同类命中率。
fn render_hit_rates(
    agg: &crate::storage::invest::verdict_reviews::HitRateAgg,
    current_regime: &str,
) -> String {
    use crate::storage::invest::verdict_reviews::HitRateRow;
    if agg.global.is_empty() && agg.by_regime.is_empty() {
        return String::new();
    }

    // 把同一 verdict_type 的多个 window 合并成一行展示,未到期窗口跳过。
    fn fmt_rows(rows: &[HitRateRow]) -> Vec<String> {
        use std::collections::BTreeMap;
        let mut by_type: BTreeMap<&str, Vec<&HitRateRow>> = BTreeMap::new();
        for r in rows {
            by_type.entry(r.verdict_type.as_str()).or_default().push(r);
        }
        let mut lines = Vec::new();
        for (vt, mut rs) in by_type {
            rs.sort_by_key(|r| r.window_days);
            let parts: Vec<String> = rs
                .iter()
                .filter(|r| r.matured)
                .map(|r| {
                    let pct = if r.total > 0 {
                        (r.hits as f64 / r.total as f64 * 100.0).round() as i64
                    } else {
                        0
                    };
                    format!("{}天 {}%(n={})", r.window_days, pct, r.total)
                })
                .collect();
            if !parts.is_empty() {
                lines.push(format!("  {}: {}", vt, parts.join(" / ")));
            }
        }
        lines
    }

    let mut out = vec![
        "[历史命中率参考 — 你过往同类判断的真实表现]".to_string(),
        "全局:".to_string(),
    ];
    out.extend(fmt_rows(&agg.global));

    // 当前 regime 段
    if let Some((_, rows)) = agg.by_regime.iter().find(|(r, _)| r == current_regime) {
        let regime_lines = fmt_rows(rows);
        if !regime_lines.is_empty() {
            out.push(format!("当前市场状态({}):", current_regime));
            out.extend(regime_lines);
        }
    }

    out.push(
        "说明:这是你过往同类判断的真实表现,供你校准信心,但不要机械套用——市场环境会变。"
            .to_string(),
    );
    // 若存在 30d 未到期组,补一句说明
    let has_unmatured_30d = agg.global.iter().any(|r| r.window_days >= 30 && !r.matured);
    if has_unmatured_30d {
        out.push("30天窗口样本尚未到期,暂不列出。".to_string());
    }

    out.join("\n")
}

/// 读库 + 渲染。供 build_cli_*_prompt 调用。失败或空时返回 ""。
fn format_hit_rates_for_prompt(current_regime: &str) -> String {
    match crate::storage::invest::verdict_reviews::aggregate_hit_rates(5) {
        Ok(agg) => render_hit_rates(&agg, current_regime),
        Err(e) => {
            log::warn!("aggregate_hit_rates failed: {}", e);
            String::new()
        }
    }
}
```
说明:`min_samples=5` 对齐 spec 小样本保护;格式串 `"{}天 {}%(n={})"` 三个占位依次为 window_days/pct/total。

- [ ] **Step 4: 在 `build_cli_quant_r2_prompt` append 命中率(cli_executor.rs L458-480)**

`build_cli_quant_r2_prompt` 当前 `cli_additions` 是不可变 `let`。在它之后、最终 `format!("{}{}{}", ...)` 之前插入(用 shadowing 重绑定,无需改原 `let`):
```rust
    // 历史命中率注入(软提示)。regime 取 Macro 信号,对齐 archive 口径。
    let regime = round_outputs
        .iter()
        .find(|o| o.role == CommitteeRole::Macro)
        .and_then(|o| o.parsed.signal.clone())
        .unwrap_or_else(|| "neutral".to_string());
    let hit_rates = format_hit_rates_for_prompt(&regime);
    let cli_additions = if hit_rates.is_empty() {
        cli_additions
    } else {
        format!("{}\n\n{}", cli_additions, hit_rates)
    };
```
(`CommitteeRole` 已在 R2 builder 顶部 `use`,可直接用。)

- [ ] **Step 5: 在 `build_cli_cio_prompt` append 命中率(cli_executor.rs L546-600)**

CIO builder 的 `cli_additions` 已是 `let mut`。在所有现有 `push_str(...)` 之后、最终 `format!("{}{}{}", ...)` 之前插入:
```rust
    // 历史命中率注入(软提示)。regime 取 Macro 信号。
    let regime = round_outputs
        .iter()
        .find(|o| o.role == CommitteeRole::Macro)
        .and_then(|o| o.parsed.signal.clone())
        .unwrap_or_else(|| "neutral".to_string());
    let hit_rates = format_hit_rates_for_prompt(&regime);
    if !hit_rates.is_empty() {
        cli_additions.push_str("\n\n");
        cli_additions.push_str(&hit_rates);
    }
```

- [ ] **Step 6: 运行确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "feat(committee): 注入历史命中率软提示到 Quant R2/CIO prompt"
```

---

### Task 7: Gate1 触发时压低 confidence

现状:Gate1(macro 与 CIO 反向)只改 `final_verdict = "HOLD"` 不动 `final_confidence`,导致"HOLD 配 0.7"自相矛盾,污染 verdict_reviews 与命中率注入。修正:Gate1 触发时同步把 confidence 压到 `min(原值, 0.4)`,与 Fallback 的 HOLD 口径一致。

**Files:**
- Modify: `src-tauri/src/invest/committee/analysis.rs`(`cio_sanity_check` Gate1 段 L155-161、新增单测)

**Interfaces:**
- Consumes: `SanityCheckResult { gate1_pass, gate2_pass, final_verdict, final_confidence, notes }`(analysis.rs 现有)。
- Produces: 行为变化——Gate1 触发后 `result.final_confidence` 被压低。无签名变化。

- [ ] **Step 1: 写失败测试(analysis.rs tests mod 内,紧接 test_sanity_gate1_inconsistency 之后)**

```rust
    #[test]
    fn test_sanity_gate1_lowers_confidence() {
        // CIO 高信念看多(0.8)，但 macro=risk_off → Gate1 降级 HOLD 并压低 confidence
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.8),
            ..Default::default()
        };
        let outputs = vec![];
        let result = cio_sanity_check(&cio, &outputs, "risk_off", None);
        assert!(!result.gate1_pass);
        assert_eq!(result.final_verdict, "HOLD");
        assert_eq!(result.final_confidence, 0.4); // min(0.8, 0.4)
    }

    #[test]
    fn test_sanity_gate1_keeps_lower_confidence() {
        // 原 confidence 已低于 0.4 时,保持原值(min 语义)
        let cio = ParsedFields {
            verdict: Some("BUY".to_string()),
            confidence: Some(0.3),
            ..Default::default()
        };
        let result = cio_sanity_check(&cio, &[], "risk_off", None);
        assert_eq!(result.final_confidence, 0.3); // min(0.3, 0.4)
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::analysis::tests::test_sanity_gate1_lowers_confidence 2>&1 | tail -10`
Expected: FAIL,断言 `final_confidence == 0.4` 不成立(当前 Gate1 不动 confidence,会保留 0.8)。本机若跑不起来,以代码评审确认逻辑。

- [ ] **Step 3: 修改 Gate1 段(analysis.rs L155-161)**

将现有 Gate1 块:
```rust
    if (macro_is_bullish && cio_is_bearish) || (macro_is_risk_off && cio_is_bullish) {
        result.gate1_pass = false;
        result.final_verdict = "HOLD".to_string();
        result
            .notes
            .push("G1: 宏观信号与CIO裁决不一致，降级为HOLD".to_string());
    }
```
改为(新增一行压低 confidence):
```rust
    if (macro_is_bullish && cio_is_bearish) || (macro_is_risk_off && cio_is_bullish) {
        result.gate1_pass = false;
        result.final_verdict = "HOLD".to_string();
        // 被否决的 HOLD 是低信念观望，压低 confidence 与 Fallback 口径一致，
        // 避免"HOLD 配 0.7"污染 verdict_reviews 命中率统计。
        result.final_confidence = result.final_confidence.min(0.4);
        result
            .notes
            .push("G1: 宏观信号与CIO裁决不一致，降级为HOLD".to_string());
    }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。能跑则 Step 1 两个测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs
git commit -m "fix(committee): Gate1 降级 HOLD 时同步压低 confidence"
```

---

### Task 8: hard rule A/B clamp 落地 orchestrator

补齐两条 prompt 承诺但无代码的 hard rule(CIO_PROMPT L554-555):
- **rule A**:`final_confidence >= 0.95 && final_verdict == "BUY"` → 降级 `final_verdict = "ACCUMULATE"`。
- **rule B**:`|suggested_alloc_cny| > 100000` → clamp 到 ±100000;`first_tranche_cny` 同步 clamp(不超过 clamp 后的 alloc 绝对值)。

落点:orchestrator post-analysis,在拿到最终 `(final_verdict, final_confidence)`(sentinel/sanity 决出后,L1942-1947 之后)、写库(archive_verdict L1960)之前。这样任何来源(CIO 原始 / Task 9 高信念升级)的高 confidence BUY 与超额 alloc 都被兜住。

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(post-analysis 段 L1942-1960 之间)

**Interfaces:**
- Consumes: `final_verdict: String`、`final_confidence: f64`(L1942-1947 决出);`cio_parsed: ParsedFields`(含 `suggested_alloc_cny`/`first_tranche_cny`,L1927-1932)。
- Produces: clamp 后的 `final_verdict`、以及供 archive/result 用的 alloc 值。alloc clamp 后的值需要被 archive 持久化吗?——当前 `archive_verdict` 不持久化 alloc(探查确认),故 rule B 的 clamp 对落库无直接影响;但为口径正确,clamp 后的 alloc 应反映在 `CommitteeResult`(若 result 暴露 alloc)。本任务先 clamp `final_verdict` 与一个本地 `clamped_alloc`/`clamped_first_tranche`,供 Task 9 之后若 result 需要时引用。

- [ ] **Step 1: 把 clamp 抽成 analysis.rs 纯函数(便于单测)**

在 `analysis.rs` 新增:
```rust
/// Hard rule A/B clamp 结果。
pub struct HardClamp {
    pub verdict: String,
    pub alloc_cny: Option<f64>,
    pub first_tranche_cny: Option<f64>,
}

/// 应用两条 hard rule(对应 CIO_PROMPT 承诺):
/// - rule A: confidence >= 0.95 且 verdict==BUY → 降级 ACCUMULATE。
/// - rule B: |alloc| > 100000 → clamp 到 ±100000;first_tranche 同步 clamp 到 [0, |alloc_clamped|]。
pub fn apply_hard_rules(
    verdict: &str,
    confidence: f64,
    alloc_cny: Option<f64>,
    first_tranche_cny: Option<f64>,
) -> HardClamp {
    // rule A
    let verdict = if confidence >= 0.95 && verdict == "BUY" {
        "ACCUMULATE".to_string()
    } else {
        verdict.to_string()
    };

    // rule B
    let alloc_cny = alloc_cny.map(|a| a.clamp(-100_000.0, 100_000.0));
    let first_tranche_cny = match (first_tranche_cny, alloc_cny) {
        (Some(ft), Some(a)) => {
            let cap = a.abs();
            // first_tranche 取与 alloc 同号、绝对值不超过 cap
            Some(ft.clamp(-cap, cap))
        }
        (ft, _) => ft,
    };

    HardClamp {
        verdict,
        alloc_cny,
        first_tranche_cny,
    }
}
```

- [ ] **Step 2: 写失败测试(analysis.rs tests mod 内)**

```rust
    #[test]
    fn test_hard_rule_a_downgrades_high_conf_buy() {
        let r = apply_hard_rules("BUY", 0.97, Some(50_000.0), Some(20_000.0));
        assert_eq!(r.verdict, "ACCUMULATE");
    }

    #[test]
    fn test_hard_rule_a_not_triggered_below_threshold() {
        // 高信念通道升级用 0.65，刻意不触发 rule A
        let r = apply_hard_rules("BUY", 0.65, None, None);
        assert_eq!(r.verdict, "BUY");
    }

    #[test]
    fn test_hard_rule_b_clamps_alloc_and_first_tranche() {
        let r = apply_hard_rules("BUY", 0.7, Some(150_000.0), Some(160_000.0));
        assert_eq!(r.alloc_cny, Some(100_000.0));
        assert_eq!(r.first_tranche_cny, Some(100_000.0)); // 同步 clamp 到 cap
    }

    #[test]
    fn test_hard_rule_b_preserves_within_limit() {
        let r = apply_hard_rules("ACCUMULATE", 0.6, Some(80_000.0), Some(30_000.0));
        assert_eq!(r.alloc_cny, Some(80_000.0));
        assert_eq!(r.first_tranche_cny, Some(30_000.0));
    }
```

- [ ] **Step 3: 运行确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -i "apply_hard_rules\|HardClamp" | head`
Expected: 测试编译前——若先只写测试会报 `cannot find function apply_hard_rules`;Step 1 已给实现,故此步确认 Step 1+2 一起编过。

- [ ] **Step 4: 在 orchestrator post-analysis 调用 clamp(orchestrator.rs L1947 之后)**

在 `let (final_verdict, final_confidence) = if let Some(ref s) = sentinel {...} else {...};`(L1942-1947)之后、`let total_latency_ms = ...`(L1949)之前插入:
```rust
    // Hard rule A/B clamp（CIO_PROMPT 承诺的"系统自动降级/clamp"）。
    // 放在最终 verdict/confidence 决出之后、写库之前，兜住任何来源的高 conf BUY 与超额 alloc。
    let clamp = crate::invest::committee::analysis::apply_hard_rules(
        &final_verdict,
        final_confidence,
        cio_parsed.suggested_alloc_cny,
        cio_parsed.first_tranche_cny,
    );
    let final_verdict = clamp.verdict;
    // alloc clamp 后的值（archive_verdict 当前不持久化 alloc，此处保留供未来 result/落库使用）
    let _clamped_alloc = clamp.alloc_cny;
    let _first_tranche = clamp.first_tranche_cny;
```
说明:`final_verdict` 用 shadowing 重绑定为 clamp 后的值;`final_confidence` 不被 rule A/B 改动(rule A 只改 verdict)。`_clamped_alloc`/`_first_tranche` 暂以 `_` 前缀避免 unused 警告,待将来 result/archive 需要 alloc 时去掉前缀接入。

- [ ] **Step 5: 运行确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。能跑则 Step 2 四个测试 PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(committee): 落地 hard rule A/B(0.95降级 + 100k clamp)"
```

---

### Task 9: 高信念主动裁决通道

纠正"信息不足即躺平"的过度 HOLD。在 `cio_sanity_check` 内、Fallback 检查之后新增"高信念升级":当无 fallback、Gate1/Gate2 都过、当前是 HOLD、Quant 与 Macro 同向且强度都 ≥6、Risk 非 high_risk 时,把 HOLD 升级到对应方向,confidence 设 `max(原值, 0.65)`(刻意 ≤0.95 避开 rule A)。

**Files:**
- Modify: `src-tauri/src/invest/committee/analysis.rs`(`cio_sanity_check` Fallback 段之后、`result` 返回之前 L208-210、新增单测)

**Interfaces:**
- Consumes: `round_outputs: &[RoundOutput]`(取 Quant 的 signal/strength)、`macro_signal: &str`、`macro_strength: Option<f64>`、`result.gate1_pass/gate2_pass/final_verdict/final_confidence`。
- Produces: 行为变化——满足条件时 `final_verdict` 由 HOLD 升级,`final_confidence = max(原值, 0.65)`,`notes` 加 `[HIGH_CONVICTION]`。无签名变化。

- [ ] **Step 1: 写失败测试(analysis.rs tests mod 内)**

```rust
    // 构造高信念升级所需的 round_outputs：Quant bullish≥6 + Macro risk_on≥6 + Risk ok
    fn high_conviction_outputs(quant_signal: &str, quant_str: f64, risk_signal: &str) -> Vec<RoundOutput> {
        let mut q = ParsedFields::default();
        q.signal = Some(quant_signal.to_string());
        q.strength = Some(quant_str);
        q.raw_text = "quant".to_string();
        let mut r = ParsedFields::default();
        r.signal = Some(risk_signal.to_string());
        r.raw_text = "risk".to_string();
        vec![
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: q, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: r, latency_ms: 0, tokens_used: 0 },
        ]
    }

    #[test]
    fn test_high_conviction_upgrades_hold() {
        // HOLD + Quant bullish 7 + Macro risk_on 7 + Risk ok → 升级到 BUY/ACCUMULATE，conf≥0.65
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 7.0, "ok");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0));
        assert!(matches!(result.final_verdict.as_str(), "BUY" | "ACCUMULATE"));
        assert!(result.final_confidence >= 0.65);
        assert!(result.notes.iter().any(|n| n.contains("HIGH_CONVICTION")));
    }

    #[test]
    fn test_high_conviction_skipped_when_risk_high() {
        // Risk=high_risk → 不升级
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 7.0, "high_risk");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0));
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_high_conviction_skipped_when_strength_low() {
        // Quant strength=5 (<6) → 不升级
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let outputs = high_conviction_outputs("bullish", 5.0, "ok");
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0));
        assert_eq!(result.final_verdict, "HOLD");
    }

    #[test]
    fn test_high_conviction_skipped_on_fallback() {
        // 有 fallback → Fallback 先把 verdict 压成 HOLD/≤0.4，高信念不得翻案
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.35), ..Default::default() };
        let mut outputs = high_conviction_outputs("bullish", 7.0, "ok");
        outputs[0].parsed.fallback_reason = Some("missing_critical_fields".to_string());
        let result = cio_sanity_check(&cio, &outputs, "risk_on", Some(7.0));
        assert_eq!(result.final_verdict, "HOLD");
    }
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml committee::analysis::tests::test_high_conviction_upgrades_hold 2>&1 | tail -10`
Expected: FAIL,`final_verdict` 仍是 HOLD(升级逻辑未实现)。本机跑不起来则以评审确认。

- [ ] **Step 3: 在 `cio_sanity_check` Fallback 段之后插入高信念升级(analysis.rs L208,`result` 返回前)**

在 Fallback 块(L202-208,`if has_unavailable {...}`)之后、`result`(L210)之前插入:
```rust
    // ── 高信念主动裁决通道 ──────────────────────────────────────────────
    // 纠正"信息不足即躺平"。仅在以下全部满足时把 HOLD 升级为方向性裁决：
    //   - 无任何角色 fallback/不可用(数据缺失绝不伪造信念)
    //   - Gate1 & Gate2 都过(未被宏观矛盾/三重恶化否决)
    //   - 当前是 HOLD(只兜底躺平，不翻已有方向)
    //   - Quant 与 Macro 同向且强度都 ≥ 6
    //   - Risk 信号 != high_risk
    if !has_unavailable
        && result.gate1_pass
        && result.gate2_pass
        && result.final_verdict == "HOLD"
    {
        let quant = round_outputs
            .iter()
            .filter(|o| o.role == CommitteeRole::Quant)
            .last();
        let quant_signal = quant.and_then(|o| o.parsed.signal.clone());
        let quant_strength = quant.and_then(|o| o.parsed.strength).unwrap_or(0.0);

        let risk_signal = round_outputs
            .iter()
            .filter(|o| o.role == CommitteeRole::Risk)
            .last()
            .and_then(|o| o.parsed.signal.clone());
        let risk_ok = risk_signal.as_deref() != Some("high_risk");

        // macro 方向:risk_on=看多, risk_off=看空
        let macro_bull = macro_signal == "risk_on";
        let macro_bear = macro_signal == "risk_off";
        let macro_strong = macro_strength.map_or(false, |s| s >= 6.0);

        // quant 方向
        let quant_bull = quant_signal.as_deref() == Some("bullish");
        let quant_bear = quant_signal.as_deref() == Some("bearish");
        let quant_strong = quant_strength >= 6.0;

        if risk_ok && macro_strong && quant_strong {
            let upgraded = if macro_bull && quant_bull {
                Some("ACCUMULATE")
            } else if macro_bear && quant_bear {
                Some("TRIM")
            } else {
                None
            };
            if let Some(v) = upgraded {
                result.final_verdict = v.to_string();
                // 设下限 0.65,避免"方向/0.3"自相矛盾,又刻意 ≤0.95 避开 hard rule A
                result.final_confidence = result.final_confidence.max(0.65);
                result
                    .notes
                    .push("[HIGH_CONVICTION] Quant与Macro同向强信号，HOLD升级为方向性裁决".to_string());
            }
        }
    }
```
说明:升级映射用 ACCUMULATE(看多)/TRIM(看空),保守优先于 BUY/SELL。sentinel 仍在 orchestrator 层(L1942-1947)压顶,本升级不影响 sentinel 决胜。

- [ ] **Step 4: 运行确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5`
Expected: 编译成功。能跑则 Step 1 五个测试全 PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs
git commit -m "feat(committee): 高信念主动裁决通道,纠正过度 HOLD"
```

---

### Task 10: Mode 枚举 + 全链路透传

新增 `Mode { Holding, Research }` 枚举,以 `HashMap<String, Mode>`(symbol → mode)从 IPC 命令经 `run_committee_batch_stream`/`run_committee_batch` → `run_committee` → `run_role_phase` 逐层透传(决策 2:与 cancel token map 同构,不走 QueueItem 回读)。QueueItem 同时加 `mode` 字段供前端持久化。本任务只做基础设施透传,默认 holding,不改任何裁决逻辑(逻辑在 Task 11/12)。

**Files:**
- Modify: `src-tauri/src/invest/committee/queue.rs`(QueueItem 加 `mode` 字段)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(定义 `Mode`;`run_committee`/`run_committee_batch`/`run_committee_batch_stream`/`run_role_phase` 加 mode 参数)
- Modify: `src-tauri/src/commands/invest.rs`(`run_committee`/`run_committee_stream` IPC 命令加 `modes: Option<HashMap<String, String>>` 参数并解析)

**Interfaces:**
- Consumes: 现有 `run_committee_batch_stream(symbols: &[String], config, emitter, dry_run, tokens: HashMap<String, CancellationToken>)`。
- Produces:
  ```rust
  // orchestrator.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
  #[serde(rename_all = "snake_case")]
  pub enum Mode { #[default] Holding, Research }
  ```
  `run_committee` 新增末位参数 `mode: Mode`;两个 batch 函数新增 `modes: HashMap<String, Mode>` 参数;`run_role_phase` 新增 `mode: Mode` 参数(Task 11/12 消费)。

- [ ] **Step 1: 定义 `Mode` 枚举(orchestrator.rs,CommitteeConfig 定义附近 L79 之前)**

```rust
/// 分析模式:持仓评估(默认)vs 研究观察。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// 持仓模式:考虑现金/集中度/真实成本(现状行为)。
    #[default]
    Holding,
    /// 研究模式:忽略现金/集中度,成本用关注价,裁决语义=标的吸引力。
    Research,
}
```

- [ ] **Step 2: QueueItem 加 mode 字段(queue.rs L24-33)**

在 `QueueItem` struct 内 `progress` 字段之后追加(裸 `#[serde(default)]` 保证读旧 JSON 不报错):
```rust
    /// 分析模式(前端持久化用;后端 mode 来源是 IPC 的 modes map，非此字段)。
    #[serde(default)]
    pub mode: crate::invest::committee::orchestrator::Mode,
```
注:`Mode` 已 derive `Default`(= Holding),旧 JSON 无 `mode` 字段时反序列化为 Holding,向后兼容。

- [ ] **Step 3: `run_committee` 加 mode 参数(orchestrator.rs L1719-1726)**

在 `run_committee` 签名末尾(`cancel: Option<CancellationToken>,` 之后)加参数:
```rust
pub(crate) async fn run_committee(
    symbol: &str,
    config: &CommitteeConfig,
    emitter: Option<EventEmitter>,
    dry_run: bool,
    portfolio_override: Option<std::sync::Arc<PortfolioData>>,
    cancel: Option<CancellationToken>,
    mode: Mode,
) -> Result<CommitteeResult, String> {
```

- [ ] **Step 4: `run_role_phase` 加 mode 参数并把 mode 透传进去**

在 `run_role_phase` 签名(orchestrator.rs L1470-1483)末尾加 `mode: Mode,` 参数。然后在 `run_committee` 内每处调用 `run_role_phase(...)` 的末尾补 `mode` 实参(Task 11/12 会在 run_role_phase 内用到 mode;本任务只透传,函数体暂不读 mode——加 `let _ = mode;` 在 run_role_phase 顶部避免 unused 警告,Task 11/12 再去掉)。

- [ ] **Step 5: 两个 batch 函数加 modes 参数(orchestrator.rs L2010, L2047)**

`run_committee_batch`(L2010)签名加 `modes: HashMap<String, Mode>,`;循环内(L2030)调用改为:
```rust
            let mode = modes.get(&symbol).copied().unwrap_or_default();
            run_committee(&symbol, &config, None, dry_run, Some(portfolio), None, mode).await
```
(`mode` 需在 `tokio::spawn` 前 `let mode = ...; ` 取出并 move 进闭包,与 `symbol`/`config` 同样 clone/copy。)

`run_committee_batch_stream`(L2047)签名加 `modes: HashMap<String, Mode>,`;循环内(L2073-2082)在 `let token = tokens.get(&symbol).cloned();` 旁加 `let mode = modes.get(&symbol).copied().unwrap_or_default();`,spawn 内调用末尾补 `mode`:
```rust
                run_committee(&symbol, &config, Some(emitter), dry_run, Some(portfolio), token, mode).await
```

- [ ] **Step 6: IPC 命令加 modes 参数(commands/invest.rs L838-852, L878-916)**

`run_committee` IPC 命令(L839)签名加 `modes: Option<std::collections::HashMap<String, String>>,`;在调用 `run_committee_batch` 前把字符串 map 解析成 `HashMap<String, Mode>`:
```rust
    let mode_map = parse_mode_map(modes);
    let results = crate::invest::committee::orchestrator::run_committee_batch(
        &symbols,
        &committee_config,
        dry_run.unwrap_or(false),
        mode_map,
    )
    .await;
```
`run_committee_stream` IPC 命令(L879)同样加 `modes` 参数,调用 `run_committee_batch_stream` 时传 `mode_map`。
新增解析辅助(commands/invest.rs):
```rust
/// 把前端传来的 symbol→mode 字符串 map 解析成 Mode 枚举 map。
/// 未知/缺失值回退 Holding(向后兼容)。
fn parse_mode_map(
    modes: Option<std::collections::HashMap<String, String>>,
) -> std::collections::HashMap<String, crate::invest::committee::orchestrator::Mode> {
    use crate::invest::committee::orchestrator::Mode;
    modes
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| {
            let m = match v.as_str() {
                "research" => Mode::Research,
                _ => Mode::Holding,
            };
            (k, m)
        })
        .collect()
}
```

- [ ] **Step 7: 验证编译(全链路签名一致)**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20`
Expected: 编译成功。若报某处 `run_committee`/`run_role_phase`/batch 调用参数数量不匹配,按提示补 `mode` 实参——预期改动点已在 Step 3-6 列全。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/invest/committee/queue.rs src-tauri/src/invest/committee/orchestrator.rs src-tauri/src/commands/invest.rs
git commit -m "feat(committee): Mode 枚举 + holding/research 全链路透传(默认 holding)"
```

---

### Task 11: research 模式成本切换 + Gate2 跳过

research 模式下:① Risk 的成本对比从 holdings 买入均价(`Holding.avg_cost` where kind=Hold)切到 watch 关注价(`Holding.avg_cost` where kind=Watch);② Gate2 的 loss_guard 依赖真实浮亏,research 无真实持仓 → Gate2 跳过。

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(`build_risk_metrics_context` L539-605 按 mode 切成本源;`run_role_phase` 把 mode 传给 risk metrics)
- Modify: `src-tauri/src/invest/committee/analysis.rs`(`cio_sanity_check` 加 mode 参数,research 跳过 Gate2)

**Interfaces:**
- Consumes: `portfolio_data.holdings: Vec<Holding>`,`Holding { symbol, kind: HoldingKind, avg_cost: Option<f64>, shares: Option<f64>, notional: f64, ... }`,`HoldingKind { Hold, Watch, Cash }`(portfolio.rs)。Task 10 的 `Mode`。
- Produces: `build_risk_metrics_context` 新增 `mode: Mode` 参数;`cio_sanity_check` 新增 `mode: Mode` 参数(research 时 Gate2 整段跳过)。

- [ ] **Step 1: 写失败测试 — Gate2 在 research 模式跳过(analysis.rs tests mod 内)**

```rust
    #[test]
    fn test_gate2_skipped_in_research_mode() {
        // 即使三重恶化条件全满足,research 模式也不触发 Gate2 强制 SELL
        let mut macro_parsed = ParsedFields::default();
        macro_parsed.signal = Some("risk_off".to_string());
        macro_parsed.strength = Some(8.0);
        macro_parsed.raw_text = "macro".to_string();
        let mut quant_parsed = ParsedFields::default();
        quant_parsed.signal = Some("bearish".to_string());
        quant_parsed.strength = Some(7.0);
        quant_parsed.raw_text = "quant".to_string();
        let mut risk_parsed = ParsedFields::default();
        risk_parsed.pnl_pct = Some(-20.0);
        risk_parsed.raw_text = "risk".to_string();
        let outputs = vec![
            RoundOutput { role: CommitteeRole::Macro, round: 1, parsed: macro_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Quant, round: 1, parsed: quant_parsed, latency_ms: 0, tokens_used: 0 },
            RoundOutput { role: CommitteeRole::Risk, round: 1, parsed: risk_parsed, latency_ms: 0, tokens_used: 0 },
        ];
        let cio = ParsedFields { verdict: Some("HOLD".to_string()), confidence: Some(0.5), ..Default::default() };
        let result = cio_sanity_check(&cio, &outputs, "risk_off", Some(8.0), Mode::Research);
        assert!(result.gate2_pass); // research 模式 Gate2 不触发
        assert_ne!(result.final_verdict, "SELL");
    }
```
注:此测试调用 `cio_sanity_check` 多了第 5 个参数 `Mode::Research`——Step 3 会给 `cio_sanity_check` 加该参数。同时需把现有所有 `cio_sanity_check(...)` 调用(测试与 orchestrator)补 `Mode::Holding`,见 Step 4。

- [ ] **Step 2: 运行确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | grep -i "cio_sanity_check" | head`
Expected: 编译失败,`cio_sanity_check` 参数数量不匹配(尚未加 mode 参数)。

- [ ] **Step 3: `cio_sanity_check` 加 mode 参数并在 research 跳过 Gate2(analysis.rs L132-137, L186)**

签名末尾加 `mode: Mode,`(需在 analysis.rs 顶部 `use super::orchestrator::Mode;` 或用全路径)。Gate2 触发条件(L186)由:
```rust
    if macro_guard && quant_guard && loss_guard {
```
改为:
```rust
    if mode != Mode::Research && macro_guard && quant_guard && loss_guard {
```
注释补一行:`// research 模式无真实持仓,loss_guard 无意义,跳过 Gate2`。

- [ ] **Step 4: 补全所有 `cio_sanity_check` 调用的 mode 实参**

- orchestrator.rs L1934 的调用:末尾加 `mode`(该 mode 由 Task 10 透传进 `run_committee`,需确认 `run_committee` 内 post-analysis 段能访问到 `mode` 形参——可直接用)。
- analysis.rs 现有测试里所有 `cio_sanity_check(&cio, &outputs, "...", ...)` 调用(test_sanity_gate1_inconsistency / test_sanity_gate2_triple_deterioration / Task 7 / Task 9 新增测试)末尾补 `Mode::Holding`,保持原行为。

- [ ] **Step 5: `build_risk_metrics_context` 按 mode 切成本源(orchestrator.rs L539-560)**

签名(L539-543)末尾加 `mode: Mode,`。函数体内 `let holding = portfolio_data.holdings.iter().find(|h| h.symbol == symbol);`(L544)改为按 mode 选 kind:
```rust
    use crate::storage::invest::portfolio::HoldingKind;
    // 持仓模式优先取真实持仓(Hold);研究模式取关注记录(Watch)的关注价当成本。
    let holding = match mode {
        Mode::Research => portfolio_data
            .holdings
            .iter()
            .find(|h| h.symbol == symbol && h.kind == HoldingKind::Watch)
            .or_else(|| portfolio_data.holdings.iter().find(|h| h.symbol == symbol)),
        Mode::Holding => portfolio_data
            .holdings
            .iter()
            .find(|h| h.symbol == symbol && h.kind == HoldingKind::Hold)
            .or_else(|| portfolio_data.holdings.iter().find(|h| h.symbol == symbol)),
    };
```
关键:research 模式下 watch 行的 `shares` 为 None,现有 `(pnl_pct, current_price, avg_cost, shares)` 计算块(L548-560)的 `let shares = h.shares?;` 会短路成 `(0.0,...)`,丢掉 avg_cost。research 模式需放宽:用 watch 的 `avg_cost` 当关注价,current_price 用最新价(`asset_context.latest_close` 或 `h.notional`/已知现价)算"关注以来涨跌"。改 L548-560 为:
```rust
    let (pnl_pct, current_price, avg_cost, shares) = match mode {
        Mode::Research => {
            // 研究模式:成本=关注价(avg_cost),涨跌相对关注价。无 shares 也算。
            let avg_cost = holding.and_then(|h| h.avg_cost).unwrap_or(0.0);
            let current_price = asset_context
                .latest_close
                .or_else(|| holding.and_then(|h| {
                    let s = h.shares?;
                    if s > 0.0 { Some(h.notional / s) } else { None }
                }))
                .unwrap_or(0.0);
            let pnl = if avg_cost > 0.0 {
                (current_price - avg_cost) / avg_cost * 100.0
            } else {
                0.0 // 无关注价 → 退化为 N/A 纯标的判断
            };
            (pnl, current_price, avg_cost, 0.0)
        }
        Mode::Holding => holding
            .and_then(|h| {
                let shares = h.shares?;
                let avg_cost = h.avg_cost?;
                if shares > 0.0 && avg_cost > 0.0 {
                    let current_price = h.notional / shares;
                    let pnl = (current_price - avg_cost) / avg_cost * 100.0;
                    Some((pnl, current_price, avg_cost, shares))
                } else {
                    None
                }
            })
            .unwrap_or((0.0, 0.0, 0.0, 0.0)),
    };
```
注:`AssetContext`(orchestrator.rs L26)确有 `latest_close: Option<f64>` 字段(L45,"最新价 rt_k 实时,不缓存")。上面代码用法已核对无误。

- [ ] **Step 6: `run_role_phase` 把 mode 传给 risk metrics 构建(orchestrator.rs L1517-1525)**

Risk R1 分支(L1521)调用 `build_cli_risk_r1_prompt`,该函数内部调 `format_risk_metrics_for_prompt` → `build_risk_metrics_context`。需把 `mode` 透传:给 `build_cli_risk_r1_prompt` 和 `format_risk_metrics_for_prompt` 都加 `mode: Mode` 参数,最终传给 `build_risk_metrics_context`。(Task 12 会进一步给 risk prompt 加 mode 分支,此处先把 mode 管道打通。)

- [ ] **Step 7: 运行确认通过**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10`
Expected: 编译成功。能跑则 Step 1 的 `test_gate2_skipped_in_research_mode` PASS。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/invest/committee/analysis.rs src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(committee): research 模式成本切换到关注价 + Gate2 跳过"
```

---

### Task 12: research 模式 Risk/CIO prompt 分支

research 模式给 Risk 和 CIO 注入不同"职责说明段":Risk 跳过现金/集中度、只评标的自身风险;CIO 裁决语义重定义为标的吸引力。走运行期 append/分支拼装,不新增 prompt 常量文件。

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`(`build_cli_risk_r1_prompt`、`build_cli_cio_prompt` 加 mode 分支)
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`(`run_role_phase` 把 mode 传给 CIO builder)

**Interfaces:**
- Consumes: Task 10 的 `Mode`;Task 11 已给 `build_cli_risk_r1_prompt` 加了 `mode` 参数。
- Produces: `build_cli_cio_prompt` 新增 `mode: Mode` 参数;两个 builder 在 research 模式 append 研究职责说明段。

- [ ] **Step 1: Risk research 职责段(cli_executor.rs build_cli_risk_r1_prompt,Task 11 已加 mode 参数)**

在 `build_cli_risk_r1_prompt` 内,构建 `cli_additions` 之后、最终 `format!` 之前插入:
```rust
    let cli_additions = if mode == crate::invest::committee::orchestrator::Mode::Research {
        format!(
            "{}\n\n【研究模式 — 风控职责调整】\n\
             本标的为研究观察,非实际持仓。请:\n\
             - 忽略现金/子弹充足度,不因无现金而提高风险信号\n\
             - 忽略组合集中度(标的不在组合内)\n\
             - 成本对比基于关注价(非真实买入均价),浮盈浮亏表示"关注以来涨跌"\n\
             - 只评估标的自身风险(估值/财务/流动性/利空)",
            cli_additions
        )
    } else {
        cli_additions
    };
```
(`cli_additions` 在该函数是 `let mut`,可改用上面 shadowing;若是 `let mut` 则保留可变性,用 if 重绑定即可。)

- [ ] **Step 2: `build_cli_cio_prompt` 加 mode 参数 + research 裁决语义段(cli_executor.rs L546-600)**

签名末尾加 `mode: crate::invest::committee::orchestrator::Mode,`。在所有现有 `cli_additions.push_str(...)` 之后、命中率注入(Task 6 Step 5)之前或之后均可,插入:
```rust
    if mode == crate::invest::committee::orchestrator::Mode::Research {
        cli_additions.push_str(
            "\n\n【研究模式 — 裁决语义重定义】\n\
             本标的为研究观察,非持仓评估。裁决语义改为"标的吸引力":\n\
             - BUY/ACCUMULATE = 值得买入 / 可分批建仓\n\
             - HOLD = 观望\n\
             - TRIM/SELL = 规避 / 看空\n\
             忽略现金充足度与组合集中度,基于标的自身基本面/技术面/催化剂判断吸引力。",
        );
    }
```

- [ ] **Step 3: `run_role_phase` 把 mode 传给 CIO builder(orchestrator.rs L1534-1542)**

CIO 分支(L1538)调用 `build_cli_cio_prompt(...)`,末尾补 `mode` 实参。Risk R1 分支(Task 11 Step 6 已透传 mode)确认 `build_cli_risk_r1_prompt(..., mode)` 已带 mode。

- [ ] **Step 4: 验证编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10`
Expected: 编译成功。

- [ ] **Step 5: 验证 holding 模式行为不变(回归)**

Run: `cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -3`
人工核对:`mode == Research` 分支全部带条件,holding 模式(默认)走原路径,prompt 文本无变化。能跑测试则跑全部 committee 测试确认无回归。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs src-tauri/src/invest/committee/orchestrator.rs
git commit -m "feat(committee): research 模式 Risk/CIO prompt 职责与裁决语义分支"
```

---

## 实现后整体验证

全部任务完成后:

- [ ] **编译 + Rust 检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: 编译成功,clippy 无 warning。

- [ ] **残留死字段确认**

Run: `grep -rn "l4_veto\|l4_check_stop_loss\|l4_check_position\|l4_check_buy_point\|l4_execution_checks_passed" src-tauri/src/`
Expected: 无输出。

- [ ] **前端整体验证(若 Block 5 触及前端)**

Run: `npm run build && npm run i18n:check`
Expected: 构建通过,i18n 一致。

- [ ] **最终 commit / release(按需)**

参考 CLAUDE.md 标准工作流:simplify 代码评审 → 修复 → 提交。
