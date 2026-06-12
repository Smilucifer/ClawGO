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
    L4Officer,
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
            Self::L4Officer,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Macro => "宏观分析师",
            Self::Quant => "量化分析师",
            Self::Risk => "风控官",
            Self::Cio => "首席投资官",
            Self::L4Officer => "L4 行为官",
        }
    }

    pub fn prompt_filename(&self) -> &'static str {
        match self {
            Self::Macro => "macro.txt",
            Self::Quant => "quant.txt",
            Self::Risk => "risk.txt",
            Self::Cio => "cio.txt",
            Self::L4Officer => "l4_officer.txt",
        }
    }

    /// Max Chinese characters for this role's output (RFC D9).
    pub fn max_chars(&self) -> usize {
        match self {
            Self::Macro => 600,
            Self::Quant | Self::Risk => 350,
            Self::Cio => 600,
            Self::L4Officer => 250,
        }
    }

    /// Default prompt text for this role (R1 variant).
    pub fn default_prompt(&self) -> &'static str {
        match self {
            Self::Macro => MACRO_PROMPT,
            Self::Quant => QUANT_PROMPT,
            Self::Risk => RISK_PROMPT,
            Self::Cio => CIO_PROMPT,
            Self::L4Officer => L4_OFFICER_PROMPT,
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

    /// L4 Officer 是否参与 R1/R2 轮次（默认不参与，仅 CIO 后置调用）
    pub fn is_l4_role(&self) -> bool {
        matches!(self, Self::L4Officer)
    }

    /// Critical field keys for this role (used by hard_truncate preservation and fallback detection).
    pub fn critical_field_keys(&self) -> &'static [&'static str] {
        match self {
            CommitteeRole::Macro => &["SIGNAL", "信号"],
            CommitteeRole::Quant => &["SIGNAL", "信号", "REGIME", "市场状态"],
            CommitteeRole::Risk => &["SIGNAL", "信号"],
            CommitteeRole::Cio => &["VERDICT", "裁决"],
            CommitteeRole::L4Officer => &["GUARD_CLAUSE", "卫语句"],
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

/// Load the prompt for a specific round, with placeholder replacement.
///
/// Replaces all `{{placeholder}}` tokens with actual values from `AssetContext`.
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
    asset_context: &crate::invest::committee::orchestrator::AssetContext,
) -> String {
    let round_enum = if round <= 1 { Round::R1 } else { Round::R2 };
    let filename = match role {
        CommitteeRole::Macro | CommitteeRole::Cio | CommitteeRole::L4Officer => {
            role.prompt_filename()
        }
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
        (CommitteeRole::L4Officer, _) => L4_OFFICER_PROMPT.to_string(),
    });

    let fmt = |v: Option<f64>, decimals: usize| -> String {
        v.map(|v| format!("{:.1$}", v, decimals))
            .unwrap_or_else(|| "N/A".to_string())
    };

    raw.replace("{{asset_name}}", asset_name)
        .replace("{{asset_symbol}}", asset_symbol)
        .replace("{{asset_type}}", &asset_context.asset_type)
        .replace("{{industry}}", asset_context.industry.as_deref().unwrap_or("N/A"))
        .replace("{{pe_ttm}}", &fmt(asset_context.pe_ttm, 1))
        .replace("{{pb}}", &fmt(asset_context.pb, 2))
        .replace("{{roe}}", &fmt(asset_context.roe, 1))
        .replace("{{turnover_rate}}", &fmt(asset_context.turnover_rate, 2))
        .replace("{{money_flow_daily_summary}}", asset_context.money_flow_daily_summary.as_deref().unwrap_or("N/A"))
        .replace("{{money_flow_summary}}", asset_context.money_flow_summary.as_deref().unwrap_or("N/A"))
        .replace("{{latest_close}}", &fmt(asset_context.latest_close, 2))
        .replace("{{pre_close}}", &fmt(asset_context.pre_close, 2))
        .replace("{{circ_mv_yi}}", &fmt(asset_context.circ_mv_yi, 2))
        .replace("{{roa}}", &fmt(asset_context.roa, 2))
        .replace("{{debt_to_assets}}", &fmt(asset_context.debt_to_assets, 1))
        .replace("{{or_yoy}}", &fmt(asset_context.or_yoy, 1))
        .replace("{{np_yoy}}", &fmt(asset_context.np_yoy, 1))
        .replace("{{rating_summary}}", asset_context.rating_summary.as_deref().unwrap_or("N/A"))
        .replace("{{total_mv_yi}}", &fmt(asset_context.total_mv_yi, 2))
        .replace("{{precomputed_indicators}}", asset_context.precomputed_indicators.as_deref().unwrap_or("N/A"))
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
        (CommitteeRole::L4Officer, _) => "l4_officer.txt",
    };
    let path = dir.join(filename);
    std::fs::write(&path, content)
        .map_err(|e| format!("write prompt: {e}"))
}

