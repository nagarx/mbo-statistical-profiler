//! # VpinTracker
//!
//! ## Purpose
//!
//! Computes Volume-Synchronized Probability of Informed Trading (VPIN),
//! the #1 out-of-sample predictor for spread, volatility, kurtosis,
//! skewness, and serial correlation changes per Easley et al. (2019).
//!
//! VPIN measures information asymmetry using volume bars (fixed-volume
//! buckets) rather than time bars, capturing the volume-volatility
//! interaction that dominates all other microstructure predictors.
//!
//! ## Formulas
//!
//! **Volume bars**: Aggregate trade events into bars of fixed volume `V_bar`
//! (default 5000 shares). Each bar tracks: total_volume, buy_volume, sell_volume,
//! vwap, close_price, timestamp.
//!
//! **Trade-side classification — AGGRESSOR convention (corrected 2026-08-23)**:
//! This tracker admits `Action::TradeAggregate` (vendor `b'T'`) ONLY, and on a
//! `T` the `Side` field is the **AGGRESSOR's own** side:
//!   * `Side::Bid` → the aggressor BOUGHT (lifted the ask) → `buy_volume`
//!   * `Side::Ask` → the aggressor SOLD  (hit the bid)     → `sell_volume`
//!
//! ⚠ THIS HEADER PREVIOUSLY STATED THE OPPOSITE — the RESTING convention
//! ("a trade hitting an `Ask` resting order is buyer-initiated"). That was
//! correct while the file was fed `Action::Fill`, whose `Side` IS the resting
//! order's, and it survived the carrier migration by ~280 lines, contradicting
//! the code it describes. A reader reaches this header long before the arm.
//! (hft-rules §11: docs must reflect current behaviour exactly.)
//!
//! We do NOT use Bulk Volume Classification (BVC) — we have a direct
//! trade-direction signal in the MBO feed for the side-determinate population.
//! ⚠ `Side::None` (no disclosed aggressor) is split evenly, which is BVC
//! evaluated at Δp = 0. See the note at that arm for why that is a placeholder
//! rather than a decision.
//!
//! **VPIN** (per-bar normalized to handle overflow-split bars):
//! `VPIN_t = (1/n) * sum_{i=t-n+1}^{t} |V_buy_i - V_sell_i| / (V_buy_i + V_sell_i)`
//! Rolling average of per-bar normalized absolute imbalance over `n` bars
//! (default n=50). Each bar is normalized by its own actual total volume, not
//! the nominal `V_bar`, because overflow splitting can produce bars with slightly
//! less than `V_bar`.
//!
//! ## References
//!
//! - Easley, D., Lopez de Prado, M., O'Hara, M., & Zhang, Z. (2019).
//!   "Microstructure in the Machine Age." Review of Financial Studies.
//! - Easley, D., Lopez de Prado, M., & O'Hara, M. (2012).
//!   "Flow Toxicity and Liquidity in a High-Frequency World."
//!   Review of Financial Studies, 25(5), 1457-1493.

use mbo_lob_reconstructor::{Action, BookConsistency, LobState, MboMessage, Side};
use serde_json::json;

use crate::statistics::{
    IntradayCurveAccumulator, RegimeAccumulator, StreamingDistribution, WelfordAccumulator,
};
use crate::AnalysisTracker;

/// Configuration constants
const DEFAULT_VOLUME_BAR_SIZE: u64 = 5000;
const DEFAULT_WINDOW_BARS: usize = 50;

/// A single volume bar.
#[derive(Debug, Clone)]
struct VolumeBar {
    #[allow(dead_code)]
    open_price: f64,
    #[allow(dead_code)]
    close_price: f64,
    #[allow(dead_code)]
    vwap: f64,
    #[allow(dead_code)]
    total_volume: u64,
    buy_volume: u64,
    sell_volume: u64,
    timestamp_ns: i64,
}

pub struct VpinTracker {
    volume_bar_size: u64,
    window_bars: usize,

    current_bar_volume: u64,
    current_bar_buy_vol: u64,
    current_bar_sell_vol: u64,
    current_bar_value_sum: f64,
    current_bar_open_price: f64,
    current_bar_last_price: f64,
    current_bar_first_ts: i64,

    completed_bars: Vec<VolumeBar>,
    vpin_values: Vec<(i64, f64)>,

    vpin_dist: StreamingDistribution,
    intraday_curve: IntradayCurveAccumulator,
    regime_vpin: RegimeAccumulator,

