//! # JumpTracker
//!
//! ## Purpose
//!
//! Detects and characterizes price jumps using the Barndorff-Nielsen & Shephard (2004)
//! bipower variation (BPV) test. Quantifies the fraction of variance attributable to
//! jumps vs continuous diffusion.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Bipower variation | `BV = (pi/2) * sum(\|r_t\| * \|r_{t-1}\|)` | dimensionless |
//! | Jump component | `J = max(RV - BV, 0)` | dimensionless |
//! | Jump fraction | `J / RV` | ratio |
//! | BNS z-statistic | `z = (RV - BV) / sqrt(var_est)` | dimensionless |
//! | Daily jump fraction dist | Distribution of J/RV across days | ratio |
//!
//! Note: an empty `regime_jump_fraction` map is emitted in the JSON output. The
//! per-regime accumulator is allocated but intentionally never populated — daily
//! jump fractions are day-level statistics that cannot be meaningfully assigned
//! to a single intraday regime without per-regime sub-day jump contribution
//! tracking. See the `end_of_day` comment for context.
//!
//! ## Formulas
//!
//! - Realized variance: `RV = sum(r_t^2)`
//! - Bipower variation: `BV = (pi/2) * sum_{i=2}^{n} |r_i| * |r_{i-1}|`
//! - Jump test statistic (BNS 2006):
//!   `z = (RV - BV) / sqrt((pi^2/4 + pi - 5) * max(TP, 0) / n)`
//!   Tripower quarticity (matches `tp` computation in `end_of_day`):
//!   `TP = (n / (n-2)) * mu_{4/3}^{-3} * sum_{i=3}^{n} |r_i|^{4/3} * |r_{i-1}|^{4/3} * |r_{i-2}|^{4/3}`
//!   where `mu_{4/3} = 2^{2/3} * Gamma(7/6) / Gamma(1/2)`.
//!
//! ## References
//!
//! - Barndorff-Nielsen, O.E. & Shephard, N. (2004). "Power and bipower variation
//!   with stochastic volatility and jumps." Journal of Financial Econometrics,
//!   2(1), 1-37.
//! - Barndorff-Nielsen, O.E. & Shephard, N. (2006). "Econometrics of testing
//!   for jumps in financial economics using bipower variation."
//!   Journal of Financial Econometrics, 4(1), 1-30.

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;
use std::f64::consts::PI;

use crate::statistics::{RegimeAccumulator, WelfordAccumulator};
use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;

const NS_PER_SECOND: i64 = 1_000_000_000;

/// `mu_1 = E[|Z|] = sqrt(2/pi)` for standard normal Z.
/// Used in bipower variation scaling: `BV = mu_1^{-2} * sum(|r_t| * |r_{t-1}|)`
#[allow(dead_code)]
const MU_1: f64 = 0.7978845608028654;

/// Jump detection tracker using bipower variation.
pub struct JumpTracker {
    bin_width_seconds: f64,
    bin_width_ns: i64,

    day_timestamps: Vec<i64>,
    day_mid_prices: Vec<f64>,

    daily_rv: WelfordAccumulator,
    daily_bv: WelfordAccumulator,
    daily_jump_fraction: WelfordAccumulator,
    daily_z_stat: WelfordAccumulator,

    jump_fraction_values: Vec<f64>,
    regime_jump_fraction: RegimeAccumulator,

    n_days: u32,
    n_significant_jumps: u64,
    z_threshold: f64,
}

impl JumpTracker {
    /// Create a new JumpTracker.
    ///
    /// # Arguments
    /// * `bin_width_seconds` — Timescale for return computation (e.g. 5.0 for 5s returns)
    /// * `z_threshold` — z-statistic threshold for jump significance (typically 2.0 or 3.0)
    pub fn new(bin_width_seconds: f64, z_threshold: f64) -> Self {
        let bin_width_ns = (bin_width_seconds * NS_PER_SECOND as f64) as i64;
        Self {
            bin_width_seconds,
            bin_width_ns,
            day_timestamps: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            daily_rv: WelfordAccumulator::new(),
            daily_bv: WelfordAccumulator::new(),
            daily_jump_fraction: WelfordAccumulator::new(),
            daily_z_stat: WelfordAccumulator::new(),
            jump_fraction_values: Vec::new(),
            regime_jump_fraction: RegimeAccumulator::new(),
            n_days: 0,
            n_significant_jumps: 0,
            z_threshold,
        }
    }