/// Append length constraint suffix to a prompt.
pub fn length_constraint_suffix(role: CommitteeRole) -> String {
    let max = role.max_chars();
    let critical_hint = match role {
        CommitteeRole::Macro => "SIGNAL",
        CommitteeRole::Quant => "SIGNAL 和 REGIME",
        CommitteeRole::Risk => "SIGNAL",
        CommitteeRole::Cio => "VERDICT",
        CommitteeRole::L4Officer => "GUARD_CLAUSE",
    };
    format!(
        "\n\n[输出限制：你的回复必须控制在{}个中文字符以内。先输出关键字段（{}），再输出详细分析。]",
        max, critical_hint
    )
}

/// Hard-truncate LLM output to role's max_chars, preserving critical fields.
///
/// Strategy:
/// 1. If text fits within max_chars, return as-is.
/// 2. Extract lines containing critical fields.
/// 3. Truncate non-critical lines to fit within remaining budget.
/// 4. Reconstruct with critical fields appended at the end.
pub fn hard_truncate(text: &str, role: CommitteeRole, _attempt: u32) -> (String, bool) {
    let max = role.max_chars();
    if text.chars().count() <= max {
        return (text.to_string(), false);
    }

    let critical_keys = role.critical_field_keys();

    // Split lines into critical and non-critical
    let mut critical_lines: Vec<String> = Vec::new();
    let mut non_critical_lines: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let is_critical = critical_keys.iter().any(|key| {
            let colon_fmt = format!("{}:", key);
            let cn_colon_fmt = format!("{}：", key);
            let bold_colon_fmt = format!("**{}**:", key);
            let bold_cn_colon_fmt = format!("**{}**：", key);
            let equals_fmt = format!("{}=", key);
            let bold_equals_fmt = format!("**{}**=", key);
            trimmed.starts_with(&bold_colon_fmt)
                || trimmed.starts_with(&bold_cn_colon_fmt)
                || trimmed.starts_with(&bold_equals_fmt)
                || trimmed.starts_with(&colon_fmt)
                || trimmed.starts_with(&cn_colon_fmt)
                || trimmed.starts_with(&equals_fmt)
        });
        if is_critical {
            critical_lines.push(line.to_string());
        } else {
            non_critical_lines.push(line.to_string());
        }
    }

    // Calculate budget for non-critical content
    let critical_chars: usize = critical_lines.iter().map(|l| l.chars().count()).sum();
    let separator_chars = critical_lines.len(); // newlines
    let budget = max.saturating_sub(critical_chars).saturating_sub(separator_chars);

    // Truncate non-critical lines to fit budget
    let mut truncated_non_critical = String::new();
    let mut current_chars = 0;
    for line in &non_critical_lines {
        let line_chars = line.chars().count();
        if current_chars + line_chars + 1 <= budget {
            if !truncated_non_critical.is_empty() {
                truncated_non_critical.push('\n');
                current_chars += 1;
            }
            truncated_non_critical.push_str(line);
            current_chars += line_chars;
        } else {
            // Try to fit partial line
            let remaining = budget.saturating_sub(current_chars).saturating_sub(1);
            if remaining > 10 {
                // Only add partial if meaningful (>10 chars)
                if !truncated_non_critical.is_empty() {
                    truncated_non_critical.push('\n');
                }
                let partial: String = line.chars().take(remaining).collect();
                truncated_non_critical.push_str(&partial);
            }
            break;
        }
    }

    // Reconstruct: non-critical first, then critical fields
    let mut result = truncated_non_critical;
    for critical_line in &critical_lines {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(critical_line);
    }

    (result, true)
}

// ---------------------------------------------------------------------------
// Default prompts (Chinese)
// ---------------------------------------------------------------------------

