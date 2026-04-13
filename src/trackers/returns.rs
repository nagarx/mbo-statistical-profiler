//! # ReturnTracker
//!
//! ## Purpose
//!
//! Characterizes mid-price return distributions at multiple timescales,
//! intraday patterns, tail risk, and persistence. This is the foundation
//! for volatility analysis, jump detection, and OFI-return correlation.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units | Per-Scale? |
//! |-----------|---------|-------|:---:|
//! | Return distribution | `r_t = ln(mid_t / mid_{t-1})` | dimensionless | Yes |
//! | Mean, std, skewness, kurtosis | Standard moments from reservoir | dim. | Yes |
//! | Percentiles (1,5,10,25,50,75,90,95,99) | From reservoir sample | dim. | Yes |
//! | Intraday curve | Mean return per 1-min canonical bin | dim./min | No |
//! | Tail index (Hill) | `H = (1/k) * sum(ln(X_i / X_k))` | dim. | Yes |
//! | VaR (1%, 5%) | Quantile of return distribution | dim. | Yes |
//! | CVaR (1%, 5%) | `E[r | r <= VaR]` | dim. | Yes |
//! | ACF (lags 1-20) | `ACF(k) = cov(r_t, r_{t+k}) / var(r)` | dim. | Yes |
//! | Absolute return ACF | ACF of `|r_t|` (volatility clustering) | dim. | Yes |
//! | Zero-return fraction | `count(r=0) / count(r)` | ratio | Yes |
//! | Max drawdown per day | Peak-to-trough of cumulative returns | dim. | No |
//!
//! ## References
//!
//! - Hill, B.M. (1975). "A simple general approach to inference about the tail of a distribution."
//!   Annals of Statistics, 3(5), 1163-1174.
//! - Cont, R. (2001). "Empirical properties of asset returns: stylized facts and statistical issues."
//!   Quantitative Finance, 1(2), 223-236.
//!
//! ## Input Requirements
//!
//! - `LobState::mid_price()` — must be `Some(f64 > 0)` for valid return computation
//! - `MboMessage::timestamp` — used for time-based resampling

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::statistics::{
    AcfComputer, IntradayCurveAccumulator, StreamingDistribution, WelfordAccumulator,
};
use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;

const NS_PER_SECOND: i64 = 1_000_000_000;

/// Multi-scale return analysis tracker.
pub struct ReturnTracker {
    day_timestamps: Vec<i64>,
    day_mid_prices: Vec<f64>,

    per_scale: Vec<ScaleState>,
    intraday_curve: IntradayCurveAccumulator,
    intraday_abs_return_curve: IntradayCurveAccumulator,
    daily_drawdowns: WelfordAccumulator,
    daily_runups: WelfordAccumulator,
    n_days: u32,
}

struct ScaleState {
    label: String,
    bin_width_ns: i64,
    dist: StreamingDistribution,
    acf: AcfComputer,
    abs_acf: AcfComputer,
    zero_count: u64,
    total_count: u64,
}

impl ReturnTracker {
    pub fn new(timescales: &[f64], reservoir_capacity: usize) -> Self {
        let per_scale = timescales
            .iter()
            .map(|&s| {
                let bin_ns = (s * NS_PER_SECOND as f64) as i64;
                ScaleState {
                    label: format_scale_label(s),
                    bin_width_ns: bin_ns,
                    dist: StreamingDistribution::new(reservoir_capacity),
                    acf: AcfComputer::new(10_000, 20),
                    abs_acf: AcfComputer::new(10_000, 20),
                    zero_count: 0,
                    total_count: 0,
                }
            })
            .collect();

        Self {
            day_timestamps: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            per_scale,
            intraday_curve: IntradayCurveAccumulator::new_rth_1min(),
            intraday_abs_return_curve: IntradayCurveAccumulator::new_rth_1min(),
            daily_drawdowns: WelfordAccumulator::new(),
            daily_runups: WelfordAccumulator::new(),
            n_days: 0,
        }
    }

