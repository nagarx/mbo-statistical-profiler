//! # DepthTracker
//!
//! ## Purpose
//!
//! Characterizes order book depth structure, imbalance distributions,
//! L1 concentration, and depth stability across regimes. Provides the
//! statistical foundation for understanding liquidity provision patterns.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Mean depth profile | Mean bid/ask sizes at each of 10 levels | shares |
//! | Depth imbalance dist | `(bid_vol - ask_vol) / (bid_vol + ask_vol)` | ratio [-1, 1] |
//! | L1 concentration | `L1_volume / total_volume` | ratio [0, 1] |
//! | Depth stability (CV) | `std(total_depth) / mean(total_depth)` | dimensionless |
//! | Regime-conditional depth | Depth imbalance by regime | ratio |
//!
//! ## Formulas
//!
//! - Depth imbalance (per snapshot):
//!   `DI = (sum(bid_sizes) - sum(ask_sizes)) / (sum(bid_sizes) + sum(ask_sizes))`
//!
//! - L1 concentration:
//!   `C_L1 = (bid_size[0] + ask_size[0]) / (total_bid_vol + total_ask_vol)`
//!
//! - Coefficient of variation:
//!   `CV = std(X) / mean(X)`
//!
//! ## References
//!
//! - Cao, C., Hansch, O. & Wang, X. (2009). "The information content of an
//!   open limit-order book." Journal of Futures Markets, 29(1), 16-41.

use mbo_lob_reconstructor::{LobState, MboMessage};
use serde_json::json;

use crate::statistics::{RegimeAccumulator, StreamingDistribution, WelfordAccumulator};
use crate::AnalysisTracker;

const DEPTH_PROFILE_LEVELS: usize = 10;

/// Order book depth analysis tracker.
pub struct DepthTracker {
    bid_depth_profile: [WelfordAccumulator; DEPTH_PROFILE_LEVELS],
    ask_depth_profile: [WelfordAccumulator; DEPTH_PROFILE_LEVELS],

    depth_imbalance_dist: StreamingDistribution,
    l1_concentration: WelfordAccumulator,
    total_depth: WelfordAccumulator,

    regime_imbalance: RegimeAccumulator,
    regime_total_depth: RegimeAccumulator,

    n_snapshots: u64,
    n_days: u32,
}

impl DepthTracker {
    pub fn new() -> Self {
        Self {
            bid_depth_profile: std::array::from_fn(|_| WelfordAccumulator::new()),
            ask_depth_profile: std::array::from_fn(|_| WelfordAccumulator::new()),
            depth_imbalance_dist: StreamingDistribution::new(10_000),
            l1_concentration: WelfordAccumulator::new(),
            total_depth: WelfordAccumulator::new(),
            regime_imbalance: RegimeAccumulator::new(),
            regime_total_depth: RegimeAccumulator::new(),
            n_snapshots: 0,
            n_days: 0,
        }
    }
}

impl Default for DepthTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisTracker for DepthTracker {
    fn process_event(
        &mut self,
        _msg: &MboMessage,
        lob_state: &LobState,
        regime: u8,
        _day_epoch_ns: i64,
    ) {
        if !lob_state.is_valid() {
            return;
        }

        self.n_snapshots += 1;

        let levels = lob_state.levels.min(DEPTH_PROFILE_LEVELS);
        for i in 0..levels {
            self.bid_depth_profile[i].update(lob_state.bid_sizes[i] as f64);
            self.ask_depth_profile[i].update(lob_state.ask_sizes[i] as f64);
        }

        let bid_vol = lob_state.total_bid_volume() as f64;
        let ask_vol = lob_state.total_ask_volume() as f64;
        let total_vol = bid_vol + ask_vol;

        if total_vol > 0.0 {
            let imbalance = (bid_vol - ask_vol) / total_vol;
            self.depth_imbalance_dist.add(imbalance);
            self.regime_imbalance.add(regime, imbalance);
        }

        if total_vol > 0.0 {
            let l1_vol = lob_state.bid_sizes[0] as f64 + lob_state.ask_sizes[0] as f64;
            self.l1_concentration.update(l1_vol / total_vol);
        }

        self.total_depth.update(total_vol);
        self.regime_total_depth.add(regime, total_vol);
    }

    fn end_of_day(&mut self, _day_index: u32) {
        self.n_days += 1;
    }

    fn reset_day(&mut self) {}

    fn finalize(&self) -> serde_json::Value {
        let bid_profile: Vec<serde_json::Value> = self
            .bid_depth_profile
            .iter()
            .enumerate()
            .map(|(i, acc)| {
                json!({
                    "level": i,
                    "mean_size": acc.mean(),
                    "std_size": acc.std(),
                    "count": acc.count(),
                })
            })
            .collect();

        let ask_profile: Vec<serde_json::Value> = self
            .ask_depth_profile
            .iter()
            .enumerate()
            .map(|(i, acc)| {
                json!({
                    "level": i,
                    "mean_size": acc.mean(),
                    "std_size": acc.std(),
                    "count": acc.count(),
                })
            })
            .collect();

        let depth_cv = if self.total_depth.mean() > 1e-10 {
            self.total_depth.std() / self.total_depth.mean()
        } else {
            f64::NAN
        };

        json!({
            "tracker": "DepthTracker",
            "n_days": self.n_days,
            "n_snapshots": self.n_snapshots,
            "bid_depth_profile": bid_profile,
            "ask_depth_profile": ask_profile,
            "depth_imbalance": self.depth_imbalance_dist.summary(),
            "l1_concentration": {
                "mean": self.l1_concentration.mean(),
                "std": self.l1_concentration.std(),
                "min": self.l1_concentration.min(),
                "max": self.l1_concentration.max(),
                "count": self.l1_concentration.count(),
            },
            "total_depth": {
                "mean": self.total_depth.mean(),
                "std": self.total_depth.std(),
                "min": self.total_depth.min(),
                "max": self.total_depth.max(),
                "cv": depth_cv,
                "count": self.total_depth.count(),
            },
            "regime_depth_imbalance": self.regime_imbalance.finalize(),
            "regime_total_depth": self.regime_total_depth.finalize(),
        })
    }