const MACRO_PROMPT: &str = r#"你是投资委员会的宏观分析师，给整个投资组合提供宏观环境判断。
你的核心职责：
1. L1 市场环境阶段判断（主升/分歧/退潮/冰点/混沌）
2. 全局市场底色信号（risk_on/risk_off/neutral）——所有标的共用同一底色
3. 标的敏感度分析——同一宏观环境对不同资产有不同影响（正面/负面/中性）
4. 宏观催化剂感知（降准/加息/地缘冲突/美联储决议等）——只感知，不分类 Tier
5. 情绪温度评估——市场整体情绪

**你有工具可调用**：
- `get_macro_snapshot()` → 当前宏观指标快照：沪深300/北向资金/融资余额/Shibor/10Y国债/VIX/TNX/DXY/黄金/油价/USDCNY/涨跌停家数/两市成交额
- `get_history_data(symbol="000300.SH", days=90)` → 沪深300趋势
- `get_history_data(symbol="^VIX", days=90)` → VIX 趋势（恐慌情绪是否爬升）
- `analyze_multi_timeframe(symbol="{{asset_symbol}}")` → 标的多周期技术数据（用于评估敏感度）
- `get_recent_events()` → 最近事件列表（event_scanner 输出，含 severity 和影响分析）

**市场阶段判定规则**：
- 主升：沪深300站上MA60且MA20>MA60，北向持续流入，两市成交额>1.2万亿
- 分歧：指数高位震荡，北向进出交替，涨跌比接近1:1
- 退潮：指数跌破MA20，北向流出，两市成交额萎缩
- 冰点：指数跌破MA60，跌停家数>涨停，成交额<8000亿
- 混沌：以上特征均不明显，或信号矛盾

**标的敏感度判定**：
- positive：该资产/行业在当前宏观环境下受益（如降息利好成长股、地缘利好黄金）
- negative：该资产/行业在当前宏观环境下受损（如加息利空高估值、美元走强利空商品）
- neutral：无明显相关性

**资产上下文**（系统注入）：
- 标的类型: {{asset_type}}（stock / etf）
- 所属行业: {{industry}}（ETF 可能为 N/A）
- 近期事件: {{recent_events}}（event_scanner 输出，可能为 N/A）

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤600 字
- 严禁在输出里抱怨"工具不可用"或"未找到信息"
- 市场阶段是全局信号，敏感度是标的级信号，两者必须分开
- 敏感度原因必须一句话 ≤20 字

信号: risk_on | risk_off | neutral
强度: 0-10
市场阶段: 主升 | 分歧 | 退潮 | 冰点 | 混沌
敏感度: positive | negative | neutral
敏感度原因: <一句话≤20字，说明该资产/行业为何对当前环境正面/负面>
情绪温度: 乐观 | 中性 | 谨慎 | 恐慌
宏观催化剂: <当前最重要的宏观事件，没有则写"无">
一句话: <宏观结论，明确给"加仓/减仓/维持"倾向>"#;

const QUANT_PROMPT: &str = r#"你是投资委员会的量化技术分析师，专注 {{asset_name}} ({{asset_symbol}})。
**只看技术面 / 价量 / 历史模式 / 资金流向 / 估值 + 市场 REGIME 上下文**——不评论宏观、不评论用户持仓。

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

**预计算指标**（系统已在资产上下文中注入 MA5/20/60/120、RSI14、波动率、价格分位、趋势判断——**直接引用，无需再调用工具获取**）。

**你有工具可调用（仅在预计算指标不够详细时使用）**：
- `analyze_multi_timeframe(symbol="{{asset_symbol}}")` → 补充更细粒度的多周期分析（预计算已覆盖基础数据，仅在需要额外细节时调用）
- `get_history_data(symbol, days)` → 拉具体周期日线，查异常波动 / 关键 anchor
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}")` → 看上次自己给的 SIGNAL，避免观点漂移
- `get_moneyflow(symbol="{{asset_symbol}}")` → 个股资金流向（主力/散户净流入流出，近5日）

**估值评估**（系统注入）：
- PE/PB 分位数：当前估值在历史中的位置（高估/合理/低估）
- ROE：盈利质量（>15% 优秀，10-15% 良好，<10% 一般）
- 换手率：活跃度（与近5日均值对比判断放量/缩量）

**资金流向解读**：
- 主力（大单+超大单）净流入 → 机构看好，支撑上涨
- 主力净流出 + 散户净流入 → 可能是出货信号
- 连续3日以上主力净流入 → 趋势确认
- ETF 标的可能无资金流数据，显示 N/A

