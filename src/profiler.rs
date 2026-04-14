//! Single-pass profiling engine.
//!
//! Coordinates LOB reconstruction and analysis tracker dispatch across
//! all trading days. Each .dbn file is processed sequentially; within each
//! day, all enabled trackers receive every event simultaneously.

use std::path::PathBuf;
use std::time::Instant;

use mbo_lob_reconstructor::{
    DbnLoader, HotStoreConfig, HotStoreManager, LobReconstructor, LobState,
};

use crate::config::ProfilerConfig;
use crate::time::regime::{midnight_utc_ns, time_regime, utc_offset_for_date};
use crate::AnalysisTracker;

/// Result of profiling a complete dataset.
pub struct ProfileResult {
    pub n_days: u32,
    pub total_events: u64,
    pub elapsed_secs: f64,
    pub reports: Vec<(String, serde_json::Value)>,
}

/// Run the profiler on a set of .dbn files.
///
/// Discovers files via the hot store or data directory, processes each day
/// through LOB reconstruction + tracker dispatch, and produces JSON reports.
pub fn run(
    config: &ProfilerConfig,
    trackers: &mut Vec<Box<dyn AnalysisTracker>>,
) -> Result<ProfileResult, Box<dyn std::error::Error>> {
    let start = Instant::now();

    let hot_store = config
        .input
        .hot_store_dir
        .as_ref()
        .map(|dir| HotStoreManager::new(HotStoreConfig::new(dir)));

    let files = discover_files(config, hot_store.as_ref())?;

    if files.is_empty() {
        return Err("No .dbn files found matching the configuration".into());
    }

    log::info!("Discovered {} day files to process", files.len());

    let mut total_events: u64 = 0;
    let mut day_index: u32 = 0;

    for (i, (date_str, file_path)) in files.iter().enumerate() {
        let day_start = Instant::now();

        let year: i32 = date_str[0..4].parse()?;
        let month: u32 = date_str[5..7].parse()?;
        let day: u32 = date_str[8..10].parse()?;

        let utc_offset = utc_offset_for_date(year, month, day);
        // CORRECTED semantics (was day_epoch_ns(y,m,d,offset) which produced
        // midnight LOCAL as UTC ns — incompatible with resample_to_grid).
        // midnight_utc_ns returns midnight UTC, matching resample_to_grid +
        // IntradayCurveAccumulator + infer_day_params conventions.
        let day_epoch = midnight_utc_ns(year, month, day);

        // Notify trackers of new day: cache utc_offset + day_epoch_ns as fields
        // for those that need them. Trackers that don't override begin_day get
        // the default no-op.
        for tracker in trackers.iter_mut() {
            tracker.begin_day(day_index, utc_offset, day_epoch);
        }

        let mut lob = LobReconstructor::new(10);
        let mut state_buf = LobState::new(10);
        let mut day_events: u64 = 0;

        let loader = DbnLoader::new(file_path)?;

        for msg in loader.iter_messages()? {
            lob.process_message_into(&msg, &mut state_buf)?;

            let regime = msg
                .timestamp
                .map(|ts| time_regime(ts, utc_offset))
                .unwrap_or(0);

            // Hot path: 3 args. Day-level context already cached via begin_day.
            for tracker in trackers.iter_mut() {
                tracker.process_event(&msg, &state_buf, regime);
            }

            day_events += 1;
        }

        for tracker in trackers.iter_mut() {
            tracker.end_of_day();
        }

        total_events += day_events;
        day_index += 1;

        let day_elapsed = day_start.elapsed().as_secs_f64();
        let throughput = day_events as f64 / day_elapsed.max(0.001);
        let eta_secs = if i > 0 {
            let avg = start.elapsed().as_secs_f64() / (i + 1) as f64;
            avg * (files.len() - i - 1) as f64
        } else {
            0.0
        };

        log::info!(
            "[{}/{}] {} — {:.1}s, {} events, {:.0} evt/s, ETA {:.0}s",
            i + 1,
            files.len(),
            date_str,
            day_elapsed,
            day_events,
            throughput,
            eta_secs,
        );

        for tracker in trackers.iter_mut() {
            tracker.reset_day();
        }
    }

    let reports: Vec<(String, serde_json::Value)> = trackers
        .iter()
        .map(|t| (t.name().to_string(), t.finalize()))
        .collect();

    let elapsed = start.elapsed().as_secs_f64();
    log::info!(
        "Profiling complete: {} days, {} events, {:.1}s ({:.0} evt/s)",
        day_index,
        total_events,
        elapsed,
        total_events as f64 / elapsed.max(0.001),
    );

    Ok(ProfileResult {
        n_days: day_index,
        total_events,
        elapsed_secs: elapsed,
        reports,
    })
}

