//! Real-data integration tests for the MBO Statistical Profiler.
//!
//! Runs the profiler ONCE on 2025-02-03 NVDA XNAS data (18.5M events)
//! and validates every tracker's output against golden reference values.
//!
//! # Running
//!
//! ```bash
//! cargo test --release --test integration_real_data -- --ignored
//! ```
//!
//! Uses `--release` for ~20s runtime instead of ~10min in debug mode.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use mbo_statistical_profiler::config::{
    InputConfig, OutputConfig, ProfilerConfig, TrackerConfig,
};
use mbo_statistical_profiler::profiler;
use mbo_statistical_profiler::trackers::*;
use mbo_statistical_profiler::AnalysisTracker;

const HOT_STORE_DIR: &str = "../data/hot_store";
const TEST_FILE: &str = "xnas-itch-20250203.mbo.dbn";

/// Singleton: run the profiler exactly once across all tests.
static PROFILER_RESULT: OnceLock<Option<HashMap<String, serde_json::Value>>> = OnceLock::new();

fn get_reports() -> &'static HashMap<String, serde_json::Value> {
    PROFILER_RESULT
        .get_or_init(|| {
            let path = PathBuf::from(HOT_STORE_DIR).join(TEST_FILE);
            if !path.exists() {
                eprintln!("SKIP: {} not found", path.display());
                return None;
            }

            let config = ProfilerConfig {
                input: InputConfig {
                    hot_store_dir: Some(PathBuf::from(HOT_STORE_DIR)),
                    data_dir: None,
                    filename_pattern: "xnas-itch-{date}.mbo.dbn".to_string(),
                    symbol: "NVDA".to_string(),
                    exchange: "XNAS".to_string(),
                    date_start: Some("2025-02-03".to_string()),
                    date_end: Some("2025-02-03".to_string()),
                },
                trackers: TrackerConfig::default(),
                timescales: vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0],
                reservoir_capacity: 10_000,
                vpin_volume_bar_size: 5_000,
                vpin_window_bars: 50,
                output: OutputConfig {
                    output_dir: PathBuf::from("/tmp/profiler_integration_test"),
                    write_summaries: false,
                },
            };

            let mut trackers: Vec<Box<dyn AnalysisTracker>> = vec![
                Box::new(QualityTracker::new()),
                Box::new(ReturnTracker::new(&config.timescales, config.reservoir_capacity)),
                Box::new(OfiTracker::new(&config.timescales, config.reservoir_capacity)),
                Box::new(SpreadTracker::new(config.reservoir_capacity)),
                Box::new(VolatilityTracker::new(&config.timescales)),
                Box::new(JumpTracker::new(1.0, 2.0)),
                Box::new(NoiseTracker::new()),
                Box::new(DepthTracker::new()),
                Box::new(TradeTracker::new()),
                Box::new(LifecycleTracker::new()),
                Box::new(LiquidityTracker::new()),
                Box::new(VpinTracker::new(config.vpin_volume_bar_size, config.vpin_window_bars)),
            ];

            let result = profiler::run(&config, &mut trackers)
                .expect("Profiler failed on real data");

            Some(result.reports.into_iter().collect())
        })
        .as_ref()
        .expect("Test data not available — run with real NVDA hot store data")
}

// =============================================================================
// QualityTracker — exact integer golden values
// =============================================================================

#[test]
#[ignore]
fn golden_quality_total_events() {
    let q = &get_reports()["QualityTracker"];
    assert_eq!(q["total_events"].as_u64().unwrap(), 18_476_041,
        "Golden: Python MBO-LOB-analyzer, 2025-02-03, n_mbo_rows = 18,476,041");
}

#[test]
#[ignore]
fn golden_quality_action_counts() {
    let ad = &get_reports()["QualityTracker"]["action_distribution"];
    assert_eq!(ad["add_count"].as_u64().unwrap(), 8_652_416);
    assert_eq!(ad["cancel_count"].as_u64().unwrap(), 8_748_922);
    assert_eq!(ad["trade_count"].as_u64().unwrap(), 1_074_702);
    assert_eq!(ad["clear_count"].as_u64().unwrap(), 1);
}

#[test]
#[ignore]
fn golden_quality_action_percentages() {
    let ad = &get_reports()["QualityTracker"]["action_distribution"];
    let add_pct = ad["add_pct"].as_f64().unwrap();
    assert!((add_pct - 46.83).abs() < 0.1, "Add%: expected ~46.83, got {}", add_pct);
    let cancel_pct = ad["cancel_pct"].as_f64().unwrap();
    assert!((cancel_pct - 47.35).abs() < 0.1, "Cancel%: expected ~47.35, got {}", cancel_pct);
}

