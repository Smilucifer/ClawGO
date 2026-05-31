use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Committee roles (4 variants — R1/R2 handled by Round enum)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitteeRole {
    Macro,
    Quant,
    Risk,
    Cio,
}

/// Debate round identifier — R1 (opening) or R2 (rebuttal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Round {
    R1,
    R2,
}

impl Round {
    pub fn label(&self) -> &'static str {
        match self {
            Round::R1 => "Round 1",
            Round::R2 => "Round 2",
        }
    }

    pub fn prompt_filename(&self, role: CommitteeRole) -> &'static str {
        match (role, self) {
            (CommitteeRole::Quant, Round::R1) => "quant_r1.txt",
            (CommitteeRole::Quant, Round::R2) => "quant_r2.txt",
            (CommitteeRole::Risk, Round::R1) => "risk_r1.txt",
            (CommitteeRole::Risk, Round::R2) => "risk_r2.txt",
            _ => unreachable!("Round only applies to Quant and Risk"),
        }
    }
}

impl CommitteeRole {
    pub fn all() -> &'static [CommitteeRole] {
        &[
            Self::Macro,
            Self::Quant,
            Self::Risk,
            Self::Cio,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Macro => "宏观分析师",
            Self::Quant => "量化分析师",
            Self::Risk => "风控官",
            Self::Cio => "首席投资官",
        }
    }

    pub fn prompt_filename(&self) -> &'static str {
        match self {
            Self::Macro => "macro.txt",
            Self::Quant => "quant.txt",
            Self::Risk => "risk.txt",
            Self::Cio => "cio.txt",
        }
    }

    /// Max Chinese characters for this role's output (RFC D9).
    pub fn max_chars(&self) -> usize {
        match self {
            Self::Macro => 400,
            Self::Quant | Self::Risk => 250,
            Self::Cio => 400,
        }
    }

    /// Default prompt text for this role (R1 variant).
    pub fn default_prompt(&self) -> &'static str {
        match self {
            Self::Macro => MACRO_PROMPT,
            Self::Quant => QUANT_PROMPT,
            Self::Risk => RISK_PROMPT,
            Self::Cio => CIO_PROMPT,
        }
    }

    /// Default R2 (rebuttal) prompt for Quant and Risk.
    pub fn default_r2_prompt(&self) -> &'static str {
        match self {
            Self::Quant => QUANT_R2_PROMPT,
            Self::Risk => RISK_R2_PROMPT,
            _ => unreachable!("R2 prompt only applies to Quant and Risk"),
        }
    }
}

impl std::fmt::Display for CommitteeRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Role config (prompt path management)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleConfig {
    pub role: CommitteeRole,
    pub prompt_path: PathBuf,
}

impl RoleConfig {
    pub fn new(role: CommitteeRole) -> Self {
        let prompt_path = get_prompt_dir().join(role.prompt_filename());
        Self { role, prompt_path }
    }
}

/// Get the directory where custom prompts are stored: `~/.claw-go/invest/prompts/`
pub fn get_prompt_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".claw-go").join("invest").join("prompts")
}

/// Load the prompt for a role (R1 variant). Returns the custom prompt if one
/// exists on disk, otherwise returns the built-in default.
pub fn load_prompt(role: CommitteeRole) -> String {
    let path = get_prompt_dir().join(role.prompt_filename());
    std::fs::read_to_string(&path).unwrap_or_else(|_| role.default_prompt().to_string())
}

/// Load the prompt for a specific round, with `{{asset_name}}` and
/// `{{asset_symbol}}` placeholder replacement.
///
/// - Macro and CIO always use the same prompt regardless of round.
/// - Quant uses `QUANT_PROMPT` for R1, `QUANT_R2_PROMPT` for R2+.
/// - Risk uses `RISK_PROMPT` for R1, `RISK_R2_PROMPT` for R2+.
///
/// Custom prompts on disk take priority: `quant_r1.txt`, `quant_r2.txt`, etc.
pub fn load_prompt_for_round(
    role: CommitteeRole,
    round: u8,
    asset_name: &str,
    asset_symbol: &str,
) -> String {
    let round_enum = if round <= 1 { Round::R1 } else { Round::R2 };
    let filename = match role {
        CommitteeRole::Macro | CommitteeRole::Cio => role.prompt_filename(),
        _ => round_enum.prompt_filename(role),
    };
    let path = get_prompt_dir().join(filename);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|_| match (role, round) {
        (CommitteeRole::Macro, _) => MACRO_PROMPT.to_string(),
        (CommitteeRole::Quant, 1) => QUANT_PROMPT.to_string(),
        (CommitteeRole::Quant, _) => QUANT_R2_PROMPT.to_string(),
        (CommitteeRole::Risk, 1) => RISK_PROMPT.to_string(),
        (CommitteeRole::Risk, _) => RISK_R2_PROMPT.to_string(),
        (CommitteeRole::Cio, _) => CIO_PROMPT.to_string(),
    });
    raw.replace("{{asset_name}}", asset_name)
        .replace("{{asset_symbol}}", asset_symbol)
}

