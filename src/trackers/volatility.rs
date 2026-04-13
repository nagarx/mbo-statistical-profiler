//! # VolatilityTracker
//!
//! ## Purpose
//!
//! Multi-scale realized volatility analysis with intraday patterns, vol-of-vol,
//! persistence (ACF), and spread-volatility correlation. Provides the statistical
//! foundation for understanding price uncertainty and its microstructural drivers.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Realized variance | `RV = sum(r_t^2)` | dimensionless |
//! | Annualized volatility | `sigma = sqrt(RV * 252) * 100` | % per annum |
//! | Intraday volatility curve | Mean `r_t^2` per 1-min bin | dim./min |
//! | Vol-of-vol | `std(daily_RV)` via Welford | dimensionless |
//! | Volatility persistence | ACF of daily RV at lags 1-20 | dimensionless |
//! | Spread-vol correlation | `corr(daily_RV, daily_mean_spread)` | dimensionless |
//!
//! ## Formulas
//!
//! - Realized variance (Barndorff-Nielsen & Shephard 2002):
//!   `RV_t = sum_{i=1}^{n} r_{t,i}^2`
//!   where `r_{t,i} = ln(mid_{t,i} / mid_{t,i-1})`
//!
//! - Annualized realized volatility:
//!   `sigma_annual = sqrt(RV * 252) * 100`
//!
//! ## References
//!
//! - Barndorff-Nielsen, O.E. & Shephard, N. (2002). "Econometric analysis of
//!   realized volatility and its use in estimating stochastic volatility models."
//!   Journal of the Royal Statistical Society B, 64(2), 253-280.

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::statistics::{AcfComputer, IntradayCurveAccumulator, WelfordAccumulator};
use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;

const NS_PER_SECOND: i64 = 1_000_000_000;

/// Multi-scale realized volatility tracker.
///
/// Collects mid-prices during `process_event`, computes realized volatility
/// and related statistics in `end_of_day`.
pub struct VolatilityTracker {
    #[allow(dead_code)]
    timescales: Vec<f64>,

    day_timestamps: Vec<i64>,
    day_mid_prices: Vec<f64>,
    day_spreads: Vec<f64>,

    per_scale: Vec<ScaleVolState>,
    intraday_curve: IntradayCurveAccumulator,
    daily_rv: WelfordAccumulator,
    daily_annualized_vol: WelfordAccumulator,
    rv_acf: AcfComputer,
    spread_vol_pairs: Vec<(f64, f64)>,
    n_days: u32,
}

struct ScaleVolState {
    label: String,
    bin_width_ns: i64,
    rv_welford: WelfordAccumulator,
    daily_rv_values: Vec<f64>,
}

impl VolatilityTracker {
    pub fn new(timescales: &[f64]) -> Self {
        let per_scale = timescales
            .iter()
            .map(|&s| {
                let bin_ns = (s * NS_PER_SECOND as f64) as i64;
                ScaleVolState {
                    label: format_scale_label(s),
                    bin_width_ns: bin_ns,
                    rv_welford: WelfordAccumulator::new(),
                    daily_rv_values: Vec::new(),
                }
            })
            .collect();

        Self {
            timescales: timescales.to_vec(),
            day_timestamps: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            day_spreads: Vec::with_capacity(20_000_000),
            per_scale,
            intraday_curve: IntradayCurveAccumulator::new_rth_1min(),
            daily_rv: WelfordAccumulator::new(),
            daily_annualized_vol: WelfordAccumulator::new(),
            rv_acf: AcfComputer::new(10_000, 20),
            spread_vol_pairs: Vec::new(),
            n_days: 0,
        }
    }

