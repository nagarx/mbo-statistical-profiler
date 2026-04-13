//! # LifecycleTracker
//!
//! ## Purpose
//!
//! Tracks the full lifecycle of individual orders from placement to resolution
//! (fill, cancel, or expiry). This is the most complex tracker, maintaining
//! active order state via `AHashMap` and computing lifetime distributions,
//! fill rates, cancel ratios, action transition probabilities, and
//! duration-size correlations.
//!
//! ## Statistics Computed
//!
//! | Statistic | Formula | Units |
//! |-----------|---------|-------|
//! | Order lifetime dist | `duration = resolve_ts - add_ts` | seconds |
//! | Fill rate | `filled / total_resolved` | ratio [0, 1] |
//! | Cancel-to-add ratio | `cancels / adds` | ratio |
//! | Transition matrix 4×4 | `P(next_action \| current_action)` | probability |
//! | Duration-size correlation | `corr(log(duration), log(size))` | dimensionless |
//! | Partial fill fraction | `partial_fills / total_fills` | ratio |
//! | Regime-conditional lifetime | Mean lifetime per regime | seconds |
//!
//! ## Formulas
//!
//! - Fill rate: `fill_rate = n_filled / n_resolved`
//! - Cancel-to-add ratio: `CTA = n_cancels / n_adds`
//! - Transition matrix states: {Add=0, Modify=1, Cancel=2, Trade=3}
//!   `P[i][j] = count(i → j) / sum_j(count(i → j))`
//!
//! ## References
//!
//! - Cont, R., Stoikov, S. & Talreja, R. (2014). "A stochastic model for
//!   order book dynamics." Operations Research, 58(3), 549-563.
//! - Hasbrouck, J. (2018). "High-frequency quoting: Short-term volatility
//!   in bids and offers." Journal of Financial and Quantitative Analysis,
//!   53(2), 613-641.

use ahash::AHashMap;
use mbo_lob_reconstructor::{Action, LobState, MboMessage, Side};
use serde_json::json;

use crate::statistics::{
    RegimeAccumulator, StreamingDistribution, TransitionMatrix, WelfordAccumulator,
};
use crate::AnalysisTracker;

const NS_PER_SECOND_F64: f64 = 1_000_000_000.0;

/// Maximum tracked active orders before eviction.
const MAX_ACTIVE_ORDERS: usize = 500_000;

/// Transition matrix state indices.
const STATE_ADD: usize = 0;
const STATE_MODIFY: usize = 1;
const STATE_CANCEL: usize = 2;
const STATE_TRADE: usize = 3;

/// Active order state for lifecycle tracking.
struct ActiveOrder {
    add_timestamp: i64,
    size: u32,
    #[allow(dead_code)]
    side: Side,
    n_modifies: u32,
    n_partial_fills: u32,
    last_action: usize,
    regime_at_add: u8,
}

/// Order lifecycle analysis tracker.
pub struct LifecycleTracker {
    active_orders: AHashMap<u64, ActiveOrder>,

    lifetime_dist: StreamingDistribution,
    modify_count_dist: StreamingDistribution,

    n_adds: u64,
    n_cancels: u64,
    n_fills: u64,
    n_partial_fills: u64,
    n_resolved: u64,

    transition_matrix: TransitionMatrix<4>,

    duration_size_pairs: Vec<(f64, f64)>,
    duration_size_welford_duration: WelfordAccumulator,
    duration_size_welford_size: WelfordAccumulator,

    regime_lifetime: RegimeAccumulator,
    regime_fill_rate: RegimeAccumulator,

    n_evictions: u64,
    n_days: u32,
}

impl LifecycleTracker {
    pub fn new() -> Self {
        Self {
            active_orders: AHashMap::with_capacity(100_000),
            lifetime_dist: StreamingDistribution::new(10_000),
            modify_count_dist: StreamingDistribution::new(10_000),
            n_adds: 0,
            n_cancels: 0,
            n_fills: 0,
            n_partial_fills: 0,
            n_resolved: 0,
            transition_matrix: TransitionMatrix::new(),
            duration_size_pairs: Vec::new(),
            duration_size_welford_duration: WelfordAccumulator::new(),
            duration_size_welford_size: WelfordAccumulator::new(),
            regime_lifetime: RegimeAccumulator::new(),
            regime_fill_rate: RegimeAccumulator::new(),
            n_evictions: 0,
            n_days: 0,
        }
    }

    fn action_state(action: Action) -> Option<usize> {
        match action {
            Action::Add => Some(STATE_ADD),
            Action::Modify => Some(STATE_MODIFY),
            Action::Cancel => Some(STATE_CANCEL),
            Action::Trade | Action::Fill => Some(STATE_TRADE),
            _ => None,
        }
    }

