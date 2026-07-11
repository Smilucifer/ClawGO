//! 每日盈记评分算法：干支收益统计 → 层分 → 综合分。
//!
//! 常数集中在本文件顶部，便于用真实数据回调。两条曲线经三个数据窗口标定/验证：
//!   - 层分：每个天干/地支单独多好，0..100。统一饱和曲线（近 0 处陡、两端压向 0/100）。
//!     24 个排行点拟合 RMSE 2.16；5+6+7 月留出验证 RMSE 1.88（全 ±3 内）。
//!   - 综合分 = G(天干层分, 地支层分)：线性放大 + 一致度回拉。成熟 19 点拟合 RMSE 5.07。
//!
//! 层分不做小样本收缩：饱和曲线本身把极端值收在 0/100 间，冷启动锐度靠展示门控（层外）处理。
//! 综合分核心是「一致度→放大」：天干地支都好才拉高，一低一高相互抵消（见基准点 丁亥 层分
//! 24.8/97.3 却只 53）。中性锚 ≈ 57（demo 略偏乐观，中性日落"平"偏上）。
//! 组合（干支对）仅统计展示，不进综合分——消融实验（新旧层分值两轮）均证明其对拟合零贡献。

// ── 可调常数 ───────────────────────────────────────────────
/// 层分：平均收益率（百分数）对 raw 的斜率。
const A_RETURN: f64 = 10.6;
/// 层分：胜率偏离 50% 每个百分点对 raw 的斜率。
const B_WINRATE: f64 = 1.08;
/// 层分饱和曲线幅度与陡度：层分 = 50 ± LAYER_AMP·(1 − e^(−LAYER_K·|raw|))。
const LAYER_AMP: f64 = 61.0;
const LAYER_K: f64 = 0.020;
/// 综合分：中性锚（天干地支层分均 50 时的综合分）。
const COMP_BASE: f64 = 56.9;
/// 综合分：天干层分偏离、地支层分偏离的权重（>1 → 放大）。
const COMP_W_STEM: f64 = 1.191;
const COMP_W_BRANCH: f64 = 0.887;
/// 综合分：一致度回拉系数，作用于 sign(s+b)·|s·b|/10（极端一致时轻微收敛，防冲爆）。
const COMP_INTERACT: f64 = 0.137;
// ───────────────────────────────────────────────────────────

/// 单个层级（某天干或某地支）的历史统计。
#[derive(Debug, Clone, Copy)]
pub struct LayerStat {
    /// 平均收益率，单位百分数（如 0.41 表示 +0.41%）。
    pub avg_return_pct: f64,
    /// 胜率，0.0..=1.0。
    pub win_rate: f64,
    /// 样本数（交易日数）。
    pub sample: u32,
}

fn clamp01_100(v: f64) -> f64 {
    v.clamp(0.0, 100.0)
}

/// 层分：把某天干/地支的平均收益与胜率映射到 0..100（统一饱和曲线，无收缩）。
/// raw≥0 向上饱和到 100，raw<0 向下饱和到 0；`sample` 不参与（冷启动锐度由展示门控处理）。
pub fn layer_score(stat: &LayerStat) -> f64 {
    let raw = A_RETURN * stat.avg_return_pct + B_WINRATE * (stat.win_rate * 100.0 - 50.0);
    let mag = LAYER_AMP * (1.0 - (-LAYER_K * raw.abs()).exp());
    clamp01_100(50.0 + mag * raw.signum())
}

/// 综合分：由天干、地支层分放大合成。核心是「一致度→放大」（两层都好才拉高）。
/// = COMP_BASE + w_s·s + w_b·b − k·sign(s+b)·|s·b|/10，其中 s/b 为层分偏离 50 的量。
pub fn composite_from_layers(stem: f64, branch: f64) -> f64 {
    let s = stem - 50.0;
    let b = branch - 50.0;
    let interact = (s + b).signum() * (s * b).abs() / 10.0;
    let v = COMP_BASE + COMP_W_STEM * s + COMP_W_BRANCH * b - COMP_INTERACT * interact;
    clamp01_100(v)
}