#[test]
#[ignore]
fn golden_quality_book_consistency() {
    let bc = &get_reports()["QualityTracker"]["book_consistency"];
    let valid = bc["valid_pct"].as_f64().unwrap();
    assert!(valid > 99.99, "Valid% expected >99.99, got {}", valid);
    assert_eq!(bc["crossed_pct"].as_f64().unwrap(), 0.0);
}

// =============================================================================
// SpreadTracker — float tolerance against Python golden
// =============================================================================

#[test]
#[ignore]
fn golden_spread_mean_usd() {
    let s = &get_reports()["SpreadTracker"];
    let mean = s["distribution_usd"]["mean"].as_f64().unwrap();
    // Golden: $0.02246 (Python), Rust actual: $0.02246
    assert!((mean - 0.02246).abs() < 0.005,
        "Spread mean: expected ~$0.02246, got ${:.6}", mean);
}

#[test]
#[ignore]
fn golden_spread_width_classification() {
    let wc = &get_reports()["SpreadTracker"]["width_classification"];
    let one_tick = wc["one_tick_pct"].as_f64().unwrap();
    assert!(one_tick > 30.0 && one_tick < 80.0,
        "1-tick% expected 30-80%, got {:.2}", one_tick);
}

// =============================================================================
// OfiTracker — structure + sanity
// =============================================================================

#[test]
#[ignore]
fn golden_ofi_all_scales_populated() {
    let o = &get_reports()["OfiTracker"];
    for scale in &["1s", "5s", "10s", "30s", "1m", "5m"] {
        assert!(o["per_scale"][scale].is_object(), "Missing OFI scale: {}", scale);
    }
}

#[test]
#[ignore]
fn golden_ofi_component_fractions_sum_to_one() {
    let cf = &get_reports()["OfiTracker"]["component_fractions"];
    let sum = cf["add_fraction"].as_f64().unwrap()
        + cf["cancel_fraction"].as_f64().unwrap()
        + cf["trade_fraction"].as_f64().unwrap();
    assert!((sum - 1.0).abs() < 0.01,
        "Component fractions sum: expected ~1.0, got {:.4}", sum);
}

// =============================================================================
// ReturnTracker
// =============================================================================

#[test]
#[ignore]
fn golden_returns_multi_scale() {
    let r = &get_reports()["ReturnTracker"];
    for scale in &["1s", "5s", "10s", "30s", "1m", "5m"] {
        let n = r["per_scale"][scale]["n_returns"].as_u64().unwrap();
        assert!(n > 0, "Zero returns at scale {}", scale);
    }
    let dd = r["daily_max_drawdown"]["mean"].as_f64().unwrap();
    assert!(dd > 0.0, "Max drawdown should be >0, got {}", dd);
}

// =============================================================================
// VolatilityTracker
// =============================================================================

#[test]
#[ignore]
fn golden_volatility_rv_positive() {
    let v = &get_reports()["VolatilityTracker"];
    assert_eq!(v["n_days"].as_u64().unwrap(), 1);
    let ann = v["daily_annualized_vol"]["mean"].as_f64().unwrap();
    assert!(ann > 10.0 && ann < 200.0,
        "Annualized vol expected 10-200%, got {}%", ann);
}

// =============================================================================
// TradeTracker — exact golden count
// =============================================================================

#[test]
#[ignore]
fn golden_trades_count() {
    let t = &get_reports()["TradeTracker"];
    assert_eq!(t["total_trades"].as_u64().unwrap(), 1_074_702,
        "Golden: Trade count must match Python exactly");
}

// =============================================================================
// LifecycleTracker — range-based golden
// =============================================================================

#[test]
#[ignore]
fn golden_lifecycle_fill_rate() {
    let l = &get_reports()["LifecycleTracker"];
    let fr = l["fill_rate"].as_f64().unwrap();
    assert!(fr > 0.02 && fr < 0.10,
        "Fill rate {} outside [0.02, 0.10]", fr);
    let cta = l["cancel_to_add_ratio"].as_f64().unwrap();
    assert!(cta > 0.9 && cta < 1.2,
        "Cancel-to-add {} outside [0.9, 1.2]", cta);
}

// =============================================================================
// Remaining trackers — structural + sanity
// =============================================================================

