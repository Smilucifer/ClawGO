//! SABC 五因子打分器。纯函数、可单测、独立于委员会 verdict。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremarketConfig {
    pub weight_sentiment: f64,
    pub weight_capital: f64,
    pub weight_technical: f64,
    pub weight_catalyst: f64,
    pub weight_sector: f64,
    /// "auto" — 固定权重由配置决定；"manual" — 用户逐项调节。
    pub weight_source: String,
    pub threshold_s: f64,
    pub threshold_a: f64,
    pub threshold_b: f64,
    /// Enable AI final review step (B3). Display/persistence only, never affects total/grade.
    #[serde(default = "default_true")]
    pub enable_ai_review: bool,
}

fn default_true() -> bool {
    true
}

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
            enable_ai_review: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Grade {
    S,
    A,
    B,
    C,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactorBreakdown {
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
    pub sector_strength: f64,
}

/// AI final review result (B3). Attached to SymbolScore.ai_review, display/persistence only.
/// Never participates in total/grade calculation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AiReview {
    pub action: String,
    pub reason: String,
    pub risk_flag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolScore {
    pub symbol: String,
    pub name: String,
    pub total: f64,
    pub grade: Grade,
    pub factors: FactorBreakdown,
    pub missing_factors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_review: Option<AiReview>,
}

/// 按 SABC 阈值把合成分映射到档位。`score()` 与板块聚合（report::build_themes）共用。
pub fn grade_of(total: f64, cfg: &PremarketConfig) -> Grade {
    if total >= cfg.threshold_s {
        Grade::S
    } else if total >= cfg.threshold_a {
        Grade::A
    } else if total >= cfg.threshold_b {
        Grade::B
    } else {
        Grade::C
    }
}

/// Rank-cut grading: input sorted by total DESC. Top 20, each bucket 5 stocks.
/// < 20 stocks: fill S→A→B→C in order, last bucket doesn't pad with worse stocks.
/// Modifies grades in-place, returns top-20 (or all if < 20).
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
    let grade = grade_of(total, cfg);
    SymbolScore {
        symbol: symbol.to_string(),
        name: name.to_string(),
        total: (total * 100.0).round() / 100.0,
        grade,
        factors,
        missing_factors: missing,
        ai_review: None,
    }
}

fn config_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    home.join(".claw-go")
        .join("invest")
        .join("premarket_config.json")
}