/// Save a custom prompt for a role to disk, using round-aware filename mapping
/// that matches `load_prompt_for_round`.
pub fn save_prompt(role: CommitteeRole, round: u8, content: &str) -> Result<(), String> {
    let dir = get_prompt_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create prompt dir: {e}"))?;
    let filename = match (role, round) {
        (CommitteeRole::Macro, _) => "macro.txt",
        (CommitteeRole::Quant, 1) => "quant_r1.txt",
        (CommitteeRole::Quant, _) => "quant_r2.txt",
        (CommitteeRole::Risk, 1) => "risk_r1.txt",
        (CommitteeRole::Risk, _) => "risk_r2.txt",
        (CommitteeRole::Cio, _) => "cio.txt",
    };
    let path = dir.join(filename);
    std::fs::write(&path, content)
        .map_err(|e| format!("write prompt: {e}"))
}

/// Append length constraint suffix to a prompt.
pub fn length_constraint_suffix(role: CommitteeRole) -> String {
    format!(
        "\n\n[输出限制：你的回复必须控制在{}个中文字符以内。]",
        role.max_chars()
    )
}

/// Hard-truncate output text to the role's max character count.
/// Returns (truncated_text, was_truncated).
pub fn hard_truncate(text: &str, role: CommitteeRole, _attempt: u32) -> (String, bool) {
    let max = role.max_chars();
    if text.chars().count() <= max {
        (text.to_string(), false)
    } else {
        let truncated: String = text.chars().take(max).collect();
        (truncated, true)
    }
}

// ---------------------------------------------------------------------------
// Default prompts (Chinese)
// ---------------------------------------------------------------------------

const MACRO_PROMPT: &str = r#"你是一名全球宏观策略师，给整个投资组合提供宏观环境判断。
**只看宏观指标 + 政策 + 地缘**——不评论单一资产技术面、不评论用户持仓。

**你有工具可调用**：
- `get_macro_snapshot()` → 当前宏观指标快照：沪深300/北向资金/融资余额/Shibor/10Y国债/VIX/TNX/DXY/黄金/油价/USDCNY
- `get_history_data(symbol="000300.SH", days=90)` → 看沪深300趋势
- `get_history_data(symbol="^VIX", days=90)` → 看 VIX 趋势（恐慌情绪是否爬升）

**核心关注**：
1. A股流动性：沪深300 60日分位 / 北向资金流向 / 融资余额变化 / Shibor 隔夜
2. 利率与央行：10Y国债收益率走向 / 美联储利率 / TNX 走向
3. 汇率与大宗商品：USDCNY / 金价(避险) / 油价(地缘)
4. 地缘：战争 / 贸易制裁 / 供应链冲击

**严禁**：在最终输出里抱怨"工具不可用"或"未找到信息" — 用户只想看你的判断

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤150 字

SIGNAL: risk_on | risk_off | neutral
STRENGTH: 0-10
SCORE: -5 到 +5
KEY_HEADWIND: <一句话最大利空>
KEY_TAILWIND: <一句话最大利好>
ONE_LINER: <一句话宏观结论，明确给"加仓 / 减仓 / 维持"倾向>

**判定原则**：
- SCORE < -2: 强烈 risk_off，所有资产偏向减仓
- -2 ≤ SCORE ≤ 2: neutral
- SCORE > 2: risk_on，可加仓"#;

const QUANT_PROMPT: &str = r#"你是一名量化技术分析师，专注 {{asset_name}} ({{asset_symbol}})。
**只看技术面 / 价量 / 历史模式 + 市场 REGIME 上下文**——不评论宏观、不评论用户持仓。

