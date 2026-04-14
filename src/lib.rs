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
/// # Lifecycle (per trading day)
///
/// 1. `begin_day(day_index, utc_offset, day_epoch_ns)` — called ONCE before any
///    `process_event` calls. Trackers that need day-level context (utc_offset,
///    day_epoch_ns) should cache them as fields. Default impl is a no-op.
/// 2. `process_event(msg, lob_state, regime)` — called for every MBO event
///    with the resulting LOB state. Hot path; no day-level args (use cached state).
/// 3. `end_of_day()` — called once when a day boundary is detected (after the
///    last `process_event` of that day). Trackers use cached `utc_offset` /
///    `day_epoch_ns` from `begin_day` for any end-of-day computation.
/// 4. `reset_day()` — called to prepare for the next day (clears any per-day
///    buffers). The next `begin_day` call will set fresh day-level context.
///
/// # Final lifecycle
///
/// 5. `finalize()` — called once after all data is processed to produce the report.
pub trait AnalysisTracker: Send {
    /// Called once at the start of each trading day, before any `process_event`
    /// calls. Trackers that need `utc_offset` or `day_epoch_ns` should cache
    /// them as fields. Default implementation is a no-op for trackers that
    /// don't need day-level context (e.g., QualityTracker, DepthTracker).
    ///
    /// # Arguments
    ///
    /// * `day_index` — 0-based day counter
    /// * `utc_offset` — Hours from UTC (e.g., -5 for EST, -4 for EDT). DST-aware,
    ///   computed by the profiler from the trading date via
    ///   `hft_statistics::time::utc_offset_for_date`.
    /// * `day_epoch_ns` — Midnight UTC of the trading date, in nanoseconds since
    ///   Unix epoch. Suitable for use with `resample_to_grid` and
    ///   `IntradayCurveAccumulator::add`. Computed by the profiler via
    ///   `hft_statistics::time::midnight_utc_ns`.
    fn begin_day(&mut self, day_index: u32, utc_offset: i32, day_epoch_ns: i64) {
        let _ = (day_index, utc_offset, day_epoch_ns);
    }

    /// Process a single MBO event with the resulting LOB state.
    ///
    /// # Arguments
    ///
    /// * `msg` — The raw MBO message
    /// * `lob_state` — The LOB state after processing this message
    /// * `regime` — Pre-computed intraday time regime (0-6) for this timestamp
    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, regime: u8);

    /// Called when a day boundary is detected. Trackers use cached day context
    /// (set in `begin_day`) for any end-of-day computation that needs it.
    fn end_of_day(&mut self);

    /// Reset day-level state in preparation for the next day.
    fn reset_day(&mut self);

    /// Produce the final JSON report after all data has been processed.
    fn finalize(&self) -> serde_json::Value;

    /// Human-readable name for logging and report identification.
    fn name(&self) -> &str;
}