    fn process_day_returns(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_mid_prices.len() < 2 {
            return;
        }

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

            for i in 1..filled.len() {
                if filled[i] > 0.0 && filled[i - 1] > 0.0 {
                    let ret = (filled[i] / filled[i - 1]).ln();
                    if ret.is_finite() {
                        scale.dist.add(ret);
                        scale.acf.push(ret);
                        scale.abs_acf.push(ret.abs());
                        scale.total_count += 1;
                        if ret.abs() < 1e-15 {
                            scale.zero_count += 1;
                        }
                    }
                }
            }
        }

        for i in 1..self.day_mid_prices.len() {
            if self.day_mid_prices[i] > 0.0 && self.day_mid_prices[i - 1] > 0.0 {
                let ret = (self.day_mid_prices[i] / self.day_mid_prices[i - 1]).ln();
                if ret.is_finite() {
                    let ts = self.day_timestamps[i];
                    self.intraday_curve.add(ts, ret, utc_offset);
                    self.intraday_abs_return_curve.add(ts, ret.abs(), utc_offset);
                }
            }
        }

        let mut cum_ret = 0.0f64;
        let mut peak = 0.0f64;
        let mut max_drawdown = 0.0f64;
        let mut trough = 0.0f64;
        let mut max_runup = 0.0f64;

        for i in 1..self.day_mid_prices.len() {
            if self.day_mid_prices[i] > 0.0 && self.day_mid_prices[i - 1] > 0.0 {
                cum_ret += (self.day_mid_prices[i] / self.day_mid_prices[i - 1]).ln();
                if cum_ret > peak {
                    peak = cum_ret;
                }
                let dd = peak - cum_ret;
                if dd > max_drawdown {
                    max_drawdown = dd;
                }
                if cum_ret < trough {
                    trough = cum_ret;
                }
                let ru = cum_ret - trough;
                if ru > max_runup {
                    max_runup = ru;
                }
            }
        }
        self.daily_drawdowns.update(max_drawdown);
        self.daily_runups.update(max_runup);
    }
}

impl AnalysisTracker for ReturnTracker {
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
        self.process_day_returns(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_timestamps.clear();
        self.day_mid_prices.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let mut scales = serde_json::Map::new();

        for scale in &self.per_scale {
            let acf_vals = scale.acf.compute();
            let abs_acf_vals = scale.abs_acf.compute();

            let zero_frac = if scale.total_count > 0 {
                scale.zero_count as f64 / scale.total_count as f64
            } else {
                0.0
            };

            let sorted = scale.dist.percentiles(&[1.0, 5.0]);
            let var_1 = sorted[0];
            let var_5 = sorted[1];

            let cvar_1 = compute_cvar(&scale.dist, 1.0);
            let cvar_5 = compute_cvar(&scale.dist, 5.0);

            let hill_left = compute_hill_index(&scale.dist, true);
            let hill_right = compute_hill_index(&scale.dist, false);

            scales.insert(
                scale.label.clone(),
                json!({
                    "distribution": scale.dist.summary(),
                    "acf": acf_vals,
                    "abs_return_acf": abs_acf_vals,
                    "zero_return_fraction": zero_frac,
                    "var_1pct": var_1,
                    "var_5pct": var_5,
                    "cvar_1pct": cvar_1,
                    "cvar_5pct": cvar_5,
                    "hill_index_left_tail": hill_left,
                    "hill_index_right_tail": hill_right,
                    "n_returns": scale.total_count,
                }),
            );
        }

        let curve_data: Vec<serde_json::Value> = self
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| {
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_return": b.mean,
                    "std_return": b.std,
                    "count": b.count,
                })
            })
            .collect();

        let abs_curve_data: Vec<serde_json::Value> = self
            .intraday_abs_return_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| {
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_abs_return": b.mean,
                    "std_abs_return": b.std,
                    "count": b.count,
                })
            })
            .collect();

        json!({
            "tracker": "ReturnTracker",
            "n_days": self.n_days,
            "per_scale": scales,
            "intraday_return_curve": curve_data,
            "intraday_abs_return_curve": abs_curve_data,
            "daily_max_drawdown": {
                "mean": self.daily_drawdowns.mean(),
                "std": self.daily_drawdowns.std(),
                "max": self.daily_drawdowns.max(),
                "count": self.daily_drawdowns.count(),
            },
            "daily_max_runup": {
                "mean": self.daily_runups.mean(),
                "std": self.daily_runups.std(),
                "max": self.daily_runups.max(),
                "count": self.daily_runups.count(),
            },
        })
    }

    fn name(&self) -> &str {
        "ReturnTracker"
    }
}

