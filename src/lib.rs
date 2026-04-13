//! MBO Statistical Profiler
//!
//! High-performance statistical profiler for MBO/LOB market microstructure analysis.
//! Processes raw .dbn files in a single pass through LOB reconstruction + composable
//! analysis trackers, producing JSON statistical profiles.
//!
//! # Architecture
//!
//! The profiler imports `mbo_lob_reconstructor` as a library (same pattern as
//! `feature-extractor-MBO-LOB`) and adds analysis-specific trackers on top.
//!
//! ```text
//! .dbn file → DbnLoader → LobReconstructor → LobState
//!                                               │
//!                                     ┌─────────┼─────────┐
//!                                     ▼         ▼         ▼
//!                              QualityTracker  OfiTracker  ...
//!                                     │         │         │
//!                                     └─────────┼─────────┘
//!                                               ▼
//!                                         JSON profiles
//! ```
//!
//! # Design Principles
//!
//! - **Single pass**: All trackers process events simultaneously
//! - **Bounded memory**: Streaming accumulators (Welford, reservoir sampling)
//! - **Composable**: Each tracker is independent, enable/disable via config
//! - **Deterministic**: Same input → same output (seeded RNG for reservoir)
//! - **Zero dependency on feature-extractor**: Uses reconstructor library directly

pub mod config;
pub mod profiler;
pub mod trackers;

pub use hft_statistics::statistics;
pub use hft_statistics::time;

use mbo_lob_reconstructor::{LobState, MboMessage};

/// Trait implemented by all analysis trackers.
///
/// Each tracker processes MBO events alongside the reconstructed LOB state,
/// accumulates statistics across days, and produces a JSON report at finalization.
///
/// # Lifecycle
///
/// 1. `process_event()` — called for every MBO event with the resulting LOB state
/// 2. `end_of_day()` — called once when a day boundary is detected
/// 3. `reset_day()` — called to prepare for the next day (if day-level state exists)
/// 4. `finalize()` — called once after all data is processed to produce the report
pub trait AnalysisTracker: Send {
    /// Process a single MBO event with the resulting LOB state.
    ///
    /// # Arguments
    ///
    /// * `msg` — The raw MBO message
    /// * `lob_state` — The LOB state after processing this message
    /// * `regime` — Pre-computed intraday time regime (0-6) for this timestamp
    /// * `day_epoch_ns` — Midnight UTC in nanoseconds for the current trading day
    fn process_event(
        &mut self,
        msg: &MboMessage,
        lob_state: &LobState,
        regime: u8,
        day_epoch_ns: i64,
    );

    /// Called when a day boundary is detected.
    fn end_of_day(&mut self, day_index: u32);

    /// Reset day-level state in preparation for the next day.
    fn reset_day(&mut self);

    /// Produce the final JSON report after all data has been processed.
    fn finalize(&self) -> serde_json::Value;

    /// Human-readable name for logging and report identification.
    fn name(&self) -> &str;
}