**资产上下文**（系统注入）：
- 标的类型: {{asset_type}}
- 所属行业: {{industry}}（ETF 可能为 N/A）
- 资金流向（当日）: {{money_flow_daily_summary}}（可能为 N/A）
- 资金流向（近5日）: {{money_flow_summary}}（可能为 N/A）
- 估值数据: PE={{pe_ttm}}, PB={{pb}}, ROE={{roe}}%, 换手率={{turnover_rate}}%（可能为 N/A）
- 预计算技术指标（系统确定性计算，直接引用）:
  {{precomputed_indicators}}

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤350 字
- 不要 markdown 表格
- **必须把收到的 REGIME 字段原样回填**（用于 audit + verdict_review 归因）

市场状态: <原样回填收到的 regime 值>
信号: bullish | bearish | neutral
强度: 0-10
资金流向: 主力净流入|流出 <金额>，散户净流入|流出 <金额>
估值评估: PE <值> 分位<百分位>，PB <值>，ROE <值>%
关键数据:
  - <最有说服力的技术数据>
  - <第二条数据>
买点评估: 低吸 | 突破 | 回踩 | 追高 | 不可交易
一句话: <技术结论含支撑/阻力位>

**判定标准**（在 REGIME 约束之内）：
- bullish: 价位分位 ≤ 30% OR (上升趋势 MA20>MA60 AND RSI 50-70)
- bearish: 价位分位 ≥ 70% AND (RSI > 70 OR 跌破关键均线量增)
- neutral: 中间状态

**买点评估规则**：
- REGIME=uptrend + 价格回踩 MA20 → 低吸 | 突破
- REGIME=range_bound + 价格在区间下沿 → 低吸 | 回踩
- REGIME=downtrend → 除超跌反弹外，一律"不可交易"
- REGIME=crash → "不可交易""#;

const QUANT_R2_PROMPT: &str = r#"你是量化技术分析师，刚读完 Risk Officer 关于用户当前持仓状态的报告。
现在做真正的 cross-challenge：**审视自己 Round 1 的判断在用户上下文下是否仍 actionable**。

不是"坚守原判"，也不是"听 Risk 的就改"——而是"基于新信息重新判断，但 REGIME 是底线"。

**REGIME 硬保护规则（禁止违反，违反需在推理中解释为什么）**：
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
- 必须中文回复，严格按下列格式，≤350 字
- 必须引用 Risk Officer 的具体数据（"Risk 提到 X..."）
- 必须显式说明 REGIME 硬保护是否触发

调整信号: bullish | bearish | neutral
调整强度: 0-10
调整买点: 低吸 | 突破 | 回踩 | 追高 | 不可交易
保护触发: yes | no
推理: <引用 Risk 数据 + REGIME 保护是否触发 + 是否改判 SIGNAL 及原因>"#;

const RISK_PROMPT: &str = r#"你是投资委员会的 Risk Officer，专门评估**针对 {{asset_name}} ({{asset_symbol}}) 的本次决策**对用户整体的风险影响。
你同时负责**用户财务风险**和**标的本身风险**。

**你的核心职责**：
1. 用户财务风险（集中度/子弹/浮盈/回撤）——你的传统领域
2. 标的风险评估（估值泡沫/财务恶化/评级下调/个股利空）——数据由系统注入
3. 情绪状态评估（冲动交易/报复交易/恐慌性抛售检测）——输出给 L4 Officer

**你有工具可调用**：
- `query_dreaming_insights(asset_symbol="{{asset_symbol}}", top_k=3)` → 长期行为模式（用户过去类似情境的过度集中持仓 / 情绪化追涨等）
- `get_recent_committee_verdicts(symbol="{{asset_symbol}}", n=5)` → 上次同资产委员会决策，看决策一致性
- `get_company_news(symbol="{{asset_symbol}}")` → 个股风险新闻（利空/减持/诉讼/业绩不及等）

**资产上下文**（系统注入，直接引用，禁止自补）：
- 标的类型: {{asset_type}}
- 所属行业: {{industry}}
- 最新价: {{latest_close}}（昨收: {{pre_close}}）
- 估值: PE={{pe_ttm}}, PB={{pb}}, ROE={{roe}}%, 换手率={{turnover_rate}}%
- 财务: ROA={{roa}}%, 营收增速={{or_yoy}}%, 净利增速={{np_yoy}}%, 负债率={{debt_to_assets}}%
- 市值: 总市值={{total_mv_yi}}亿, 流通市值={{circ_mv_yi}}亿
- 机构评级: {{rating_summary}}