    vpin_spread_pairs: Vec<(f64, f64)>,

    daily_mean_vpin: WelfordAccumulator,
    n_days: u32,
    n_volume_bars_total: u64,

    /// Kept as `Vec<(i64, f64)>` (NOT eliminated like Spread/Trade buffers): the
    /// timestamps are essential for VPIN-spread temporal pairing in
    /// `process_day_vpin` where each VPIN value's timestamp is matched with the
    /// closest-in-time spread.
    day_spreads: Vec<(i64, f64)>,
    day_squared_returns: Vec<f64>,

    /// Cached at start of each day via `begin_day` (replaces the old
    /// `infer_utc_offset(&self.day_spreads.iter().map(...))` call which
    /// allocated a throwaway Vec just to read timestamps[0]).
    utc_offset: i32,
}

impl VpinTracker {
    pub fn new(volume_bar_size: u64, window_bars: usize) -> Self {
        Self {
            volume_bar_size: if volume_bar_size > 0 {
                volume_bar_size
            } else {
                DEFAULT_VOLUME_BAR_SIZE
            },
            window_bars: if window_bars > 0 {
                window_bars
            } else {
                DEFAULT_WINDOW_BARS
            },
            current_bar_volume: 0,
            current_bar_buy_vol: 0,
            current_bar_sell_vol: 0,
            current_bar_value_sum: 0.0,
            current_bar_open_price: 0.0,
            current_bar_last_price: 0.0,
            current_bar_first_ts: 0,
            completed_bars: Vec::new(),
            vpin_values: Vec::new(),
            vpin_dist: StreamingDistribution::new(10_000),
            intraday_curve: IntradayCurveAccumulator::new_rth_1min(),
            regime_vpin: RegimeAccumulator::new(),
            vpin_spread_pairs: Vec::new(),
            daily_mean_vpin: WelfordAccumulator::new(),
            n_days: 0,
            n_volume_bars_total: 0,
            day_spreads: Vec::new(),
            day_squared_returns: Vec::new(),
            utc_offset: -5, // EST default; overwritten by begin_day at start of each day
        }
    }

    fn complete_bar(&mut self, ts: i64) {
        if self.current_bar_volume == 0 {
            return;
        }

        let vwap = if self.current_bar_volume > 0 {
            self.current_bar_value_sum / self.current_bar_volume as f64
        } else {
            self.current_bar_last_price
        };

        self.completed_bars.push(VolumeBar {
            open_price: self.current_bar_open_price,
            close_price: self.current_bar_last_price,
            vwap,
            total_volume: self.current_bar_volume,
            buy_volume: self.current_bar_buy_vol,
            sell_volume: self.current_bar_sell_vol,
            timestamp_ns: self.current_bar_first_ts,
        });

        self.n_volume_bars_total += 1;
        self.reset_current_bar(ts);
    }

    fn reset_current_bar(&mut self, _ts: i64) {
        self.current_bar_volume = 0;
        self.current_bar_buy_vol = 0;
        self.current_bar_sell_vol = 0;
        self.current_bar_value_sum = 0.0;
        self.current_bar_open_price = 0.0;
        self.current_bar_last_price = 0.0;
        self.current_bar_first_ts = 0;
    }

    fn compute_vpin_series(&mut self) {
        let bars = &self.completed_bars;
        let n = self.window_bars;
        if bars.len() < n {
            return;
        }

        let _bar_size = self.volume_bar_size as f64;

        for i in n..=bars.len() {
            let window = &bars[i - n..i];
            // VPIN = (1/n) * sum_i |V_buy_i - V_sell_i| / V_bar
            // Use |2*buy - total| since buy + sell may not exactly equal V_bar
            // due to overflow splitting imprecision
            let sum_abs_imbalance: f64 = window
                .iter()
                .map(|b| {
                    let total = b.buy_volume + b.sell_volume;
                    if total == 0 {
                        return 0.0;
                    }
                    (b.buy_volume as f64 - b.sell_volume as f64).abs() / total as f64
                })
                .sum();

            let vpin = sum_abs_imbalance / n as f64;
            let ts = window.last().map(|b| b.timestamp_ns).unwrap_or(0);

            self.vpin_values.push((ts, vpin));
            self.vpin_dist.add(vpin);
        }
    }

