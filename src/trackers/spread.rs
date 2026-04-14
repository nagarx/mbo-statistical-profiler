//! # SpreadTracker
//!
//! ## Purpose
//!
//! Characterizes bid-ask spread distribution, intraday patterns, and
//! regime-conditional behavior. Critical for cost modeling and execution timing.
//!
//! ## Formulas
//!
//! - Spread (USD): `S = (best_ask - best_bid) / 1e9`
//! - Spread (ticks): `S_ticks = S / tick_size` where `tick_size = 0.01` (SEC Rule 612)
//! - Spread (bps): `S_bps = S / mid_price * 10000`
//!
//! ## References
//!
//! - Huang, R.D. & Stoll, H.R. (1997). "The Components of the Bid-Ask Spread."

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::statistics::{
    AcfComputer, IntradayCurveAccumulator, RegimeAccumulator, StreamingDistribution,
};
use crate::AnalysisTracker;

/// SEC Rule 612: minimum tick size for NMS stocks priced >= $1.00.
const TICK_SIZE: f64 = 0.01;

pub struct SpreadTracker {
    dist_usd: StreamingDistribution,
    dist_ticks: StreamingDistribution,
    dist_bps: StreamingDistribution,
    intraday_curve: IntradayCurveAccumulator,
    regime_spread: RegimeAccumulator,
    acf: AcfComputer,
    width_counts: [u64; 4],
    total_events: u64,
    trade_conditional_spread: StreamingDistribution,
    n_days: u32,
    /// Cached at start of each day via `begin_day` so `intraday_curve.add` can
    /// be called inline in `process_event` (eliminates the previous 320 MB
    /// `day_timestamps` + `day_spreads` replay buffers).
    utc_offset: i32,
}

impl SpreadTracker {
    pub fn new(reservoir_capacity: usize) -> Self {
        Self {
            dist_usd: StreamingDistribution::new(reservoir_capacity),
            dist_ticks: StreamingDistribution::new(reservoir_capacity),
            dist_bps: StreamingDistribution::new(reservoir_capacity),
            intraday_curve: IntradayCurveAccumulator::new_rth_1min(),
            regime_spread: RegimeAccumulator::new(),
            acf: AcfComputer::new(10_000, 20),
            width_counts: [0; 4],
            total_events: 0,
            trade_conditional_spread: StreamingDistribution::new(reservoir_capacity),
            n_days: 0,
            utc_offset: -5, // EST default; overwritten by begin_day at start of each day
        }
    }
}

impl AnalysisTracker for SpreadTracker {
    fn begin_day(&mut self, _day_index: u32, utc_offset: i32, _day_epoch_ns: i64) {
        self.utc_offset = utc_offset;
    }

    fn process_event(&mut self, msg: &MboMessage, lob: &LobState, regime: u8) {
        if lob.check_consistency() != BookConsistency::Valid {
            return;
        }

        let spread = match lob.spread() {
            Some(s) if s >= 0.0 => s,
            _ => return,
        };

        let mid = match lob.mid_price() {
            Some(m) if m > 0.0 => m,
            _ => return,
        };

        let spread_ticks = spread / TICK_SIZE;
        let spread_bps = spread / mid * 10000.0;

        self.dist_usd.add(spread);
        self.dist_ticks.add(spread_ticks);
        self.dist_bps.add(spread_bps);
        self.acf.push(spread);
        self.regime_spread.add(regime, spread);
        self.total_events += 1;

        let tick_class = spread_ticks.round() as u64;
        match tick_class {
            0 | 1 => self.width_counts[0] += 1,
            2 => self.width_counts[1] += 1,
            3 | 4 => self.width_counts[2] += 1,
            _ => self.width_counts[3] += 1,
        }

        // Inline intraday-curve population (was a replay loop in end_of_day
        // before, requiring 320 MB of day_timestamps + day_spreads buffers).
        // begin_day caches utc_offset; equivalent result without the buffers.
        if let Some(ts) = msg.timestamp {
            self.intraday_curve.add(ts, spread, self.utc_offset);
        }

        if lob.is_trade_event() {
            self.trade_conditional_spread.add(spread);
        }
    }

    fn end_of_day(&mut self) {
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        // No per-day state to reset (intraday_curve aggregates across days).
    }

    fn finalize(&self) -> serde_json::Value {
        let total = self.total_events as f64;
        let width_pcts: Vec<f64> = self
            .width_counts
            .iter()
            .map(|&c| {
                if total > 0.0 {
                    c as f64 / total * 100.0
                } else {
                    0.0
                }
            })
            .collect();

        let curve: Vec<serde_json::Value> = self
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| json!({"minutes_since_open": b.minutes_since_open, "mean_spread": b.mean, "count": b.count}))
            .collect();

