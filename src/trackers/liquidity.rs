//! # LiquidityTracker
//!
//! ## Purpose
//!
//! Measures execution quality and liquidity cost through effective spread,
//! volume-weighted spread, and microprice deviation. These metrics quantify
//! the true cost of trading beyond the quoted spread.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Effective spread | `2 * \|trade_price - mid_before\| / mid_before * 10000` | bps |
//! | Volume-weighted spread | `sum(spread * trade_size) / sum(trade_size)` | bps |
//! | Microprice deviation | `\|microprice - mid\| / mid * 10000` | bps |
//!
//! ## Formulas
//!
//! - Effective spread (Kyle 1985):
//!   `ES_i = 2 * d_i * (P_i - M_i) / M_i * 10000`
//!   where `d_i = +1` for buyer-initiated, `-1` for seller-initiated,
//!   `P_i` = trade price, `M_i` = midpoint before trade.
//!   Unsigned version: `ES_i = 2 * |P_i - M_i| / M_i * 10000`
//!
//! - Volume-weighted effective spread:
//!   `VWES = sum(ES_i * size_i) / sum(size_i)`
//!
//! - Microprice deviation:
//!   `MPD = |microprice - mid| / mid * 10000` (bps)
//!   Indicates degree of asymmetry in liquidity provision.
//!
//! ## References
//!
//! - Kyle, A.S. (1985). "Continuous auctions and insider trading."
//!   Econometrica, 53(6), 1315-1335.
//! - Amihud, Y. (2002). "Illiquidity and stock returns: cross-section and
//!   time-series effects." Journal of Financial Markets, 5(1), 31-56.