    fn resolve_order(&mut self, order: ActiveOrder, resolve_ts: i64) {
        let duration_ns = resolve_ts - order.add_timestamp;
        if duration_ns < 0 {
            return;
        }

        let duration_s = duration_ns as f64 / NS_PER_SECOND_F64;
        self.lifetime_dist.add(duration_s);
        self.modify_count_dist.add(order.n_modifies as f64);
        self.n_resolved += 1;

        self.regime_lifetime.add(order.regime_at_add, duration_s);

        if order.size > 0 && duration_s > 0.0 {
            let log_duration = duration_s.ln();
            let log_size = (order.size as f64).ln();
            if log_duration.is_finite() && log_size.is_finite() {
                self.duration_size_pairs.push((log_duration, log_size));
                self.duration_size_welford_duration.update(log_duration);
                self.duration_size_welford_size.update(log_size);
            }
        }
    }

    fn evict_oldest(&mut self) {
        if self.active_orders.len() <= MAX_ACTIVE_ORDERS {
            return;
        }

        let n_to_evict = self.active_orders.len() - MAX_ACTIVE_ORDERS + MAX_ACTIVE_ORDERS / 10;
        let mut oldest: Vec<(u64, i64)> = self
            .active_orders
            .iter()
            .map(|(&id, order)| (id, order.add_timestamp))
            .collect();
        oldest.sort_by_key(|&(_, ts)| ts);

        for &(id, _) in oldest.iter().take(n_to_evict) {
            self.active_orders.remove(&id);
            self.n_evictions += 1;
        }
    }
}

impl Default for LifecycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisTracker for LifecycleTracker {
    fn process_event(
        &mut self,
        msg: &MboMessage,
        _lob_state: &LobState,
        regime: u8,
        _day_epoch_ns: i64,
    ) {
        let ts = match msg.timestamp {
            Some(t) => t,
            None => return,
        };

        let _current_state = match Self::action_state(msg.action) {
            Some(s) => s,
            None => return,
        };

        match msg.action {
            Action::Add => {
                self.n_adds += 1;

                if self.active_orders.len() >= MAX_ACTIVE_ORDERS {
                    self.evict_oldest();
                }

                self.active_orders.insert(
                    msg.order_id,
                    ActiveOrder {
                        add_timestamp: ts,
                        size: msg.size,
                        side: msg.side,
                        n_modifies: 0,
                        n_partial_fills: 0,
                        last_action: STATE_ADD,
                        regime_at_add: regime,
                    },
                );
            }
            Action::Modify => {
                if let Some(order) = self.active_orders.get_mut(&msg.order_id) {
                    self.transition_matrix.record(order.last_action, STATE_MODIFY);
                    order.last_action = STATE_MODIFY;
                    order.n_modifies += 1;
                    if msg.size > 0 {
                        order.size = msg.size;
                    }
                }
            }
            Action::Cancel => {
                self.n_cancels += 1;

                if let Some(order) = self.active_orders.remove(&msg.order_id) {
                    self.transition_matrix.record(order.last_action, STATE_CANCEL);
                    self.resolve_order(order, ts);
                    self.regime_fill_rate.add(regime, 0.0);
                }
            }
            Action::Trade | Action::Fill => {
                if let Some(order) = self.active_orders.get_mut(&msg.order_id) {
                    self.transition_matrix.record(order.last_action, STATE_TRADE);
                    order.last_action = STATE_TRADE;

                    let trade_size = msg.size;
                    if trade_size >= order.size {
                        // Full fill — remove order
                        self.n_fills += 1;
                        let resolved = self.active_orders.remove(&msg.order_id).unwrap();
                        self.resolve_order(resolved, ts);
                        self.regime_fill_rate.add(regime, 1.0);
                    } else {
                        // Partial fill
                        order.size -= trade_size;
                        order.n_partial_fills += 1;
                        self.n_partial_fills += 1;
                    }
                }
            }
            _ => {}
        }
    }

    fn end_of_day(&mut self, _day_index: u32) {
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.active_orders.clear();
    }

