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
//! **Trade-side classification (MBO convention)**:
//! Trades are classified using the MBO `Side` field on the trade event. The
//! resting-order side is the *passive* side, so a trade hitting an `Ask` resting
//! order is buyer-initiated (aggressive buy → `buy_volume`), and vice versa for
//! `Bid` resting orders. We do NOT use Bulk Volume Classification (BVC) — we have
//! direct trade-direction signal in the MBO feed.
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

    day_spreads: Vec<(i64, f64)>,
    day_squared_returns: Vec<f64>,
}

impl VpinTracker {
    pub fn new(volume_bar_size: u64, window_bars: usize) -> Self {
        Self {
            volume_bar_size: if volume_bar_size > 0 { volume_bar_size } else { DEFAULT_VOLUME_BAR_SIZE },
            window_bars: if window_bars > 0 { window_bars } else { DEFAULT_WINDOW_BARS },
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
                    if total == 0 { return 0.0; }
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
    fn process_event(
        &mut self,
        msg: &MboMessage,
        lob_state: &LobState,
        regime: u8,
        _day_epoch_ns: i64,
    ) {
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

        if msg.action != Action::Trade && msg.action != Action::Fill {
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

        // MBO convention: msg.side = resting order's side.
        // Side::Bid resting → aggressor is SELLER (hit the bid)
        // Side::Ask resting → aggressor is BUYER (lifted the ask)
        if msg.side == Side::Ask {
            self.current_bar_buy_vol += size;
        } else if msg.side == Side::Bid {
            self.current_bar_sell_vol += size;
        } else {
            // Side::None — split evenly
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
                if msg.side == Side::Ask {
                    // Buyer aggressor (ask-side resting filled)
                    self.current_bar_buy_vol = overflow;
                    self.current_bar_sell_vol = 0;
                } else {
                    self.current_bar_sell_vol = overflow;
                    self.current_bar_buy_vol = 0;
                }
            }
        }

        for &(_, vpin_val) in self.vpin_values.iter().rev().take(1) {
            self.regime_vpin.add(regime, vpin_val);
        }
    }

    fn end_of_day(&mut self, _day_index: u32) {
        let utc_offset = crate::time::regime::infer_utc_offset(
            &self.day_spreads.iter().map(|(ts, _)| *ts).collect::<Vec<_>>(),
        );
        self.process_day_vpin(utc_offset);
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

        let vpin_spread_corr = compute_correlation(&self.vpin_spread_pairs);

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
            "vpin_spread_correlation": vpin_spread_corr,
            "regime_conditional_vpin": self.regime_vpin.finalize(),
            "intraday_vpin_curve": curve,
        })
    }

    fn name(&self) -> &str {
        "VpinTracker"
    }
}

fn compute_correlation(pairs: &[(f64, f64)]) -> f64 {
    let n = pairs.len();
    if n < 3 {
        return f64::NAN;
    }
    let nf = n as f64;
    let (mut sx, mut sy, mut sxy, mut sx2, mut sy2) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for &(x, y) in pairs {
        sx += x;
        sy += y;
        sxy += x * y;
        sx2 += x * x;
        sy2 += y * y;
    }
    let cov = sxy / nf - (sx / nf) * (sy / nf);
    let var_x = sx2 / nf - (sx / nf).powi(2);
    let var_y = sy2 / nf - (sy / nf).powi(2);
    let denom = (var_x * var_y).sqrt();
    if denom < 1e-15 {
        f64::NAN
    } else {
        cov / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS_PER_SECOND: i64 = 1_000_000_000;

    fn make_trade(price_nanodollars: i64, size: u32, side: Side, ts: i64) -> MboMessage {
        MboMessage::new(1, Action::Trade, side, price_nanodollars, size).with_timestamp(ts)
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
            tracker.process_event(&msg, &lob, 3, 0);
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
            tracker.process_event(&msg, &lob, 3, 0);
        }
        tracker.end_of_day(0);

        for &(_, vpin) in &tracker.vpin_values {
            assert!(
                vpin >= 0.0 && vpin <= 1.0,
                "VPIN {} outside [0, 1]", vpin
            );
        }
    }

    #[test]
    fn test_vpin_all_buy_equals_one() {
        // All buyer-initiated (Side::Ask = resting ask filled by buyer aggressor)
        // → every bar has |buy - sell| = bar_size → VPIN = 1.0
        let mut tracker = VpinTracker::new(100, 2);
        let lob = make_valid_lob();
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        for i in 0..10 {
            let msg = make_trade(100_000_000_000, 100, Side::Ask, ts + i * NS_PER_SECOND);
            tracker.process_event(&msg, &lob, 3, 0);
        }
        tracker.end_of_day(0);

        if !tracker.vpin_values.is_empty() {
            let last_vpin = tracker.vpin_values.last().unwrap().1;
            assert!(
                (last_vpin - 1.0).abs() < 1e-10,
                "All-buy VPIN should be 1.0, got {}", last_vpin
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
            tracker.process_event(&msg, &lob, 3, 0);
        }
        tracker.end_of_day(0);

        if !tracker.vpin_values.is_empty() {
            let last_vpin = tracker.vpin_values.last().unwrap().1;
            assert!(
                last_vpin < 0.1,
                "Balanced buy/sell VPIN should be near 0, got {}", last_vpin
            );
        }
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