    fn process_day_vpin(&mut self, utc_offset: i32) {
        if self.current_bar_volume > 0 {
            self.complete_bar(0);
        }

        self.compute_vpin_series();

        if self.vpin_values.is_empty() {
            return;
        }

        let mean_vpin: f64 =
            self.vpin_values.iter().map(|(_, v)| v).sum::<f64>() / self.vpin_values.len() as f64;
        self.daily_mean_vpin.update(mean_vpin);

        for &(ts, vpin) in &self.vpin_values {
            self.intraday_curve.add(ts, vpin, utc_offset);
        }

        if !self.day_spreads.is_empty() && !self.vpin_values.is_empty() {
            let mut spread_idx = 0;
            for &(vpin_ts, vpin_val) in &self.vpin_values {
                while spread_idx < self.day_spreads.len() - 1
                    && self.day_spreads[spread_idx + 1].0 <= vpin_ts
                {
                    spread_idx += 1;
                }
                let spread = self.day_spreads[spread_idx].1;
                self.vpin_spread_pairs.push((vpin_val, spread));
            }
        }
    }
}

impl AnalysisTracker for VpinTracker {
    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, regime: u8) {
        if lob_state.check_consistency() != BookConsistency::Valid {
            return;
        }

        if let Some(spread) = lob_state.spread() {
            if let Some(ts) = msg.timestamp {
                if spread >= 0.0 {
                    self.day_spreads.push((ts, spread));
                }
            }
        }

        // The AGGRESSOR print only (`TradeAggregate`).
        //
        // ⚠ WHY THIS PAIRS WITH THE SIDE FLIP BELOW, AND WHY NEITHER IS SAFE
        // ALONE. VPIN is built on the bar-level BUY/SELL IMBALANCE, so it
        // depends entirely on the side convention being right for the admitted
        // carrier. Admitting `T ∪ F` while classifying with the RESTING
        // convention (as this file did) makes the two carriers CANCEL: one
        // physical execution appears as `F|A` -> buy and as `T|A` -> buy, but
        // `T|A ≡ F|B`, so every trade contributes to both buckets and the
        // imbalance collapses toward zero. Measured: it cancelled to EXACTLY
        // zero. VPIN measures toxicity THROUGH that imbalance, so the published
        // series was structurally near-zero rather than uninformative-by-market.
        // Same mechanism as `FINDING-170`, in a different consumer.
        //
        // ⚠ THE BAR-POPULATION DROP IS VOLUME-DRIVEN, NOT RECORD-DRIVEN, AND AN
        // EARLIER VERSION OF THIS COMMENT USED THE WRONG BASIS. These are
        // 5,000-SHARE volume bars (`complete_bar`), so bar count tracks admitted
        // VOLUME. Measured both ways:
        //     2025-02-03  records −44.05%   VOLUME −33.19%   bars 23,303 -> 15,569
        //     2025-07-01  records −45.02%   VOLUME −36.88%   bars 14,990 ->  9,461
        // The record ratio (−44.05%) is right for 2025-02-03 and is what the
        // disposition table quotes, but applying it to a volume-bar count
        // overstates the drop by ~11 percentage points.
        if msg.action != Action::TradeAggregate {
            return;
        }
        if msg.size == 0 {
            return;
        }

        let trade_price = msg.price as f64 / 1e9;
        let size = msg.size as u64;
        let ts = msg.timestamp.unwrap_or(0);

        if self.current_bar_volume == 0 {
            self.current_bar_open_price = trade_price;
            self.current_bar_first_ts = ts;
        }
        self.current_bar_last_price = trade_price;
        self.current_bar_value_sum += trade_price * size as f64;

        // ⚠ AGGRESSOR CONVENTION — FLIPPED 2026-08-23 WITH THE ADMISSION ABOVE.
        // On a `TradeAggregate`, `msg.side` is the AGGRESSOR's own side:
        //     Side::Bid  -> the aggressor BOUGHT (lifted the ask)
        //     Side::Ask  -> the aggressor SOLD   (hit the bid)
        // This is the OPPOSITE of the convention this block carried while it
        // was fed `Fill`, where `side` was the RESTING order's. The previous
        // comment stated the resting rule and was correct FOR THAT CARRIER;
        // keeping it after the admission change would have inverted every bar.
        //
        // ⚠ CORRECTED 2026-08-23 — "DOING EITHER ALONE SHIPS A SILENTLY INVERTED
        // VPIN" IS FALSE, AND THE TRUTH IS SHARPER. `finalize()` publishes only
        // functions of `vpin_values`, computed as `|buy − sell| / total`, which
        // is INVARIANT under a buy/sell relabel — and `completed_bars`, the only
        // place the two survive separately, is never emitted. So THE FLIP ALONE
        // CHANGES NOTHING OBSERVABLE IN THE JSON.
        //
        // The flip is still required: the internal state would otherwise be
        // wrong, and any future consumer of the per-bar split (or of
        // `regime_conditional_vpin` extended to signed flow) would inherit an
        // inversion. But it means the ONLY instrument that can see this edit is
        // the direction-sensitive assertion in
        // `test_vpin_all_buy_equals_one` — which is therefore load-bearing, not
        // belt-and-braces. The ADMISSION change, by contrast, is observable
        // everywhere.
        if msg.side == Side::Bid {
            self.current_bar_buy_vol += size;
        } else if msg.side == Side::Ask {
            self.current_bar_sell_vol += size;
        } else {
            // `Side::None` — no aggressor side disclosed (hidden executions and
            // the auction crosses). Split evenly, PRESERVING today's behaviour.
            //
            // ⚠ THIS DILUTES VPIN AND THE EFFECT IS LARGE. `T|N` is 68,063
            // records / 19,661,605 shares on 2025-07-01 — 41.6% of the day's
            // traded volume. An even split contributes ZERO to |buy − sell|
            // while contributing its full weight to the bar denominator, so
            // VPIN is attenuated by roughly that fraction. Preserved rather than
            // changed because choosing a different treatment (exclude from
            // bars, or bar on side-determinate volume only) is a MODELLING
            // decision, not part of a carrier migration. Recorded as owed.
            self.current_bar_buy_vol += size / 2;
            self.current_bar_sell_vol += size - size / 2;
        }
        self.current_bar_volume += size;

        while self.current_bar_volume >= self.volume_bar_size {
            let overflow = self.current_bar_volume - self.volume_bar_size;
            self.current_bar_volume = self.volume_bar_size;
            self.complete_bar(ts);
            if overflow > 0 {
                self.current_bar_volume = overflow;
                self.current_bar_open_price = trade_price;
                self.current_bar_first_ts = ts;
                self.current_bar_last_price = trade_price;
                self.current_bar_value_sum = trade_price * overflow as f64;
                // ⚠ THE SECOND SIDE SITE. Same aggressor flip as above — and
                // it MUST stay in step: classifying the bar body one way and
                // its overflow the other corrupts only the bars that happen to
                // straddle a boundary, which is the hardest kind of wrong
                // number to notice.
                //
                // ⚠ AND THIS BRANCH CARRIED A PRE-EXISTING BUG, INDEPENDENT OF
                // THE CARRIER SPLIT: its `else` swept `Side::None` into SELL in
                // full, while the body block above splits `None` evenly. So an
                // undisclosed-side execution was classified differently
                // depending only on whether it crossed a bar boundary. Now
                // consistent with the body.
                if msg.side == Side::Bid {
                    // Aggressor BOUGHT (lifted the ask).
                    self.current_bar_buy_vol = overflow;
                    self.current_bar_sell_vol = 0;
                } else if msg.side == Side::Ask {
                    // Aggressor SOLD (hit the bid).
                    self.current_bar_sell_vol = overflow;
                    self.current_bar_buy_vol = 0;
                } else {
                    self.current_bar_buy_vol = overflow / 2;
                    self.current_bar_sell_vol = overflow - overflow / 2;
                }
            }
        }

        for &(_, vpin_val) in self.vpin_values.iter().rev().take(1) {
            self.regime_vpin.add(regime, vpin_val);
        }
    }

    fn begin_day(&mut self, _day_index: u32, utc_offset: i32, _day_epoch_ns: i64) {
        self.utc_offset = utc_offset;
    }

    fn end_of_day(&mut self) {
        // Use cached utc_offset from begin_day (eliminates throwaway Vec
        // allocation that was needed by the old infer_utc_offset call).
        self.process_day_vpin(self.utc_offset);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.completed_bars.clear();
        self.vpin_values.clear();
        self.day_spreads.clear();
        self.day_squared_returns.clear();
        self.reset_current_bar(0);
    }

    fn finalize(&self) -> serde_json::Value {
        let curve: Vec<serde_json::Value> = self
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| {
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_vpin": b.mean,
                    "count": b.count,
                })
            })
            .collect();

        // Two-pass numerically-stable Pearson r from hft_statistics.
        let vpin_spread_corr = hft_statistics::statistics::pearson_r_pairs(&self.vpin_spread_pairs);
        let vpin_spread_corr_json = if vpin_spread_corr.is_finite() {
            json!(vpin_spread_corr)
        } else {
            json!(null)
        };

        json!({
            "tracker": "VpinTracker",
            "n_days": self.n_days,
            "n_volume_bars_total": self.n_volume_bars_total,
            "volume_bar_size": self.volume_bar_size,
            "window_bars": self.window_bars,
            "vpin_distribution": self.vpin_dist.summary(),
            "daily_mean_vpin": {
                "mean": self.daily_mean_vpin.mean(),
                "std": self.daily_mean_vpin.std(),
                "min": self.daily_mean_vpin.min(),
                "max": self.daily_mean_vpin.max(),
                "count": self.daily_mean_vpin.count(),
            },
            "vpin_spread_correlation": vpin_spread_corr_json,
            "regime_conditional_vpin": self.regime_vpin.finalize(),
            "intraday_vpin_curve": curve,
        })
    }

    fn name(&self) -> &str {
        "VpinTracker"
    }
}