    fn finalize(&self) -> serde_json::Value {
        let fill_rate = if self.n_resolved > 0 {
            self.n_fills as f64 / self.n_resolved as f64
        } else {
            0.0
        };

        let cancel_to_add = if self.n_adds > 0 {
            self.n_cancels as f64 / self.n_adds as f64
        } else {
            0.0
        };

        let partial_fill_frac = if self.n_fills + self.n_partial_fills > 0 {
            self.n_partial_fills as f64 / (self.n_fills + self.n_partial_fills) as f64
        } else {
            0.0
        };

        let duration_size_corr = compute_correlation(&self.duration_size_pairs);

        let state_labels = ["Add", "Modify", "Cancel", "Trade"];

        json!({
            "tracker": "LifecycleTracker",
            "n_days": self.n_days,
            "n_adds": self.n_adds,
            "n_cancels": self.n_cancels,
            "n_fills": self.n_fills,
            "n_partial_fills": self.n_partial_fills,
            "n_resolved": self.n_resolved,
            "n_evictions": self.n_evictions,
            "fill_rate": fill_rate,
            "cancel_to_add_ratio": cancel_to_add,
            "partial_fill_fraction": partial_fill_frac,
            "lifetime_distribution": self.lifetime_dist.summary(),
            "modify_count_distribution": self.modify_count_dist.summary(),
            "transition_matrix": {
                "states": state_labels,
                "probabilities": self.transition_matrix.probability_matrix(),
                "counts": self.transition_matrix.count_matrix(),
                "total_transitions": self.transition_matrix.total(),
            },
            "duration_size_correlation": duration_size_corr,
            "regime_lifetime": self.regime_lifetime.finalize(),
            "regime_fill_rate": self.regime_fill_rate.finalize(),
        })
    }

    fn name(&self) -> &str {
        "LifecycleTracker"
    }
}

