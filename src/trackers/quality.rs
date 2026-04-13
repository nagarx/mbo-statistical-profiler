//! Data quality tracker — validates data integrity and characterizes the dataset.
//!
//! Equivalent to MBO-LOB-analyzer's `DataQualityAnalyzer`. Produces:
//! - Total row counts (events processed)
//! - Action distribution (Add%, Cancel%, Trade%, etc.)
//! - Book consistency distribution (Valid%, Empty%, Locked%, Crossed%)
//! - Time regime distribution (per-regime event fraction)
//! - Per-day event counts

use mbo_lob_reconstructor::{Action, BookConsistency, LobState, MboMessage};
use serde_json::json;

use crate::time::N_REGIMES;
use crate::AnalysisTracker;

/// Data quality analysis tracker.
pub struct QualityTracker {
    total_events: u64,
    action_counts: [u64; 7],  // Add, Modify, Cancel, Trade, Fill, Clear, None
    book_consistency_counts: [u64; 4],  // Valid, Empty, Locked, Crossed
    regime_counts: [u64; N_REGIMES],
    n_days: u32,
    per_day_counts: Vec<(u32, u64)>,  // (day_index, event_count)
    current_day_count: u64,
}

impl QualityTracker {
    pub fn new() -> Self {
        Self {
            total_events: 0,
            action_counts: [0; 7],
            book_consistency_counts: [0; 4],
            regime_counts: [0; N_REGIMES],
            n_days: 0,
            per_day_counts: Vec::new(),
            current_day_count: 0,
        }
    }

    fn action_index(action: Action) -> usize {
        match action {
            Action::Add => 0,
            Action::Modify => 1,
            Action::Cancel => 2,
            Action::Trade => 3,
            Action::Fill => 4,
            Action::Clear => 5,
            Action::None => 6,
        }
    }

    fn consistency_index(consistency: BookConsistency) -> usize {
        match consistency {
            BookConsistency::Valid => 0,
            BookConsistency::Empty => 1,
            BookConsistency::Locked => 2,
            BookConsistency::Crossed => 3,
        }
    }
}

impl AnalysisTracker for QualityTracker {
    fn process_event(
        &mut self,
        msg: &MboMessage,
        lob_state: &LobState,
        regime: u8,
        _day_epoch_ns: i64,
    ) {
        self.total_events += 1;
        self.current_day_count += 1;
        self.action_counts[Self::action_index(msg.action)] += 1;
        self.book_consistency_counts[Self::consistency_index(lob_state.check_consistency())] += 1;

        if (regime as usize) < N_REGIMES {
            self.regime_counts[regime as usize] += 1;
        }
    }

    fn end_of_day(&mut self, day_index: u32) {
        self.per_day_counts
            .push((day_index, self.current_day_count));
        self.n_days += 1;
    }

    fn reset_day(&mut self) {
        self.current_day_count = 0;
    }

    fn finalize(&self) -> serde_json::Value {
        let total = self.total_events as f64;
        let action_pct = |idx: usize| -> f64 {
            if total > 0.0 {
                self.action_counts[idx] as f64 / total * 100.0
            } else {
                0.0
            }
        };
        let consistency_pct = |idx: usize| -> f64 {
            if total > 0.0 {
                self.book_consistency_counts[idx] as f64 / total * 100.0
            } else {
                0.0
            }
        };
        let regime_pct = |idx: usize| -> f64 {
            if total > 0.0 {
                self.regime_counts[idx] as f64 / total * 100.0
            } else {
                0.0
            }
        };

        let mean_per_day = if self.n_days > 0 {
            total / self.n_days as f64
        } else {
            0.0
        };

        json!({
            "tracker": "QualityTracker",
            "total_events": self.total_events,
            "n_days": self.n_days,
            "mean_events_per_day": mean_per_day,
            "action_distribution": {
                "add_pct": action_pct(0),
                "modify_pct": action_pct(1),
                "cancel_pct": action_pct(2),
                "trade_pct": action_pct(3),
                "fill_pct": action_pct(4),
                "clear_pct": action_pct(5),
                "none_pct": action_pct(6),
                "add_count": self.action_counts[0],
                "modify_count": self.action_counts[1],
                "cancel_count": self.action_counts[2],
                "trade_count": self.action_counts[3],
                "fill_count": self.action_counts[4],
                "clear_count": self.action_counts[5],
                "none_count": self.action_counts[6],
            },
            "book_consistency": {
                "valid_pct": consistency_pct(0),
                "empty_pct": consistency_pct(1),
                "locked_pct": consistency_pct(2),
                "crossed_pct": consistency_pct(3),
            },
            "regime_distribution": {
                "pre_market_pct": regime_pct(0),
                "open_auction_pct": regime_pct(1),
                "morning_pct": regime_pct(2),
                "midday_pct": regime_pct(3),
                "afternoon_pct": regime_pct(4),
                "close_auction_pct": regime_pct(5),
                "post_market_pct": regime_pct(6),
            },
        })
    }

    fn name(&self) -> &str {
        "QualityTracker"
    }
}

impl Default for QualityTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(action: Action) -> MboMessage {
        MboMessage::new(1, action, mbo_lob_reconstructor::Side::Bid, 100_000_000_000, 100)
            .with_timestamp(1_000_000_000)
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
    fn test_counts_events() {
        let mut tracker = QualityTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_msg(Action::Add), &lob, 3, 0);
        tracker.process_event(&make_msg(Action::Cancel), &lob, 3, 0);
        tracker.process_event(&make_msg(Action::Trade), &lob, 3, 0);

        assert_eq!(tracker.total_events, 3);
        assert_eq!(tracker.action_counts[0], 1); // Add
        assert_eq!(tracker.action_counts[2], 1); // Cancel
        assert_eq!(tracker.action_counts[3], 1); // Trade
    }

    #[test]
    fn test_day_boundary() {
        let mut tracker = QualityTracker::new();
        let lob = make_lob();

        tracker.process_event(&make_msg(Action::Add), &lob, 3, 0);
        tracker.process_event(&make_msg(Action::Add), &lob, 3, 0);
        tracker.end_of_day(0);

        assert_eq!(tracker.n_days, 1);
        assert_eq!(tracker.per_day_counts.len(), 1);
        assert_eq!(tracker.per_day_counts[0], (0, 2));

        tracker.reset_day();
        tracker.process_event(&make_msg(Action::Add), &lob, 3, 0);
        tracker.end_of_day(1);

        assert_eq!(tracker.n_days, 2);
        assert_eq!(tracker.total_events, 3);
    }

    #[test]
    fn test_finalize_produces_valid_json() {
        let mut tracker = QualityTracker::new();
        let lob = make_lob();

        for _ in 0..10 {
            tracker.process_event(&make_msg(Action::Add), &lob, 3, 0);
        }
        tracker.end_of_day(0);

        let report = tracker.finalize();
        assert_eq!(report["total_events"], 10);
        assert_eq!(report["n_days"], 1);
        assert!(report["action_distribution"]["add_pct"].as_f64().unwrap() > 99.0);
    }

    #[test]
    fn test_book_consistency_tracking() {
        let mut tracker = QualityTracker::new();

        let valid_lob = make_lob();
        tracker.process_event(&make_msg(Action::Add), &valid_lob, 3, 0);

        let empty_lob = LobState::new(10);
        tracker.process_event(&make_msg(Action::Add), &empty_lob, 0, 0);

        let report = tracker.finalize();
        assert!(report["book_consistency"]["valid_pct"].as_f64().unwrap() > 0.0);
        assert!(report["book_consistency"]["empty_pct"].as_f64().unwrap() > 0.0);
    }
}