    fn process_day_volatility(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_mid_prices.len() < 2 {
            return;
        }

        let mean_spread = if self.day_spreads.is_empty() {
            f64::NAN
        } else {
            self.day_spreads.iter().sum::<f64>() / self.day_spreads.len() as f64
        };

        for scale in &mut self.per_scale {
            let resampled = resample_to_grid(
                &self.day_timestamps,
                &self.day_mid_prices,
                scale.bin_width_ns,
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

            if filled.len() < 2 {
                continue;
            }

            let mut rv = 0.0f64;
            for i in 1..filled.len() {
                if filled[i] > 0.0 && filled[i - 1] > 0.0 {
                    let ret = (filled[i] / filled[i - 1]).ln();
                    if ret.is_finite() {
                        rv += ret * ret;
                    }
                }
            }

            scale.rv_welford.update(rv);
            scale.daily_rv_values.push(rv);
        }

        // Use the first (finest) scale for daily aggregates
        if let Some(first_scale) = self.per_scale.first() {
            if let Some(&rv) = first_scale.daily_rv_values.last() {
                self.daily_rv.update(rv);
                let annualized = (rv * 252.0).sqrt() * 100.0;
                self.daily_annualized_vol.update(annualized);
                self.rv_acf.push(rv);

                if mean_spread.is_finite() {
                    self.spread_vol_pairs.push((rv, mean_spread));
                }
            }
        }

        // Intraday volatility curve: per-tick squared returns
        for i in 1..self.day_mid_prices.len() {
            if self.day_mid_prices[i] > 0.0 && self.day_mid_prices[i - 1] > 0.0 {
                let ret = (self.day_mid_prices[i] / self.day_mid_prices[i - 1]).ln();
                if ret.is_finite() {
                    let ts = self.day_timestamps[i];
                    self.intraday_curve.add(ts, ret * ret, utc_offset);
                }
            }
        }
    }
}

impl AnalysisTracker for VolatilityTracker {
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
        if let Some(spread) = lob_state.spread() {
            if spread.is_finite() && spread >= 0.0 {
                self.day_spreads.push(spread);
            }
        }
    }

    fn end_of_day(&mut self, _day_index: u32) {
        let (utc_offset, day_epoch_ns) =
            crate::time::regime::infer_day_params(&self.day_timestamps);
        self.process_day_volatility(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_timestamps.clear();
        self.day_mid_prices.clear();
        self.day_spreads.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let mut scales = serde_json::Map::new();

        for scale in &self.per_scale {
            scales.insert(
                scale.label.clone(),
                json!({
                    "daily_rv": {
                        "mean": scale.rv_welford.mean(),
                        "std": scale.rv_welford.std(),
                        "min": scale.rv_welford.min(),
                        "max": scale.rv_welford.max(),
                        "count": scale.rv_welford.count(),
                    },
                }),
            );
        }

        let rv_acf_vals = self.rv_acf.compute();

        let spread_vol_corr = compute_correlation(&self.spread_vol_pairs);

        let curve_data: Vec<serde_json::Value> = self
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| {
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_squared_return": b.mean,
                    "std": b.std,
                    "count": b.count,
                })
            })
            .collect();

        json!({
            "tracker": "VolatilityTracker",
            "n_days": self.n_days,
            "per_scale": scales,
            "daily_rv": {
                "mean": self.daily_rv.mean(),
                "std": self.daily_rv.std(),
                "min": self.daily_rv.min(),
                "max": self.daily_rv.max(),
                "count": self.daily_rv.count(),
            },
            "daily_annualized_vol": {
                "mean": self.daily_annualized_vol.mean(),
                "std": self.daily_annualized_vol.std(),
                "min": self.daily_annualized_vol.min(),
                "max": self.daily_annualized_vol.max(),
                "count": self.daily_annualized_vol.count(),
            },
            "vol_of_vol": self.daily_rv.std(),
            "rv_persistence_acf": rv_acf_vals,
            "spread_vol_correlation": spread_vol_corr,
            "intraday_volatility_curve": curve_data,
        })
    }

    fn name(&self) -> &str {
        "VolatilityTracker"
    }
}