    fn name(&self) -> &str {
        "DepthTracker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::{Action, Side};

    fn make_msg() -> MboMessage {
        MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100)
            .with_timestamp(1_000_000_000)
    }

    fn make_lob_symmetric() -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        for i in 0..10 {
            lob.bid_sizes[i] = 100;
            lob.ask_sizes[i] = 100;
            lob.bid_prices[i] = 100_000_000_000 - (i as i64 * 10_000_000);
            lob.ask_prices[i] = 100_010_000_000 + (i as i64 * 10_000_000);
        }
        lob
    }

    fn make_lob_imbalanced() -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        for i in 0..10 {
            lob.bid_sizes[i] = 200;
            lob.ask_sizes[i] = 100;
            lob.bid_prices[i] = 100_000_000_000 - (i as i64 * 10_000_000);
            lob.ask_prices[i] = 100_010_000_000 + (i as i64 * 10_000_000);
        }
        lob
    }

    #[test]
    fn test_symmetric_book_zero_imbalance() {
        let mut tracker = DepthTracker::new();
        let lob = make_lob_symmetric();

        tracker.process_event(&make_msg(), &lob, 3, 0);

        assert_eq!(tracker.n_snapshots, 1);
        assert!(
            (tracker.depth_imbalance_dist.mean() - 0.0).abs() < 1e-10,
            "Symmetric book should have zero imbalance, got {}",
            tracker.depth_imbalance_dist.mean()
        );
    }

    #[test]
    fn test_imbalanced_book_positive() {
        let mut tracker = DepthTracker::new();
        let lob = make_lob_imbalanced();

        tracker.process_event(&make_msg(), &lob, 3, 0);

        let imbalance = tracker.depth_imbalance_dist.mean();
        assert!(
            imbalance > 0.0,
            "More bids than asks should give positive imbalance, got {}",
            imbalance
        );
        // (2000 - 1000) / (2000 + 1000) = 1/3
        assert!(
            (imbalance - 1.0 / 3.0).abs() < 1e-10,
            "Expected imbalance 1/3, got {}",
            imbalance
        );
    }

    #[test]
    fn test_l1_concentration() {
        let mut tracker = DepthTracker::new();
        let lob = make_lob_symmetric();

        tracker.process_event(&make_msg(), &lob, 3, 0);

        // L1 = 200, Total = 2000, concentration = 0.1
        let l1_conc = tracker.l1_concentration.mean();
        assert!(
            (l1_conc - 0.1).abs() < 1e-10,
            "L1 concentration expected 0.1, got {}",
            l1_conc
        );
    }

    #[test]
    fn test_finalize_structure() {
        let mut tracker = DepthTracker::new();
        let lob = make_lob_symmetric();
        tracker.process_event(&make_msg(), &lob, 3, 0);
        tracker.end_of_day(0);

        let report = tracker.finalize();
        assert_eq!(report["tracker"], "DepthTracker");
        assert!(report.get("bid_depth_profile").is_some());
        assert!(report.get("ask_depth_profile").is_some());
        assert!(report.get("depth_imbalance").is_some());
        assert!(report.get("l1_concentration").is_some());
        assert!(report.get("total_depth").is_some());
        assert!(report.get("regime_depth_imbalance").is_some());
    }

    #[test]
    fn test_empty_book_skipped() {
        let mut tracker = DepthTracker::new();
        let lob = LobState::new(10);

        tracker.process_event(&make_msg(), &lob, 3, 0);
        assert_eq!(tracker.n_snapshots, 0);
    }

    #[test]
    fn test_depth_imbalance_exact() {
        // bid_vol=300, ask_vol=200
        // DI = (300 - 200) / (300 + 200) = 100 / 500 = 0.2
        let mut tracker = DepthTracker::new();
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        lob.bid_sizes[0] = 300;
        lob.ask_sizes[0] = 200;
        for i in 0..10 {
            lob.bid_prices[i] = 100_000_000_000 - (i as i64 * 10_000_000);
            lob.ask_prices[i] = 100_010_000_000 + (i as i64 * 10_000_000);
        }

        tracker.process_event(&make_msg(), &lob, 3, 0);

        let imbalance = tracker.depth_imbalance_dist.mean();
        assert!(
            (imbalance - 0.2).abs() < 1e-10,
            "DI = (300-200)/500 = 0.2, got {}",
            imbalance
        );
    }

    #[test]
    fn test_l1_concentration_exact() {
        // L1 = bid_sizes[0] + ask_sizes[0] = 50 + 50 = 100
        // total = (50+200) + (50+200) = 500
        // C = 100 / 500 = 0.2
        let mut tracker = DepthTracker::new();
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        lob.bid_sizes[0] = 50;
        lob.ask_sizes[0] = 50;
        lob.bid_sizes[1] = 200;
        lob.ask_sizes[1] = 200;
        for i in 0..10 {
            lob.bid_prices[i] = 100_000_000_000 - (i as i64 * 10_000_000);
            lob.ask_prices[i] = 100_010_000_000 + (i as i64 * 10_000_000);
        }

        tracker.process_event(&make_msg(), &lob, 3, 0);

        let l1_conc = tracker.l1_concentration.mean();
        assert!(
            (l1_conc - 0.2).abs() < 1e-10,
            "L1 concentration = 100/500 = 0.2, got {}",
            l1_conc
        );
    }
}
