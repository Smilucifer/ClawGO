# 委员会自我迭代优化 (v1) — 设计文档

> 状态:**待用户通读定稿**。第一~六块已与用户逐项确认。

## 背景与问题

ClawGO 投资委员会(`src-tauri/src/invest/committee/`)是一个多角色 LLM 辩论管线,逐 symbol 产出裁决(BUY/ACCUMULATE/HOLD/TRIM/SELL)。系统已有两套"自我迭代"基础设施,但**产出物到决策之间的最后一公里断了**:

1. **Dreaming 管线**(`invest/dreaming/`)每天凌晨 3 点跑,从历史 verdict 挖 `symbol×verdict×regime` 模式,写入 `domain_insights` 表。但委员会角色 prompt **没有注入点**,洞察是否被用全凭 LLM 是否主动调 `query_dreaming_insights` 工具(且 Quant/CIO 连这个工具都没有)。
2. **复盘闭环**(`verdict_review` cron,每交易日 17:00)算出每类裁决 1d/7d/30d 命中率,存入 `verdict_reviews` 表。但这个命中率**从未进过任何角色 prompt**。

### 数据现状(2026-06-19 实测)

```
verdicts:         135 条  (HOLD 103, ACCUMULATE 24, TRIM 6, SELL 2)
verdict_reviews:  363 条  ← 当下可用的真实命中率数据
domain_insights:    0 条  ← 空
dream_snapshots: 9472 条  ← 几乎全是 "0 candidates found",且出现每分钟一次的空转
```

`verdict_reviews` 命中率:1天窗口 ~46%、7天窗口 ~12%。

**HOLD 成分拆解**:103 条 HOLD 中 89 条(86%)置信度 ≤0.4,平均仅 0.38(全场最低)。经排查,这些低置信度 0.3/0.4 **来自 CIO 原始输出本身**,不是后处理误杀——即委员会在信息不足/neutral 环境下大量"主动躺平"。76/103 的 HOLD 发生在 neutral 宏观信号下。

### 为什么 dreaming 产物是空的

三个结构性原因叠加,**dreaming 不是坏了,是数据不够**:

1. **历史太短**:verdicts 最早 2026-06-03,仅 16 天。REM 打分公式 `score = 30天命中率×0.7 + ...`,30 天窗口尚未到期(`price_after` 取不到),命中数全为 0,分数 ≈ 0,全低于 `min_score=0.5`。
2. **103/135 是 HOLD**:REM 只挖方向性裁决,可挖样本仅 32 条。
3. **样本太散**:32 条按 `symbol×verdict×regime` 分组后凑不满 `min_count=2`。

**结论**:短期内不能指望 dreaming 喂养记忆闭环。改用当下就有数据的 `verdict_reviews` 命中率作为主信号,dreaming 模式洞察作为将来的附加层(口径一致,可无缝并入)。

## 目标

打通"决策 → 复盘 → 记忆回流"闭环,让委员会在高信念时敢于出手,并让委员会能服务"研究观察"而不仅是"持仓评估":

1. 让 **Quant R2** 和 **CIO** 在决策时看到自己过往同类判断的真实命中率,从经验中校准。
2. 输出结构化点位(进场价/目标价/止损价),让未来复盘更精细,反哺记忆质量。
3. 增设"高信念主动裁决通道",纠正"信息不足即躺平"的过度 HOLD 倾向;并配套补齐两条 prompt 承诺但无代码的 hard rule(0.95 降级、100k clamp)、修正 Gate 1 改 verdict 不压 confidence 的口径瑕疵。
4. 新增 symbol 级"分析模式"开关,区分持仓评估与研究观察,使现金/集中度不再误伤研究类判断。
5. 顺手清除 CLI prompt 工具噪声、移除 L4 死字段。(dream 空转修复与污染快照清理已划归另一 session,见第四块)

**非目标(本次明确排除)**:
- Prompt 自进化(系统自动改写 role_instruction)——风险高,留待后续。
- dreaming 模式洞察注入——数据未就绪,口径已对齐,将来并入同一注入块。
- per-symbol 命中率(单票样本噪声大);按模式分别聚合命中率(首版统一聚合,留待后续)。