use mbo_lob_reconstructor::{Action, BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::statistics::{StreamingDistribution, WelfordAccumulator};
use crate::AnalysisTracker;

/// Liquidity cost and execution quality tracker.
pub struct LiquidityTracker {
    effective_spread_bps: StreamingDistribution,
    vw_spread_sum_bps: f64,
    vw_spread_vol_sum: f64,

    microprice_deviation_bps: WelfordAccumulator,

    daily_vwes: WelfordAccumulator,
    day_vw_spread_sum: f64,
    day_vw_vol_sum: f64,

    n_trade_events: u64,
    n_microprice_obs: u64,
    n_days: u32,
}

impl LiquidityTracker {
    pub fn new() -> Self {
        Self {
            effective_spread_bps: StreamingDistribution::new(10_000),
            vw_spread_sum_bps: 0.0,
            vw_spread_vol_sum: 0.0,
            microprice_deviation_bps: WelfordAccumulator::new(),
            daily_vwes: WelfordAccumulator::new(),
            day_vw_spread_sum: 0.0,
            day_vw_vol_sum: 0.0,
            n_trade_events: 0,
            n_microprice_obs: 0,
            n_days: 0,
        }
    }
}

impl Default for LiquidityTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisTracker for LiquidityTracker {
    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, _regime: u8) {
        if lob_state.check_consistency() != BookConsistency::Valid {
            return;
        }
        // Microprice deviation: measured on every valid snapshot
        if let (Some(mid), Some(microprice)) = (lob_state.mid_price(), lob_state.microprice()) {
            if mid > 0.0 {
                let deviation = ((microprice - mid).abs() / mid) * 10_000.0;
                if deviation.is_finite() {
                    self.microprice_deviation_bps.update(deviation);
                    self.n_microprice_obs += 1;
                }
            }
        }

        // Effective spread: only on trade events
        let is_trade = matches!(msg.action, Action::Trade | Action::Fill);
        if !is_trade {
            return;
        }

        let mid = match lob_state.mid_price() {
            Some(m) if m > 0.0 => m,
            _ => return,
        };

        let trade_price = msg.price_as_f64();
        if trade_price <= 0.0 {
            return;
        }

        // ES = 2 * |P - M| / M * 10000 (bps)
        let es_bps = 2.0 * (trade_price - mid).abs() / mid * 10_000.0;
        if !es_bps.is_finite() {
            return;
        }

        self.effective_spread_bps.add(es_bps);
        self.n_trade_events += 1;

        let size = msg.size as f64;
        self.vw_spread_sum_bps += es_bps * size;
        self.vw_spread_vol_sum += size;

        self.day_vw_spread_sum += es_bps * size;
        self.day_vw_vol_sum += size;
    }

    fn end_of_day(&mut self) {
        if self.day_vw_vol_sum > 0.0 {
            let daily_vwes = self.day_vw_spread_sum / self.day_vw_vol_sum;
            self.daily_vwes.update(daily_vwes);
        }
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_vw_spread_sum = 0.0;
        self.day_vw_vol_sum = 0.0;
    }

    fn finalize(&self) -> serde_json::Value {
        let vw_effective_spread = if self.vw_spread_vol_sum > 0.0 {
            self.vw_spread_sum_bps / self.vw_spread_vol_sum
        } else {
            f64::NAN
        };

        json!({
            "tracker": "LiquidityTracker",
            "n_days": self.n_days,
            "n_trade_events": self.n_trade_events,
            "n_microprice_observations": self.n_microprice_obs,
            "effective_spread_bps": self.effective_spread_bps.summary(),
            "volume_weighted_effective_spread_bps": vw_effective_spread,
            "daily_vw_effective_spread_bps": {
                "mean": self.daily_vwes.mean(),
                "std": self.daily_vwes.std(),
                "min": self.daily_vwes.min(),
                "max": self.daily_vwes.max(),
                "count": self.daily_vwes.count(),
            },
            "microprice_deviation_bps": {
                "mean": self.microprice_deviation_bps.mean(),
                "std": self.microprice_deviation_bps.std(),
                "min": self.microprice_deviation_bps.min(),
                "max": self.microprice_deviation_bps.max(),
                "count": self.microprice_deviation_bps.count(),
            },
        })
    }

    fn name(&self) -> &str {
        "LiquidityTracker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::Side;

    fn make_trade_msg(price: i64, size: u32) -> MboMessage {
        MboMessage::new(1, Action::Trade, Side::Bid, price, size).with_timestamp(1_000_000_000)
    }

    fn make_lob_with_mid_and_sizes(
        mid_nanodollars: i64,
        half_spread: i64,
        bid_size: u32,
        ask_size: u32,
    ) -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(mid_nanodollars - half_spread);
        lob.best_ask = Some(mid_nanodollars + half_spread);
        lob.bid_sizes[0] = bid_size;
        lob.ask_sizes[0] = ask_size;
        lob
    }

    #[test]
    fn test_effective_spread_at_midpoint() {
        let mut tracker = LiquidityTracker::new();
        // Trade exactly at midpoint: ES should be 0
        let mid = 100_000_000_000i64;
        let half_spread = 5_000_000i64; // $0.005
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        tracker.process_event(&make_trade_msg(mid, 100), &lob, 3);

        let es = tracker.effective_spread_bps.mean();
        assert!(
            es.abs() < 1e-6,
            "Trade at mid should have ~0 effective spread, got {}",
            es
        );
    }

    #[test]
    fn test_effective_spread_at_ask() {
        let mut tracker = LiquidityTracker::new();
        let mid = 100_000_000_000i64; // $100.00
        let half_spread = 5_000_000i64; // $0.005
        let ask_price = mid + half_spread; // $100.005
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        tracker.process_event(&make_trade_msg(ask_price, 100), &lob, 3);

        // ES = 2 * |ask - mid| / mid * 10000 = 2 * 0.005 / 100.0 * 10000 = 1.0 bps
        let es = tracker.effective_spread_bps.mean();
        assert!(
            (es - 1.0).abs() < 0.01,
            "Trade at ask expected ~1.0 bps effective spread, got {}",
            es
        );
    }

    #[test]
    fn test_microprice_deviation_symmetric() {
        let mut tracker = LiquidityTracker::new();
        let mid = 100_000_000_000i64;
        let half_spread = 5_000_000i64;
        // Equal sizes: microprice == mid, deviation == 0
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        let add_msg =
            MboMessage::new(1, Action::Add, Side::Bid, mid, 100).with_timestamp(1_000_000_000);
        tracker.process_event(&add_msg, &lob, 3);

        assert!(
            tracker.microprice_deviation_bps.mean() < 1e-6,
            "Symmetric book should have ~0 microprice deviation, got {}",
            tracker.microprice_deviation_bps.mean()
        );
    }

    #[test]
    fn test_microprice_deviation_asymmetric() {
        let mut tracker = LiquidityTracker::new();
        let mid = 100_000_000_000i64;
        let half_spread = 5_000_000i64;
        // Asymmetric: more ask volume pushes microprice toward bid
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 300);

        let add_msg =
            MboMessage::new(1, Action::Add, Side::Bid, mid, 100).with_timestamp(1_000_000_000);
        tracker.process_event(&add_msg, &lob, 3);

        assert!(
            tracker.microprice_deviation_bps.mean() > 0.0,
            "Asymmetric book should have positive microprice deviation"
        );
    }

    #[test]
    fn test_volume_weighted_spread() {
        let mut tracker = LiquidityTracker::new();
        let mid = 100_000_000_000i64;
        let half_spread = 5_000_000i64;
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        // Small trade at midpoint (ES=0)
        tracker.process_event(&make_trade_msg(mid, 10), &lob, 3);
        // Large trade at ask (ES≈1bps)
        tracker.process_event(&make_trade_msg(mid + half_spread, 90), &lob, 3);

        // VWES should be weighted toward the ask trade
        let report = tracker.finalize();
        let vwes = report["volume_weighted_effective_spread_bps"]
            .as_f64()
            .unwrap();
        assert!(
            vwes > 0.5,
            "VWES should be weighted toward the larger trade, got {}",
            vwes
        );
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = LiquidityTracker::new();
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "LiquidityTracker");
        assert!(report.get("effective_spread_bps").is_some());
        assert!(report.get("volume_weighted_effective_spread_bps").is_some());
        assert!(report.get("microprice_deviation_bps").is_some());
        assert!(report.get("daily_vw_effective_spread_bps").is_some());
    }

    #[test]
    fn test_non_trade_ignored_for_es() {
        let mut tracker = LiquidityTracker::new();
        let mid = 100_000_000_000i64;
        let half_spread = 5_000_000i64;
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        let add_msg =
            MboMessage::new(1, Action::Add, Side::Bid, mid, 100).with_timestamp(1_000_000_000);
        tracker.process_event(&add_msg, &lob, 3);

        assert_eq!(
            tracker.n_trade_events, 0,
            "Add events should not count as trades"
        );
    }

    #[test]
    fn test_effective_spread_at_bid_exact() {
        // Trade at bid=$100.00, mid=$100.005
        // ES = 2 * |100.0 - 100.005| / 100.005 * 10000
        //    = 2 * 0.005 / 100.005 * 10000
        //    ≈ 0.99995 bps
        let mut tracker = LiquidityTracker::new();
        let mid = 100_005_000_000i64;
        let half_spread = 5_000_000i64;
        let bid_price = mid - half_spread; // 100_000_000_000 = $100.00
        let lob = make_lob_with_mid_and_sizes(mid, half_spread, 100, 100);

        tracker.process_event(&make_trade_msg(bid_price, 100), &lob, 3);

        let es = tracker.effective_spread_bps.mean();
        let expected = 2.0 * 0.005 / 100.005 * 10000.0;
        assert!(
            (es - expected).abs() < 0.01,
            "ES at bid: expected {:.4} bps, got {:.4}",
            expected,
            es
        );
    }
}
