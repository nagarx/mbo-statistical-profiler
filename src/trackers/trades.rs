//! # TradeTracker
//!
//! ## Purpose
//!
//! Characterizes trade flow: size distributions, arrival rates by regime,
//! directional trade sizes (buyer vs seller initiated), and trade price
//! level classification. Essential for understanding informed trading
//! and market-taking behavior.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Trade size distribution | Full distribution of trade sizes | shares |
//! | Trade rate by regime | trades per second per regime | trades/s |
//! | Buyer-initiated size dist | Distribution for buyer trades | shares |
//! | Seller-initiated size dist | Distribution for seller trades | shares |
//! | Trade price classification | at_bid, at_ask, inside, outside counts | count |
//!
//! ## Formulas
//!
//! - Trade rate per regime:
//!   `rate_r = n_trades_r / duration_r_seconds`
//!
//! - Price classification (given trade price P, best bid B, best ask A):
//!   - `at_bid`: P == B
//!   - `at_ask`: P == A
//!   - `inside`: B < P < A
//!   - `outside`: P < B or P > A
//!
//! ## References
//!
//! - Kyle, A.S. (1985). "Continuous auctions and insider trading."
//!   Econometrica, 53(6), 1315-1335.
//! - Lee, C.M.C. & Ready, M.J. (1991). "Inferring trade direction from
//!   intraday data." Journal of Finance, 46(2), 733-746.

use mbo_lob_reconstructor::{Action, BookConsistency, LobState, MboMessage, Side};
use serde_json::json;

use crate::statistics::{
    IntradayCurveAccumulator, RegimeAccumulator, StreamingDistribution, WelfordAccumulator,
};
use crate::time::N_REGIMES;
use crate::AnalysisTracker;

const NS_PER_SECOND_F64: f64 = 1_000_000_000.0;

/// Default cluster gap threshold (1 second).
const DEFAULT_CLUSTER_GAP_NS: i64 = 1_000_000_000;

/// Trade flow analysis tracker.
pub struct TradeTracker {
    trade_size_dist: StreamingDistribution,
    buyer_size_dist: StreamingDistribution,
    seller_size_dist: StreamingDistribution,

    regime_trade_size: RegimeAccumulator,
    regime_trade_count: [u64; N_REGIMES],

    at_bid_count: u64,
    at_ask_count: u64,
    inside_count: u64,
    outside_count: u64,

    trade_value: WelfordAccumulator,
    total_trades: u64,
    total_volume: u64,

    // Trade-through: trades outside the BBO
    trade_through_count: u64,
    regime_trade_through: [u64; N_REGIMES],

    // Inter-trade time distribution
    inter_trade_time_dist: StreamingDistribution,
    prev_trade_ts: Option<i64>,

    // Trade clustering
    cluster_gap_ns: i64,
    current_cluster_size: u64,
    total_clustered_trades: u64,
    total_clusters: u64,
    max_cluster_size: u64,
    cluster_sizes: WelfordAccumulator,

    // Large trade impact
    large_trade_threshold_size: f64, // computed from 95th percentile after first day
    large_trade_impact_dist: StreamingDistribution,
    large_trade_buyer_count: u64,
    large_trade_seller_count: u64,

    intraday_trade_rate_curve: IntradayCurveAccumulator,
    day_trade_timestamps: Vec<i64>,

    n_days: u32,
}

impl TradeTracker {
    pub fn new() -> Self {
        Self {
            trade_size_dist: StreamingDistribution::new(10_000),
            buyer_size_dist: StreamingDistribution::new(10_000),
            seller_size_dist: StreamingDistribution::new(10_000),
            regime_trade_size: RegimeAccumulator::new(),
            regime_trade_count: [0u64; N_REGIMES],
            at_bid_count: 0,
            at_ask_count: 0,
            inside_count: 0,
            outside_count: 0,
            trade_value: WelfordAccumulator::new(),
            total_trades: 0,
            total_volume: 0,
            trade_through_count: 0,
            regime_trade_through: [0u64; N_REGIMES],
            inter_trade_time_dist: StreamingDistribution::new(10_000),
            prev_trade_ts: None,
            cluster_gap_ns: DEFAULT_CLUSTER_GAP_NS,
            current_cluster_size: 0,
            total_clustered_trades: 0,
            total_clusters: 0,
            max_cluster_size: 0,
            cluster_sizes: WelfordAccumulator::new(),
            large_trade_threshold_size: f64::MAX,
            large_trade_impact_dist: StreamingDistribution::new(10_000),
            large_trade_buyer_count: 0,
            large_trade_seller_count: 0,
            intraday_trade_rate_curve: IntradayCurveAccumulator::new_rth_1min(),
            day_trade_timestamps: Vec::with_capacity(2_000_000),
            n_days: 0,
        }
    }