/// Pearson correlation from (x, y) pairs.
fn compute_correlation(pairs: &[(f64, f64)]) -> f64 {
    let n = pairs.len();
    if n < 3 {
        return f64::NAN;
    }
    let n_f = n as f64;
    let mean_x = pairs.iter().map(|p| p.0).sum::<f64>() / n_f;
    let mean_y = pairs.iter().map(|p| p.1).sum::<f64>() / n_f;

    let mut cov = 0.0f64;
    let mut var_x = 0.0f64;
    let mut var_y = 0.0f64;
    for &(x, y) in pairs {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom < 1e-15 {
        0.0
    } else {
        cov / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_add(order_id: u64, ts: i64, size: u32) -> MboMessage {
        MboMessage::new(order_id, Action::Add, Side::Bid, 100_000_000_000, size)
            .with_timestamp(ts)
    }

    fn make_cancel(order_id: u64, ts: i64) -> MboMessage {
        MboMessage::new(order_id, Action::Cancel, Side::Bid, 100_000_000_000, 0)
            .with_timestamp(ts)
    }

    fn make_trade(order_id: u64, ts: i64, size: u32) -> MboMessage {
        MboMessage::new(order_id, Action::Trade, Side::Bid, 100_000_000_000, size)
            .with_timestamp(ts)
    }

    fn make_modify(order_id: u64, ts: i64, new_size: u32) -> MboMessage {
        MboMessage::new(
            order_id,
            Action::Modify,
            Side::Bid,
            100_000_000_000,
            new_size,
        )
        .with_timestamp(ts)
    }

    fn make_lob() -> LobState {
        let mut lob = LobState::new(10);
        lob.best_bid = Some(100_000_000_000);
        lob.best_ask = Some(100_010_000_000);
        lob.bid_sizes[0] = 100;
        lob.ask_sizes[0] = 100;
        lob
    }

    const NS: i64 = 1_000_000_000;

    #[test]
    fn test_add_cancel_lifecycle() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        assert_eq!(tracker.n_adds, 1);
        assert_eq!(tracker.active_orders.len(), 1);

        tracker.process_event(&make_cancel(1, 15 * NS), &lob, 3, 0);
        assert_eq!(tracker.n_cancels, 1);
        assert_eq!(tracker.n_resolved, 1);
        assert_eq!(tracker.active_orders.len(), 0);

        let lifetime = tracker.lifetime_dist.mean();
        assert!(
            (lifetime - 5.0).abs() < 1e-6,
            "Expected 5s lifetime, got {}",
            lifetime
        );
    }

    #[test]
    fn test_add_trade_full_fill() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        tracker.process_event(&make_trade(1, 12 * NS, 100), &lob, 3, 0);

        assert_eq!(tracker.n_fills, 1);
        assert_eq!(tracker.n_resolved, 1);
        assert_eq!(tracker.active_orders.len(), 0);
    }

    #[test]
    fn test_partial_fill() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        tracker.process_event(&make_trade(1, 11 * NS, 30), &lob, 3, 0);

        assert_eq!(tracker.n_partial_fills, 1);
        assert_eq!(tracker.active_orders.len(), 1);

        let order = tracker.active_orders.get(&1).unwrap();
        assert_eq!(order.size, 70);
        assert_eq!(order.n_partial_fills, 1);

        // Complete the fill
        tracker.process_event(&make_trade(1, 12 * NS, 70), &lob, 3, 0);
        assert_eq!(tracker.n_fills, 1);
        assert_eq!(tracker.active_orders.len(), 0);
    }

    #[test]
    fn test_transition_matrix() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        // Add → Modify → Cancel
        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        tracker.process_event(&make_modify(1, 11 * NS, 90), &lob, 3, 0);
        tracker.process_event(&make_cancel(1, 12 * NS), &lob, 3, 0);

        // Add → Modify transition
        assert_eq!(tracker.transition_matrix.count(STATE_ADD, STATE_MODIFY), 1);
        // Modify → Cancel transition
        assert_eq!(
            tracker.transition_matrix.count(STATE_MODIFY, STATE_CANCEL),
            1
        );
    }

    #[test]
    fn test_fill_rate() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        // 2 orders: 1 filled, 1 cancelled
        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        tracker.process_event(&make_trade(1, 11 * NS, 100), &lob, 3, 0);

        tracker.process_event(&make_add(2, 10 * NS, 100), &lob, 3, 0);
        tracker.process_event(&make_cancel(2, 11 * NS), &lob, 3, 0);

        let report = tracker.finalize();
        let fill_rate = report["fill_rate"].as_f64().unwrap();
        assert!(
            (fill_rate - 0.5).abs() < 1e-10,
            "Expected fill rate 0.5, got {}",
            fill_rate
        );
    }

    #[test]
    fn test_finalize_structure() {
        let tracker = LifecycleTracker::new();
        let report = tracker.finalize();

        assert_eq!(report["tracker"], "LifecycleTracker");
        assert!(report.get("fill_rate").is_some());
        assert!(report.get("cancel_to_add_ratio").is_some());
        assert!(report.get("transition_matrix").is_some());
        assert!(report.get("lifetime_distribution").is_some());
        assert!(report.get("duration_size_correlation").is_some());
        assert!(report.get("regime_lifetime").is_some());
    }

    #[test]
    fn test_reset_day_clears_active() {
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_add(1, 10 * NS, 100), &lob, 3, 0);
        assert_eq!(tracker.active_orders.len(), 1);

        tracker.reset_day();
        assert_eq!(tracker.active_orders.len(), 0);
    }

    #[test]
    fn test_fill_rate_exact_value() {
        // 10 orders: 3 fills, 7 cancels → fill_rate = 3/10 = 0.3
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        for i in 1..=10u64 {
            tracker.process_event(&make_add(i, (10 + i as i64) * NS, 100), &lob, 3, 0);
        }
        for i in 1..=3u64 {
            tracker.process_event(&make_trade(i, (20 + i as i64) * NS, 100), &lob, 3, 0);
        }
        for i in 4..=10u64 {
            tracker.process_event(&make_cancel(i, (20 + i as i64) * NS), &lob, 3, 0);
        }

        assert_eq!(tracker.n_fills, 3);
        assert_eq!(tracker.n_resolved, 10);
        let report = tracker.finalize();
        let fill_rate = report["fill_rate"].as_f64().unwrap();
        assert!(
            (fill_rate - 0.3).abs() < 1e-10,
            "fill_rate = 3/10 = 0.3, got {}",
            fill_rate
        );
    }

    #[test]
    fn test_cancel_to_add_exact() {
        // n_adds=10, n_cancels=9 → CTA = 9/10 = 0.9
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        for i in 1..=10u64 {
            tracker.process_event(&make_add(i, (10 + i as i64) * NS, 100), &lob, 3, 0);
        }
        for i in 1..=9u64 {
            tracker.process_event(&make_cancel(i, (20 + i as i64) * NS), &lob, 3, 0);
        }

        assert_eq!(tracker.n_adds, 10);
        assert_eq!(tracker.n_cancels, 9);
        let report = tracker.finalize();
        let cta = report["cancel_to_add_ratio"].as_f64().unwrap();
        assert!(
            (cta - 0.9).abs() < 1e-10,
            "CTA = 9/10 = 0.9, got {}",
            cta
        );
    }

    #[test]
    fn test_duration_seconds_exact() {
        // Add at ts=1*NS, cancel at ts=2*NS → duration = 1.0 second
        let mut tracker = LifecycleTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_add(1, NS, 100), &lob, 3, 0);
        tracker.process_event(&make_cancel(1, 2 * NS), &lob, 3, 0);

        let lifetime = tracker.lifetime_dist.mean();
        assert!(
            (lifetime - 1.0).abs() < 1e-9,
            "Duration = (2e9 - 1e9) / 1e9 = 1.0s, got {}",
            lifetime
        );
    }
}
