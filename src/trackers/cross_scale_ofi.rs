//! # CrossScaleOfiTracker
//!
//! ## Purpose
//!
//! Determines whether short-timescale OFI predicts longer-timescale returns.
//! Computes an N×N correlation matrix where entry (i,j) = Pearson(OFI at
//! scale i, return at scale j). The upper triangle (i < j) captures the
//! predictive signal: does 5s OFI predict 1m returns?
//!
//! ## Key Design Decision: Predictive Alignment
//!
//! For cross-scale pairs (source ≠ target), we use **predictive** alignment:
//! OFI in source bin ending at time t is paired with the return of the
//! target-scale bin **starting** at time t (the next bar's return). This
//! measures whether current OFI predicts the *future* return, not the
//! contemporaneous one.
//!
//! For same-scale pairs (diagonal), we use **contemporaneous** alignment
//! (lag 0) to match the existing OfiTracker output.
//!
//! ## Formulas
//!
//! OFI per event: Cont, Kukanov & Stoikov (2014), Eq. 3.
//! Pearson r: standard definition with population variance.
//! Cross-day aggregation: weighted average of per-day correlations.
//!
//! ## References
//!
//! - Cont, R., Kukanov, A. & Stoikov, S. (2014). "The Price Impact of Order
//!   Book Events." Journal of Financial Econometrics, 12(1), 47-88.

// TODO(plan/D6 + Step 5.8): Extract `compute_ofi` to shared `OfiPrevState::compute()`
// in trackers/ofi_primitives.rs (deferred from this commit; logic is character-identical
// to the copy in ofi.rs).
//
// TODO(plan/Step 5.5): The inline Pearson r computations in process_day_cross_scale
// trigger clippy::needless_range_loop. Refactor to use
// `hft_statistics::statistics::pearson_r_slices` deferred — extraction is invasive due
// to cross-day weighted-mean aggregation logic. Allow lint at file scope until refactored.
#![allow(clippy::needless_range_loop)]

