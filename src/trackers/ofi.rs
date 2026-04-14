//! # OfiTracker
//!
//! ## Purpose
//!
//! Computes Order Flow Imbalance (OFI) at multiple timescales and measures
//! its predictive power for returns. OFI is the most predictive short-term
//! signal in microstructure literature (r = 0.702 at 5m for NVDA).
//!
//! ## Formulas
//!
//! **OFI per event** (Cont, Kukanov & Stoikov 2014, Eq. 3):
//!
//! ```text
//! OFI_t = (bid_size_t * I(bid_price_t >= bid_price_{t-1})
//!        - bid_size_{t-1} * I(bid_price_t <= bid_price_{t-1}))
//!       - (ask_size_t * I(ask_price_t <= ask_price_{t-1})
//!        - ask_size_{t-1} * I(ask_price_t >= ask_price_{t-1}))
//! ```
//!
//! **Decomposition**: OFI = OFI_add + OFI_cancel + OFI_trade
//! Each component isolated by filtering on triggering_action.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units | Per-Scale? |
//! |-----------|---------|-------|:---:|
//! | OFI distribution | Welford + reservoir over all bins | shares | Yes |
//! | OFI-return correlation | Pearson(OFI_t, return_t) at lags 0-5 | dim. | Yes |
//! | Component fractions | |OFI_add| / |OFI_total|, etc. | ratio | No |
//! | OFI ACF | ACF(k) of bin-level OFI | dim. | Yes |
//! | Regime intensity | mean(|OFI|) per regime | shares | No |
//! | Intraday OFI curve | 390-bin canonical grid | shares | No |
//! | Cumulative delta | sum(trade_size * sign) per day | shares | No |
//! | Aggressor ratio | buyer_vol / (buyer_vol + seller_vol) | ratio | No |
//!
//! ## References
//!
//! - Cont, R., Kukanov, A. & Stoikov, S. (2014). "The Price Impact of Order Book Events."
//!   Journal of Financial Econometrics, 12(1), 47-88.
//! - Kolm, P., Turiel, J. & Westray, N. (2023). "Deep Order Flow Imbalance."

// TODO(plan/D6 + Step 5.8): Extract `compute_ofi` to shared `OfiPrevState::compute()`
// in trackers/ofi_primitives.rs (deferred; character-identical to cross_scale_ofi.rs).
//
// TODO(plan/Step 5.5): Refactor inline Pearson r in process_day_ofi (lag correlation,
// OFI-spread correlation, conditional OFI) to use `pearson_r_slices` from hft_statistics.
// Deferred: the inline running-sums logic is intertwined with cross-day weighted-mean
// aggregation. Allow needless_range_loop at file scope until refactored.
#![allow(clippy::needless_range_loop)]

use mbo_lob_reconstructor::{Action, BookConsistency, LobState, MboMessage, Side};
use serde_json::json;

use crate::statistics::{
    AcfComputer, IntradayCorrelationAccumulator, IntradayCurveAccumulator, RegimeAccumulator,
    StreamingDistribution, WelfordAccumulator,
};
use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;
use hft_statistics::time::format_scale_label as format_label;
use hft_statistics::time::NS_PER_SECOND;

const N_SPREAD_BUCKETS: usize = 4;
const SPREAD_BUCKET_LABELS: [&str; N_SPREAD_BUCKETS] =
    ["1_tick", "2_tick", "3_4_tick", "5_plus_tick"];
const TICK_SIZE_USD: f64 = 0.01;

fn spread_bucket(spread_usd: f64) -> Option<usize> {
    if !spread_usd.is_finite() || spread_usd <= 0.0 {
        return None;
    }
    let ticks = spread_usd / TICK_SIZE_USD;
    if ticks < 1.5 {
        Some(0)
    } else if ticks < 2.5 {
        Some(1)
    } else if ticks < 4.5 {
        Some(2)
    } else {
        Some(3)
    }
}

struct SpreadBucketCorrelations {
    sum_xy: [f64; N_SPREAD_BUCKETS],
    sum_x: [f64; N_SPREAD_BUCKETS],
    sum_y: [f64; N_SPREAD_BUCKETS],
    sum_x2: [f64; N_SPREAD_BUCKETS],
    sum_y2: [f64; N_SPREAD_BUCKETS],
    counts: [u64; N_SPREAD_BUCKETS],
}

impl SpreadBucketCorrelations {
    fn new() -> Self {
        Self {
            sum_xy: [0.0; N_SPREAD_BUCKETS],
            sum_x: [0.0; N_SPREAD_BUCKETS],
            sum_y: [0.0; N_SPREAD_BUCKETS],
            sum_x2: [0.0; N_SPREAD_BUCKETS],
            sum_y2: [0.0; N_SPREAD_BUCKETS],
            counts: [0; N_SPREAD_BUCKETS],
        }
    }