// Local one-pass `compute_correlation` removed — replaced with
// `hft_statistics::statistics::pearson_r_pairs` (two-pass, numerically stable).

#[cfg(test)]
mod tests {
    use super::*;

    const NS_PER_SECOND: i64 = 1_000_000_000;

    /// An AGGRESSOR print — the carrier this tracker admits.
    ///
    /// ⚠ `side` is the AGGRESSOR's: `Side::Bid` = the aggressor BOUGHT. Every
    /// fixture below was written against the RESTING convention and had to be
    /// re-derived, not merely renamed.
    fn make_trade(price_nanodollars: i64, size: u32, side: Side, ts: i64) -> MboMessage {
        MboMessage::new(1, Action::TradeAggregate, side, price_nanodollars, size).with_timestamp(ts)
    }

    fn make_valid_lob() -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    #[test]
    fn test_volume_bar_construction() {
        let mut tracker = VpinTracker::new(100, 5);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        for i in 0..10 {
            let msg = make_trade(100_000_000_000, 50, Side::Bid, ts + i * NS_PER_SECOND);
            tracker.process_event(&msg, &lob, 3);
        }
        // 10 trades * 50 shares = 500 shares / 100 per bar = 5 bars
        assert_eq!(tracker.completed_bars.len(), 5, "Should have 5 volume bars");
    }