## 设计

### 第一块 · 命中率记忆闭环(已定稿)

新增纯读 `verdict_reviews` 表的聚合函数,位置:`storage/invest/verdict_reviews.rs`。

**实时聚合,不做缓存**:委员会跑到 Quant R2 / CIO 时当场 `GROUP BY` 算一次。表仅几百行,聚合毫秒级,无需缓存表及其失效复杂度。

输出两个维度:
- **全局 by verdict_type**:每类裁决 1d/7d/30d 命中率 + 样本数。例:`ACCUMULATE: 1d 48%(n=21) / 7d 12%(n=21)`。
- **by verdict_type × regime**:再按市场状态切一层。regime 取自 verdict 的 `macro_signal`(risk_on/risk_off/neutral),与 dreaming 口径一致。

**小样本保护**:某维度样本数 `< min_samples(=5)` 不输出该行。
**30d 未到期处理**:30 天窗口数据未到期,命中数普遍为 0,标注"样本未到期"而非误报"0% 命中"。

**注入点**:
- Quant R2 → `cli_executor.rs::build_cli_quant_r2_prompt`
- CIO → `cli_executor.rs::build_cli_cio_prompt`

与现有 `format_recent_verdicts_for_prompt`、`format_round_outputs_for_prompt` 并列追加。**不动 R1、不动 Macro/Risk,不碰 `roles.rs` 常量**(运行期聚合数据走 append 比 `{{}}` 占位符更干净)。设计理由:R1 保持独立首读不被历史带偏;R2/CIO 在"修正/拍板"环节才对照历史校准。

**注入文本格式**(示意):

```
[历史命中率参考 — 你过往同类判断的真实表现]
全局:
  ACCUMULATE: 1天 48%(n=21) / 7天 12%(n=21)
  TRIM:       1天 50%(n=6)  / 7天 33%(n=6)
当前市场状态(risk_off):
  ACCUMULATE: 7天 15%(n=8)
说明:这是你过往同类判断的真实表现,供你校准信心,但不要机械套用——市场环境会变。
30天窗口样本尚未到期,暂不列出。
```

**使用方式 = 纯参考软提示**:如实呈现 + 一句引导语,让 LLM 自己权衡,**不引入硬规则**。
**降级**:聚合为空时整块不注入,不塞空壳。

### 第二块 · 结构化点位(与闭环复利)

现状:Quant 买点是定性的(`低吸|突破|回踩|追高`),CIO 止损是自由文本,**全程无结构化价格**,导致 `verdict_review` 只能判断"涨没涨",无法判断"是否到目标价/是否触发止损"。

改动:
- Quant R1 输出 `进场价`、`目标价`(`build_cli_quant_r1_prompt` 对应 prompt 常量 + 输出格式)。
- CIO 输出结构化 `止损价`。
- parser `ParsedFields` 新增 `entry_price`、`target_price`、`stop_loss_price` 字段及解析。

价值:让未来 `verdict_review` 能做"是否到目标价/是否触发止损"的精细复盘,反过来喂养更优质的命中率记忆——与第一块复利。本次只负责"输出并落库",精细复盘判定逻辑可后续接入。

### 第三块 · 高信念主动裁决通道

**根因**:89 条低置信度 HOLD 来自 CIO 自身,委员会在 neutral/信息不足时缺乏"敢于下注"的框架。本通道**不降低下注门槛**,而是建立"高信念时禁止怯场"的通道,低信念时仍保持克制。

**落点**:嵌入 `analysis.rs::cio_sanity_check`,放在 fallback 检查**之后**。守卫顺序:
```
Gate1 → Gate2 → Fallback(数据缺失止血) → 【高信念升级】
sentinel 仍在 orchestrator 层(run_committee post-analysis)压顶,不被翻案
```

**触发条件(全部满足)**:
- `!has_unavailable`(无任何角色 fallback/WORKER_UNAVAILABLE——数据缺失绝不伪造信念)
- `gate1_pass && gate2_pass`(未被宏观矛盾/三重恶化否决)
- 当前 `final_verdict == "HOLD"`(只兜底躺平,不翻已有方向)
- Quant 与 Macro **同向且强度都 ≥ 6**(b 档阈值)
- Risk 信号 `!= high_risk`(允许 ok / concerned)