    fn add(&mut self, bucket: usize, ofi: f64, ret: f64) {
        self.sum_xy[bucket] += ofi * ret;
        self.sum_x[bucket] += ofi;
        self.sum_y[bucket] += ret;
        self.sum_x2[bucket] += ofi * ofi;
        self.sum_y2[bucket] += ret * ret;
        self.counts[bucket] += 1;
    }

    fn correlations(&self) -> Vec<(f64, u64)> {
        (0..N_SPREAD_BUCKETS)
            .map(|b| {
                let n = self.counts[b];
                if n < 3 {
                    return (f64::NAN, n);
                }
                let nf = n as f64;
                let cov = self.sum_xy[b] / nf - (self.sum_x[b] / nf) * (self.sum_y[b] / nf);
                let var_x = self.sum_x2[b] / nf - (self.sum_x[b] / nf).powi(2);
                let var_y = self.sum_y2[b] / nf - (self.sum_y[b] / nf).powi(2);
                let denom = (var_x * var_y).sqrt();
                if denom > 1e-15 {
                    (cov / denom, n)
                } else {
                    (f64::NAN, n)
                }
            })
            .collect()
    }
}

pub struct OfiTracker {
    prev_bid_price: i64,
    prev_ask_price: i64,
    prev_bid_size: u32,
    prev_ask_size: u32,
    initialized: bool,

    day_ofi_ts: Vec<i64>,
    day_ofi_vals: Vec<f64>,
    day_ofi_add: Vec<f64>,
    day_ofi_cancel: Vec<f64>,
    day_ofi_trade: Vec<f64>,
    day_mid_prices: Vec<f64>,
    day_mid_ts: Vec<i64>,
    day_spreads: Vec<f64>,
    day_spread_ts: Vec<i64>,

    per_scale: Vec<OfiScaleState>,
    regime_abs_ofi: RegimeAccumulator,
    intraday_curve: IntradayCurveAccumulator,
    intraday_ofi_return_r_curve: IntradayCorrelationAccumulator,

    total_abs_add: f64,
    total_abs_cancel: f64,
    total_abs_trade: f64,

    daily_deltas: WelfordAccumulator,
    total_buyer_vol: f64,
    total_seller_vol: f64,

    n_days: u32,

    /// Cached at start of each day via `begin_day` (replaces the old
    /// `infer_day_params(&self.day_*)` call which re-derived these from timestamps).
    utc_offset: i32,
    day_epoch_ns: i64,
}

struct OfiScaleState {
    label: String,
    bin_width_ns: i64,
    dist: StreamingDistribution,
    acf: AcfComputer,
    ofi_return_correlations: Vec<f64>,
    ofi_return_counts: Vec<u64>,
    ofi_spread_correlations: Vec<f64>,
    ofi_spread_counts: Vec<u64>,
    conditional_ofi_return: SpreadBucketCorrelations,
}

impl OfiTracker {
    pub fn new(timescales: &[f64], reservoir_capacity: usize) -> Self {
        let per_scale = timescales
            .iter()
            .map(|&s| OfiScaleState {
                label: format_label(s),
                bin_width_ns: (s * NS_PER_SECOND as f64) as i64,
                dist: StreamingDistribution::new(reservoir_capacity),
                acf: AcfComputer::new(10_000, 20),
                ofi_return_correlations: vec![0.0; 6],
                ofi_return_counts: vec![0; 6],
                ofi_spread_correlations: vec![0.0; 6],
                ofi_spread_counts: vec![0; 6],
                conditional_ofi_return: SpreadBucketCorrelations::new(),
            })
            .collect();

        Self {
            prev_bid_price: 0,
            prev_ask_price: 0,
            prev_bid_size: 0,
            prev_ask_size: 0,
            initialized: false,
            day_ofi_ts: Vec::with_capacity(20_000_000),
            day_ofi_vals: Vec::with_capacity(20_000_000),
            day_ofi_add: Vec::with_capacity(20_000_000),
            day_ofi_cancel: Vec::with_capacity(20_000_000),
            day_ofi_trade: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            day_mid_ts: Vec::with_capacity(20_000_000),
            day_spreads: Vec::with_capacity(20_000_000),
            day_spread_ts: Vec::with_capacity(20_000_000),
            per_scale,
            regime_abs_ofi: RegimeAccumulator::new(),
            intraday_curve: IntradayCurveAccumulator::new_rth_1min(),
            intraday_ofi_return_r_curve: IntradayCorrelationAccumulator::new_rth_1min(),
            total_abs_add: 0.0,
            total_abs_cancel: 0.0,
            total_abs_trade: 0.0,
            daily_deltas: WelfordAccumulator::new(),
            total_buyer_vol: 0.0,
            total_seller_vol: 0.0,
            n_days: 0,
            utc_offset: -5, // EST default; overwritten by begin_day at start of each day
            day_epoch_ns: 0,
        }
    }