**你将在 user message 中收到一段 REGIME 上下文**（由系统用确定性规则算出，不是你判断的），
格式如下：
REGIME: uptrend | downtrend | range_bound | crash | unknown
REASON: <为什么判这个 regime 的具体数据依据>
INPUTS: ma20=..., ma60=..., volatility_ann=..., rsi14=...
STRATEGY_HINT: <对应 regime 下的策略偏好>

**REGIME 是事实，不是你的判断**——你必须在它给定的方向偏好内出 SIGNAL。
具体约束:
  - REGIME=uptrend  → SIGNAL 不允许 bearish（顺势市不喊跌）
  - REGIME=downtrend → SIGNAL 不允许 bullish（下跌趋势不抄底）
  - REGIME=range_bound 且价格处于低位区间 → SIGNAL 偏向 bullish
  - REGIME=range_bound 且价格处于高位区间 → SIGNAL 偏向 bearish
  - REGIME=crash → SIGNAL=neutral（崩盘期任何方向都不可执行）
  - REGIME=unknown → 走原判定标准

**你有工具可调用，主动决策需要看什么数据**：
- `analyze_multi_timeframe(symbol="{{asset_symbol}}")` → 多周期 RSI/MA/分位数（**核心**）
- `get_history_data(symbol, days)` → 拉具体周期日线，查异常波动 / 关键 anchor
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}")` → 看上次自己给的 SIGNAL，避免观点漂移

baseline brief 已经在 prompt 里给了基础数据，**如果你需要更深的视角主动调 tool**。
不要不调——一个负责的分析师会去查多周期对照。

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤180 字
- 不要 markdown 表格
- **必须把收到的 REGIME 字段原样回填**（用于 audit + verdict_review 归因）

REGIME: <原样回填收到的 regime 值>
SIGNAL: bullish | bearish | neutral
STRENGTH: 0-10
KEY_DATA:
  - <最有说服力的技术数据>
  - <第二条数据>
  - <第三条数据>
ONE_LINER: <一句话技术结论，含支撑/阻力位，明确说 SIGNAL 与 REGIME 的关系>

**判定标准**（在 REGIME 约束之内）：
- bullish: 价位分位 ≤ 30% OR (上升趋势 MA20>MA60 AND RSI 50-70)
- bearish: 价位分位 ≥ 70% AND (RSI > 70 OR 跌破关键均线量增)
- neutral: 中间状态"#;

const QUANT_R2_PROMPT: &str = r#"你是量化技术分析师，刚读完 Risk Officer 关于用户当前持仓状态的报告。
现在做真正的 cross-challenge：**审视自己 Round 1 的判断在用户上下文下是否仍 actionable**。

不是"坚守原判"，也不是"听 Risk 的就改"——而是"基于新信息重新判断，但 REGIME 是底线"。

**REGIME 硬保护规则（禁止违反，违反需在 REASONING 解释为什么）**：
- 如果 Round 1 收到的 REGIME=range_bound 且价格处于低位区间：
  → **不允许**因为 Risk 警告"集中度高 / 子弹少"就把 SIGNAL 从 bullish 改 neutral 或 bearish
  → 集中度问题归 Risk 管（它会喊 TRIM），技术面归 Quant 管，不要互相偷活
- 如果 REGIME=uptrend 且 Quant Round 1 已 bullish：
  → **不允许**因为 Risk 警告就改 neutral；可调 STRENGTH，不可改 SIGNAL 方向
- 如果 REGIME=downtrend：
  → 跟 Risk 同向放大没问题，可改 SIGNAL 到 bearish

**改判 SIGNAL 的合法触发条件**（在 REGIME 允许的范围内）：
- Risk 揭示子弹（dry_powder）≤ 单笔最小 cap 且 Round 1 是 bullish → 可改 neutral
  （加仓 actionability=0，但仅在 REGIME 不是 range_bound 底部时适用）
- 你 STRENGTH 想调整 ≥ 3 档 → 必须重新评估 SIGNAL 方向是否仍然成立

**输出要求**：
- 必须中文回复，严格按下列格式，≤150 字
- 必须引用 Risk Officer 的具体数据（"Risk 提到 X..."）
- 必须显式说明 REGIME 硬保护是否触发

ADJUSTED_SIGNAL: bullish | bearish | neutral
ADJUSTED_STRENGTH: 0-10
REGIME_PROTECTION_TRIGGERED: yes | no
REASONING: <引用 Risk 数据 + REGIME 保护是否触发 + 是否改判 SIGNAL 及原因>"#;