**升级动作**:
- HOLD → 对应方向裁决(Quant+Macro 共同方向映射到 BUY/ACCUMULATE 或 TRIM/SELL)。
- `final_confidence = max(原值, 0.65)`(避免"BUY/0.3"自相矛盾,又不虚高到触发未来的 0.95 降级)。
- 在 `sanity.notes` 标注 `[HIGH_CONVICTION]`,不改写 `reasoning`(archive 会渲染 notes)。

**现有四条降级规则(供实现参考,优先级高→低)**:
1. Sentinel(orchestrator 层最先决胜)→ 集中度暴涨,强制 TRIM/0.3。
2. Gate 1 → macro 与 CIO 反向,改 HOLD,不动 confidence。
3. Gate 2 → risk_off≥7 + Quant bearish≥7 + 亏损≥15%,强制 SELL/0.2。
4. Fallback → 任一角色不可用/数据缺失,拍回 HOLD 且 confidence≤0.4。

**配套:补齐两条"prompt 承诺但无代码"的 hard rule**:

CIO_PROMPT 第 554-555 行写明"系统会自动降级/clamp",但经排查**根本无代码实现**(详见探查:`parser.rs` 解析出 `confidence`/`suggested_alloc_cny`/`first_tranche_cny` 后全程无任何赋值/裁剪)。这是真 bug 而非可选优化,且与高信念通道改的是同一段后处理代码(orchestrator post-analysis,拿到 `final_verdict/final_confidence` 之后、写库之前),顺手补齐:

- **rule A**:`final_confidence >= 0.95 && final_verdict == "BUY"` → 降级 `final_verdict = "ACCUMULATE"`。
- **rule B**:`|suggested_alloc_cny| > 100000` → clamp 到 ±100000;`first_tranche_cny` 同步 clamp(不超过 `suggested_alloc_cny`)。

**与高信念通道的次序**:高信念升级在 `cio_sanity_check` 内产出 `final_verdict/final_confidence`(升级时 confidence 设 `max(原值,0.65)`,刻意不超过 0.95 以避开 rule A);rule A/B 作为最终 clamp 放在 orchestrator post-analysis 拿到最终值之后、sentinel 判定之外的写库前一步,确保任何来源(CIO 原始 / 高信念升级)的高 confidence BUY 与超额 alloc 都被兜住。alloc clamp 仅对 holding 模式有意义(research 模式无 alloc 概念,值通常为 0,clamp 对其无副作用)。

**配套:修正 Gate 1 改 verdict 不动 confidence**:

Gate 1(macro 与 CIO 反向 → 强制 HOLD)目前只改 `final_verdict` 不动 `final_confidence`,导致出现"HOLD 配 0.7"这类自相矛盾组合——CIO 本来高信念看多,被宏观矛盾否决降为 HOLD 后,confidence 却还留着原来的高值,既污染 `verdict_reviews` 命中率统计,也误导记忆注入(命中率闭环正要靠这批数据)。

- **修正**:Gate 1 触发时,在改 `final_verdict = "HOLD"` 的同时,把 `final_confidence` 压低到 `min(原值, 0.4)`——表达"被否决的 HOLD 是低信念的观望",与 Fallback 的 HOLD 口径一致。
- **数据口径权衡**:现有 363 行 `verdict_reviews` 是按旧 Gate 1 行为算出的,改后新旧 confidence 不完全可比。但当前数据量小(16 天、363 行),正是改口径扰动最小的时机;越往后改,历史包袱越重。趁早改更优。记忆注入聚合的是**命中率**(hit/total),不直接依赖 confidence 绝对值,故此改动对第一块影响有限。

### 第四块 · 顺手清理与修复

