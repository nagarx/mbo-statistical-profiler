//! CLI for MBO statistical profiling.
//!
//! Processes raw .dbn files through LOB reconstruction and composable
//! analysis trackers, producing JSON statistical profiles.
//!
//! # Usage
//!
//! ```bash
//! cargo run --release --bin profile_mbo -- --config configs/default.toml
//! ```

use std::path::PathBuf;

use mbo_statistical_profiler::config::ProfilerConfig;
use mbo_statistical_profiler::profiler;
use mbo_statistical_profiler::trackers::*;
use mbo_statistical_profiler::AnalysisTracker;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = parse_args();

    let config = match ProfilerConfig::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config from {}: {}", config_path.display(), e);
            std::process::exit(1);
        }
    };

    log::info!(
        "MBO Statistical Profiler v{}",
        env!("CARGO_PKG_VERSION")
    );
    log::info!(
        "Symbol: {}, Exchange: {}",
        config.input.symbol,
        config.input.exchange
    );

    let mut trackers: Vec<Box<dyn AnalysisTracker>> = Vec::new();

    if config.trackers.quality {
        trackers.push(Box::new(QualityTracker::new()));
    }
    if config.trackers.returns {
        trackers.push(Box::new(ReturnTracker::new(
            &config.timescales,
            config.reservoir_capacity,
        )));
    }
    if config.trackers.ofi {
        trackers.push(Box::new(OfiTracker::new(
            &config.timescales,
            config.reservoir_capacity,
        )));
    }
    if config.trackers.spread {
        trackers.push(Box::new(SpreadTracker::new(config.reservoir_capacity)));
    }
    if config.trackers.volatility {
        trackers.push(Box::new(VolatilityTracker::new(&config.timescales)));
    }
    if config.trackers.jumps {
        let primary_scale = config.timescales.first().copied().unwrap_or(1.0);
        trackers.push(Box::new(JumpTracker::new(primary_scale, 2.0)));
    }
    if config.trackers.noise {
        trackers.push(Box::new(NoiseTracker::new()));
    }
    if config.trackers.depth {
        trackers.push(Box::new(DepthTracker::new()));
    }
    if config.trackers.trades {
        trackers.push(Box::new(TradeTracker::new()));
    }
    if config.trackers.lifecycle {
        trackers.push(Box::new(LifecycleTracker::new()));
    }
    if config.trackers.liquidity {
        trackers.push(Box::new(LiquidityTracker::new()));
    }
    if config.trackers.vpin {
        trackers.push(Box::new(VpinTracker::new(
            config.vpin_volume_bar_size,
            config.vpin_window_bars,
        )));
    }
    if config.trackers.cross_scale_ofi {
        trackers.push(Box::new(CrossScaleOfiTracker::new(&config.timescales)));
    }

    if trackers.is_empty() {
        eprintln!("No trackers enabled in configuration. Enable at least one tracker.");
        std::process::exit(1);
    }

    log::info!("Enabled {} tracker(s)", trackers.len());

    match profiler::run(&config, &mut trackers) {
        Ok(result) => {
            if let Err(e) = profiler::write_output(&config, &result) {
                eprintln!("Failed to write output: {}", e);
                std::process::exit(1);
            }

            log::info!(
                "Done: {} days, {} events, {:.1}s ({:.0} evt/s)",
                result.n_days,
                result.total_events,
                result.elapsed_secs,
                result.total_events as f64 / result.elapsed_secs.max(0.001),
            );
        }
        Err(e) => {
            eprintln!("Profiling failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_args() -> PathBuf {
    let args: Vec<String> = std::env::args().collect();

    let mut config_path = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" | "-c" => {
                i += 1;
                if i < args.len() {
                    config_path = Some(PathBuf::from(&args[i]));
                } else {
                    eprintln!("--config requires a path argument");
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                println!("MBO Statistical Profiler");
                println!();
                println!("Usage: profile_mbo --config <path.toml>");
                println!();
                println!("Options:");
                println!("  -c, --config <path>  Path to TOML configuration file");
                println!("  -h, --help           Show this help");
                std::process::exit(0);
            }
            other => {
                eprintln!("Unknown argument: {}", other);
                eprintln!("Usage: profile_mbo --config <path.toml>");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    config_path.unwrap_or_else(|| {
        eprintln!("Missing required --config argument");
        eprintln!("Usage: profile_mbo --config <path.toml>");
        std::process::exit(1);
    })
}