    fn classify_trade_price(&mut self, trade_price: i64, best_bid: i64, best_ask: i64) {
        if trade_price == best_bid {
            self.at_bid_count += 1;
        } else if trade_price == best_ask {
            self.at_ask_count += 1;
        } else if trade_price > best_bid && trade_price < best_ask {
            self.inside_count += 1;
        } else {
            self.outside_count += 1;
        }
    }
}

impl Default for TradeTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisTracker for TradeTracker {
    fn process_event(
        &mut self,
        msg: &MboMessage,
        lob_state: &LobState,
        regime: u8,
        _day_epoch_ns: i64,
    ) {
        let is_trade = matches!(msg.action, Action::Trade | Action::Fill);
        if !is_trade {
            return;
        }

        let size = msg.size as f64;
        let ts = msg.timestamp.unwrap_or(0);
        self.total_trades += 1;
        self.total_volume += msg.size as u64;
        if ts > 0 {
            self.day_trade_timestamps.push(ts);
        }

        self.trade_size_dist.add(size);
        self.regime_trade_size.add(regime, size);

        if (regime as usize) < N_REGIMES {
            self.regime_trade_count[regime as usize] += 1;
        }

        match msg.side {
            Side::Bid => self.buyer_size_dist.add(size),
            Side::Ask => self.seller_size_dist.add(size),
            Side::None => {}
        }

        let value = msg.price_as_f64() * size;
        if value.is_finite() {
            self.trade_value.update(value);
        }

        if let (Some(best_bid), Some(best_ask)) = (lob_state.best_bid, lob_state.best_ask) {
            self.classify_trade_price(msg.price, best_bid, best_ask);

            // Trade-through: trade price outside BBO
            if msg.price < best_bid || msg.price > best_ask {
                self.trade_through_count += 1;
                if (regime as usize) < N_REGIMES {
                    self.regime_trade_through[regime as usize] += 1;
                }
            }

            // Large trade impact: |trade_price - mid| / mid * 10000 bps
            if lob_state.check_consistency() == BookConsistency::Valid {
                if let Some(mid) = lob_state.mid_price() {
                    if mid > 0.0 && size >= self.large_trade_threshold_size {
                        let trade_price = msg.price as f64 / 1e9;
                        let impact_bps = 2.0 * (trade_price - mid).abs() / mid * 10_000.0;
                        self.large_trade_impact_dist.add(impact_bps);
                        match msg.side {
                            Side::Bid => self.large_trade_buyer_count += 1,
                            Side::Ask => self.large_trade_seller_count += 1,
                            _ => {}
                        }
                    }
                }
            }
        }

        // Inter-trade time
        if let Some(prev_ts) = self.prev_trade_ts {
            if ts > prev_ts {
                let gap_ns = ts - prev_ts;
                let gap_s = gap_ns as f64 / NS_PER_SECOND_F64;
                self.inter_trade_time_dist.add(gap_s);

                // Clustering: gap < threshold → same cluster
                if gap_ns < self.cluster_gap_ns {
                    self.current_cluster_size += 1;
                } else {
                    if self.current_cluster_size > 1 {
                        self.cluster_sizes.update(self.current_cluster_size as f64);
                        self.total_clustered_trades += self.current_cluster_size;
                        self.total_clusters += 1;
                        if self.current_cluster_size > self.max_cluster_size {
                            self.max_cluster_size = self.current_cluster_size;
                        }
                    }
                    self.current_cluster_size = 1;
                }
            }
        } else {
            self.current_cluster_size = 1;
        }
        self.prev_trade_ts = Some(ts);
    }