> **注**:dream 空转 bug 与 `dream_snapshots` 历史快照清理(9472 条 "0 candidates" 污染)均由另一 session 负责——根因已定位为 `config.rs::load_jobs_base` 读取时未做 `normalize_cron_6field`,5 字段 cron 解析失败致 `next_run` 返回 None 而每 tick 触发(详见 memory `project_invest-scheduler-bugs`)。两者同属 scheduler/dream 子系统,归对方处理可避免两 session 同碰 `dream_snapshots` 表。**本 spec 第四块仅保留纯委员会清理项。**

- **CLI prompt 工具噪声**:`--max-turns 1` 下角色不能调工具,但 `strip_tool_section` 只删了 `**你有工具可调用**` 标题块,正文里散落的工具引用(如 Quant 提 `get_recent_committee_verdicts`、Macro 提 `get_recent_events()`)是误导且白占 token,清除。
- **L4 死字段移除**:经排查,L4Officer 角色已 removed,但残留两类零消费的死字段,LLM 仍被要求输出、parser 仍解析、却无任何下游消费(既不进裁决,前端类型亦未声明):
  - Risk:`l4_veto` / `l4_veto_reason` / `l4_veto_r2`(RISK_PROMPT 的 "L4否决/否决原因" 输出项)。
  - CIO:`l4_check_stop_loss` / `l4_check_position` / `l4_check_buy_point` / `l4_execution_checks_passed`(CIO_PROMPT 的 "止损明确/仓位合理/买点合理" 输出项)。
  - 处理:从 RISK_PROMPT、CIO_PROMPT 输出契约 + `parser.rs` `ParsedFields` + 解析逻辑 + 相关单测一并移除。结构化 `止损价`(第二块)成为止损维度唯一权威来源。
  - 注:Gate2 三重恶化卫语句虽血统来源于 L4,但读的是 macro/quant/risk 常规字段,与 L4 字段无关,**保留不动**。L4Officer 枚举壳的彻底拆除超出本次范围,仅清理产生 token 浪费的输出字段。

### 第五块 · 分析模式开关(symbol 级)

**核心问题**:委员会硬假设"评估要不要为我自己的组合动用我的现金",导致研究/替人观察类标的因"无子弹/不在组合"被误降级。引入 symbol 级模式标签解耦用途。

**模式标签**:委员会队列项(`queue.rs` 的 run item)新增 `mode` 字段,取值 `holding`(默认,向后兼容——现有队列项无该字段即视为 holding)或 `research`。同一批运行可混合,各走各的逻辑。具体存储位置实现阶段确认。

**两种模式差异**:

| 维度 | 持仓模式 holding(现状) | 研究模式 research(新增) |
|---|---|---|
| 现金/子弹 | 考虑,无子弹可降级 | **忽略**,不因无现金降级 |
| 集中度 | Risk 评估超配 | **忽略**(不在组合里) |
| 成本/盈亏 | 成本=真实买入均价,算真实浮盈 | **成本=关注价/watch 价**,算"关注以来涨跌 + 当前进场吸引力";无关注价则退化为 N/A 纯标的判断 |
| 裁决语义 | 仓位动作(BUY=建仓/TRIM=减仓) | **标的吸引力**:BUY/ACCUMULATE=值得买入/可分批,HOLD=观望,TRIM/SELL=规避/看空。沿用五档词,prompt 重定义语义 |

**实现要点**:
- **成本来源切换**:研究模式下 Risk 的成本对比从 holdings 买入均价切到 watch 记录的关注价。需确认 holdings/watch 表关注价字段的实际名称与可得性(实现阶段)。
- **prompt 分支**:Risk 和 CIO 按模式注入不同"职责说明段"。研究模式版本:Risk 跳过现金/集中度、成本改用关注价、只评标的自身风险;CIO 裁决语义重定义为标的吸引力。**走运行期 append/分支拼装,不新增 prompt 常量文件**,沿用现有 `build_cli_*_prompt` 结构。
- **下游零改动**:parser(裁决词不变)、verdict_review、归档、UI 不变。命中率闭环天然复用——研究模式判断同样能复盘"看好之后涨没涨"。

**与前几块的交叉点**:
- **第三块高信念通道**:Gate2 的 loss_guard 依赖真实浮亏,研究模式无真实持仓 → **研究模式下 Gate2 跳过**(实现阶段处理)。
- **第一块命中率注入**:首版统一聚合,不按模式拆口径(留待后续)。

