use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Committee roles
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitteeRole {
    Macro,
    QuantR1,
    RiskR1,
    Wealth,
    QuantR2,
    RiskR2,
    Cio,
}

impl CommitteeRole {
    pub fn all() -> &'static [CommitteeRole] {
        &[
            Self::Macro,
            Self::QuantR1,
            Self::RiskR1,
            Self::Wealth,
            Self::QuantR2,
            Self::RiskR2,
            Self::Cio,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Macro => "宏观分析师",
            Self::QuantR1 => "量化分析师 R1",
            Self::RiskR1 => "风控官 R1",
            Self::Wealth => "财富配置官",
            Self::QuantR2 => "量化分析师 R2",
            Self::RiskR2 => "风控官 R2",
            Self::Cio => "首席投资官",
        }
    }

    pub fn prompt_filename(&self) -> &'static str {
        match self {
            Self::Macro => "macro.txt",
            Self::QuantR1 => "quant.txt",
            Self::RiskR1 => "risk.txt",
            Self::Wealth => "wealth.txt",
            Self::QuantR2 => "quant_r2.txt",
            Self::RiskR2 => "risk_r2.txt",
            Self::Cio => "cio.txt",
        }
    }

    /// Max Chinese characters for this role's output (RFC D9).
    pub fn max_chars(&self) -> usize {
        match self {
            Self::Macro => 400,
            Self::QuantR1 | Self::QuantR2 | Self::RiskR1 | Self::RiskR2 | Self::Wealth => 200,
            Self::Cio => 300,
        }
    }

    /// Default prompt text for this role.
    pub fn default_prompt(&self) -> &'static str {
        match self {
            Self::Macro => MACRO_PROMPT,
            Self::QuantR1 => QUANT_PROMPT,
            Self::RiskR1 => RISK_PROMPT,
            Self::Wealth => WEALTH_PROMPT,
            Self::QuantR2 => QUANT_REBUTTAL_PROMPT,
            Self::RiskR2 => RISK_REBUTTAL_PROMPT,
            Self::Cio => CIO_PROMPT,
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

/// Load the prompt for a role. Returns the custom prompt if one exists on disk,
/// otherwise returns the built-in default.
pub fn load_prompt(role: CommitteeRole) -> String {
    let path = get_prompt_dir().join(role.prompt_filename());
    std::fs::read_to_string(&path).unwrap_or_else(|_| role.default_prompt().to_string())
}

/// Save a custom prompt for a role to disk.
pub fn save_prompt(role: CommitteeRole, content: &str) -> Result<(), String> {
    let dir = get_prompt_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create prompt dir: {e}"))?;
    let path = dir.join(role.prompt_filename());
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

const MACRO_PROMPT: &str = r#"你是一位资深宏观经济分析师，专注于中国A股市场。

你的职责：
1. 分析当前宏观经济环境（GDP、CPI、PMI、社融等）
2. 判断市场趋势（沪深300指数60日分位数）
3. 监测资金流向（北向资金、两融余额、主力资金）
4. 评估政策环境对A股的影响
5. 输出宏观信号 (risk_on / risk_off)

输出格式（严格遵循）：
- 先给出一段简要的宏观分析（100字以内）
- 然后在最后一行输出：SIGNAL: risk_on 或 SIGNAL: risk_off
- 如果判断为 risk_off，追加一行：STRENGTH: N（1-10，10为最强）

你有以下工具可用，可以调用来获取实时数据：
- get_market_overview: 获取沪深300/中证500的PE/PB/股息率等估值指标
- get_money_flow: 获取北向资金近20日净流入、两融余额变化
- get_macro_indicators: 获取最新GDP、CPI、PMI、社融等宏观数据
- get_policy_news: 获取最新宏观政策和监管动态
- get_risk_events: 获取重大风险事件（战争、疫情、自然灾害、政策突变等）"#;

const QUANT_PROMPT: &str = r#"你是一位资深量化分析师（Round 1），专注于技术面和资金面分析。

你的职责：
1. 分析股票/资产的技术指标（RSI、MACD、布林带等）
2. 评估量价关系和资金流向
3. 判断技术支撑位和阻力位
4. 对宏观信号给出量化确认或挑战

输出格式（严格遵循）：
- 先给出技术分析摘要（80字以内）
- 然后输出以下字段（每个单独一行）：
QUANT_VIEW: AGREE_with_Macro 或 CHALLENGE_with_Macro
STRENGTH: N（1-10，确认或挑战的强度）

重要：你的分析必须基于数据，不要凭感觉判断。"#;

const RISK_PROMPT: &str = r#"你是一位资深风控官（Round 1），专注于风险评估和仓位管理。

你的职责：
1. 评估当前持仓的集中度风险
2. 分析流动性风险和变现能力
3. 计算风险敞口和对冲需求
4. 对量化分析师的观点给出风控视角

输出格式（严格遵循）：
- 先给出风险评估摘要（80字以内）
- 然后输出以下字段（每个单独一行）：
RISK_VIEW: SUPPORT 或 CHALLENGE
CONCENTRATION_PCT: N（当前持仓集中度百分比）
DRY_POWDER_CNY: N（可用现金，元）
STRENGTH: N（1-10，风险判断的强度）"#;

const WEALTH_PROMPT: &str = r#"你是一位财富配置官，专注于资产配置和长期规划。

你的职责：
1. 评估投资者的风险承受能力和流动性需求
2. 分析当前资产配置的合理性
3. 提供财富保值增值的配置建议
4. 考虑税务优化和传承规划

输出格式（严格遵循）：
- 先给出配置评估摘要（80字以内）
- 然后输出以下字段（每个单独一行）：
WEALTH_CONTEXT: BULLISH / NEUTRAL / CAUTIOUS
SOLVENCY_BUFFER_LEVEL: HIGH / MEDIUM / LOW

注意：你的评估应基于保守原则，优先保障资金安全。"#;

const QUANT_REBUTTAL_PROMPT: &str = r#"你是一位资深量化分析师（Round 2 — 反驳轮），你已经看到了Round 1的分析结果。

你的职责：
1. 审视Round 1中量化和风控的分析
2. 寻找他们可能遗漏的技术信号
3. 挑战或确认之前的量化观点
4. 如果你改变观点，必须给出充分理由

输出格式（严格遵循）：
- 先给出你的反驳或确认摘要（80字以内）
- 然后输出以下字段（每个单独一行）：
QUANT_VIEW: AGREE_with_Macro 或 CHALLENGE_with_Macro
STRENGTH: N（1-10）

重要：不要为了反驳而反驳，必须基于数据和逻辑。"#;

const RISK_REBUTTAL_PROMPT: &str = r#"你是一位资深风控官（Round 2 — 反驳轮），你已经看到了Round 1的所有分析。

你的职责：
1. 审视Round 1中量化和风控的分析
2. 重新评估风险敞口和集中度
3. 挑战或确认之前的风险判断
4. 如果你改变观点，必须给出充分理由

输出格式（严格遵循）：
- 先给出你的反驳或确认摘要（80字以内）
- 然后输出以下字段（每个单独一行）：
RISK_VIEW: SUPPORT 或 CHALLENGE
CONCENTRATION_PCT: N
DRY_POWDER_CNY: N
STRENGTH: N（1-10）

重要：风控官的职责是保护资本，宁可错过机会也不能忽视风险。"#;

const CIO_PROMPT: &str = r#"你是首席投资官（CIO），负责最终投资决策。

你的职责：
1. 综合所有分析师的观点（宏观、量化、风控、财富配置）
2. 做出最终的投资决策
3. 给出明确的操作指令和执行计划
4. 设定风险止损线和目标位

你必须通过以下三项"合理性检查"（Sanity Check 3 Gates）：
Gate 1: 你的决策是否与宏观信号一致？
Gate 2: 量化分析是否支持你的决策？
Gate 3: 风控指标是否在可接受范围内？

输出格式（严格遵循）：
- 先给出综合分析和决策理由（100字以内）
- 然后输出以下字段（每个单独一行）：
VERDICT: BUY / ACCUMULATE / HOLD / TRIM / SELL
CONFIDENCE: N（0.0-1.0）
CONCENTRATION_PCT: N（建议的目标集中度）
PERSONAL_NOTE: 给投资者的个人建议（50字以内）
EXECUTION_PLAN: 具体执行步骤（100字以内）
RISK_PLAN: 风险控制措施（50字以内）

注意：你的决策直接影响投资收益，请慎重考虑每个因素。"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_all_count() {
        assert_eq!(CommitteeRole::all().len(), 7);
    }

    #[test]
    fn test_role_labels() {
        assert_eq!(CommitteeRole::Macro.label(), "宏观分析师");
        assert_eq!(CommitteeRole::Cio.label(), "首席投资官");
    }

    #[test]
    fn test_prompt_filenames() {
        assert_eq!(CommitteeRole::Macro.prompt_filename(), "macro.txt");
        assert_eq!(CommitteeRole::QuantR1.prompt_filename(), "quant.txt");
        assert_eq!(CommitteeRole::RiskR1.prompt_filename(), "risk.txt");
        assert_eq!(CommitteeRole::Wealth.prompt_filename(), "wealth.txt");
        assert_eq!(CommitteeRole::QuantR2.prompt_filename(), "quant_r2.txt");
        assert_eq!(CommitteeRole::RiskR2.prompt_filename(), "risk_r2.txt");
        assert_eq!(CommitteeRole::Cio.prompt_filename(), "cio.txt");
    }

    #[test]
    fn test_max_chars() {
        assert_eq!(CommitteeRole::Macro.max_chars(), 400);
        assert_eq!(CommitteeRole::QuantR1.max_chars(), 200);
        assert_eq!(CommitteeRole::RiskR1.max_chars(), 200);
        assert_eq!(CommitteeRole::Wealth.max_chars(), 200);
        assert_eq!(CommitteeRole::QuantR2.max_chars(), 200);
        assert_eq!(CommitteeRole::RiskR2.max_chars(), 200);
        assert_eq!(CommitteeRole::Cio.max_chars(), 300);
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
        let long = "这是一段超过200个汉字的测试文本".repeat(50);
        let (result, was_truncated) = hard_truncate(&long, CommitteeRole::QuantR1, 1);
        assert!(was_truncated);
        assert!(result.chars().count() <= 200);
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
        // No custom file on disk, should return default
        let prompt = load_prompt(CommitteeRole::Macro);
        assert!(prompt.contains("宏观"));
        assert!(prompt.contains("risk_on"));
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