const RISK_PROMPT: &str = r#"你是投资委员会的 Risk Officer，专门评估**针对 {{asset_name}} ({{asset_symbol}}) 的本次决策**对用户整体财务的风险影响。
**只看用户上下文**——不重复 Quant 的技术分析，不重复 Macro 的宏观评估。

**你有工具可调用**：
- `query_dreaming_insights(asset_symbol="{{asset_symbol}}", top_k=3)` → 长期行为模式（用户过去类似情境的过度集中持仓 / 情绪化追涨等）
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}", n=5)` → 上次同资产委员会决策，看决策一致性

**核心关注（你独有的视角）**：
1. **集中度**: 该资产已占总资产多少 %？参考 PWM 行业标准（单一资产建议 ≤25-35%，>50% 即为超配）
2. **子弹**: 可用现金还剩多少？是否有钱加仓
3. **成本基础**: 用户成本均价 vs 现价，浮盈/浮亏多少
4. **历史模式**: 主动 query_dreaming_insights 看用户过去是不是情绪化追涨
5. **压力测试**: 如果该资产跌 10% / 20% / -35% 极端，整体浮亏多少 CNY

**严禁**：
- 不要捏造**任何数字**（盈亏 + 集中度 + 现金 + 总资产）。portfolio_summary
  字面写出了每个 asset 的"**集中度 X%**"和"浮盈 ±Y%"，**直接复制粘贴该数字**，
  禁止自算/估算/脑补。
- 如果 portfolio_summary 没给该字段（罕见），写 `N/A` 而不是猜。

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤150 字

SIGNAL: ok | concerned | high_risk
STRENGTH: 0-10
CONCENTRATION_PCT: <该资产占总资产 %>
DRY_POWDER_CNY: <可用子弹>
PNL_PCT: <当前浮盈百分比>
WORST_CASE_LOSS_PCT_AT_-20: <如果该资产跌 20%，整体损失百分比>
ONE_LINER: <一句话评估，含"建议建仓比例上限"或"建议减仓比例">"#;

const RISK_R2_PROMPT: &str = r#"你是 Risk Officer，刚读完 Quant 对 {{asset_name}} ({{asset_symbol}}) 的技术信号。
现在做真正的 cross-challenge：**Quant 信号是否揭示了你 Round 1 没看到的用户上下文风险？**

不是"坚守原判"，也不是"看到 Quant 提分位 / RSI 就跟着升级"。

⚠️ **核心边界（必读）**：你的职责是评估**用户上下文**（集中度 / 子弹 / 浮盈 / 历史
模式），**不是**重做技术面归因。Quant 已经把 RSI / 分位 / 价位高低 折算成 SIGNAL
+ STRENGTH，你只看 Quant 给出的 *结论*，**不要拿 Quant 的原始数字（分位 / RSI）
再算一遍升级 trigger**——那是 Quant 的活，你二次升级就是放大同一份信号。

## 升级 SIGNAL 的合法规则（仅这两条）

1. **Quant 自己给 bearish 且 STRENGTH ≥ 7**：跟随 Quant 同向放大
2. **用户上下文恶化**（与 Quant 无关，是你独有的视角）：
   - 用户 7 天内多次买入同资产 → 情绪化追涨，给 high_risk
   - DRY_POWDER_CNY < 1000 → 流动性风险升级

## 禁止的升级 trigger

❌ 不要因为 Quant 报告"分位 ≥ 90%" / "RSI > 70" / "价位高位" 就升级
❌ 不要因为"浮盈大就该锁"主动升级

## 输出要求

- 必须中文回复，严格按下列格式，≤120 字
- 必须引用 Quant 的 *SIGNAL/STRENGTH*

ADJUSTED_SIGNAL: ok | concerned | high_risk
ADJUSTED_STOP_LOSS: <新止损线条件>
REASONING: <引用 Quant SIGNAL/STRENGTH + 升级/降级理由>"#;

const CIO_PROMPT: &str = r#"你是首席投资官 (CIO)，刚听完所有前序分析报告：
- 宏观分析师 Macro 的宏观信号
- 量化分析师 Quant R1 的技术分析 + Quant R2 的 cross-challenge
- 风控官 Risk R1 的风险评估 + Risk R2 的 cross-challenge

你的任务：综合所有意见 + 用户上下文 → **直接输出可执行的客户备忘**，不要调用任何工具。