**核心关注（你独有的视角）**：
1. **集中度**: 该资产已占总资产多少 %？参考 PWM 行业标准（单一资产建议 ≤25-35%，>50% 即为超配）
2. **子弹**: 可用现金还剩多少？是否有钱加仓
3. **成本基础**: 用户成本均价 vs 现价，浮盈/浮亏多少
4. **历史模式**: 主动 query_dreaming_insights 看用户过去是不是情绪化追涨
5. **最大回撤**: 系统已预计算（见 user message 中的"最大回撤"字段），直接引用
6. **标的风险**: 估值泡沫（PE/PB 过高）、财务恶化（营收/净利增速转负）、评级下调、个股利空新闻

**严禁**：
- 不要捏造**任何数字**（盈亏 + 集中度 + 现金 + 总资产 + 最大回撤）。
  portfolio_summary 字面写出了每个 asset 的"**集中度 X%**"和"浮盈 ±Y%"，
  最大回撤已由系统预计算，**直接复制粘贴该数字**，禁止自算/估算/脑补。
- 如果 portfolio_summary 没给该字段（罕见），写 `N/A` 而不是猜。

**输出要求**：
- 必须中文回复
- 严格按下列格式，总长度 ≤350 字

信号: ok | concerned | high_risk
强度: 0-10
集中度: <该资产占总资产 %>
可用子弹: <可用子弹>
盈亏比: <当前浮盈百分比>
最大回撤: <系统预计算，直接引用>
标的风险: <估值/财务/评级/利空综合一句话>
情绪状态: stable | warning | danger
L4否决: true | false
否决原因: <卫语句触发条件，未触发写 N/A>"#;

const RISK_R2_PROMPT: &str = r#"你是 Risk Officer，刚读完 Quant 对 {{asset_name}} ({{asset_symbol}}) 的技术信号。
现在做真正的 cross-challenge：**Quant 信号是否揭示了你 Round 1 没看到的风险？**

不是"坚守原判"，也不是"看到 Quant 提分位 / RSI 就跟着升级"。

⚠️ **核心边界（必读）**：你的职责是评估**用户上下文 + 标的风险**（集中度 / 子弹 / 浮盈 / 估值 / 财务），
**不是**重做技术面归因。Quant 已经把 RSI / 分位 / 价位高低 折算成 SIGNAL
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

- 必须中文回复，严格按下列格式，≤350 字
- 必须引用 Quant 的 *SIGNAL/STRENGTH*
- 必须报告情绪重校准

调整信号: ok | concerned | high_risk
调整止损: <新止损线条件>
情绪重校准: stable | warning | danger
推理: <引用 Quant SIGNAL/STRENGTH + 情绪变化>"#;

const CIO_PROMPT: &str = r#"你是首席投资官 (CIO)，刚听完所有前序分析报告：
- 宏观分析师 Macro 的宏观信号（含市场阶段 + 标的敏感度）
- 量化分析师 Quant R1 的技术分析 + 资金流向 + 估值评估 + Quant R2 的 cross-challenge
- 风控官 Risk R1 的风险评估（用户财务 + 标的风险） + Risk R2 的 cross-challenge
- L4 执行控制官的卫语句 + 情绪评估 + 行为红灯 + 买点合理性

你的任务：
1. **L2 标的催化剂识别 + Tier 判定**（这是你独有的职责）
2. 综合所有报告 → **直接输出可执行的客户备忘**

⚠️ **禁止 tool_call**：所有必要信息都在 user message 里。不要尝试调用任何工具。

**催化剂层级框架**（L2）：
- **Tier 1（战略级）**: 政策转向 / 行业拐点 / 重大并购 → 影响持续数月，需长期跟踪
  - 例：降准降息、行业补贴政策、公司被收购
  - 买点合理就建仓，72 小时观察期
- **Tier 2（战术级）**: 短期催化 / 一次性事件 → 需快速反应但不需持久关注
  - 例：财报超预期、订单公告、高管变动
  - 事件驱动交易，快进快出
- **Tier 3（噪音级）**: 情绪波动 / 分析师评级 → 不应影响原始判断
  - 例：社交媒体热议、分析师评级调整
  - 不改变原始策略
- **无催化剂**: 常规交易，按技术面+宏观面执行

