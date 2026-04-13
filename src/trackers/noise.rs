//! # NoiseTracker
//!
//! ## Purpose
//!
//! Characterizes microstructure noise using the signature plot method, estimates
//! noise variance, signal-to-noise ratio, and Roll's (1984) implied spread
//! from first-order return autocovariance.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Signature plot | RV at 20 log-spaced scales (0.1s–60s) | dimensionless |
//! | Noise variance | `(RV_fast - RV_slow) / (2 * n_fast)` | dimensionless |
//! | Signal-to-noise ratio | `true_var / noise_var` | dimensionless |
//! | Roll spread | `2 * sqrt(-gamma_1)` where `gamma_1 = autocovariance(1)` | dollars |
//!
//! ## Formulas
//!
//! - Signature plot (Zhang et al. 2005):
//!   Compute `RV(delta) = sum(r_{t,delta}^2)` at multiple timescales delta.
//!   In the presence of noise, RV increases as delta decreases (noise dominates).
//!
//! - Noise variance estimator:
//!   `sigma^2_noise = (RV_fast - RV_slow) / (2 * n_fast)`
//!   Using fastest scale as RV_fast and slowest as RV_slow.
//!
//! - Signal-to-noise ratio:
//!   `SNR = sigma^2_true / sigma^2_noise`
//!   where `sigma^2_true = RV_slow` (assumed less contaminated).
//!
//! - Roll (1984) implied spread:
//!   `S = 2 * sqrt(-gamma_1)` if `gamma_1 < 0`, else `NaN`
//!   where `gamma_1 = cov(r_t, r_{t-1})` (first-order autocovariance of tick returns).
//!
//! ## References
//!
//! - Zhang, L., Mykland, P.A. & Aït-Sahalia, Y. (2005). "A tale of two time
//!   scales: Determining integrated volatility with noisy high-frequency data."
//!   Journal of the American Statistical Association, 100(472), 1394-1411.
//! - Roll, R. (1984). "A simple implicit measure of the effective bid-ask spread
//!   in an efficient market." Journal of Finance, 39(4), 1127-1139.

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::statistics::WelfordAccumulator;
use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;

const NS_PER_SECOND: i64 = 1_000_000_000;

/// Number of scales in the signature plot.
const N_SIGNATURE_SCALES: usize = 20;

/// Microstructure noise analysis tracker.
pub struct NoiseTracker {
    day_timestamps: Vec<i64>,
    day_mid_prices: Vec<f64>,

    signature_scales: Vec<f64>,
    per_scale_rv: Vec<WelfordAccumulator>,

    daily_noise_var: WelfordAccumulator,
    daily_snr: WelfordAccumulator,
    daily_roll_spread: WelfordAccumulator,

    n_days: u32,
}

impl NoiseTracker {
    /// Create a new NoiseTracker with default log-spaced scales (0.1s to 60s).
    pub fn new() -> Self {
        let scales = log_spaced_scales(0.1, 60.0, N_SIGNATURE_SCALES);
        let per_scale_rv = (0..N_SIGNATURE_SCALES)
            .map(|_| WelfordAccumulator::new())
            .collect();

        Self {
            day_timestamps: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            signature_scales: scales,
            per_scale_rv,
            daily_noise_var: WelfordAccumulator::new(),
            daily_snr: WelfordAccumulator::new(),
            daily_roll_spread: WelfordAccumulator::new(),
            n_days: 0,
        }
    }

