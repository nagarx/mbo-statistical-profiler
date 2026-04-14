//! Profiler configuration, TOML-driven.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level profiler configuration.
///
/// `deny_unknown_fields` ensures that misplaced keys (e.g., a top-level key
/// accidentally placed under `[trackers]` in TOML) fail at parse time rather
/// than being silently dropped. This prevents config-schema drift from
/// silently overriding intended values with defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse every committed config file to verify contract integrity.
    ///
    /// Regression guard: on 2026-04-14 we discovered that all 27 committed
    /// configs had runtime keys (`timescales`, `reservoir_capacity`,
    /// `vpin_volume_bar_size`, `vpin_window_bars`) placed AFTER the
    /// `[trackers]` section header. Because these keys don't exist in
    /// `TrackerConfig`, they were silently dropped by serde's default
    /// behavior. `deny_unknown_fields` now makes this fail loudly, but a
    /// direct parse test ensures no future config file regresses.
    #[test]
    fn all_committed_configs_parse() {
        let configs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs");
        let mut count = 0;
        let mut failures: Vec<String> = Vec::new();

        for entry in std::fs::read_dir(&configs_dir).expect("configs/ directory exists") {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match ProfilerConfig::from_file(&path) {
                Ok(_) => count += 1,
                Err(e) => failures.push(format!("{}: {}", path.display(), e)),
            }
        }

        assert!(
            failures.is_empty(),
            "Config parse failures (deny_unknown_fields catches misplaced keys):\n{}",
            failures.join("\n")
        );
        assert!(count >= 27, "Expected at least 27 configs, found {}", count);
    }

    #[test]
    fn deny_unknown_fields_catches_misplaced_key() {
        // Regression: a `timescales` key placed inside `[trackers]` should fail,
        // not silently drop to the default.
        let bad_toml = r#"
[input]
filename_pattern = "test.dbn"

[trackers]
quality = true
timescales = [1.0, 5.0]
"#;
        let result: Result<ProfilerConfig, _> = toml::from_str(bad_toml);
        assert!(
            result.is_err(),
            "Expected parse error for misplaced key, but parse succeeded"
        );
    }

    #[test]
    fn deny_unknown_fields_catches_misplaced_runtime_key_under_input() {
        // Regression: a runtime key (e.g., vpin_volume_bar_size) placed inside
        // [input] should fail at parse time, not be silently ignored.
        let bad_toml = r#"
[input]
filename_pattern = "test.dbn"
vpin_volume_bar_size = 500

[trackers]
quality = true
"#;
        let result: Result<ProfilerConfig, _> = toml::from_str(bad_toml);
        assert!(
            result.is_err(),
            "Expected parse error for misplaced runtime key under [input]"
        );
    }

    #[test]
    fn deny_unknown_fields_catches_misplaced_key_under_output() {
        // Regression: any unknown key under [output] (typo or misplaced runtime
        // key) should fail loudly at parse time.
        let bad_toml = r#"
[input]
filename_pattern = "test.dbn"

[trackers]
quality = true

[output]
output_dir = "out"
timescales = [1.0]
"#;
        let result: Result<ProfilerConfig, _> = toml::from_str(bad_toml);
        assert!(
            result.is_err(),
            "Expected parse error for misplaced key under [output]"
        );
    }

    #[test]
    fn deny_unknown_fields_catches_top_level_typo() {
        // Regression: a typo in a top-level key (e.g., "timescale" singular,
        // or "vpin_volume_barsize") should fail at parse time. This is the
        // class of error that originally caused multi-stock VPIN bar sizes
        // to silently fall back to defaults.
        let bad_toml = r#"
timescale = [1.0]

[input]
filename_pattern = "test.dbn"

[trackers]
quality = true
"#;
        let result: Result<ProfilerConfig, _> = toml::from_str(bad_toml);
        assert!(
            result.is_err(),
            "Expected parse error for top-level typo (timescale instead of timescales)"
        );
    }

    #[test]
    fn semantic_value_propagation_crsp_config() {
        // Regression: parse the CRSP multi-stock config and assert the intended
        // VPIN bar size of 500 actually loaded into the struct field. This catches
        // the original bug class (silent default substitution) at the semantic
        // level — even if `deny_unknown_fields` were accidentally removed, this
        // test would still detect a runtime-key → struct-field disconnect.
        let crsp_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("configs")
            .join("xnas_crsp_134day.toml");
        let cfg = ProfilerConfig::from_file(&crsp_path).expect("CRSP config must parse");
        assert_eq!(
            cfg.vpin_volume_bar_size, 500,
            "CRSP intended vpin_volume_bar_size = 500 (thin book); \
             got {} — runtime key was not propagated from TOML to struct",
            cfg.vpin_volume_bar_size
        );
    }
}