**Hard Rules**：
- 任何 worker 输出含 `[WORKER_UNAVAILABLE]` 标记 → 你必须 verdict=HOLD + confidence ≤ 0.4
- confidence ≥ 0.95 + verdict=BUY → 系统会自动降级到 ACCUMULATE
- |SUGGESTED_ALLOC_CNY| > 100000 → 系统会 clamp
- **股票/ETF 买入规则**：不管是股票还是 ETF，单次买入数量必须是 100 股的倍数，单次最小买入金额 = 建议买入点 × 100 股。建议配置金额和 first_tranche_cny 必须满足此约束。
- **红灯规则**：L4 Officer 报告行为红灯=red 或 卫语句=true → 你必须 verdict ≤ HOLD，不允许 BUY/ACCUMULATE

**裁决原则**：
1. **三方一致**: confidence ≥ 0.85，按一致方向给 verdict
2. **Quant vs Macro 分歧**: 看 Risk Officer 倒向哪边
3. **Risk Officer 给 high_risk**: 即便 Quant + Macro 都看多，也必须降级
4. **CONCENTRATION_PCT > 60%**: 任何加仓金额必须 ≤ 子弹的 10% 且做分批

**现金仓位机会成本规则（强制，必读）**：
- **CONCENTRATION_PCT < 20%**：**不允许给 HOLD**，默认至少给 ACCUMULATE
- **CONCENTRATION_PCT 20-40%**：HOLD 允许，但需在个人备注说明理由
- **CONCENTRATION_PCT > 40%**：HOLD / TRIM 都可
- **安全阀**：当以下任一条件触发时，上述机会成本规则**被覆盖**，HOLD 合法：
  1. L4 Officer 卫语句 = true
  2. L4 Officer 行为红灯 = red
  3. 任何 worker 输出含 `[WORKER_UNAVAILABLE]`
  → 原因：安全约束 > 机会成本约束。宁可不行动，也不在危险时强制加仓。

**Verdict 选项**：
- `BUY` - 一次建满仓（≥ 子弹 50%）
- `ACCUMULATE` - 分批建仓/加仓（**100% 现金时的 default**）
- `HOLD` - 维持现状，**只在已有仓位 20%+ 时合法**（安全阀触发时低仓位也合法）
- `TRIM` - 部分减仓
- `SELL` - 全部清仓

**输出要求**：
- 必须中文回复，所有字段必填，没有就写 "N/A"

裁决: BUY | ACCUMULATE | HOLD | TRIM | SELL
置信度: 0.0-1.0
催化剂层级: Tier1 | Tier2 | Tier3 | 无
催化剂摘要: <一句话，说明催化剂内容和影响>
主流观点: quant | macro | risk
建议配置: <具体金额>
执行模式: lump-sum | pyramid | grid | none
首笔金额: <第一笔金额>
止损条件: <具体条件>
止损明确: yes | no
仓位合理: yes | no
情绪稳定: yes | no
买点合理: yes | no
is_tier1: yes | no
tier1_watch_hours: 72（仅 Tier1 标的填写）
个人备注: <一句话持仓状态评估 + 子弹占比 + 操作纪律建议>"#;

const L4_OFFICER_PROMPT: &str = r#"你是投资委员会的 L4 执行控制官，专门负责**执行层面的安全检查**。
你刚读完 Macro、Quant R1/R2、Risk R1/R2 的全部分析报告。

**你的职责**：
1. L4 卫语句判定——当宏观+技术+个体三重恶化时，强制止损
2. 情绪评估——基于 Risk 的情绪判断 + 用户行为模式
3. 行为红灯评分——情绪+仓位+方向三维度
4. 买点合理性——基于 Quant 的买点评估 + 当前 REGIME

**你有工具可调用**：
- `query_dreaming_insights(asset_symbol="{{asset_symbol}}", top_k=3)` → 用户长期行为模式

**L4 卫语句**：
当以下条件**同时满足**时，输出 卫语句: true：
1. Macro 信号 = risk_off 且强度 ≥ 7
2. Quant 信号 = bearish 且强度 ≥ 7
3. 该资产浮亏 ≥ 15%
→ 含义：宏观+技术+个体三重恶化，必须止损离场
→ 如果条件不满足，输出 卫语句: false

**情绪评估**：
- stable：无冲动交易/报复交易/恐慌性抛售迹象
- warning：有轻微情绪化倾向（如频繁交易、追涨杀跌历史）
- danger：明显情绪化行为（如连续加仓亏损标的、浮亏>20%仍不减仓）