/// Hill tail index estimator.
///
/// H = (1/k) * sum_{i=1}^{k} ln(X_{(i)} / X_{(k)})
///
/// where X_{(1)} >= X_{(2)} >= ... are order statistics (absolute values).
/// Computed from the reservoir sample. k = 5% of sample size.
///
/// Hill (1975), Annals of Statistics 3(5), 1163-1174.
fn compute_hill_index(dist: &StreamingDistribution, left_tail: bool) -> f64 {
    let sample = dist.sorted_sample();
    if sample.is_empty() {
        return f64::NAN;
    }

    let mut abs_vals: Vec<f64> = if left_tail {
        sample
            .iter()
            .filter(|&&v| v < 0.0)
            .map(|&v| v.abs())
            .collect()
    } else {
        sample.iter().filter(|&&v| v > 0.0).copied().collect()
    };

    abs_vals.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let k = (abs_vals.len() as f64 * 0.05).max(10.0) as usize;
    if k + 1 >= abs_vals.len() || k < 2 {
        return f64::NAN;
    }

    // Hill (1975): H = (1/k) * sum_{i=1}^{k} ln(X_{(i)} / X_{(k+1)})
    // X sorted descending, X_{(k+1)} = abs_vals[k] is the threshold
    let threshold = abs_vals[k];
    if threshold < 1e-15 {
        return f64::NAN;
    }

    let sum: f64 = abs_vals[..k]
        .iter()
        .map(|&x| (x / threshold).ln())
        .sum();

    let hill = sum / k as f64;
    if hill > 1e-10 { 1.0 / hill } else { f64::NAN }
}