#[test]
#[ignore]
fn golden_depth_profile() {
    let d = &get_reports()["DepthTracker"];
    assert_eq!(d["bid_depth_profile"].as_array().unwrap().len(), 10);
    assert_eq!(d["ask_depth_profile"].as_array().unwrap().len(), 10);
    let di = d["depth_imbalance"]["mean"].as_f64().unwrap();
    assert!(di >= -1.0 && di <= 1.0, "DI mean {} outside [-1,1]", di);
}

#[test]
#[ignore]
fn golden_liquidity_effective_spread() {
    let l = &get_reports()["LiquidityTracker"];
    let es = l["effective_spread_bps"]["mean"].as_f64().unwrap();
    assert!(es > 0.0 && es < 20.0,
        "Effective spread {} bps outside [0, 20]", es);
}

#[test]
#[ignore]
fn golden_noise_signature_plot() {
    let n = &get_reports()["NoiseTracker"];
    assert_eq!(n["signature_plot"].as_array().unwrap().len(), 20);
    let snr = n["daily_snr"]["mean"].as_f64().unwrap();
    assert!(snr > 0.0, "SNR mean should be >0, got {}", snr);
}

#[test]
#[ignore]
fn golden_jumps_fraction_valid() {
    let j = &get_reports()["JumpTracker"];
    assert_eq!(j["n_days"].as_u64().unwrap(), 1);
    let jf = j["daily_jump_fraction"]["mean"].as_f64().unwrap();
    assert!(jf >= 0.0 && jf <= 1.0, "Jump fraction {} outside [0,1]", jf);
}

// =============================================================================
// VpinTracker
// =============================================================================

#[test]
#[ignore]
fn golden_vpin_in_valid_range() {
    let v = &get_reports()["VpinTracker"];
    assert_eq!(v["n_days"].as_u64().unwrap(), 1);
    assert!(v["n_volume_bars_total"].as_u64().unwrap() > 100,
        "Should have >100 volume bars for an active trading day");

    let vpin_mean = v["vpin_distribution"]["mean"].as_f64().unwrap();
    assert!(vpin_mean > 0.0 && vpin_mean < 1.0,
        "VPIN mean {} outside (0, 1)", vpin_mean);
}

// =============================================================================
// OFI-Spread Correlation (enhanced OfiTracker)
// =============================================================================

#[test]
#[ignore]
fn golden_ofi_spread_correlation_exists() {
    let o = &get_reports()["OfiTracker"];
    for scale in &["1s", "5s", "10s", "30s", "1m", "5m"] {
        let sc = &o["per_scale"][scale]["ofi_spread_correlation"];
        assert!(sc.is_object(), "Missing ofi_spread_correlation at scale {}", scale);
        let lag0 = sc["lag_0"].as_f64().unwrap();
        assert!(lag0.is_finite(), "OFI-spread correlation lag_0 at {} should be finite", scale);
    }
}

// =============================================================================
// Trade Clustering (enhanced TradeTracker)
// =============================================================================

#[test]
#[ignore]
fn golden_trade_clustering_valid() {
    let t = &get_reports()["TradeTracker"];

    let cf = t["clustering"]["cluster_fraction"].as_f64().unwrap();
    assert!(cf >= 0.0 && cf <= 1.0,
        "Cluster fraction {} outside [0, 1]", cf);

    assert!(t["clustering"]["total_clusters"].as_u64().unwrap() > 0,
        "Active day should have trade clusters");

    let tt_pct = t["trade_through"]["pct"].as_f64().unwrap();
    assert!(tt_pct >= 0.0 && tt_pct <= 100.0,
        "Trade-through pct {} outside [0, 100]", tt_pct);

    let itt_mean = t["inter_trade_time"]["mean"].as_f64().unwrap();
    assert!(itt_mean > 0.0, "Inter-trade time mean should be >0, got {}", itt_mean);
}

// =============================================================================
// Cross-tracker consistency
// =============================================================================

#[test]
#[ignore]
fn golden_all_trackers_present() {
    let reports = get_reports();
    assert_eq!(reports.len(), 12, "Should have exactly 12 trackers");
    let expected = [
        "QualityTracker", "ReturnTracker", "OfiTracker", "SpreadTracker",
        "VolatilityTracker", "JumpTracker", "NoiseTracker", "DepthTracker",
        "TradeTracker", "LifecycleTracker", "LiquidityTracker", "VpinTracker",
    ];
    for name in &expected {
        assert!(reports.contains_key(*name), "Missing: {}", name);
    }
}