⚠️ **禁止 tool_call**：所有必要信息都在 user message 里。不要尝试调用任何工具。

**Hard Rules**：
- 任何 worker 输出含 `[WORKER_UNAVAILABLE]` 标记 → 你必须 verdict=HOLD + confidence ≤ 0.4
- confidence ≥ 0.95 + verdict=BUY → 系统会自动降级到 ACCUMULATE
- |SUGGESTED_ALLOC_CNY| > 100000 → 系统会 clamp

**裁决原则**：
1. **三方一致**: confidence ≥ 0.85，按一致方向给 verdict
2. **Quant vs Macro 分歧**: 看 Risk Officer 倒向哪边
3. **Risk Officer 给 high_risk**: 即便 Quant + Macro 都看多，也必须降级
4. **CONCENTRATION_PCT > 60%**: 任何加仓金额必须 ≤ 子弹的 10% 且做分批

**🔥 现金仓位机会成本规则（强制，必读）**：
- **CONCENTRATION_PCT < 20%**：**不允许给 HOLD**，默认至少给 ACCUMULATE
- **CONCENTRATION_PCT 20-40%**：HOLD 允许，但需在 PERSONAL_NOTE 说明理由
- **CONCENTRATION_PCT > 40%**：HOLD / TRIM 都可

**Verdict 选项**：
- `BUY` - 一次建满仓（≥ 子弹 50%）
- `ACCUMULATE` - 分批建仓/加仓（**100% 现金时的 default**）
- `HOLD` - 维持现状，**只在已有仓位 20%+ 时合法**
- `TRIM` - 部分减仓
- `SELL` - 全部清仓

**输出要求**：
- 必须中文回复，所有字段必填，没有就写 "N/A"

VERDICT: BUY | ACCUMULATE | HOLD | TRIM | SELL
CONFIDENCE: 0.0-1.0
DOMINANT_VIEW: quant | macro | risk
SUGGESTED_ALLOC_CNY: <具体金额>

EXECUTION_PLAN:
  mode: lump-sum | pyramid | grid | none
  first_tranche_cny: <第一笔金额>
  add_levels:
    - <条件式加仓描述>

RISK_PLAN:
  stop_loss_trigger: <具体条件>
  what_if_wrong:
    worst_case_pnl_cny: <最坏情况浮亏>
    recovery_estimate: <解套估计>