    fn process_day_noise(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_mid_prices.len() < 10 {
            return;
        }

        let mut scale_rvs = Vec::with_capacity(self.signature_scales.len());

        for (idx, &scale_s) in self.signature_scales.iter().enumerate() {
            let bin_width_ns = (scale_s * NS_PER_SECOND as f64) as i64;
            if bin_width_ns < 1 {
                scale_rvs.push(f64::NAN);
                continue;
            }

            let resampled = resample_to_grid(
                &self.day_timestamps,
                &self.day_mid_prices,
                bin_width_ns,
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
                scale_rvs.push(f64::NAN);
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

            self.per_scale_rv[idx].update(rv);
            scale_rvs.push(rv);
        }

        // Noise variance = (RV_fast - RV_slow) / (2 * n_fast_bins)
        let rv_fast = scale_rvs.first().copied().unwrap_or(f64::NAN);
        let rv_slow = scale_rvs.last().copied().unwrap_or(f64::NAN);

        if rv_fast.is_finite() && rv_slow.is_finite() && rv_fast > rv_slow {
            let bin_width_fast_ns = (self.signature_scales[0] * NS_PER_SECOND as f64) as i64;
            let rth_ns = 6 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
            let n_fast_bins = (rth_ns / bin_width_fast_ns) as f64;

            if n_fast_bins > 0.0 {
                let noise_var = (rv_fast - rv_slow) / (2.0 * n_fast_bins);
                self.daily_noise_var.update(noise_var);

                if noise_var > 1e-20 {
                    let snr = rv_slow / noise_var;
                    self.daily_snr.update(snr);
                }
            }
        }

        // Roll spread from tick-level returns
        self.compute_roll_spread();
    }

    fn compute_roll_spread(&mut self) {
        if self.day_mid_prices.len() < 3 {
            return;
        }

        let mut returns = Vec::with_capacity(self.day_mid_prices.len() - 1);
        for i in 1..self.day_mid_prices.len() {
            if self.day_mid_prices[i] > 0.0 && self.day_mid_prices[i - 1] > 0.0 {
                let ret = (self.day_mid_prices[i] / self.day_mid_prices[i - 1]).ln();
                if ret.is_finite() {
                    returns.push(ret);
                }
            }
        }

        if returns.len() < 3 {
            return;
        }

        // First-order autocovariance: gamma_1 = (1/n) * sum(r_t * r_{t-1})
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let gamma_1: f64 = returns
            .windows(2)
            .map(|w| (w[0] - mean) * (w[1] - mean))
            .sum::<f64>()
            / n;

        // Roll spread: S = 2 * sqrt(-gamma_1) if gamma_1 < 0
        if gamma_1 < -1e-20 {
            let roll_spread = 2.0 * (-gamma_1).sqrt();
            self.daily_roll_spread.update(roll_spread);
        }
    }
}

impl Default for NoiseTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisTracker for NoiseTracker {
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
        self.process_day_noise(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_timestamps.clear();
        self.day_mid_prices.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let signature_plot: Vec<serde_json::Value> = self
            .signature_scales
            .iter()
            .zip(self.per_scale_rv.iter())
            .map(|(&scale, rv)| {
                json!({
                    "scale_seconds": scale,
                    "mean_rv": rv.mean(),
                    "std_rv": rv.std(),
                    "count": rv.count(),
                })
            })
            .collect();

        json!({
            "tracker": "NoiseTracker",
            "n_days": self.n_days,
            "signature_plot": signature_plot,
            "daily_noise_variance": {
                "mean": self.daily_noise_var.mean(),
                "std": self.daily_noise_var.std(),
                "min": self.daily_noise_var.min(),
                "max": self.daily_noise_var.max(),
                "count": self.daily_noise_var.count(),
            },
            "daily_snr": {
                "mean": self.daily_snr.mean(),
                "std": self.daily_snr.std(),
                "min": self.daily_snr.min(),
                "max": self.daily_snr.max(),
                "count": self.daily_snr.count(),
            },
            "daily_roll_spread": {
                "mean": self.daily_roll_spread.mean(),
                "std": self.daily_roll_spread.std(),
                "min": self.daily_roll_spread.min(),
                "max": self.daily_roll_spread.max(),
                "count": self.daily_roll_spread.count(),
            },
        })
    }