    #[test]
    fn test_vpin_in_zero_one_range() {
        let mut tracker = VpinTracker::new(100, 3);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        for i in 0..20 {
            let side = if i % 3 == 0 { Side::Ask } else { Side::Bid };
            let msg = make_trade(100_000_000_000, 50, side, ts + i * NS_PER_SECOND);
            tracker.process_event(&msg, &lob, 3);
        }
        tracker.end_of_day();

        for &(_, vpin) in &tracker.vpin_values {
            assert!((0.0..=1.0).contains(&vpin), "VPIN {} outside [0, 1]", vpin);
        }
    }

    #[test]
    fn test_vpin_all_buy_equals_one() {
        // ⚠ AGGRESSOR CONVENTION. `Side::Bid` on a `TradeAggregate` means the
        // AGGRESSOR BOUGHT. This fixture previously used `Side::Ask` with the
        // comment "resting ask filled by buyer aggressor" — correct for the
        // `Fill` carrier this tracker no longer admits.
        //
        // ⚠ AND THE ASSERTION HAD TO CHANGE, NOT ONLY THE SIDE. VPIN is
        // |buy − sell| / total, which is DIRECTION-BLIND: all-buy and all-sell
        // both give EXACTLY 1.0. So the old body passed identically under the
        // inverted convention — a test named `all_buy` that could not tell buy
        // from sell, and therefore could not detect the very flip it appeared
        // to guard (hft-rules §6: a passing test can lock a bug). The bar-level
        // split below is the direction-sensitive claim; the VPIN value alone
        // never was one.
        let mut tracker = VpinTracker::new(100, 2);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        for i in 0..10 {
            let msg = make_trade(100_000_000_000, 100, Side::Bid, ts + i * NS_PER_SECOND);
            tracker.process_event(&msg, &lob, 3);
        }
        tracker.end_of_day();

        // ⚠ NOT `if !is_empty()`. The former body wrapped every assertion in
        // that guard, so an empty series passed the test while asserting
        // nothing at all — the failure path returning success.
        assert!(
            !tracker.vpin_values.is_empty(),
            "no VPIN values were produced; the assertions below would be vacuous"
        );
        let last_vpin = tracker.vpin_values.last().unwrap().1;
        assert!(
            (last_vpin - 1.0).abs() < 1e-10,
            "one-sided flow must give VPIN 1.0, got {last_vpin}"
        );

        // THE DIRECTION-SENSITIVE CLAIM — this is what a side flip breaks.
        assert!(!tracker.completed_bars.is_empty(), "no completed bars");
        for bar in &tracker.completed_bars {
            assert_eq!(
                bar.sell_volume, 0,
                "aggressor bought (Side::Bid) — sell_volume must be 0, got {}",
                bar.sell_volume
            );
            assert!(
                bar.buy_volume > 0,
                "aggressor bought (Side::Bid) — buy_volume must be positive"
            );
        }
    }