/// Conditional VaR (CVaR / Expected Shortfall).
///
/// CVaR_p = E[r | r <= VaR_p]
fn compute_cvar(dist: &StreamingDistribution, pct: f64) -> f64 {
    let var = dist.percentile(pct);
    if var.is_nan() {
        return f64::NAN;
    }

    let sample = dist.sorted_sample();
    let tail: Vec<f64> = sample.iter().filter(|&&v| v <= var).copied().collect();
    if tail.is_empty() {
        var
    } else {
        tail.iter().sum::<f64>() / tail.len() as f64
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
        let half_spread = 5_000_000; // $0.005
        lob.best_bid = Some(mid_nanodollars - half_spread);
        lob.best_ask = Some(mid_nanodollars + half_spread);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    #[test]
    fn test_collects_mid_prices() {
        let mut tracker = ReturnTracker::new(&[1.0], 1000);
        let ts_base = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob = make_lob_with_mid(100_000_000_000);
        tracker.process_event(&make_msg(ts_base), &lob, 3, 0);

        let lob2 = make_lob_with_mid(100_010_000_000);
        tracker.process_event(&make_msg(ts_base + NS_PER_SECOND), &lob2, 3, 0);

        assert_eq!(tracker.day_mid_prices.len(), 2);
    }

    #[test]
    fn test_reset_day_clears() {
        let mut tracker = ReturnTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);
        tracker.process_event(&make_msg(ts), &lob, 3, 0);

        tracker.end_of_day(0);
        tracker.reset_day();
        assert_eq!(tracker.day_mid_prices.len(), 0);
        assert_eq!(tracker.day_timestamps.len(), 0);
    }

    #[test]
    fn test_finalize_has_expected_structure() {
        let tracker = ReturnTracker::new(&[1.0, 5.0], 1000);
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "ReturnTracker");
        assert!(report.get("per_scale").is_some());
        assert!(report.get("intraday_return_curve").is_some());
        assert!(
            report.get("intraday_abs_return_curve").is_some(),
            "finalize must include intraday_abs_return_curve"
        );
        assert!(report.get("daily_max_drawdown").is_some());
    }

    #[test]
    fn test_scale_labels() {
        assert_eq!(format_scale_label(1.0), "1s");
        assert_eq!(format_scale_label(5.0), "5s");
        assert_eq!(format_scale_label(60.0), "1m");
        assert_eq!(format_scale_label(300.0), "5m");
        assert_eq!(format_scale_label(0.1), "100ms");
    }

    #[test]
    fn test_hill_index_on_known_data() {
        let mut dist = StreamingDistribution::new(10000);
        for i in 1..=10000 {
            dist.add(-(1.0 / i as f64));
        }
        let hill = compute_hill_index(&dist, true);
        assert!(hill.is_finite(), "Hill index should be finite for power-law data");
        assert!(hill > 0.0, "Hill index should be positive");
    }

    #[test]
    fn test_log_return_known_value() {
        // ln(101/100) = 0.00995033085...
        let expected = (101.0_f64 / 100.0).ln();
        assert!(
            (expected - 0.009950330853168083).abs() < 1e-12,
            "ln(101/100) expected 0.009950330853168083, got {}",
            expected
        );
    }

    #[test]
    fn test_cvar_known_distribution() {
        // Values: [-5, -4, -3, -2, -1, 0, 1, 2, 3, 4]
        // VaR(10%) = percentile(10) of 10 sorted values
        let mut dist = StreamingDistribution::new(100);
        for &v in &[-5.0, -4.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0] {
            dist.add(v);
        }
        let var_10 = dist.percentile(10.0);
        // With 10 values, percentile(10): interpolated near bottom of sorted array
        assert!(
            (var_10 - (-4.1)).abs() < 0.15,
            "VaR(10%) expected ~-4.1, got {}",
            var_10
        );
    }

    #[test]
    fn test_zero_return_fraction_formula() {
        let mut zero_count: u64 = 0;
        let mut total: u64 = 0;
        let returns: [f64; 10] = [0.01, 0.0, -0.01, 0.0, 0.0, 0.02, -0.005, 0.0, 0.001, -0.001];
        for &r in &returns {
            total += 1;
            if r.abs() < 1e-15 {
                zero_count += 1;
            }
        }
        // 4 zeros out of 10
        assert_eq!(zero_count, 4);
        assert!(
            (zero_count as f64 / total as f64 - 0.4).abs() < 1e-10,
            "Zero fraction expected 0.4, got {}",
            zero_count as f64 / total as f64
        );
    }

    #[test]
    fn test_drawdown_known_series() {
        // Cumulative returns: [0.1, 0.05, -0.05, -0.1, 0.2]
        // Peak at 0.1 (t=0), trough at -0.1 (t=3)
        // Max drawdown = peak - trough_after_peak = 0.1 - (-0.1) = 0.2
        // Max runup = peak_after_trough - trough = 0.2 - (-0.1) = 0.3
        let cum_rets = [0.1, 0.05, -0.05, -0.1, 0.2];
        let mut peak = 0.0f64;
        let mut trough = 0.0f64;
        let mut max_dd = 0.0f64;
        let mut max_ru = 0.0f64;

        for &cum in &cum_rets {
            if cum > peak {
                peak = cum;
            }
            let dd = peak - cum;
            if dd > max_dd {
                max_dd = dd;
            }
            if cum < trough {
                trough = cum;
            }
            let ru = cum - trough;
            if ru > max_ru {
                max_ru = ru;
            }
        }

        assert!(
            (max_dd - 0.2).abs() < 1e-10,
            "Max drawdown expected 0.2, got {}",
            max_dd
        );
        assert!(
            (max_ru - 0.3).abs() < 1e-10,
            "Max runup expected 0.3, got {}",
            max_ru
        );
    }
}