    fn compute_ofi(&self, lob: &LobState) -> f64 {
        let bid = lob.best_bid.unwrap_or(0);
        let ask = lob.best_ask.unwrap_or(0);
        let bid_size = lob.bid_sizes[0] as f64;
        let ask_size = lob.ask_sizes[0] as f64;

        let prev_bid = self.prev_bid_price;
        let prev_ask = self.prev_ask_price;
        let prev_bid_size = self.prev_bid_size as f64;
        let prev_ask_size = self.prev_ask_size as f64;

        let bid_contrib = if bid > prev_bid {
            bid_size
        } else if bid == prev_bid {
            bid_size - prev_bid_size
        } else {
            -prev_bid_size
        };

        let ask_contrib = if ask < prev_ask {
            -ask_size
        } else if ask == prev_ask {
            -(ask_size - prev_ask_size)
        } else {
            prev_ask_size
        };

        bid_contrib + ask_contrib
    }

    fn process_day_ofi(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_ofi_vals.len() < 2 {
            return;
        }

        for scale in &mut self.per_scale {
            let ofi_bins = resample_to_grid(
                &self.day_ofi_ts,
                &self.day_ofi_vals,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Sum,
            );

            let mid_bins = resample_to_grid(
                &self.day_mid_ts,
                &self.day_mid_prices,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Last,
            );

            let ofi_filled: Vec<(usize, f64)> = ofi_bins
                .values
                .iter()
                .enumerate()
                .filter(|(i, v)| {
                    v.is_finite() && *i < ofi_bins.counts.len() && ofi_bins.counts[*i] > 0
                })
                .map(|(i, &v)| (i, v))
                .collect();

            for &(_, val) in &ofi_filled {
                scale.dist.add(val);
                scale.acf.push(val);
            }

            let n_mid = mid_bins.values.len();
            let mut returns: Vec<f64> = vec![f64::NAN; n_mid];
            for i in 1..n_mid {
                if mid_bins.values[i].is_finite()
                    && mid_bins.values[i - 1].is_finite()
                    && mid_bins.values[i] > 0.0
                    && mid_bins.values[i - 1] > 0.0
                {
                    returns[i] = (mid_bins.values[i] / mid_bins.values[i - 1]).ln();
                }
            }

            for lag in 0..6usize {
                let mut sum_xy = 0.0f64;
                let mut sum_x = 0.0f64;
                let mut sum_y = 0.0f64;
                let mut sum_x2 = 0.0f64;
                let mut sum_y2 = 0.0f64;
                let mut n = 0u64;

                for &(bin_i, ofi_val) in &ofi_filled {
                    let ret_i = bin_i + lag;
                    if ret_i < returns.len() && returns[ret_i].is_finite() {
                        let r = returns[ret_i];
                        sum_xy += ofi_val * r;
                        sum_x += ofi_val;
                        sum_y += r;
                        sum_x2 += ofi_val * ofi_val;
                        sum_y2 += r * r;
                        n += 1;
                    }
                }

                if n > 2 {
                    let nf = n as f64;
                    let cov = sum_xy / nf - (sum_x / nf) * (sum_y / nf);
                    let var_x = sum_x2 / nf - (sum_x / nf).powi(2);
                    let var_y = sum_y2 / nf - (sum_y / nf).powi(2);
                    let denom = (var_x * var_y).sqrt();
                    if denom > 1e-15 {
                        let corr = cov / denom;
                        let prev = scale.ofi_return_correlations[lag];
                        let prev_n = scale.ofi_return_counts[lag];
                        let total_n = prev_n + n;
                        scale.ofi_return_correlations[lag] =
                            (prev * prev_n as f64 + corr * n as f64) / total_n as f64;
                        scale.ofi_return_counts[lag] = total_n;
                    }
                }
            }
        }

        // Conditional OFI-return correlation by spread bucket (lag 0 only)
        for scale in &mut self.per_scale {
            let spread_bins_for_cond = resample_to_grid(
                &self.day_spread_ts,
                &self.day_spreads,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Mean,
            );

            let ofi_bins_cond = resample_to_grid(
                &self.day_ofi_ts,
                &self.day_ofi_vals,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Sum,
            );

            let mid_bins_cond = resample_to_grid(
                &self.day_mid_ts,
                &self.day_mid_prices,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Last,
            );

            let n_mid_cond = mid_bins_cond.values.len();
            let mut returns_cond: Vec<f64> = vec![f64::NAN; n_mid_cond];
            for i in 1..n_mid_cond {
                if mid_bins_cond.values[i].is_finite()
                    && mid_bins_cond.values[i - 1].is_finite()
                    && mid_bins_cond.values[i] > 0.0
                    && mid_bins_cond.values[i - 1] > 0.0
                {
                    returns_cond[i] = (mid_bins_cond.values[i] / mid_bins_cond.values[i - 1]).ln();
                }
            }

            for (bin_i, &ofi_val) in ofi_bins_cond.values.iter().enumerate() {
                if !ofi_val.is_finite()
                    || bin_i >= ofi_bins_cond.counts.len()
                    || ofi_bins_cond.counts[bin_i] == 0
                {
                    continue;
                }
                if bin_i >= returns_cond.len() || !returns_cond[bin_i].is_finite() {
                    continue;
                }
                if bin_i >= spread_bins_for_cond.values.len() {
                    continue;
                }
                let spread_val = spread_bins_for_cond.values[bin_i];
                if let Some(bucket) = spread_bucket(spread_val) {
                    scale
                        .conditional_ofi_return
                        .add(bucket, ofi_val, returns_cond[bin_i]);
                }
            }
        }

        // OFI-spread cross-correlation at each timescale
        for scale in &mut self.per_scale {
            let spread_bins = resample_to_grid(
                &self.day_spread_ts,
                &self.day_spreads,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Mean,
            );

            let n_spread = spread_bins.values.len();
            let mut spread_changes: Vec<f64> = vec![f64::NAN; n_spread];
            for i in 1..n_spread {
                if spread_bins.values[i].is_finite() && spread_bins.values[i - 1].is_finite() {
                    spread_changes[i] = spread_bins.values[i] - spread_bins.values[i - 1];
                }
            }

            let ofi_bins = resample_to_grid(
                &self.day_ofi_ts,
                &self.day_ofi_vals,
                scale.bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Sum,
            );

            let ofi_filled: Vec<(usize, f64)> = ofi_bins
                .values
                .iter()
                .enumerate()
                .filter(|(i, v)| {
                    v.is_finite() && *i < ofi_bins.counts.len() && ofi_bins.counts[*i] > 0
                })
                .map(|(i, &v)| (i, v))
                .collect();

            for lag in 0..6usize {
                let mut sx = 0.0f64;
                let mut sy = 0.0f64;
                let mut sxy = 0.0f64;
                let mut sx2 = 0.0f64;
                let mut sy2 = 0.0f64;
                let mut n = 0u64;

                for &(bin_i, ofi_val) in &ofi_filled {
                    let sp_i = bin_i + lag;
                    if sp_i < spread_changes.len() && spread_changes[sp_i].is_finite() {
                        let sc = spread_changes[sp_i];
                        sxy += ofi_val * sc;
                        sx += ofi_val;
                        sy += sc;
                        sx2 += ofi_val * ofi_val;
                        sy2 += sc * sc;
                        n += 1;
                    }
                }

                if n > 2 {
                    let nf = n as f64;
                    let cov = sxy / nf - (sx / nf) * (sy / nf);
                    let var_x = sx2 / nf - (sx / nf).powi(2);
                    let var_y = sy2 / nf - (sy / nf).powi(2);
                    let denom = (var_x * var_y).sqrt();
                    if denom > 1e-15 {
                        let corr = cov / denom;
                        let prev = scale.ofi_spread_correlations[lag];
                        let prev_n = scale.ofi_spread_counts[lag];
                        let total_n = prev_n + n;
                        scale.ofi_spread_correlations[lag] =
                            (prev * prev_n as f64 + corr * n as f64) / total_n as f64;
                        scale.ofi_spread_counts[lag] = total_n;
                    }
                }
            }
        }

        for i in 0..self.day_ofi_vals.len() {
            let ts = self.day_ofi_ts[i];
            let val = self.day_ofi_vals[i];
            self.intraday_curve.add(ts, val, utc_offset);
        }

        // Per-minute OFI-return r: resample at 1s, then feed each aligned
        // (OFI, return) pair into the 390-bin correlation accumulator.
        {
            let ofi_1s = resample_to_grid(
                &self.day_ofi_ts,
                &self.day_ofi_vals,
                NS_PER_SECOND,
                day_epoch_ns,
                utc_offset,
                AggMode::Sum,
            );
            let mid_1s = resample_to_grid(
                &self.day_mid_ts,
                &self.day_mid_prices,
                NS_PER_SECOND,
                day_epoch_ns,
                utc_offset,
                AggMode::Last,
            );

            let n = mid_1s.values.len();
            for i in 1..n {
                let bin_i = i;
                if bin_i >= ofi_1s.values.len()
                    || bin_i >= ofi_1s.counts.len()
                    || ofi_1s.counts[bin_i] == 0
                    || !ofi_1s.values[bin_i].is_finite()
                {
                    continue;
                }
                if !mid_1s.values[i].is_finite()
                    || !mid_1s.values[i - 1].is_finite()
                    || mid_1s.values[i] <= 0.0
                    || mid_1s.values[i - 1] <= 0.0
                {
                    continue;
                }
                let ret = (mid_1s.values[i] / mid_1s.values[i - 1]).ln();
                if ret.is_finite() {
                    let bin_ts = ofi_1s.edges_ns[bin_i];
                    self.intraday_ofi_return_r_curve.add(
                        bin_ts,
                        ofi_1s.values[bin_i],
                        ret,
                        utc_offset,
                    );
                }
            }
        }

        let add_total: f64 = self.day_ofi_add.iter().map(|v| v.abs()).sum();
        let cancel_total: f64 = self.day_ofi_cancel.iter().map(|v| v.abs()).sum();
        let trade_total: f64 = self.day_ofi_trade.iter().map(|v| v.abs()).sum();
        self.total_abs_add += add_total;
        self.total_abs_cancel += cancel_total;
        self.total_abs_trade += trade_total;
    }
}