    #[test]
    fn test_vpin_balanced_near_zero() {
        // Alternating buy/sell → |buy-sell| ≈ 0 per bar → VPIN ≈ 0
        let mut tracker = VpinTracker::new(200, 2);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        for i in 0..20 {
            let side = if i % 2 == 0 { Side::Bid } else { Side::Ask };
            let msg = make_trade(100_000_000_000, 100, side, ts + i * NS_PER_SECOND);
            tracker.process_event(&msg, &lob, 3);
        }
        tracker.end_of_day();

        if !tracker.vpin_values.is_empty() {
            let last_vpin = tracker.vpin_values.last().unwrap().1;
            assert!(
                last_vpin < 0.1,
                "Balanced buy/sell VPIN should be near 0, got {}",
                last_vpin
            );
        }
    }

    #[test]
    fn test_overflow_carry_uses_the_same_side_convention_as_the_bar_body() {
        // ⚠ CLOSES A MEASURED COVERAGE GAP. Reverting the aggressor flip in the
        // OVERFLOW branch alone left all 105 tests green: every existing fixture
        // used sizes that divide the bar size exactly, so no test ever crossed a
        // bar boundary with a remainder. A bar body classified one way and its
        // carry the other corrupts only the bars that straddle a boundary —
        // the hardest kind of wrong number to notice.
        let mut tracker = VpinTracker::new(100, 2);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        // 150 shares against a 100-share bar: completes one bar, carries 50.
        tracker.process_event(&make_trade(100_000_000_000, 150, Side::Bid, ts), &lob, 3);

        assert_eq!(
            tracker.completed_bars.len(),
            1,
            "one bar should have closed"
        );
        assert_eq!(
            tracker.current_bar_volume, 50,
            "50 shares should have carried"
        );
        assert_eq!(
            tracker.current_bar_buy_vol, 50,
            "aggressor BOUGHT — the carry belongs in buy_vol"
        );
        assert_eq!(
            tracker.current_bar_sell_vol, 0,
            "aggressor BOUGHT — sell_vol must stay 0 across the boundary"
        );
    }

    #[test]
    fn test_overflow_carry_splits_undisclosed_side_like_the_bar_body() {
        // ⚠ A PRE-EXISTING INCONSISTENCY, INDEPENDENT OF THE CARRIER SPLIT.
        // The overflow branch used a bare `else`, sweeping `Side::None` wholly
        // into SELL, while the bar body splits it evenly. So an
        // undisclosed-side execution was classified differently depending only
        // on whether it happened to cross a bar boundary.
        let mut tracker = VpinTracker::new(100, 2);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        tracker.process_event(&make_trade(100_000_000_000, 140, Side::None, ts), &lob, 3);

        assert_eq!(
            tracker.current_bar_volume, 40,
            "40 shares should have carried"
        );
        assert_eq!(
            tracker.current_bar_buy_vol, 20,
            "undisclosed carry splits evenly"
        );
        assert_eq!(
            tracker.current_bar_sell_vol, 20,
            "undisclosed carry splits evenly"
        );
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = VpinTracker::new(5000, 50);
        let report = tracker.finalize();
        assert_eq!(report["tracker"], "VpinTracker");
        assert!(report.get("vpin_distribution").is_some());
        assert!(report.get("daily_mean_vpin").is_some());
        assert!(report.get("vpin_spread_correlation").is_some());
    }
}