/// 端到端：天干统计 + 地支统计 → 综合分。
pub fn composite_score(stem: &LayerStat, branch: &LayerStat) -> f64 {
    composite_from_layers(layer_score(stem), layer_score(branch))
}

/// 吉凶等级。
#[derive(Debug, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FortuneLevel {
    GreatFortune, // 大吉 ≥75
    Fortune,      // 吉 60..75
    Neutral,      // 平 45..60
    Misfortune,   // 凶 30..45
    GreatMisfortune, // 大凶 <30
}

/// 综合分 → 吉凶等级。
pub fn fortune_level(score: f64) -> FortuneLevel {
    match score {
        s if s >= 75.0 => FortuneLevel::GreatFortune,
        s if s >= 60.0 => FortuneLevel::Fortune,
        s if s >= 45.0 => FortuneLevel::Neutral,
        s if s >= 30.0 => FortuneLevel::Misfortune,
        _ => FortuneLevel::GreatMisfortune,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ls(avg: f64, win: f64, sample: u32) -> LayerStat {
        LayerStat { avg_return_pct: avg, win_rate: win, sample }
    }

    /// 19 个真实成熟基准点：综合分应落在目标 ±12 内（拟合 RMSE 5.07，单点最大偏差 9.9）。
    /// 样本数不参与层分，此处仅占位。纯冷启动点（丙戌6月/丁亥6月）不服成熟规律，靠展示门控处理，不入此测。
    #[test]
    fn composite_matches_real_benchmarks() {
        // (名称, 天干(avg,win), 地支(avg,win), 目标综合分)
        let cases: &[(&str, (f64, f64), (f64, f64), f64)] = &[
            ("乙酉预测", (0.19, 0.56), (0.65, 0.54), 78.0),
            ("甲申盘后", (0.58, 0.44), (0.82, 0.75), 85.0),
            ("乙酉盘后", (0.02, 0.53), (0.41, 0.50), 68.0),
            ("癸未预测", (0.06, 0.50), (-0.58, 0.38), 43.0),
            ("甲申预测", (0.36, 0.41), (0.60, 0.73), 80.0),
            ("壬午预测", (-0.04, 0.44), (-0.05, 0.35), 41.0),
            ("丁丑盘后", (-0.97, 0.35), (-1.02, 0.40), 9.0),
            ("戊寅预测", (-0.37, 0.56), (0.33, 0.50), 59.0),
            ("甲戌预测", (0.20, 0.38), (0.00, 0.62), 43.0),
            ("乙亥预测", (0.12, 0.53), (0.38, 0.54), 78.0),
            ("丙戌全量", (-0.24, 0.47), (0.21, 0.64), 61.0),
            ("丁亥全量", (-0.97, 0.35), (0.43, 0.57), 47.0),
            ("庚辰", (0.29, 0.61), (-0.77, 0.43), 62.0),
            ("辛巳", (0.0, 0.44), (0.44, 0.63), 61.0),
            ("己卯", (0.52, 0.67), (-0.21, 0.50), 75.0),
            ("乙亥备", (0.19, 0.56), (0.44, 0.57), 78.0),
            ("丙子", (-0.02, 0.50), (-0.03, 0.38), 51.0),
            ("丙戌567", (-0.23, 0.40), (0.19, 0.75), 54.0),
            ("丁亥567", (-1.49, 0.40), (1.94, 1.00), 53.0),
        ];
        for (name, (sa, sw), (ba, bw), target) in cases {
            let got = composite_score(&ls(*sa, *sw, 20), &ls(*ba, *bw, 20));
            let err = (got - target).abs();
            assert!(
                err <= 12.0,
                "{name}: 综合分 {got:.1} 偏离目标 {target} 达 {err:.1}（容差 12）"
            );
        }
    }

    /// 单层排行/风险基准：层分应贴合 demo 排序值（±6）。跨全量 + 6月冷启动两窗口，
    /// 覆盖极端（甲6=100/辰6=0）与中段（己/申/丁），验证同一曲线通吃。
    #[test]
    fn layer_score_matches_ranking_benchmarks() {
        // (avg, win, 目标层分, 名称)
        let cases: &[(f64, f64, f64, &str)] = &[
            (0.52, 0.67, 72.59, "己全"),
            (0.29, 0.61, 64.37, "庚全"),
            (0.58, 0.44, 54.63, "甲全"),
            (-0.97, 0.35, 21.84, "丁全"),
            (0.82, 0.75, 81.47, "申全"),
            (-1.02, 0.40, 26.34, "丑全"),
            (4.00, 1.00, 100.0, "甲6"),
            (2.59, 1.00, 98.36, "壬6"),
            (2.02, 1.00, 96.08, "巳6"),
            (-0.49, 0.00, 6.59, "丙6"),
            (-3.69, 0.00, 0.0, "丑6"),
            (0.04, 0.33, 34.36, "丁6"),
        ];
        for (avg, win, target, name) in cases {
            let got = layer_score(&ls(*avg, *win, 20));
            assert!((got - target).abs() <= 6.0, "{name}: 层分 {got:.1} vs 目标 {target}");
        }
    }

    #[test]
    fn neutral_layer_is_fifty_composite_is_base() {
        // 零收益、50% 胜率 → 层分正好 50；综合分 = 中性锚 COMP_BASE（≈57，demo 略偏乐观）。
        let neutral = ls(0.0, 0.50, 10);
        assert!((layer_score(&neutral) - 50.0).abs() < 1e-9);
        assert!((composite_score(&neutral, &neutral) - COMP_BASE).abs() < 1e-9);
    }

    #[test]
    fn layer_saturates_not_shrinks() {
        // 无收缩：样本数不影响层分（同信号 n=1 与 n=20 相等）。
        let strong = (0.82, 0.75);
        assert!((layer_score(&ls(strong.0, strong.1, 1)) - layer_score(&ls(strong.0, strong.1, 20))).abs() < 1e-9);
        // 饱和：收益递增，层分增量递减（凹性），且始终 < 100。
        let s1 = layer_score(&ls(0.5, 0.60, 5));
        let s2 = layer_score(&ls(1.5, 0.60, 5));
        assert!(s2 > s1 && s2 < 100.0 && (s2 - s1) < (s1 - 50.0), "应饱和递减: {s1:.1} → {s2:.1}");
    }

    #[test]
    fn composite_amplifies_agreement_cancels_disagreement() {
        // 两层都好（一致）→ 放大到高于均值；一低一高（背离）→ 抵消，靠近中性。
        let agree = composite_from_layers(60.0, 60.0);
        assert!(agree > 65.0, "一致向上应放大: {agree:.1}");
        let disagree = composite_from_layers(30.0, 90.0); // 均值 60 但背离
        assert!(disagree < agree, "背离应弱于一致: 背离{disagree:.1} < 一致{agree:.1}");
    }

    #[test]
    fn level_bands_are_correct() {
        assert_eq!(fortune_level(85.0), FortuneLevel::GreatFortune);
        assert_eq!(fortune_level(68.0), FortuneLevel::Fortune);
        assert_eq!(fortune_level(50.0), FortuneLevel::Neutral);
        assert_eq!(fortune_level(41.0), FortuneLevel::Misfortune);
        assert_eq!(fortune_level(9.0), FortuneLevel::GreatMisfortune);
    }

    #[test]
    fn scores_stay_in_bounds() {
        // 极端输入不应越界。
        let hi = composite_score(&ls(50.0, 1.0, 5), &ls(50.0, 1.0, 5));
        let lo = composite_score(&ls(-50.0, 0.0, 5), &ls(-50.0, 0.0, 5));
        assert!((0.0..=100.0).contains(&hi));
        assert!((0.0..=100.0).contains(&lo));
    }
}