/// Discover .dbn files sorted by date.
///
/// Resolves through hot store when available, falls back to data_dir.
fn discover_files(
    config: &ProfilerConfig,
    hot_store: Option<&HotStoreManager>,
) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    let pattern = &config.input.filename_pattern;

    let search_dir = config
        .input
        .hot_store_dir
        .as_ref()
        .or(config.input.data_dir.as_ref())
        .ok_or("Either hot_store_dir or data_dir must be specified")?;

    if !search_dir.exists() {
        return Err(format!("Data directory does not exist: {}", search_dir.display()).into());
    }

    let date_placeholder = "{date}";
    let (prefix, suffix) = if let Some(pos) = pattern.find(date_placeholder) {
        (&pattern[..pos], &pattern[pos + date_placeholder.len()..])
    } else {
        return Err("filename_pattern must contain {date} placeholder".into());
    };

    let mut files: Vec<(String, PathBuf)> = Vec::new();

    for entry in std::fs::read_dir(search_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        let name = filename.to_string_lossy();

        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }

        let date_part = &name[prefix.len()..name.len() - suffix.len()];
        if date_part.len() != 8 || date_part.chars().any(|c| !c.is_ascii_digit()) {
            continue;
        }

        let date_str = format!(
            "{}-{}-{}",
            &date_part[0..4],
            &date_part[4..6],
            &date_part[6..8]
        );

        if let Some(ref start) = config.input.date_start {
            if date_str < *start {
                continue;
            }
        }
        if let Some(ref end) = config.input.date_end {
            if date_str > *end {
                continue;
            }
        }

        let file_path = if let Some(hs) = hot_store {
            hs.resolve(entry.path())
        } else {
            entry.path()
        };

        files.push((date_str, file_path));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(files)
}

/// Write profiler output to disk.
pub fn write_output(
    config: &ProfilerConfig,
    result: &ProfileResult,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = &config.output.output_dir;
    std::fs::create_dir_all(output_dir)?;

    // Provenance: every runtime config field that affects output values is recorded
    // so a JSON consumer can audit which parameters produced this dataset. The
    // 2026-04 incident (multi-stock VPIN bar sizes silently using defaults due to
    // a TOML schema misplacement) was hard to detect because vpin_volume_bar_size
    // was not in provenance — fixed by including all runtime keys here.
    let provenance = serde_json::json!({
        "profiler_version": env!("CARGO_PKG_VERSION"),
        "symbol": config.input.symbol,
        "exchange": config.input.exchange,
        "n_days": result.n_days,
        "total_events": result.total_events,
        "runtime_secs": result.elapsed_secs,
        "throughput_events_per_sec": result.total_events as f64 / result.elapsed_secs.max(0.001),
        "timescales": config.timescales,
        "reservoir_capacity": config.reservoir_capacity,
        "vpin_volume_bar_size": config.vpin_volume_bar_size,
        "vpin_window_bars": config.vpin_window_bars,
    });

    for (i, (name, report)) in result.reports.iter().enumerate() {
        let mut full_report = report.clone();
        if let Some(obj) = full_report.as_object_mut() {
            obj.insert("_provenance".to_string(), provenance.clone());
        }

        let json_path = output_dir.join(format!("{:02}_{}.json", i + 1, name));
        let json_str = serde_json::to_string_pretty(&full_report)?;
        std::fs::write(&json_path, &json_str)?;
        log::info!("Wrote {}", json_path.display());
    }

    Ok(())
}