pub fn get_premarket_config() -> PremarketConfig {
    let path = config_path();
    if !path.exists() {
        return PremarketConfig::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save_premarket_config(cfg: PremarketConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write premarket_config: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5-factor config matching default weights: 0.25+0.25+0.20+0.15+0.15=1.0
    fn cfg() -> PremarketConfig {
        PremarketConfig::default()
    }

    #[test]
    fn default_config_sums_to_one() {
        let c = PremarketConfig::default();
        let sum = c.weight_sentiment + c.weight_capital + c.weight_technical
            + c.weight_catalyst + c.weight_sector;
        assert!((sum - 1.0).abs() < 1e-9, "weights sum to {sum}, expected 1.0");
    }

    #[test]
    fn grade_of_thresholds() {
        let c = cfg();
        assert!(matches!(grade_of(80.0, &c), Grade::S));
        assert!(matches!(grade_of(62.0, &c), Grade::A));
        assert!(matches!(grade_of(45.0, &c), Grade::B));
        assert!(matches!(grade_of(30.0, &c), Grade::C));
    }

    #[test]
    fn score_weights_five_factors() {
        // All factors 100 → total should be 100.0 (weights sum to 1.0)
        let f = FactorBreakdown {
            sentiment: 100.0,
            capital: 100.0,
            technical: 100.0,
            catalyst: 100.0,
            sector_strength: 100.0,
        };
        let s = score("600519", "茅台", f, vec![], &cfg());
        assert!((s.total - 100.0).abs() < 0.01, "total={}", s.total);
        assert!(matches!(s.grade, Grade::S));
    }

    #[test]
    fn score_sector_contributes_to_total() {
        // Only sector_strength=100, others=0 → total = 100 * 0.15 = 15
        let f = FactorBreakdown {
            sentiment: 0.0,
            capital: 0.0,
            technical: 0.0,
            catalyst: 0.0,
            sector_strength: 100.0,
        };
        let s = score("x", "x", f, vec![], &cfg());
        assert!((s.total - 15.0).abs() < 0.01, "total={}", s.total);
        assert!(matches!(s.grade, Grade::C));
    }

    #[test]
    fn test_grade_c_low() {
        let f = FactorBreakdown {
            sentiment: 10.0,
            capital: 10.0,
            technical: 10.0,
            catalyst: 10.0,
            sector_strength: 10.0,
        };
        let s = score("x", "x", f, vec![], &cfg());
        assert!(matches!(s.grade, Grade::C));
    }

    #[test]
    fn test_weighted_sum() {
        // sentiment=80(w.25) capital=60(w.25) technical=40(w.20) catalyst=20(w.15) sector=30(w.15)
        // = 20 + 15 + 8 + 3 + 4.5 = 50.5 → B (45<=50.5<62)
        let f = FactorBreakdown {
            sentiment: 80.0,
            capital: 60.0,
            technical: 40.0,
            catalyst: 20.0,
            sector_strength: 30.0,
        };
        let s = score("x", "x", f, vec![], &cfg());
        assert!((s.total - 50.5).abs() < 0.01, "total={}", s.total);
        assert!(matches!(s.grade, Grade::B));
    }

    #[test]
    fn test_missing_factor_recorded() {
        let f = FactorBreakdown {
            sentiment: 50.0,
            capital: 50.0,
            technical: 50.0,
            catalyst: 50.0,
            sector_strength: 50.0,
        };
        let s = score("x", "x", f, vec!["capital".to_string()], &cfg());
        assert_eq!(s.missing_factors, vec!["capital".to_string()]);
    }

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
                sector_strength: 0.0,
            },
            missing_factors: vec![],
            ai_review: None,
        }
    }

    #[test]
    fn test_assign_grades_by_rank_25_stocks_takes_top20_and_cuts_5_per_bucket() {
        let scores: Vec<SymbolScore> = (0..25).map(|i| mk(&format!("s{i}"), 100.0 - i as f64)).collect();
        let result = assign_grades_by_rank(scores);
        assert_eq!(result.len(), 20);
        for (i, s) in result.iter().enumerate() {
            match i {
                0..=4 => assert!(matches!(s.grade, Grade::S), "rank {i} should be S"),
                5..=9 => assert!(matches!(s.grade, Grade::A), "rank {i} should be A"),
                10..=14 => assert!(matches!(s.grade, Grade::B), "rank {i} should be B"),
                15..=19 => assert!(matches!(s.grade, Grade::C), "rank {i} should be C"),
                _ => unreachable!(),
            }
        }
        for w in result.windows(2) {
            assert!(w[0].total >= w[1].total, "expected monotone DESC");
        }
    }

    #[test]
    fn test_assign_grades_by_rank_12_stocks_last_bucket_underfilled() {
        let scores: Vec<SymbolScore> = (0..12).map(|i| mk(&format!("s{i}"), 90.0 - i as f64)).collect();
        let result = assign_grades_by_rank(scores);
        assert_eq!(result.len(), 12);
        let s_count = result.iter().filter(|s| matches!(s.grade, Grade::S)).count();
        let a_count = result.iter().filter(|s| matches!(s.grade, Grade::A)).count();
        let b_count = result.iter().filter(|s| matches!(s.grade, Grade::B)).count();
        let c_count = result.iter().filter(|s| matches!(s.grade, Grade::C)).count();
        assert_eq!(s_count, 5, "S should be 5");
        assert_eq!(a_count, 5, "A should be 5");
        assert_eq!(b_count, 2, "B should be 2 (underfilled)");
        assert_eq!(c_count, 0, "C should be 0");
    }

    #[test]
    fn test_assign_grades_by_rank_unsorted_input_gets_sorted_desc() {
        let mut scores: Vec<SymbolScore> = (0..10).map(|i| mk(&format!("s{i}"), (i * 11) as f64)).collect();
        scores.reverse();
        let result = assign_grades_by_rank(scores);
        assert_eq!(result.len(), 10);
        assert_eq!(result[0].symbol, "s9");
        assert!(matches!(result[0].grade, Grade::S));
        assert_eq!(result[9].symbol, "s0");
        assert!(matches!(result[9].grade, Grade::A));
    }
}