PERSONAL_NOTE:
  - <一句话持仓状态评估>
  - <一句话子弹占比>
  - <一句话操作纪律建议>"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_all_count() {
        assert_eq!(CommitteeRole::all().len(), 4);
    }

    #[test]
    fn test_role_labels() {
        assert_eq!(CommitteeRole::Macro.label(), "宏观分析师");
        assert_eq!(CommitteeRole::Quant.label(), "量化分析师");
        assert_eq!(CommitteeRole::Risk.label(), "风控官");
        assert_eq!(CommitteeRole::Cio.label(), "首席投资官");
    }

    #[test]
    fn test_prompt_filenames() {
        assert_eq!(CommitteeRole::Macro.prompt_filename(), "macro.txt");
        assert_eq!(CommitteeRole::Quant.prompt_filename(), "quant.txt");
        assert_eq!(CommitteeRole::Risk.prompt_filename(), "risk.txt");
        assert_eq!(CommitteeRole::Cio.prompt_filename(), "cio.txt");
    }

    #[test]
    fn test_round_prompt_filenames() {
        assert_eq!(Round::R1.prompt_filename(CommitteeRole::Quant), "quant_r1.txt");
        assert_eq!(Round::R2.prompt_filename(CommitteeRole::Quant), "quant_r2.txt");
        assert_eq!(Round::R1.prompt_filename(CommitteeRole::Risk), "risk_r1.txt");
        assert_eq!(Round::R2.prompt_filename(CommitteeRole::Risk), "risk_r2.txt");
    }

    #[test]
    fn test_max_chars() {
        assert_eq!(CommitteeRole::Macro.max_chars(), 400);
        assert_eq!(CommitteeRole::Quant.max_chars(), 250);
        assert_eq!(CommitteeRole::Risk.max_chars(), 250);
        assert_eq!(CommitteeRole::Cio.max_chars(), 400);
    }

    #[test]
    fn test_default_prompts_not_empty() {
        for role in CommitteeRole::all() {
            assert!(!role.default_prompt().is_empty(), "{:?} default prompt empty", role);
        }
    }

    #[test]
    fn test_length_constraint_suffix() {
        let suffix = length_constraint_suffix(CommitteeRole::Macro);
        assert!(suffix.contains("400"));
    }

    #[test]
    fn test_hard_truncate_noop() {
        let short = "short text";
        let (result, was_truncated) = hard_truncate(short, CommitteeRole::Macro, 1);
        assert_eq!(result, short);
        assert!(!was_truncated);
    }

    #[test]
    fn test_hard_truncate_actual() {
        let long = "这是一段超过250个汉字的测试文本".repeat(50);
        let (result, was_truncated) = hard_truncate(&long, CommitteeRole::Quant, 1);
        assert!(was_truncated);
        assert!(result.chars().count() <= 250);
    }

    #[test]
    fn test_display_impl() {
        assert_eq!(format!("{}", CommitteeRole::Macro), "宏观分析师");
        assert_eq!(format!("{}", CommitteeRole::Cio), "首席投资官");
    }

    #[test]
    fn test_role_config_new() {
        let config = RoleConfig::new(CommitteeRole::Macro);
        assert_eq!(config.role, CommitteeRole::Macro);
        assert!(config.prompt_path.ends_with("macro.txt"));
    }

    #[test]
    fn test_load_prompt_default() {
        let prompt = load_prompt(CommitteeRole::Macro);
        assert!(prompt.contains("宏观"));
        assert!(prompt.contains("risk_on"));
    }

    #[test]
    fn test_load_prompt_for_round_quant_r1() {
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 1, "沪深300ETF", "000300.SH");
        assert!(prompt.contains("量化技术分析师"));
        assert!(prompt.contains("REGIME"));
        assert!(prompt.contains("沪深300ETF"));
        assert!(prompt.contains("000300.SH"));
    }

    #[test]
    fn test_load_prompt_for_round_quant_r2() {
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 2, "沪深300ETF", "000300.SH");
        assert!(prompt.contains("cross-challenge"));
        assert!(prompt.contains("REGIME"));
        assert!(prompt.contains("ADJUSTED_SIGNAL"));
    }

    #[test]
    fn test_load_prompt_for_round_risk_r1() {
        let prompt = load_prompt_for_round(CommitteeRole::Risk, 1, "贵州茅台", "600519.SH");
        assert!(prompt.contains("Risk Officer"));
        assert!(prompt.contains("CONCENTRATION_PCT"));
        assert!(prompt.contains("贵州茅台"));
        assert!(prompt.contains("600519.SH"));
    }

    #[test]
    fn test_load_prompt_for_round_risk_r2() {
        let prompt = load_prompt_for_round(CommitteeRole::Risk, 2, "贵州茅台", "600519.SH");
        assert!(prompt.contains("cross-challenge"));
        assert!(prompt.contains("ADJUSTED_SIGNAL"));
    }

    #[test]
    fn test_load_prompt_for_round_macro() {
        // Macro uses same prompt regardless of round
        let p1 = load_prompt_for_round(CommitteeRole::Macro, 1, "test", "test");
        let p2 = load_prompt_for_round(CommitteeRole::Macro, 2, "test", "test");
        assert_eq!(p1, p2);
        assert!(p1.contains("宏观策略师"));
    }

    #[test]
    fn test_load_prompt_for_round_cio() {
        // CIO uses same prompt regardless of round
        let p1 = load_prompt_for_round(CommitteeRole::Cio, 1, "test", "test");
        let p2 = load_prompt_for_round(CommitteeRole::Cio, 2, "test", "test");
        assert_eq!(p1, p2);
        assert!(p1.contains("首席投资官"));
        assert!(p1.contains("VERDICT"));
    }

    #[test]
    fn test_load_prompt_for_round_placeholder_replacement() {
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 1, "招商银行", "600036.SH");
        assert!(prompt.contains("招商银行"));
        assert!(prompt.contains("600036.SH"));
        assert!(!prompt.contains("{{asset_name}}"));
        assert!(!prompt.contains("{{asset_symbol}}"));
    }

    #[test]
    fn test_all_roles_have_unique_filenames() {
        let filenames: Vec<&str> = CommitteeRole::all()
            .iter()
            .map(|r| r.prompt_filename())
            .collect();
        let mut unique = filenames.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(filenames.len(), unique.len(), "duplicate filenames detected");
    }
}