    fn name(&self) -> &str {
        "NoiseTracker"
    }
}

/// Generate log-spaced values from `start` to `end` (inclusive) with `n` points.
fn log_spaced_scales(start: f64, end: f64, n: usize) -> Vec<f64> {
    let log_start = start.ln();
    let log_end = end.ln();
    (0..n)
        .map(|i| {
            let frac = i as f64 / (n - 1).max(1) as f64;
            (log_start + frac * (log_end - log_start)).exp()
        })
        .collect()
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
    fn test_log_spaced_scales() {
        let scales = log_spaced_scales(0.1, 60.0, 20);
        assert_eq!(scales.len(), 20);
        assert!((scales[0] - 0.1).abs() < 1e-10);
        assert!((scales[19] - 60.0).abs() < 1e-6);
        for i in 1..scales.len() {
            assert!(
                scales[i] > scales[i - 1],
                "Scales must be monotonically increasing"
            );
        }
    }

    #[test]
    fn test_collects_mid_prices() {
        let mut tracker = NoiseTracker::new();
        let ts_base = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob_with_mid(100_000_000_000);

        tracker.process_event(&make_msg(ts_base), &lob, 3, 0);
        tracker.process_event(&make_msg(ts_base + NS_PER_SECOND), &lob, 3, 0);

        assert_eq!(tracker.day_mid_prices.len(), 2);
    }

    #[test]
    fn test_reset_day_clears() {
        let mut tracker = NoiseTracker::new();
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
        let tracker = NoiseTracker::new();
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "NoiseTracker");
        assert!(report.get("signature_plot").is_some());
        assert!(report.get("daily_noise_variance").is_some());
        assert!(report.get("daily_snr").is_some());
        assert!(report.get("daily_roll_spread").is_some());

        let sig = report["signature_plot"].as_array().unwrap();
        assert_eq!(sig.len(), N_SIGNATURE_SCALES);
    }

    #[test]
    fn test_roll_spread_negative_autocovariance() {
        // With bid-ask bounce, first-order autocovariance of returns should be negative.
        // gamma_1 < 0 → Roll spread is defined.
        let returns = vec![0.01, -0.01, 0.01, -0.01, 0.01, -0.01];
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let gamma_1: f64 = returns
            .windows(2)
            .map(|w| (w[0] - mean) * (w[1] - mean))
            .sum::<f64>()
            / n;

        assert!(
            gamma_1 < 0.0,
            "Alternating returns should produce negative gamma_1, got {}",
            gamma_1
        );
        let roll = 2.0 * (-gamma_1).sqrt();
        assert!(roll > 0.0, "Roll spread should be positive, got {}", roll);
    }

    #[test]
    fn test_roll_spread_alternating_exact() {
        // Returns: [0.01, -0.01, 0.01, -0.01, 0.01, -0.01] (6 values)
        // mean = 0
        // gamma_1 = sum((r_t - mean)(r_{t+1} - mean)) / n
        //         = sum(r_t * r_{t+1}) / 6
        //         = 5 * (0.01 * -0.01) / 6
        //         = 5 * (-0.0001) / 6 = -0.0005 / 6
        //         = -0.000083333...
        // Roll = 2 * sqrt(-gamma_1) = 2 * sqrt(0.000083333...)
        //      = 2 * 0.009129 = 0.018257...
        let returns = [0.01_f64, -0.01, 0.01, -0.01, 0.01, -0.01];
        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let gamma_1: f64 = returns
            .windows(2)
            .map(|w| (w[0] - mean) * (w[1] - mean))
            .sum::<f64>()
            / n;

        let expected_gamma = -0.0005 / 6.0;
        assert!(
            (gamma_1 - expected_gamma).abs() < 1e-15,
            "gamma_1 expected {}, got {}",
            expected_gamma,
            gamma_1
        );

        let roll = 2.0 * (-gamma_1).sqrt();
        let expected_roll = 2.0 * (0.0005 / 6.0_f64).sqrt();
        assert!(
            (roll - expected_roll).abs() < 1e-10,
            "Roll spread expected {}, got {}",
            expected_roll,
            roll
        );
    }
}