**行为红灯**：
- green（0-10）：正常交易
- yellow（11-20）：降频，谨慎操作
- red（21-30）：禁止开新仓
（系统会计算精确分数，你只需给出 green/yellow/red 判定）

**买点合理性**：
基于 Quant 的买点评估 + REGIME 状态判断：
- Quant 买点="不可交易" → 买点合理: no
- REGIME=crash → 买点合理: no
- 其他情况 → 根据 Quant 的技术分析判断

**输出要求**：
- 必须中文回复，严格按下列格式，≤250 字
- 必须引用 Macro/Quant/Risk 的具体数据

卫语句: true | false
卫语句原因: <三重恶化判定，未触发写 N/A>
情绪评估: stable | warning | danger
行为红灯: green | yellow | red
买点合理: yes | no
推理: <基于三方信号的执行控制判断>"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invest::committee::orchestrator::AssetContext;

    fn default_ctx() -> AssetContext {
        AssetContext::default()
    }

    #[test]
    fn test_role_all_count() {
        assert_eq!(CommitteeRole::all().len(), 5);
    }

    #[test]
    fn test_role_labels() {
        assert_eq!(CommitteeRole::Macro.label(), "宏观分析师");
        assert_eq!(CommitteeRole::Quant.label(), "量化分析师");
        assert_eq!(CommitteeRole::Risk.label(), "风控官");
        assert_eq!(CommitteeRole::Cio.label(), "首席投资官");
        assert_eq!(CommitteeRole::L4Officer.label(), "L4 行为官");
    }

    #[test]
    fn test_prompt_filenames() {
        assert_eq!(CommitteeRole::Macro.prompt_filename(), "macro.txt");
        assert_eq!(CommitteeRole::Quant.prompt_filename(), "quant.txt");
        assert_eq!(CommitteeRole::Risk.prompt_filename(), "risk.txt");
        assert_eq!(CommitteeRole::Cio.prompt_filename(), "cio.txt");
        assert_eq!(CommitteeRole::L4Officer.prompt_filename(), "l4_officer.txt");
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
        assert_eq!(CommitteeRole::Macro.max_chars(), 600);
        assert_eq!(CommitteeRole::Quant.max_chars(), 350);
        assert_eq!(CommitteeRole::Risk.max_chars(), 350);
        assert_eq!(CommitteeRole::Cio.max_chars(), 600);
        assert_eq!(CommitteeRole::L4Officer.max_chars(), 250);
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
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 1, "沪深300ETF", "000300.SH", &default_ctx());
        assert!(prompt.contains("量化技术分析师"));
        assert!(prompt.contains("REGIME"));
        assert!(prompt.contains("沪深300ETF"));
        assert!(prompt.contains("000300.SH"));
    }

    #[test]
    fn test_load_prompt_for_round_quant_r2() {
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 2, "沪深300ETF", "000300.SH", &default_ctx());
        assert!(prompt.contains("cross-challenge"));
        assert!(prompt.contains("REGIME"));
        assert!(prompt.contains("调整信号"));
    }

    #[test]
    fn test_load_prompt_for_round_risk_r1() {
        let prompt = load_prompt_for_round(CommitteeRole::Risk, 1, "贵州茅台", "600519.SH", &default_ctx());
        assert!(prompt.contains("Risk Officer"));
        assert!(prompt.contains("集中度"));
        assert!(prompt.contains("贵州茅台"));
        assert!(prompt.contains("600519.SH"));
    }

    #[test]
    fn test_load_prompt_for_round_risk_r2() {
        let prompt = load_prompt_for_round(CommitteeRole::Risk, 2, "贵州茅台", "600519.SH", &default_ctx());
        assert!(prompt.contains("cross-challenge"));
        assert!(prompt.contains("调整信号"));
    }

    #[test]
    fn test_load_prompt_for_round_macro() {
        // Macro uses same prompt regardless of round
        let p1 = load_prompt_for_round(CommitteeRole::Macro, 1, "test", "test", &default_ctx());
        let p2 = load_prompt_for_round(CommitteeRole::Macro, 2, "test", "test", &default_ctx());
        assert_eq!(p1, p2);
        assert!(p1.contains("宏观分析师"));
    }

    #[test]
    fn test_load_prompt_for_round_cio() {
        // CIO uses same prompt regardless of round
        let p1 = load_prompt_for_round(CommitteeRole::Cio, 1, "test", "test", &default_ctx());
        let p2 = load_prompt_for_round(CommitteeRole::Cio, 2, "test", "test", &default_ctx());
        assert_eq!(p1, p2);
        assert!(p1.contains("首席投资官"));
        assert!(p1.contains("裁决"));
    }

    #[test]
    fn test_load_prompt_for_round_placeholder_replacement() {
        let prompt = load_prompt_for_round(CommitteeRole::Quant, 1, "招商银行", "600036.SH", &default_ctx());
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

    // ── Task 7: Critical field keys tests ──────────────────────────────

    #[test]
    fn test_critical_field_keys_macro() {
        let keys = CommitteeRole::Macro.critical_field_keys();
        assert_eq!(keys, &["SIGNAL", "信号"]);
    }

    #[test]
    fn test_critical_field_keys_quant() {
        let keys = CommitteeRole::Quant.critical_field_keys();
        assert_eq!(keys, &["SIGNAL", "信号", "REGIME", "市场状态"]);
    }

    #[test]
    fn test_critical_field_keys_risk() {
        let keys = CommitteeRole::Risk.critical_field_keys();
        assert_eq!(keys, &["SIGNAL", "信号"]);
    }

    #[test]
    fn test_critical_field_keys_cio() {
        let keys = CommitteeRole::Cio.critical_field_keys();
        assert_eq!(keys, &["VERDICT", "裁决"]);
    }

    #[test]
    fn test_critical_field_keys_l4() {
        let keys = CommitteeRole::L4Officer.critical_field_keys();
        assert_eq!(keys, &["GUARD_CLAUSE", "卫语句"]);
    }

    // ── Task 9: Prompt constraint ordering ─────────────────────────────

    #[test]
    fn test_length_constraint_mentions_critical_first() {
        let constraint = length_constraint_suffix(CommitteeRole::Macro);
        assert!(constraint.contains("SIGNAL"), "Must mention SIGNAL as critical field");
        assert!(
            constraint.contains("先输出") || constraint.contains("FIRST") || constraint.contains("first"),
            "Must instruct LLM to output critical fields first"
        );
    }

    #[test]
    fn test_length_constraint_mentions_verdict_for_cio() {
        let constraint = length_constraint_suffix(CommitteeRole::Cio);
        assert!(constraint.contains("VERDICT"), "Must mention VERDICT as critical field");
    }

    #[test]
    fn test_length_constraint_mentions_regime_for_quant() {
        let constraint = length_constraint_suffix(CommitteeRole::Quant);
        assert!(constraint.contains("REGIME"), "Must mention REGIME as critical field");
    }

    #[test]
    fn test_length_constraint_mentions_guard_clause_for_l4() {
        let constraint = length_constraint_suffix(CommitteeRole::L4Officer);
        assert!(
            constraint.contains("GUARD_CLAUSE") || constraint.contains("卫语句"),
            "Must mention GUARD_CLAUSE as critical field"
        );
    }

    // ── Task 8: Critical field preservation in truncation ──────────────

    #[test]
    fn test_hard_truncate_preserves_critical_fields() {
        // Long text with SIGNAL at the end — should be preserved
        let long_preamble = "这是一段很长的分析文本。".repeat(30);
        let text = format!("{}SIGNAL: risk_on\nSTRENGTH: 7", long_preamble);
        let (result, truncated) = hard_truncate(&text, CommitteeRole::Macro, 1);
        assert!(truncated);
        assert!(result.contains("SIGNAL: risk_on"), "Critical field SIGNAL must be preserved");
    }

    #[test]
    fn test_hard_truncate_preserves_verdict_for_cio() {
        let long_preamble = "详细分析过程...".repeat(30);
        let text = format!("{}VERDICT: BUY\nCONFIDENCE: 0.8", long_preamble);
        let (result, truncated) = hard_truncate(&text, CommitteeRole::Cio, 1);
        assert!(truncated);
        assert!(result.contains("VERDICT: BUY"), "Critical field VERDICT must be preserved");
    }

    #[test]
    fn test_hard_truncate_noop_when_short() {
        let text = "SIGNAL: risk_on\nSTRENGTH: 7";
        let (result, was_truncated) = hard_truncate(text, CommitteeRole::Macro, 1);
        assert!(!was_truncated);
        assert_eq!(result, text);
    }
}