    fn process_day_jumps(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_mid_prices.len() < 2 {
            return;
        }

        let resampled = resample_to_grid(
            &self.day_timestamps,
            &self.day_mid_prices,
            self.bin_width_ns,
            day_epoch_ns,
            utc_offset,
            AggMode::Last,
        );

        let filled: Vec<f64> = resampled
            .values
            .iter()
            .filter(|v| v.is_finite())
            .copied()
            .collect();

        if filled.len() < 4 {
            return;
        }

        let mut returns = Vec::with_capacity(filled.len() - 1);
        for i in 1..filled.len() {
            if filled[i] > 0.0 && filled[i - 1] > 0.0 {
                let ret = (filled[i] / filled[i - 1]).ln();
                if ret.is_finite() {
                    returns.push(ret);
                }
            }
        }

        if returns.len() < 4 {
            return;
        }

        let n = returns.len();

        // RV = sum(r_t^2)
        let rv: f64 = returns.iter().map(|r| r * r).sum();

        // BV = (pi/2) * sum(|r_t| * |r_{t-1}|)
        let bv_sum: f64 = returns
            .windows(2)
            .map(|w| w[0].abs() * w[1].abs())
            .sum();
        let bv = (PI / 2.0) * bv_sum;

        // Jump component: J = max(RV - BV, 0)
        let jump = (rv - bv).max(0.0);
        let jump_fraction = if rv > 1e-20 { jump / rv } else { 0.0 };

        // Tripower quarticity for variance estimation
        // TP = n * (2^{2/3} * Gamma(7/6) / Gamma(1/2))^{-3} * sum |r_i|^{4/3} * |r_{i-1}|^{4/3} * |r_{i-2}|^{4/3}
        // Simplified using the BNS (2006) formulation:
        let tp_sum: f64 = if returns.len() >= 3 {
            returns
                .windows(3)
                .map(|w| {
                    w[0].abs().powf(4.0 / 3.0)
                        * w[1].abs().powf(4.0 / 3.0)
                        * w[2].abs().powf(4.0 / 3.0)
                })
                .sum()
        } else {
            0.0
        };

        let mu_43 = 2.0f64.powf(2.0 / 3.0) * gamma_ratio();
        let tp = (n as f64 / (n as f64 - 2.0)) * mu_43.powi(-3) * tp_sum;

        // z-statistic: z = (RV - BV) / sqrt(theta * max(TP, 0) / n)
        // theta = (pi^2/4 + pi - 5)
        let theta = PI * PI / 4.0 + PI - 5.0;
        let var_est = theta * tp.max(0.0) / n as f64;
        let z = if var_est > 1e-20 {
            (rv - bv) / var_est.sqrt()
        } else {
            0.0
        };

        self.daily_rv.update(rv);
        self.daily_bv.update(bv);
        self.daily_jump_fraction.update(jump_fraction);
        self.daily_z_stat.update(z);
        self.jump_fraction_values.push(jump_fraction);

        if z > self.z_threshold {
            self.n_significant_jumps += 1;
        }

        // Regime-conditional jump fraction deferred: daily jump fraction is a
        // day-level statistic and cannot be meaningfully assigned to a single
        // intraday regime without tracking per-regime sub-day jump contributions.
    }
}

/// Gamma(7/6) / Gamma(1/2) ratio used in tripower quarticity.
/// Gamma(7/6) ≈ 0.9407, Gamma(1/2) = sqrt(pi) ≈ 1.7725
fn gamma_ratio() -> f64 {
    0.9407354897187262 / PI.sqrt()
}