    fn end_of_day(&mut self, _day_index: u32) {
        // Finalize last cluster of the day
        if self.current_cluster_size > 1 {
            self.cluster_sizes.update(self.current_cluster_size as f64);
            self.total_clustered_trades += self.current_cluster_size;
            self.total_clusters += 1;
            if self.current_cluster_size > self.max_cluster_size {
                self.max_cluster_size = self.current_cluster_size;
            }
        }

        // After first day, set large trade threshold from 95th percentile
        if self.n_days == 0 {
            self.large_trade_threshold_size = self.trade_size_dist.percentile(95.0);
            if self.large_trade_threshold_size.is_nan() || self.large_trade_threshold_size < 1.0 {
                self.large_trade_threshold_size = f64::MAX;
            }
        }

        if !self.day_trade_timestamps.is_empty() {
            let utc_offset =
                crate::time::regime::infer_utc_offset(&self.day_trade_timestamps);
            for &ts in &self.day_trade_timestamps {
                self.intraday_trade_rate_curve.add(ts, 1.0, utc_offset);
            }
        }

        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.prev_trade_ts = None;
        self.current_cluster_size = 0;
        self.day_trade_timestamps.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let total_classified =
            self.at_bid_count + self.at_ask_count + self.inside_count + self.outside_count;
        let pct = |count: u64| -> f64 {
            if total_classified > 0 {
                count as f64 / total_classified as f64 * 100.0
            } else {
                0.0
            }
        };

        let mean_trades_per_day = if self.n_days > 0 {
            self.total_trades as f64 / self.n_days as f64
        } else {
            0.0
        };

        let mean_volume_per_day = if self.n_days > 0 {
            self.total_volume as f64 / self.n_days as f64
        } else {
            0.0
        };

        let trade_through_pct = if self.total_trades > 0 {
            self.trade_through_count as f64 / self.total_trades as f64 * 100.0
        } else {
            0.0
        };

        let cluster_fraction = if self.total_trades > 0 {
            self.total_clustered_trades as f64 / self.total_trades as f64
        } else {
            0.0
        };

        json!({
            "tracker": "TradeTracker",
            "n_days": self.n_days,
            "total_trades": self.total_trades,
            "total_volume": self.total_volume,
            "mean_trades_per_day": mean_trades_per_day,
            "mean_volume_per_day": mean_volume_per_day,
            "trade_size_distribution": self.trade_size_dist.summary(),
            "buyer_initiated_size": self.buyer_size_dist.summary(),
            "seller_initiated_size": self.seller_size_dist.summary(),
            "trade_value": {
                "mean": self.trade_value.mean(),
                "std": self.trade_value.std(),
                "min": self.trade_value.min(),
                "max": self.trade_value.max(),
                "count": self.trade_value.count(),
            },
            "price_level_classification": {
                "at_bid_count": self.at_bid_count,
                "at_ask_count": self.at_ask_count,
                "inside_count": self.inside_count,
                "outside_count": self.outside_count,
                "at_bid_pct": pct(self.at_bid_count),
                "at_ask_pct": pct(self.at_ask_count),
                "inside_pct": pct(self.inside_count),
                "outside_pct": pct(self.outside_count),
            },
            "trade_through": {
                "total_count": self.trade_through_count,
                "pct": trade_through_pct,
            },
            "inter_trade_time": self.inter_trade_time_dist.summary(),
            "clustering": {
                "cluster_gap_seconds": self.cluster_gap_ns as f64 / NS_PER_SECOND_F64,
                "total_clusters": self.total_clusters,
                "total_clustered_trades": self.total_clustered_trades,
                "cluster_fraction": cluster_fraction,
                "max_cluster_size": self.max_cluster_size,
                "mean_cluster_size": self.cluster_sizes.mean(),
            },
            "large_trade_impact": {
                "threshold_size": self.large_trade_threshold_size,
                "impact_bps": self.large_trade_impact_dist.summary(),
                "buyer_count": self.large_trade_buyer_count,
                "seller_count": self.large_trade_seller_count,
            },
            "regime_trade_size": self.regime_trade_size.finalize(),
            "intraday_trade_rate_curve": self.intraday_trade_rate_curve.finalize()
                .into_iter()
                .filter(|b| b.count > 0)
                .map(|b| json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_trade_count": b.mean,
                    "total_trade_count": b.count,
                }))
                .collect::<Vec<_>>(),
        })
    }

    fn name(&self) -> &str {
        "TradeTracker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trade_msg(side: Side, price: i64, size: u32) -> MboMessage {
        MboMessage::new(1, Action::Trade, side, price, size).with_timestamp(1_000_000_000)
    }

    fn make_lob() -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    #[test]
    fn test_counts_trades() {
        let mut tracker = TradeTracker::new();
        let lob = make_lob();

        tracker.process_event(
            &make_trade_msg(Side::Bid, 100_000_000_000, 50),
            &lob,
            3,
            0,
        );
        tracker.process_event(
            &make_trade_msg(Side::Ask, 100_010_000_000, 100),
            &lob,
            3,
            0,
        );

        assert_eq!(tracker.total_trades, 2);
        assert_eq!(tracker.total_volume, 150);
    }

    #[test]
    fn test_ignores_non_trades() {
        let mut tracker = TradeTracker::new();
        let lob = make_lob();
        let add_msg =
            MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100)
                .with_timestamp(1_000_000_000);

        tracker.process_event(&add_msg, &lob, 3, 0);
        assert_eq!(tracker.total_trades, 0);
    }

    #[test]
    fn test_price_classification() {
        let mut tracker = TradeTracker::new();
        let lob = make_lob(); // bid=100, ask=100.01

        // At bid
        tracker.process_event(
            &make_trade_msg(Side::Bid, 100_000_000_000, 10),
            &lob,
            3,
            0,
        );
        // At ask
        tracker.process_event(
            &make_trade_msg(Side::Ask, 100_010_000_000, 10),
            &lob,
            3,
            0,
        );
        // Inside
        tracker.process_event(
            &make_trade_msg(Side::None, 100_005_000_000, 10),
            &lob,
            3,
            0,
        );
        // Outside (below bid)
        tracker.process_event(
            &make_trade_msg(Side::Bid, 99_990_000_000, 10),
            &lob,
            3,
            0,
        );

        assert_eq!(tracker.at_bid_count, 1);
        assert_eq!(tracker.at_ask_count, 1);
        assert_eq!(tracker.inside_count, 1);
        assert_eq!(tracker.outside_count, 1);
    }

    #[test]
    fn test_directional_size_tracking() {
        let mut tracker = TradeTracker::new();
        let lob = make_lob();

        tracker.process_event(
            &make_trade_msg(Side::Bid, 100_000_000_000, 50),
            &lob,
            3,
            0,
        );
        tracker.process_event(
            &make_trade_msg(Side::Ask, 100_010_000_000, 200),
            &lob,
            3,
            0,
        );

        assert_eq!(tracker.buyer_size_dist.count(), 1);
        assert_eq!(tracker.seller_size_dist.count(), 1);
        assert!((tracker.buyer_size_dist.mean() - 50.0).abs() < 1e-10);
        assert!((tracker.seller_size_dist.mean() - 200.0).abs() < 1e-10);
    }

    #[test]
    fn test_finalize_structure() {
        let mut tracker = TradeTracker::new();
        let lob = make_lob();
        tracker.process_event(
            &make_trade_msg(Side::Bid, 100_000_000_000, 100),
            &lob,
            3,
            0,
        );
        tracker.end_of_day(0);

        let report = tracker.finalize();
        assert_eq!(report["tracker"], "TradeTracker");
        assert!(report.get("trade_size_distribution").is_some());
        assert!(report.get("buyer_initiated_size").is_some());
        assert!(report.get("seller_initiated_size").is_some());
        assert!(report.get("price_level_classification").is_some());
        assert!(report.get("regime_trade_size").is_some());
        assert!(
            report.get("intraday_trade_rate_curve").is_some(),
            "finalize must include intraday_trade_rate_curve"
        );
    }

    #[test]
    fn test_price_classification_counts_exact() {
        // bid=$100.00 (100_000_000_000), ask=$100.01 (100_010_000_000)
        // 2 at bid, 3 at ask, 1 inside, 1 outside
        // Total classified = 7
        // at_bid_pct = 2/7 * 100 ≈ 28.57%
        // at_ask_pct = 3/7 * 100 ≈ 42.86%
        let mut tracker = TradeTracker::new();
        let lob = make_lob();

        // 2 trades at bid
        for _ in 0..2 {
            tracker.process_event(
                &make_trade_msg(Side::Bid, 100_000_000_000, 10),
                &lob,
                3,
                0,
            );
        }
        // 3 trades at ask
        for _ in 0..3 {
            tracker.process_event(
                &make_trade_msg(Side::Ask, 100_010_000_000, 10),
                &lob,
                3,
                0,
            );
        }
        // 1 inside spread
        tracker.process_event(
            &make_trade_msg(Side::None, 100_005_000_000, 10),
            &lob,
            3,
            0,
        );
        // 1 outside (above ask)
        tracker.process_event(
            &make_trade_msg(Side::Bid, 100_020_000_000, 10),
            &lob,
            3,
            0,
        );

        assert_eq!(tracker.at_bid_count, 2, "Expected 2 at-bid trades");
        assert_eq!(tracker.at_ask_count, 3, "Expected 3 at-ask trades");
        assert_eq!(tracker.inside_count, 1, "Expected 1 inside trade");
        assert_eq!(tracker.outside_count, 1, "Expected 1 outside trade");
        assert_eq!(tracker.total_trades, 7, "Expected 7 total trades");

        let report = tracker.finalize();
        let at_bid_pct = report["price_level_classification"]["at_bid_pct"]
            .as_f64()
            .unwrap();
        let expected_pct = 2.0 / 7.0 * 100.0;
        assert!(
            (at_bid_pct - expected_pct).abs() < 1e-10,
            "at_bid_pct expected {}, got {}",
            expected_pct,
            at_bid_pct
        );
    }
}