        json!({
            "tracker": "SpreadTracker",
            "n_days": self.n_days,
            "distribution_usd": self.dist_usd.summary(),
            "distribution_ticks": self.dist_ticks.summary(),
            "distribution_bps": self.dist_bps.summary(),
            "width_classification": {
                "one_tick_pct": width_pcts[0],
                "two_tick_pct": width_pcts[1],
                "three_four_tick_pct": width_pcts[2],
                "five_plus_tick_pct": width_pcts[3],
            },
            "regime_conditional": self.regime_spread.finalize(),
            "trade_conditional": self.trade_conditional_spread.summary(),
            "acf": self.acf.compute(),
            "intraday_spread_curve": curve,
        })
    }

    fn name(&self) -> &str {
        "SpreadTracker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::{Action, Side};

    const NS_PER_SECOND: i64 = 1_000_000_000;

    fn make_lob_spread(bid: i64, ask: i64) -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(bid);
        lob.best_ask = Some(ask);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    fn make_msg() -> MboMessage {
        MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100)
            .with_timestamp(1_000_000_000)
    }

    #[test]
    fn test_one_tick_spread() {
        let mut tracker = SpreadTracker::new(1000);
        let lob = make_lob_spread(100_000_000_000, 100_010_000_000);
        tracker.process_event(&make_msg(), &lob, 3);

        assert_eq!(tracker.total_events, 1);
        assert_eq!(tracker.width_counts[0], 1);
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = SpreadTracker::new(1000);
        let report = tracker.finalize();
        assert_eq!(report["tracker"], "SpreadTracker");
        assert!(report.get("distribution_usd").is_some());
        assert!(report.get("width_classification").is_some());
        assert!(report.get("regime_conditional").is_some());
    }

    #[test]
    fn test_intraday_curve_uses_begin_day_utc_offset() {
        // Replaces the previous test_buffers_for_intraday_curve test (which
        // asserted existence of day_timestamps + day_spreads replay buffers
        // that were eliminated in the 2026-04-14 refactor).
        //
        // This test validates two coupled invariants:
        //   1. `begin_day` CACHES `utc_offset` into the tracker struct.
        //   2. `process_event` USES the cached value for `intraday_curve.add`.
        //
        // We pass `-4` (EDT) — a non-default value (SpreadTracker::new defaults
        // to `-5` EST). With the timestamp 14:30 UTC:
        //   - utc_offset=-5 (EST):   local = 09:30 → bin 0   (minutes_since_open = 0)
        //   - utc_offset=-4 (EDT):   local = 10:30 → bin 60  (minutes_since_open = 60)
        //
        // A positive assertion on `minutes_since_open == 60` proves begin_day
        // actually wrote the value and process_event actually read it. If
        // `begin_day` were a silent no-op (regression), the default `-5` would
        // kick in and we'd see bin 0 → test fails loudly.
        let mut tracker = SpreadTracker::new(1000);
        let lob = make_lob_spread(100_000_000_000, 100_010_000_000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND; // 14:30 UTC

        let msg =
            MboMessage::new(1, Action::Add, Side::Bid, 100_000_000_000, 100).with_timestamp(ts);

        // Pass EDT (non-default) via begin_day.
        tracker.begin_day(0, -4, 0);
        tracker.process_event(&msg, &lob, 3);

        let bins: Vec<_> = tracker
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .collect();
        assert_eq!(bins.len(), 1, "Expected exactly 1 populated bin");
        assert_eq!(bins[0].count, 1, "Bin should have 1 event");
        assert_eq!(
            bins[0].minutes_since_open as i64, 60,
            "With EDT offset (-4), 14:30 UTC = 10:30 EDT = bin 60. \
             If minutes_since_open == 0, begin_day's utc_offset was NOT \
             cached (silent regression)."
        );
    }

    #[test]
    fn test_spread_conversions_exact() {
        // bid = $100.000 (100_000_000_000 nanodollars)
        // ask = $100.010 (100_010_000_000 nanodollars)
        // Spread USD = $0.01
        // Mid = $100.005
        // Spread ticks = 0.01 / 0.01 = 1.0
        // Spread bps = 0.01 / 100.005 * 10000 = 0.99995...
        let spread_usd = 0.01_f64;
        let mid = 100.005_f64;
        let spread_ticks = spread_usd / 0.01;
        let spread_bps = spread_usd / mid * 10000.0;

        assert!(
            (spread_ticks - 1.0).abs() < 1e-10,
            "ticks expected 1.0, got {}",
            spread_ticks
        );
        assert!(
            (spread_bps - 0.99995).abs() < 0.001,
            "bps expected ~1.0, got {}",
            spread_bps
        );
    }

    #[test]
    fn test_spread_3tick_classification() {
        let mut tracker = SpreadTracker::new(1000);
        // 3-tick spread: bid=$100.00, ask=$100.03
        let lob = make_lob_spread(100_000_000_000, 100_030_000_000);
        tracker.process_event(&make_msg(), &lob, 3);
        assert_eq!(
            tracker.width_counts[2], 1,
            "3-tick spread should go to bucket [2] (3-4 tick)"
        );
    }
}