impl AnalysisTracker for OfiTracker {
    fn begin_day(&mut self, _day_index: u32, utc_offset: i32, day_epoch_ns: i64) {
        self.utc_offset = utc_offset;
        self.day_epoch_ns = day_epoch_ns;
    }

    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, regime: u8) {
        if lob_state.check_consistency() != BookConsistency::Valid {
            return;
        }

        let bid = lob_state.best_bid.unwrap_or(0);
        let ask = lob_state.best_ask.unwrap_or(0);
        if bid == 0 || ask == 0 {
            return;
        }

        if let Some(mid) = lob_state.mid_price() {
            if let Some(ts) = msg.timestamp {
                self.day_mid_prices.push(mid);
                self.day_mid_ts.push(ts);
            }
        }

        if let (Some(spread), Some(ts)) = (lob_state.spread(), msg.timestamp) {
            if spread >= 0.0 {
                self.day_spreads.push(spread);
                self.day_spread_ts.push(ts);
            }
        }

        if !self.initialized {
            self.prev_bid_price = bid;
            self.prev_ask_price = ask;
            self.prev_bid_size = lob_state.bid_sizes[0];
            self.prev_ask_size = lob_state.ask_sizes[0];
            self.initialized = true;
            return;
        }

        let ofi = self.compute_ofi(lob_state);

        if let Some(ts) = msg.timestamp {
            self.day_ofi_ts.push(ts);
            self.day_ofi_vals.push(ofi);

            let action = lob_state.triggering_action.unwrap_or(Action::None);
            match action {
                Action::Add => self.day_ofi_add.push(ofi),
                Action::Cancel => self.day_ofi_cancel.push(ofi),
                Action::Trade | Action::Fill => self.day_ofi_trade.push(ofi),
                _ => {}
            }

            self.regime_abs_ofi.add(regime, ofi.abs());
        }

        if (msg.action == Action::Trade || msg.action == Action::Fill) && msg.side != Side::None {
            let size = msg.size as f64;
            if msg.side == Side::Bid {
                self.total_buyer_vol += size;
            } else {
                self.total_seller_vol += size;
            }
        }

        self.prev_bid_price = bid;
        self.prev_ask_price = ask;
        self.prev_bid_size = lob_state.bid_sizes[0];
        self.prev_ask_size = lob_state.ask_sizes[0];
    }

    fn end_of_day(&mut self) {
        // Use cached values from begin_day (replaces infer_day_params)
        let utc_offset = self.utc_offset;
        let day_epoch_ns = self.day_epoch_ns;

        let eod_delta: f64 = self.day_ofi_vals.iter().sum();
        self.daily_deltas.update(eod_delta);

        self.process_day_ofi(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_ofi_ts.clear();
        self.day_ofi_vals.clear();
        self.day_ofi_add.clear();
        self.day_ofi_cancel.clear();
        self.day_ofi_trade.clear();
        self.day_mid_prices.clear();
        self.day_mid_ts.clear();
        self.day_spreads.clear();
        self.day_spread_ts.clear();
        self.initialized = false;
    }

    fn finalize(&self) -> serde_json::Value {
        let mut scales = serde_json::Map::new();

        for scale in &self.per_scale {
            let cond_corrs = scale.conditional_ofi_return.correlations();
            let mut cond_map = serde_json::Map::new();
            for (i, (r, n)) in cond_corrs.iter().enumerate() {
                let r_val = if r.is_finite() { json!(r) } else { json!(null) };
                cond_map.insert(
                    SPREAD_BUCKET_LABELS[i].to_string(),
                    json!({"r": r_val, "n": n}),
                );
            }

            scales.insert(
                scale.label.clone(),
                json!({
                    "distribution": scale.dist.summary(),
                    "acf": scale.acf.compute(),
                    "ofi_return_correlation": {
                        "lag_0": scale.ofi_return_correlations[0],
                        "lag_1": scale.ofi_return_correlations[1],
                        "lag_2": scale.ofi_return_correlations[2],
                        "lag_3": scale.ofi_return_correlations[3],
                        "lag_4": scale.ofi_return_correlations[4],
                        "lag_5": scale.ofi_return_correlations[5],
                    },
                    "ofi_spread_correlation": {
                        "lag_0": scale.ofi_spread_correlations[0],
                        "lag_1": scale.ofi_spread_correlations[1],
                        "lag_2": scale.ofi_spread_correlations[2],
                        "lag_3": scale.ofi_spread_correlations[3],
                        "lag_4": scale.ofi_spread_correlations[4],
                        "lag_5": scale.ofi_spread_correlations[5],
                    },
                    "conditional_ofi_return": cond_map,
                }),
            );
        }

        let total_component = self.total_abs_add + self.total_abs_cancel + self.total_abs_trade;
        let add_frac = if total_component > 0.0 {
            self.total_abs_add / total_component
        } else {
            0.0
        };
        let cancel_frac = if total_component > 0.0 {
            self.total_abs_cancel / total_component
        } else {
            0.0
        };
        let trade_frac = if total_component > 0.0 {
            self.total_abs_trade / total_component
        } else {
            0.0
        };

        let total_vol = self.total_buyer_vol + self.total_seller_vol;
        let aggressor_ratio = if total_vol > 0.0 {
            self.total_buyer_vol / total_vol
        } else {
            0.5
        };

        let curve: Vec<serde_json::Value> = self
            .intraday_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count > 0)
            .map(|b| {
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "mean_ofi": b.mean,
                    "count": b.count,
                })
            })
            .collect();

        let ofi_return_r_curve: Vec<serde_json::Value> = self
            .intraday_ofi_return_r_curve
            .finalize()
            .into_iter()
            .filter(|b| b.count >= 3)
            .map(|b| {
                let r_val = if b.pearson_r.is_finite() {
                    json!(b.pearson_r)
                } else {
                    json!(null)
                };
                json!({
                    "minutes_since_open": b.minutes_since_open,
                    "ofi_return_r": r_val,
                    "count": b.count,
                })
            })
            .collect();

        json!({
            "tracker": "OfiTracker",
            "n_days": self.n_days,
            "per_scale": scales,
            "component_fractions": {
                "add_fraction": add_frac,
                "cancel_fraction": cancel_frac,
                "trade_fraction": trade_frac,
            },
            "regime_intensity": self.regime_abs_ofi.finalize(),
            "intraday_ofi_curve": curve,
            "intraday_ofi_return_r_curve": ofi_return_r_curve,
            "cumulative_delta": {
                "mean_eod_delta": self.daily_deltas.mean(),
                "std_eod_delta": self.daily_deltas.std(),
            },
            "aggressor_ratio": aggressor_ratio,
        })
    }

    fn name(&self) -> &str {
        "OfiTracker"
    }
}
// Local format function removed — replaced with hft_statistics::time::format_scale_label
// (imported as `format_label` alias to preserve existing call sites)
#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg_ts(action: Action, side: Side, ts: i64) -> MboMessage {
        MboMessage::new(1, action, side, 100_000_000_000, 100).with_timestamp(ts)
    }

    fn make_lob(bid: i64, ask: i64, bid_size: u32, ask_size: u32) -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(bid);
        lob.best_ask = Some(ask);
        lob.bid_prices[0] = bid;
        lob.ask_prices[0] = ask;
        lob.bid_sizes[0] = bid_size;
        lob.ask_sizes[0] = ask_size;
        lob.triggering_action = Some(Action::Add);
        lob
    }

    #[test]
    fn test_ofi_positive_on_bid_increase() {
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob1, 3);

        let lob2 = make_lob(100_000_000_000, 100_010_000_000, 200, 100);
        tracker.process_event(
            &make_msg_ts(Action::Add, Side::Bid, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert!(!tracker.day_ofi_vals.is_empty());
        let total: f64 = tracker.day_ofi_vals.iter().sum();
        assert!(
            total > 0.0,
            "Bid size increase should produce positive OFI, got {}",
            total
        );
    }

    #[test]
    fn test_filters_invalid_books() {
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let empty_lob = LobState::new(10);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &empty_lob, 3);

        assert!(tracker.day_ofi_vals.is_empty());
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = OfiTracker::new(&[1.0, 5.0], 1000);
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "OfiTracker");
        assert!(report.get("per_scale").is_some());
        assert!(report.get("component_fractions").is_some());
        assert!(report.get("regime_intensity").is_some());
        assert!(report.get("aggressor_ratio").is_some());
        assert!(
            report.get("intraday_ofi_return_r_curve").is_some(),
            "finalize must include intraday_ofi_return_r_curve"
        );
    }

    #[test]
    fn test_reset_day_clears_state() {
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob, 3);

        tracker.end_of_day();
        tracker.reset_day();

        assert!(tracker.day_ofi_vals.is_empty());
        assert!(!tracker.initialized);
    }

    #[test]
    fn test_ofi_bid_size_increase_exact_value() {
        // Bid price unchanged ($100.00), bid size 100→150. Ask unchanged.
        // bid_contrib = bid_size - prev_bid_size = 150 - 100 = +50
        // ask_contrib = -(ask_size - prev_ask_size) = -(100 - 100) = 0
        // OFI = 50 + 0 = 50.0
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob1, 3);

        let lob2 = make_lob(100_000_000_000, 100_010_000_000, 150, 100);
        tracker.process_event(
            &make_msg_ts(Action::Add, Side::Bid, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert_eq!(tracker.day_ofi_vals.len(), 1);
        assert!(
            (tracker.day_ofi_vals[0] - 50.0).abs() < 1e-10,
            "Bid size +50: expected OFI=50.0, got {}",
            tracker.day_ofi_vals[0]
        );
    }

    #[test]
    fn test_ofi_bid_price_drop_bearish() {
        // Bid price drops: $100.00 → $99.99. prev_bid_size=100.
        // bid_contrib = -prev_bid_size = -100 (bid < prev_bid)
        // ask_contrib = -(100 - 100) = 0 (ask unchanged)
        // OFI = -100
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob1, 3);

        let lob2 = make_lob(99_990_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(
            &make_msg_ts(Action::Trade, Side::Ask, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert!(
            (tracker.day_ofi_vals[0] - (-100.0)).abs() < 1e-10,
            "Bid price drop: expected OFI=-100, got {}",
            tracker.day_ofi_vals[0]
        );
    }

    #[test]
    fn test_ofi_ask_price_improves() {
        // Ask drops (improves): $100.01 → $100.005. ask_size=200.
        // bid_contrib = bid_size - prev_bid_size = 100 - 100 = 0 (bid unchanged)
        // ask_contrib: ask < prev_ask → -ask_size = -200
        // OFI = 0 + (-200) = -200
        // Per Cont et al.: ask improvement = sellers posting at better prices = sell pressure.
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_010_000_000, 100, 200);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Ask, ts), &lob1, 3);

        let lob2 = make_lob(100_000_000_000, 100_005_000_000, 100, 200);
        tracker.process_event(
            &make_msg_ts(Action::Add, Side::Ask, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert!(
            (tracker.day_ofi_vals[0] - (-200.0)).abs() < 1e-10,
            "Ask price drops (improve): OFI should be -200, got {}",
            tracker.day_ofi_vals[0]
        );
    }

    #[test]
    fn test_ofi_only_size_changes() {
        // No price changes. Bid size 100→120 (+20), ask size 100→130 (+30).
        // bid_contrib = bid_size - prev_bid_size = 20
        // ask_contrib = -(ask_size - prev_ask_size) = -30
        // OFI = 20 + (-30) = -10
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob1, 3);

        let lob2 = make_lob(100_000_000_000, 100_010_000_000, 120, 130);
        tracker.process_event(
            &make_msg_ts(Action::Add, Side::Bid, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert!(
            (tracker.day_ofi_vals[0] - (-10.0)).abs() < 1e-10,
            "Size only: bid+20, ask+30, OFI should be -10, got {}",
            tracker.day_ofi_vals[0]
        );
    }

    #[test]
    fn test_spread_bucket_classification() {
        assert_eq!(spread_bucket(0.01), Some(0)); // exactly 1 tick
        assert_eq!(spread_bucket(0.014), Some(0)); // < 1.5 ticks
        assert_eq!(spread_bucket(0.02), Some(1)); // exactly 2 ticks
        assert_eq!(spread_bucket(0.024), Some(1)); // < 2.5 ticks
        assert_eq!(spread_bucket(0.03), Some(2)); // 3 ticks
        assert_eq!(spread_bucket(0.04), Some(2)); // 4 ticks
        assert_eq!(spread_bucket(0.05), Some(3)); // 5 ticks
        assert_eq!(spread_bucket(0.10), Some(3)); // 10 ticks
        assert_eq!(spread_bucket(0.0), None); // zero spread
        assert_eq!(spread_bucket(-0.01), None); // negative
        assert_eq!(spread_bucket(f64::NAN), None); // NaN
        assert_eq!(spread_bucket(f64::INFINITY), None); // Inf
    }

    #[test]
    fn test_spread_bucket_boundary_precision() {
        // 1.5 ticks = $0.015 → should be bucket 1 (2-tick)
        assert_eq!(spread_bucket(0.015), Some(1));
        // 2.5 ticks = $0.025 → should be bucket 2 (3-4 tick)
        assert_eq!(spread_bucket(0.025), Some(2));
        // 4.5 ticks = $0.045 → should be bucket 3 (5+ tick)
        assert_eq!(spread_bucket(0.045), Some(3));
    }

    #[test]
    fn test_spread_bucket_correlations_empty() {
        let sbc = SpreadBucketCorrelations::new();
        let corrs = sbc.correlations();
        assert_eq!(corrs.len(), N_SPREAD_BUCKETS);
        for (r, n) in &corrs {
            assert!(r.is_nan(), "Empty bucket should produce NaN correlation");
            assert_eq!(*n, 0);
        }
    }

    #[test]
    fn test_spread_bucket_correlations_perfect_positive() {
        let mut sbc = SpreadBucketCorrelations::new();
        // bucket 0: perfect positive correlation (y = x)
        for i in 0..100 {
            let x = i as f64;
            sbc.add(0, x, x);
        }
        let corrs = sbc.correlations();
        assert!(
            (corrs[0].0 - 1.0).abs() < 1e-10,
            "Perfect positive: expected r=1.0, got {}",
            corrs[0].0
        );
        assert_eq!(corrs[0].1, 100);
        assert!(corrs[1].0.is_nan(), "Bucket 1 should be NaN (no data)");
    }

    #[test]
    fn test_spread_bucket_correlations_negative() {
        let mut sbc = SpreadBucketCorrelations::new();
        // bucket 2: perfect negative correlation (y = -x)
        for i in 0..50 {
            let x = i as f64;
            sbc.add(2, x, -x);
        }
        let corrs = sbc.correlations();
        assert!(
            (corrs[2].0 - (-1.0)).abs() < 1e-10,
            "Perfect negative: expected r=-1.0, got {}",
            corrs[2].0
        );
    }

    #[test]
    fn test_conditional_ofi_in_finalize() {
        let tracker = OfiTracker::new(&[1.0, 300.0], 1000);
        let report = tracker.finalize();
        let ps = &report["per_scale"]["1s"];
        assert!(
            ps.get("conditional_ofi_return").is_some(),
            "finalize output must include conditional_ofi_return"
        );
        let cond = &ps["conditional_ofi_return"];
        for label in &SPREAD_BUCKET_LABELS {
            assert!(
                cond.get(*label).is_some(),
                "conditional_ofi_return must include bucket '{}'",
                label
            );
            let bucket = &cond[*label];
            assert!(bucket.get("r").is_some());
            assert!(bucket.get("n").is_some());
        }
    }

    #[test]
    fn test_ofi_bid_up_and_ask_down() {
        // Bid price up ($100.00→$100.01), bid_size=150.
        // Ask price down ($100.02→$100.015), ask_size=200.
        // bid_contrib: bid > prev_bid → +bid_size = +150
        // ask_contrib: ask < prev_ask → -ask_size = -200
        // OFI = 150 + (-200) = -50
        let mut tracker = OfiTracker::new(&[1.0], 1000);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;

        let lob1 = make_lob(100_000_000_000, 100_020_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob1, 3);

        let lob2 = make_lob(100_010_000_000, 100_015_000_000, 150, 200);
        tracker.process_event(
            &make_msg_ts(Action::Add, Side::Bid, ts + NS_PER_SECOND),
            &lob2,
            3,
        );

        assert!(
            (tracker.day_ofi_vals[0] - (-50.0)).abs() < 1e-10,
            "Bid up + ask down: OFI should be -50, got {}",
            tracker.day_ofi_vals[0]
        );
    }
}