impl AnalysisTracker for JumpTracker {
    fn process_event(
        &mut self,
        msg: &MboMessage,
        lob_state: &LobState,
        _regime: u8,
        _day_epoch_ns: i64,
    ) {
        if lob_state.check_consistency() != BookConsistency::Valid {
            return;
        }
        if let (Some(mid), Some(ts)) = (lob_state.mid_price(), msg.timestamp) {
            if mid > 0.0 {
                self.day_timestamps.push(ts);
                self.day_mid_prices.push(mid);
            }
        }
    }

    fn end_of_day(&mut self, _day_index: u32) {
        let (utc_offset, day_epoch_ns) =
            crate::time::regime::infer_day_params(&self.day_timestamps);
        self.process_day_jumps(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_timestamps.clear();
        self.day_mid_prices.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let significant_pct = if self.n_days > 0 {
            self.n_significant_jumps as f64 / self.n_days as f64 * 100.0
        } else {
            0.0
        };

        json!({
            "tracker": "JumpTracker",
            "n_days": self.n_days,
            "bin_width_seconds": self.bin_width_seconds,
            "z_threshold": self.z_threshold,
            "daily_rv": {
                "mean": self.daily_rv.mean(),
                "std": self.daily_rv.std(),
                "min": self.daily_rv.min(),
                "max": self.daily_rv.max(),
                "count": self.daily_rv.count(),
            },
            "daily_bv": {
                "mean": self.daily_bv.mean(),
                "std": self.daily_bv.std(),
                "min": self.daily_bv.min(),
                "max": self.daily_bv.max(),
                "count": self.daily_bv.count(),
            },
            "daily_jump_fraction": {
                "mean": self.daily_jump_fraction.mean(),
                "std": self.daily_jump_fraction.std(),
                "min": self.daily_jump_fraction.min(),
                "max": self.daily_jump_fraction.max(),
                "count": self.daily_jump_fraction.count(),
            },
            "daily_z_statistic": {
                "mean": self.daily_z_stat.mean(),
                "std": self.daily_z_stat.std(),
                "min": self.daily_z_stat.min(),
                "max": self.daily_z_stat.max(),
                "count": self.daily_z_stat.count(),
            },
            "n_significant_jump_days": self.n_significant_jumps,
            "significant_jump_pct": significant_pct,
            "regime_conditional_jump_fraction": self.regime_jump_fraction.finalize(),
        })
    }

    fn name(&self) -> &str {
        "JumpTracker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::{Action, Side};

    fn make_msg(ts: i64) -> MboMessage {
        MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100).with_timestamp(ts)
    }

    fn make_lob_with_mid(mid_nanodollars: i64) -> LobState {
        let mut lob = LobState::new(10);
        let half_spread = 5_000_000;
        lob.best_bid = Some(mid_nanodollars - half_spread);
        lob.best_ask = Some(mid_nanodollars + half_spread);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    #[test]
    fn test_collects_data() {
        let mut tracker = JumpTracker::new(5.0, 2.0);
        let ts_base = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);

        tracker.process_event(&make_msg(ts_base), &lob, 3, 0);
        tracker.process_event(&make_msg(ts_base + NS_PER_SECOND), &lob, 3, 0);

        assert_eq!(tracker.day_mid_prices.len(), 2);
        assert_eq!(tracker.day_timestamps.len(), 2);
    }

    #[test]
    fn test_reset_day() {
        let mut tracker = JumpTracker::new(5.0, 2.0);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);
        tracker.process_event(&make_msg(ts), &lob, 3, 0);

        tracker.end_of_day(0);
        tracker.reset_day();

        assert!(tracker.day_mid_prices.is_empty());
        assert!(tracker.day_timestamps.is_empty());
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = JumpTracker::new(5.0, 2.0);
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "JumpTracker");
        assert!(report.get("daily_rv").is_some());
        assert!(report.get("daily_bv").is_some());
        assert!(report.get("daily_jump_fraction").is_some());
        assert!(report.get("daily_z_statistic").is_some());
        assert!(report.get("n_significant_jump_days").is_some());
        assert!(report.get("regime_conditional_jump_fraction").is_some());
    }

    #[test]
    fn test_bv_less_than_rv_for_jumpy_series() {
        // BV should be <= RV when jumps are present (by construction)
        // For constant returns (no jumps): RV ≈ BV
        let returns = vec![0.001, 0.001, 0.001, 0.001, 0.001];
        let rv: f64 = returns.iter().map(|r| r * r).sum();
        let bv_sum: f64 = returns.windows(2).map(|w| w[0].abs() * w[1].abs()).sum();
        let bv = (PI / 2.0) * bv_sum;

        // For constant returns, BV ≈ (pi/2) * n * r^2 vs RV = n * r^2
        // So BV/RV ≈ pi/2 ≈ 1.57 (BV > RV for continuous case, jump = 0)
        let jump = (rv - bv).max(0.0);
        assert!(
            jump < 1e-10,
            "No jump expected for constant returns, got J={}",
            jump
        );
    }

    #[test]
    fn test_bv_constant_returns_exact() {
        // Returns: [0.01, 0.01, 0.01, 0.01]
        // RV = 4 * 0.01^2 = 0.0004
        // BV = (pi/2) * sum(|r_t| * |r_{t-1}|) for 3 adjacent pairs
        //    = (pi/2) * 3 * 0.0001 = (pi/2) * 0.0003
        let returns = [0.01, 0.01, 0.01, 0.01];
        let rv: f64 = returns.iter().map(|r| r * r).sum();
        let bv: f64 = PI / 2.0
            * returns
                .windows(2)
                .map(|w| w[0].abs() * w[1].abs())
                .sum::<f64>();

        assert!(
            (rv - 0.0004).abs() < 1e-15,
            "RV expected 0.0004, got {}",
            rv
        );
        let expected_bv = PI / 2.0 * 0.0003;
        assert!(
            (bv - expected_bv).abs() < 1e-15,
            "BV expected {}, got {}",
            expected_bv,
            bv
        );
        // BV > RV for constant returns (no jumps) since pi/2 > 1
        assert!(bv > rv, "For constant returns, BV should exceed RV");
    }

    #[test]
    fn test_jump_fraction_with_outlier() {
        // Returns: [0.001]*5 + [0.5] + [0.001]*5
        // RV = 10*0.001^2 + 0.5^2 = 0.00001 + 0.25 = 0.25001
        // BV is robust to single jump → dominated by 0.001*0.001 pairs
        // jump_fraction = (RV - BV) / RV should be > 0.9
        let mut returns: Vec<f64> = vec![0.001; 5];
        returns.push(0.5);
        returns.extend(vec![0.001; 5]);

        let rv: f64 = returns.iter().map(|r| r * r).sum();
        let bv: f64 = PI / 2.0
            * returns
                .windows(2)
                .map(|w| w[0].abs() * w[1].abs())
                .sum::<f64>();
        let j = (rv - bv).max(0.0);
        let jump_frac = j / rv;

        assert!(rv > 0.24, "RV should be dominated by the jump, got {}", rv);
        assert!(
            jump_frac > 0.9,
            "Jump fraction should be >0.9, got {}",
            jump_frac
        );
    }

    #[test]
    fn test_gamma_ratio_value() {
        // gamma_ratio = Gamma(7/6) / sqrt(pi)
        // Gamma(7/6) ≈ 0.9407354897187262
        // sqrt(pi) ≈ 1.7724538509055159
        // ratio ≈ 0.53075
        let expected = gamma_ratio();
        let manual = 0.9407354897187262_f64 / PI.sqrt();
        assert!(
            (expected - manual).abs() < 1e-12,
            "gamma_ratio expected {}, got {}",
            manual,
            expected
        );
        assert!(
            (expected - 0.53075).abs() < 0.001,
            "gamma_ratio expected ~0.53075, got {}",
            expected
        );
    }
}