### 第六块 · parser 改动汇总

本次唯一需要改 `ParsedFields` 的是结构化点位(第二块)与 L4 清理(第四块):

**新增字段**(第二块):
- `entry_price: Option<f64>` — Quant R1 进场价
- `target_price: Option<f64>` — Quant R1 目标价
- `stop_loss_price: Option<f64>` — CIO 止损价

`止损价`(数字)与现有自由文本 `止损条件` **并存**(走 a 方案):数字用于复盘,文本用于人读/审计,互补不替换。

**移除字段**(第四块 L4 清理):
- `l4_veto`、`l4_veto_reason`、`l4_veto_r2`
- `l4_check_stop_loss`、`l4_check_position`、`l4_check_buy_point`、`l4_execution_checks_passed`

**不改 parser 的块**:第一块(只塞 prompt 文本)、第三块(纯 Rust 后处理,读已有 `signal`/`strength`/`verdict`)、第五块(裁决词不变,`pnl_pct` 复用承载研究模式的关注价涨跌)。

## 测试

- **聚合函数**:构造已知 `verdict_reviews` 数据,断言命中率、样本数、`min_samples=5` 过滤、30d 未到期标注正确。
- **prompt 注入**:断言聚合为空时不注入、有数据时格式正确。
- **高信念通道**:构造各组合(满足全部条件 / 缺一条 / 有 fallback / Risk=high_risk),断言只在该升级时升级,confidence 下限正确,sentinel 仍压顶。
- **hard rule clamp**:断言 BUY/0.97 → ACCUMULATE;alloc 150000 → clamp 100000 且 first_tranche 同步;高信念升级(0.65)不触发 rule A。
- **Gate 1 confidence 压低**:断言 Gate 1 触发改 HOLD 时 confidence = `min(原值, 0.4)`。
- **结构化点位 parser**:断言 `entry_price`/`target_price`/`stop_loss_price` 正确解析,缺失时为 None。
- **L4 字段移除**:断言移除后解析不再产出 l4_* 字段,现有契约其余字段不受影响。
- **分析模式**:断言 `mode` 缺省为 holding(向后兼容);research 模式下 prompt 走研究分支、Gate2 跳过。
- 均为纯函数/纯 SQL,易单测。注意 CLAUDE.md §11 的 Rust test 运行时问题(`STATUS_ENTRYPOINT_NOT_FOUND`)——优先 `cargo check` 保证编译,单测能跑则跑。

## 关键文件参考

- `invest/committee/cli_executor.rs` — prompt 拼装(注入点)、`strip_tool_section`、研究模式分支
- `invest/committee/roles.rs` — prompt 常量(第二块改 Quant/CIO 输出格式;第四块删 L4 输出项)
- `invest/committee/analysis.rs` — `cio_sanity_check`(第三块落点、第五块 Gate2 跳过)、`check_sentinel`
- `invest/committee/orchestrator.rs` — 管线驱动、regime 来源、post-analysis 调用链
- `invest/committee/parser.rs` — `ParsedFields`(第六块:新增 3 价格字段、移除 7 个 L4 字段)
- `invest/committee/queue.rs` — 队列项 `mode` 字段(第五块)
- `storage/invest/verdict_reviews.rs` — 命中率数据 + 新增聚合函数
- `group_chat/memory_injection.rs` — 注入模式参考样板

> dream 空转 bug 与 `dream_snapshots` 快照清理涉及的 `scheduler/runner.rs`、`config.rs`、`storage/invest/dream_snapshots.rs` 由另一 session 处理,本 spec 不碰。

## 已知问题(本次不做,仅记录)

- L4Officer 枚举壳(`CommitteeRole::L4Officer` 及其在 roles/parser/events/tools/round_cache/i18n 的占位分支)仍残留,本次只清产生 token 浪费的输出字段,枚举壳彻底拆除留待后续。理由:这些占位 match 分支零运行时开销(不吃 token、不影响裁决),拆除需动 7-8 个文件却无功能收益。
