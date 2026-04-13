//! Profiler configuration, TOML-driven.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level profiler configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilerConfig {
    /// Input data configuration.
    pub input: InputConfig,

    /// Which analysis trackers to enable.
    #[serde(default)]
    pub trackers: TrackerConfig,

    /// Timescales for multi-scale analysis (seconds).
    #[serde(default = "default_timescales")]
    pub timescales: Vec<f64>,

    /// Reservoir sampling capacity for distribution estimation.
    #[serde(default = "default_reservoir_capacity")]
    pub reservoir_capacity: usize,

    /// Volume bar size for VPIN computation (shares per bar).
    #[serde(default = "default_vpin_bar_size")]
    pub vpin_volume_bar_size: u64,

    /// Number of volume bars in VPIN rolling window.
    #[serde(default = "default_vpin_window")]
    pub vpin_window_bars: usize,

    /// Output configuration.
    #[serde(default)]
    pub output: OutputConfig,
}

/// Input data source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Path to hot store directory (decompressed .dbn files).
    pub hot_store_dir: Option<PathBuf>,

    /// Path to raw data directory (.dbn.zst files).
    pub data_dir: Option<PathBuf>,

    /// Filename pattern (e.g., "xnas-itch-{date}.mbo.dbn").
    pub filename_pattern: String,

    /// Symbol name for metadata.
    #[serde(default = "default_symbol")]
    pub symbol: String,

    /// Exchange identifier for metadata.
    #[serde(default = "default_exchange")]
    pub exchange: String,

    /// Optional date range filter (inclusive).
    pub date_start: Option<String>,
    pub date_end: Option<String>,
}

/// Which analysis trackers to enable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    #[serde(default = "default_true")]
    pub quality: bool,
    #[serde(default = "default_true")]
    pub ofi: bool,
    #[serde(default = "default_true")]
    pub spread: bool,
    #[serde(default = "default_true")]
    pub returns: bool,
    #[serde(default = "default_true")]
    pub volatility: bool,
    #[serde(default = "default_true")]
    pub lifecycle: bool,
    #[serde(default = "default_true")]
    pub trades: bool,
    #[serde(default = "default_true")]
    pub depth: bool,
    #[serde(default = "default_true")]
    pub liquidity: bool,
    #[serde(default = "default_true")]
    pub jumps: bool,
    #[serde(default = "default_true")]
    pub noise: bool,
    #[serde(default = "default_true")]
    pub vpin: bool,
    #[serde(default)]
    pub cross_scale_ofi: bool,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            quality: true,
            ofi: true,
            spread: true,
            returns: true,
            volatility: true,
            lifecycle: true,
            trades: true,
            depth: true,
            liquidity: true,
            jumps: true,
            noise: true,
            vpin: true,
            cross_scale_ofi: false,
        }
    }
}

/// Output configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Directory for JSON output files.
    #[serde(default = "default_output_dir")]
    pub output_dir: PathBuf,

    /// Whether to write human-readable TXT summaries alongside JSON.
    #[serde(default = "default_true")]
    pub write_summaries: bool,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            output_dir: PathBuf::from("profiler_output"),
            write_summaries: true,
        }
    }
}

fn default_timescales() -> Vec<f64> {
    vec![1.0, 5.0, 10.0, 30.0, 60.0, 300.0]
}

fn default_reservoir_capacity() -> usize {
    10_000
}

fn default_vpin_bar_size() -> u64 {
    5_000
}

fn default_vpin_window() -> usize {
    50
}

fn default_symbol() -> String {
    "NVDA".to_string()
}

fn default_exchange() -> String {
    "XNAS".to_string()
}

fn default_true() -> bool {
    true
}

fn default_output_dir() -> PathBuf {
    PathBuf::from("profiler_output")
}

impl ProfilerConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }
}