/// Pearson correlation from (x, y) pairs.
fn compute_correlation(pairs: &[(f64, f64)]) -> f64 {
    let n = pairs.len();
    if n < 3 {
        return f64::NAN;
    }
    let n_f = n as f64;
    let mean_x = pairs.iter().map(|p| p.0).sum::<f64>() / n_f;
    let mean_y = pairs.iter().map(|p| p.1).sum::<f64>() / n_f;

    let mut cov = 0.0f64;
    let mut var_x = 0.0f64;
    let mut var_y = 0.0f64;
    for &(x, y) in pairs {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom < 1e-15 {
        0.0
    } else {
        cov / denom
    }
}

fn format_scale_label(seconds: f64) -> String {
    if seconds < 1.0 {
        format!("{}ms", (seconds * 1000.0) as u64)
    } else if seconds < 60.0 {
        format!("{}s", seconds as u64)
    } else {
        format!("{}m", (seconds / 60.0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::{Action, Side};

    fn make_msg(ts: i64) -> MboMessage {
        MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100)
            .with_timestamp(ts)
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
    fn test_collects_mid_prices_and_spreads() {
        let mut tracker = VolatilityTracker::new(&[1.0]);
        let ts_base = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);

        tracker.process_event(&make_msg(ts_base), &lob, 3, 0);
        tracker.process_event(&make_msg(ts_base + NS_PER_SECOND), &lob, 3, 0);

        assert_eq!(tracker.day_mid_prices.len(), 2);
        assert_eq!(tracker.day_spreads.len(), 2);
    }

    #[test]
    fn test_reset_day_clears_buffers() {
        let mut tracker = VolatilityTracker::new(&[1.0]);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);
        tracker.process_event(&make_msg(ts), &lob, 3, 0);

        tracker.end_of_day(0);
        tracker.reset_day();

        assert!(tracker.day_mid_prices.is_empty());
        assert!(tracker.day_timestamps.is_empty());
        assert!(tracker.day_spreads.is_empty());
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = VolatilityTracker::new(&[1.0, 5.0]);
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "VolatilityTracker");
        assert!(report.get("per_scale").is_some());
        assert!(report.get("daily_rv").is_some());
        assert!(report.get("vol_of_vol").is_some());
        assert!(report.get("rv_persistence_acf").is_some());
        assert!(report.get("spread_vol_correlation").is_some());
        assert!(report.get("intraday_volatility_curve").is_some());
    }

    #[test]
    fn test_correlation_known_values() {
        let pairs: Vec<(f64, f64)> = vec![
            (1.0, 2.0),
            (2.0, 4.0),
            (3.0, 6.0),
            (4.0, 8.0),
            (5.0, 10.0),
        ];
        let corr = compute_correlation(&pairs);
        assert!(
            (corr - 1.0).abs() < 1e-10,
            "Perfect positive correlation expected, got {}",
            corr
        );
    }

    #[test]
    fn test_correlation_insufficient_data() {
        let pairs = vec![(1.0, 2.0)];
        assert!(compute_correlation(&pairs).is_nan());
    }

    #[test]
    fn test_rv_known_returns_exact() {
        // Returns: [0.01, -0.02, 0.015, -0.005]
        // RV = 0.01^2 + 0.02^2 + 0.015^2 + 0.005^2
        //    = 0.0001 + 0.0004 + 0.000225 + 0.000025 = 0.00075
        let returns = [0.01, -0.02, 0.015, -0.005];
        let rv: f64 = returns.iter().map(|r| r * r).sum();
        assert!(
            (rv - 0.00075).abs() < 1e-15,
            "RV expected 0.00075, got {}",
            rv
        );
    }

    #[test]
    fn test_annualized_vol_formula() {
        // RV = 0.00075
        // Annualized = sqrt(RV * 252) * 100 = sqrt(0.189) * 100
        // sqrt(0.189) = 0.43474...
        // Result ≈ 43.47%
        let rv = 0.00075;
        let annualized = (rv * 252.0_f64).sqrt() * 100.0;
        assert!(
            (annualized - 43.47).abs() < 0.1,
            "Annualized vol expected ~43.47%, got {}%",
            annualized
        );
    }

    #[test]
    fn test_rv_zero_for_constant_prices() {
        // All same prices → all returns = 0 → RV = 0
        let returns: Vec<f64> = vec![0.0; 100];
        let rv: f64 = returns.iter().map(|r| r * r).sum();
        assert_eq!(rv, 0.0, "Constant prices: RV should be exactly 0");
    }
}
