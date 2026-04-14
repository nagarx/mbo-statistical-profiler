# MBO Statistical Profiler

High-performance Rust crate for MBO (Market-by-Order) / LOB (Limit Order Book) market microstructure statistical profiling. Processes raw `.dbn` files in a single pass through LOB reconstruction and 13 composable analysis trackers, producing JSON statistical profiles with 200+ metrics.

## Key Capabilities

- **13 analysis trackers** covering OFI, spread, returns, volatility, depth, trades, liquidity, order lifecycle, jumps, microstructure noise, VPIN, and cross-scale predictability
- **200+ computed metrics** with academic references (Cont-Kukanov-Stoikov, Barndorff-Nielsen-Shephard, Roll, Kyle, Easley-Lopez de Prado-O'Hara, Hill, Zhang-Mykland-Aït-Sahalia)
- **854K–2.9M events/sec** throughput (single-threaded, release mode)
- **Single-pass processing** — all trackers receive every event simultaneously
- **Bounded memory** — streaming accumulators (Welford, reservoir sampling), no full-dataset storage
- **TOML-driven configuration** — enable/disable trackers, set timescales, tune parameters
- **125 tests** — 105 self-contained unit tests (including 6 config schema regression guards + 2 `begin_day` lifecycle regression guards in Spread/Trade trackers) + 20 golden-value integration tests

## Architecture

```
.dbn file → DbnLoader → LobReconstructor → LobState
                                              │
                                    ┌─────────┼─────────┐
                                    ▼         ▼         ▼
                             QualityTracker  OfiTracker  ... (13 total)
                                    │         │         │
                                    └─────────┼─────────┘
                                              ▼
                                    JSON profiles + provenance
```

Each tracker implements the `AnalysisTracker` trait:
```rust
pub trait AnalysisTracker: Send {
    /// Called ONCE per day, before any process_event. Default no-op.
    /// Trackers needing day context cache utc_offset / day_epoch_ns as fields.
    fn begin_day(&mut self, day_index: u32, utc_offset: i32, day_epoch_ns: i64) {
        let _ = (day_index, utc_offset, day_epoch_ns);
    }
    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, regime: u8);
    fn end_of_day(&mut self);
    fn reset_day(&mut self);
    fn finalize(&self) -> serde_json::Value;
    fn name(&self) -> &str;
}
```

## Trackers

| # | Tracker | Key Formulas | Reference |
|---|---------|-------------|-----------|
| 1 | **QualityTracker** | Event counts, action/consistency/regime distribution | — |
| 2 | **OfiTracker** | OFI (Cont-Kukanov-Stoikov Eq. 3), multi-scale distributions, OFI-return Pearson r at lags 0-5, component decomposition, spread-conditional correlations | Cont et al. (2014) |
| 3 | **SpreadTracker** | `S_bps = S/mid*10000`, tick classification, regime-conditional, ACF(20) | Huang & Stoll (1997) |
| 4 | **ReturnTracker** | `r = ln(mid_t/mid_{t-1})`, Hill tail index, VaR/CVaR, ACF(20), abs-return ACF | Hill (1975), Cont (2001) |
| 5 | **VolatilityTracker** | `RV = Σr²`, annualized `√(RV·252)·100`, vol-of-vol, spread-vol correlation | BNS (2002) |
| 6 | **LifecycleTracker** | Order lifetime, fill rate, cancel-to-add ratio, 4×4 transition matrix | Cont et al. (2014) |
| 7 | **TradeTracker** | Lee-Ready classification, inter-trade time, clustering, large trade impact | Kyle (1985) |
| 8 | **DepthTracker** | `DI = (bid-ask)/(bid+ask)`, L1 concentration, CV, 10-level profile | Cao et al. (2009) |
| 9 | **LiquidityTracker** | Effective spread `2·|P-M|/M·10000` bps, VWES, microprice deviation | Kyle (1985), Amihud (2002) |
| 10 | **JumpTracker** | BNS bipower variation `BV = (π/2)·Σ|r_t|·|r_{t-1}|`, jump fraction, z-statistic | BNS (2004, 2006) |
| 11 | **NoiseTracker** | Signature plot (20 scales), noise variance, SNR, Roll spread `2·√(-γ₁)` | Zhang et al. (2005), Roll (1984) |
| 12 | **VpinTracker** | `VPIN = (1/n)·Σ|V_buy-V_sell|/V_bar` over volume bars | Easley et al. (2012, 2019) |
| 13 | **CrossScaleOfiTracker** | N×N OFI-return Pearson r matrix with predictive alignment | Cont et al. (2014) |

See [CODEBASE.md](CODEBASE.md) for complete formulas, statistics tables, and configuration reference.

## Quick Start

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run --release --bin profile_mbo -- --config configs/default.toml
```

### Test

```bash
# Unit tests (105 tests, self-contained — no external data needed)
cargo test

# Integration tests (20 tests, require real .dbn data at ../data/hot_store/)
cargo test -- --ignored
```

## Configuration

All behavior is TOML-configurable. See `configs/` for examples:

| Config | Purpose |
|--------|---------|
| `default.toml` | Single-symbol default (12 trackers, no CrossScaleOFI) |
| `xnas_full_234day.toml` | Full 233-day NVDA XNAS run (all 13 trackers) |
| `arcx_full_233day.toml` | Full 233-day NVDA ARCX run |
| `xnas_monthly_*.toml` | 12 monthly configs for signal stability analysis |
| `xnas_crsp_134day.toml` | Multi-stock universality study |

Key configurable parameters: tracker toggles, timescales (default: `[1, 5, 10, 30, 60, 300]` seconds), reservoir capacity, VPIN bar size and window, output directory.

## Output

Each run produces numbered JSON files per tracker plus provenance metadata:

```
output_dir/
├── 01_QualityTracker.json
├── 02_ReturnTracker.json
├── 03_OfiTracker.json
├── ...
├── 13_CrossScaleOfiTracker.json
└── {EXCHANGE}_{SYMBOL}_STATISTICAL_PROFILE.md  (if write_summaries = true)
```

Pre-computed analysis results are included in `output_xnas_full/`, `output_arcx_full/`, `output_xnas_monthly/`, and `output_CRSP_134day/`.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [`mbo-lob-reconstructor`](https://github.com/nagarx/MBO-LOB-reconstructor) | LOB reconstruction from raw MBO data, Databento I/O |
| [`hft-statistics`](https://github.com/nagarx/hft-statistics) | Shared statistical primitives (Welford, reservoir, ACF, regime classification, DST) |

Plus standard crates: `serde`, `serde_json`, `toml`, `ahash`, `log`, `env_logger`, `rand`, `chrono`.

## Monorepo Development

For local development within the HFT pipeline monorepo, create `.cargo/config.toml` (gitignored) to patch git dependencies to sibling directories:

```toml
[patch."https://github.com/nagarx/MBO-LOB-reconstructor.git"]
mbo-lob-reconstructor = { path = "../MBO-LOB-reconstructor" }

[patch."https://github.com/nagarx/hft-statistics.git"]
hft-statistics = { path = "../hft-statistics" }
```

Without this file, Cargo fetches dependencies from their GitHub repositories.

## Performance

| Config | Events/sec | Notes |
|--------|-----------|-------|
| QualityTracker only | 2.9M evt/s | Single tracker baseline |
| All 13 trackers | 854K evt/s | Full profiling |
| 233-day XNAS (2.87B events) | 854K evt/s | ~56 min wall time |
| 233-day ARCX (1.37B events) | 760K evt/s | ~30 min wall time |
| Python MBO-LOB-analyzer | 72K evt/s | 25+ hours for same dataset |

Release build: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`.

## Documentation

| Document | Description |
|----------|-------------|
| [CODEBASE.md](CODEBASE.md) | Complete technical reference: all formulas, statistics, configuration, architecture |
| [NVDA_UNIFIED_ANALYSIS_CONCLUSION.md](NVDA_UNIFIED_ANALYSIS_CONCLUSION.md) | Definitive 233-day cross-exchange (XNAS + ARCX) analysis findings |
| [CROSS_EXCHANGE_COMPARISON.md](CROSS_EXCHANGE_COMPARISON.md) | XNAS vs ARCX side-by-side comparison (10 sections) |
| [TIER1_ANALYSIS_FINDINGS.md](TIER1_ANALYSIS_FINDINGS.md) | Cross-scale OFI predictability and conditional OFI-return correlation |
| [ZERO_DTE_STRATEGY_BRIDGE.md](ZERO_DTE_STRATEGY_BRIDGE.md) | Bridge from equity microstructure to 0DTE options strategy |