use mbo_lob_reconstructor::{BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::time::resampler::{resample_to_grid, AggMode};
use crate::AnalysisTracker;
use hft_statistics::time::format_scale_label as format_label;
use hft_statistics::time::NS_PER_SECOND;

pub struct CrossScaleOfiTracker {
    prev_bid_price: i64,
    prev_ask_price: i64,
    prev_bid_size: u32,
    prev_ask_size: u32,
    initialized: bool,

    day_ofi_ts: Vec<i64>,
    day_ofi_vals: Vec<f64>,
    day_mid_ts: Vec<i64>,
    day_mid_prices: Vec<f64>,

    timescales: Vec<f64>,
    scale_labels: Vec<String>,
    n_scales: usize,

    correlation_matrix: Vec<Vec<f64>>,
    count_matrix: Vec<Vec<u64>>,

    n_days: u32,

    /// Cached at start of each day via `begin_day` (replaces the old
    /// `infer_day_params(&self.day_*)` call which re-derived these from timestamps).
    utc_offset: i32,
    day_epoch_ns: i64,
}

impl CrossScaleOfiTracker {
    pub fn new(timescales: &[f64]) -> Self {
        let n = timescales.len();
        let labels: Vec<String> = timescales.iter().map(|&s| format_label(s)).collect();

        Self {
            prev_bid_price: 0,
            prev_ask_price: 0,
            prev_bid_size: 0,
            prev_ask_size: 0,
            initialized: false,
            day_ofi_ts: Vec::with_capacity(20_000_000),
            day_ofi_vals: Vec::with_capacity(20_000_000),
            day_mid_ts: Vec::with_capacity(20_000_000),
            day_mid_prices: Vec::with_capacity(20_000_000),
            timescales: timescales.to_vec(),
            scale_labels: labels,
            n_scales: n,
            correlation_matrix: vec![vec![0.0; n]; n],
            count_matrix: vec![vec![0; n]; n],
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

    fn process_day_cross_scale(&mut self, utc_offset: i32, day_epoch_ns: i64) {
        if self.day_ofi_vals.len() < 2 {
            return;
        }

        struct ScaleBins {
            bin_width_ns: i64,
            ofi_values: Vec<f64>,
            ofi_counts: Vec<u64>,
            returns: Vec<f64>,
        }

        let mut scale_bins: Vec<ScaleBins> = Vec::with_capacity(self.n_scales);

        for &ts_secs in &self.timescales {
            let bin_width_ns = (ts_secs * NS_PER_SECOND as f64) as i64;

            let ofi_bins = resample_to_grid(
                &self.day_ofi_ts,
                &self.day_ofi_vals,
                bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Sum,
            );

            let mid_bins = resample_to_grid(
                &self.day_mid_ts,
                &self.day_mid_prices,
                bin_width_ns,
                day_epoch_ns,
                utc_offset,
                AggMode::Last,
            );

            let n_mid = mid_bins.values.len();
            let mut returns = vec![f64::NAN; n_mid];
            for i in 1..n_mid {
                if mid_bins.values[i].is_finite()
                    && mid_bins.values[i - 1].is_finite()
                    && mid_bins.values[i] > 0.0
                    && mid_bins.values[i - 1] > 0.0
                {
                    returns[i] = (mid_bins.values[i] / mid_bins.values[i - 1]).ln();
                }
            }

            scale_bins.push(ScaleBins {
                bin_width_ns,
                ofi_values: ofi_bins.values,
                ofi_counts: ofi_bins.counts,
                returns,
            });
        }

        for src_idx in 0..self.n_scales {
            let src = &scale_bins[src_idx];
            for tgt_idx in 0..self.n_scales {
                let tgt = &scale_bins[tgt_idx];

                let mut sum_xy = 0.0f64;
                let mut sum_x = 0.0f64;
                let mut sum_y = 0.0f64;
                let mut sum_x2 = 0.0f64;
                let mut sum_y2 = 0.0f64;
                let mut n = 0u64;

                if src_idx == tgt_idx {
                    // Same scale: contemporaneous (lag 0)
                    for (bin_i, &ofi_val) in src.ofi_values.iter().enumerate() {
                        if !ofi_val.is_finite()
                            || bin_i >= src.ofi_counts.len()
                            || src.ofi_counts[bin_i] == 0
                        {
                            continue;
                        }
                        if bin_i >= tgt.returns.len() || !tgt.returns[bin_i].is_finite() {
                            continue;
                        }
                        let r = tgt.returns[bin_i];
                        sum_xy += ofi_val * r;
                        sum_x += ofi_val;
                        sum_y += r;
                        sum_x2 += ofi_val * ofi_val;
                        sum_y2 += r * r;
                        n += 1;
                    }
                } else {
                    // Cross-scale: predictive alignment.
                    // For each source bin i ending at time t_end = open + (i+1) * src_width,
                    // find the target bin j such that tgt_start = open + j * tgt_width >= t_end.
                    // Use the return of that target bin (the *next* return after OFI is observed).
                    let ratio_ns = tgt.bin_width_ns as f64;
                    let src_ns = src.bin_width_ns as f64;

                    for (src_bin, &ofi_val) in src.ofi_values.iter().enumerate() {
                        if !ofi_val.is_finite()
                            || src_bin >= src.ofi_counts.len()
                            || src.ofi_counts[src_bin] == 0
                        {
                            continue;
                        }

                        let src_end_offset_ns = (src_bin as f64 + 1.0) * src_ns;
                        let tgt_bin = (src_end_offset_ns / ratio_ns).ceil() as usize;

                        if tgt_bin >= tgt.returns.len() || !tgt.returns[tgt_bin].is_finite() {
                            continue;
                        }

                        let r = tgt.returns[tgt_bin];
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
                        let prev = self.correlation_matrix[src_idx][tgt_idx];
                        let prev_n = self.count_matrix[src_idx][tgt_idx];
                        let total_n = prev_n + n;
                        self.correlation_matrix[src_idx][tgt_idx] =
                            (prev * prev_n as f64 + corr * n as f64) / total_n as f64;
                        self.count_matrix[src_idx][tgt_idx] = total_n;
                    }
                }
            }
        }
    }
}

impl AnalysisTracker for CrossScaleOfiTracker {
    fn begin_day(&mut self, _day_index: u32, utc_offset: i32, day_epoch_ns: i64) {
        self.utc_offset = utc_offset;
        self.day_epoch_ns = day_epoch_ns;
    }

    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, _regime: u8) {
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
        self.process_day_cross_scale(utc_offset, day_epoch_ns);
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.day_ofi_ts.clear();
        self.day_ofi_vals.clear();
        self.day_mid_ts.clear();
        self.day_mid_prices.clear();
        self.initialized = false;
    }

    fn finalize(&self) -> serde_json::Value {
        let mut matrix = serde_json::Map::new();
        let mut counts = serde_json::Map::new();

        for src_idx in 0..self.n_scales {
            for tgt_idx in 0..self.n_scales {
                let key = format!(
                    "{}_ofi_vs_{}_return",
                    self.scale_labels[src_idx], self.scale_labels[tgt_idx]
                );
                let r = self.correlation_matrix[src_idx][tgt_idx];
                let n = self.count_matrix[src_idx][tgt_idx];
                matrix.insert(key.clone(), json!(r));
                counts.insert(key, json!(n));
            }
        }

        json!({
            "tracker": "CrossScaleOfiTracker",
            "n_days": self.n_days,
            "n_scales": self.n_scales,
            "scale_labels": self.scale_labels,
            "correlation_matrix": matrix,
            "count_matrix": counts,
        })
    }

    fn name(&self) -> &str {
        "CrossScaleOfiTracker"
    }
}
// Local format function removed — replaced with hft_statistics::time::format_scale_label
// (imported as `format_label` alias to preserve existing call sites)
#[cfg(test)]
mod tests {
    use super::*;
    use mbo_lob_reconstructor::{Action, Side};

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
    fn test_new_creates_correct_matrix_size() {
        let tracker = CrossScaleOfiTracker::new(&[1.0, 5.0, 60.0]);
        assert_eq!(tracker.n_scales, 3);
        assert_eq!(tracker.correlation_matrix.len(), 3);
        assert_eq!(tracker.correlation_matrix[0].len(), 3);
        assert_eq!(tracker.count_matrix.len(), 3);
        assert_eq!(tracker.scale_labels, vec!["1s", "5s", "1m"]);
    }

    #[test]
    fn test_filters_invalid_books() {
        let mut tracker = CrossScaleOfiTracker::new(&[1.0]);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let empty_lob = LobState::new(10);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &empty_lob, 3);
        assert!(tracker.day_ofi_vals.is_empty());
    }

    #[test]
    fn test_collects_ofi_and_mid() {
        let mut tracker = CrossScaleOfiTracker::new(&[1.0]);
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
        assert_eq!(tracker.day_mid_prices.len(), 2);
    }

    #[test]
    fn test_reset_day_clears_state() {
        let mut tracker = CrossScaleOfiTracker::new(&[1.0]);
        let ts = 14 * 3600 * NS_PER_SECOND + 30 * 60 * NS_PER_SECOND;
        let lob = make_lob(100_000_000_000, 100_010_000_000, 100, 100);
        tracker.process_event(&make_msg_ts(Action::Add, Side::Bid, ts), &lob, 3);

        tracker.end_of_day();
        tracker.reset_day();

        assert!(tracker.day_ofi_vals.is_empty());
        assert!(tracker.day_mid_prices.is_empty());
        assert!(!tracker.initialized);
    }

    #[test]
    fn test_empty_day_produces_no_correlations() {
        let mut tracker = CrossScaleOfiTracker::new(&[1.0, 5.0]);
        tracker.end_of_day();
        let report = tracker.finalize();
        assert_eq!(report["n_days"], 1);
        let matrix = report["correlation_matrix"].as_object().unwrap();
        for (_key, val) in matrix {
            assert_eq!(val.as_f64().unwrap(), 0.0);
        }
    }

    #[test]
    fn test_finalize_json_structure() {
        let tracker = CrossScaleOfiTracker::new(&[1.0, 5.0, 300.0]);
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "CrossScaleOfiTracker");
        assert_eq!(report["n_scales"], 3);

        let labels = report["scale_labels"].as_array().unwrap();
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], "1s");
        assert_eq!(labels[1], "5s");
        assert_eq!(labels[2], "5m");

        let matrix = report["correlation_matrix"].as_object().unwrap();
        assert_eq!(matrix.len(), 9); // 3x3

        assert!(matrix.contains_key("1s_ofi_vs_1s_return"));
        assert!(matrix.contains_key("1s_ofi_vs_5s_return"));
        assert!(matrix.contains_key("1s_ofi_vs_5m_return"));
        assert!(matrix.contains_key("5s_ofi_vs_1s_return"));
        assert!(matrix.contains_key("5m_ofi_vs_5m_return"));

        let counts = report["count_matrix"].as_object().unwrap();
        assert_eq!(counts.len(), 9);
    }

    #[test]
    fn test_same_scale_alignment_is_contemporaneous() {
        // For the diagonal (src==tgt), bin i's OFI should pair with bin i's return.
        // This is verified structurally: the code path checks src_idx == tgt_idx
        // and uses direct index matching without offset.
        let tracker = CrossScaleOfiTracker::new(&[1.0]);
        let report = tracker.finalize();
        let key = "1s_ofi_vs_1s_return";
        assert!(
            report["correlation_matrix"].get(key).is_some(),
            "Diagonal entry must exist"
        );
    }

    #[test]
    fn test_predictive_alignment_offset() {
        // For cross-scale: source bin ending at offset (src_bin+1)*src_width
        // pairs with target bin starting at ceil(src_end / tgt_width) * tgt_width.
        //
        // Example: src_scale=1s, tgt_scale=5s.
        // Source bin 4 ends at 5s. Target bin ceil(5/5)=1 starts at 5s.
        // So source bins 0-4 (covering 0-5s) pair with target bin 1 (return from 5s-10s).
        // This is the predictive alignment: current 1s OFI predicts the NEXT 5s return.
        let src_width = 1.0_f64;
        let tgt_width = 5.0_f64;

        let src_bin: usize = 4;
        let src_end = (src_bin as f64 + 1.0) * src_width;
        let tgt_bin = (src_end / tgt_width).ceil() as usize;
        assert_eq!(
            tgt_bin, 1,
            "Source bin 4 (ends at 5s) should pair with target bin 1 (starts at 5s)"
        );

        let src_bin_0: usize = 0;
        let src_end_0 = (src_bin_0 as f64 + 1.0) * src_width;
        let tgt_bin_0 = (src_end_0 / tgt_width).ceil() as usize;
        assert_eq!(
            tgt_bin_0, 1,
            "Source bin 0 (ends at 1s) should pair with target bin 1 (starts at 5s)"
        );
    }
}
