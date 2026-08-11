# MBO Statistical Profiler

High-performance Rust crate for MBO (Market-by-Order) / LOB (Limit Order Book) market microstructure statistical profiling. It decodes `.dbn` inputs once, narrows each successfully converted record to the reconstructor's `MboMessage`/`LobState`, and runs 13 composable analysis trackers. Tracker inputs are not the complete wire record.

> **Pipeline scope (2026-06-02).** This module is part of an **intraday trading research pipeline** — an experiment-first platform for discovering and validating *any* profitable **intraday** trading edge (no overnight positions), across approach classes (microstructure/HFT, scalping, intraday momentum, intraday statistical arbitrage, …) and instruments (equities, futures, same-day options). The pipeline *originated* as a high-frequency NVDA MBO/LOB microstructure system — that origin explains the "HFT" / "LOB" / "MBO" naming here — and that microstructure-direction program is now one (largely-closed) track among many. **Names are historical; the mission is general.** This module's role: a Rust MBO statistical profiler — 13 trackers / 50+ metrics (854K evt/s) for offline microstructure characterization of order-flow data. For the full mission + approach taxonomy + capability-readiness boundary, see root `CLAUDE.md` §Research Scope & Charter (+ `CROSS_ASSET_OFI_FINDINGS_AND_ISSUES_2026_06_01.md` §9).

> **Current Databento decode boundary (2026-08-02).** This checkout pins
> `mbo-lob-reconstructor` `v0.3.0` and inherited `dbn` `v0.64.0` at commit
> `64e5416f53b8ebecc9f1799d715dec8baa4c17eb`, using `AsIs` upgrade policy.
> The profiler still calls the legacy `iter_messages()` surface, not the typed
> iterator/finalization contract used by the feature extractor. The current
> bridge stores `ts_event` internally (dropping MBO-primary `ts_recv`) and maps
> both wire `T` (aggressor side) and wire `F` (resting side) to internal
> `Action::Trade`. Consequently direct profiler trade-side metrics are
> sign-annihilated, `fill_count` is structurally zero, and trade counts combine
> two wire populations. Preserve committed JSON as historical output, but do
> not use its signed Trade/Fill, aggressor, fill/lifecycle, VPIN, or
> trade-conditional conclusions as corrected-DBN evidence. See FINDING-122 and
> the SSD Databento release before any rerun or comparison.

> **Output provenance limitation.** Each tracker JSON records bounded
> run/config summary fields, but not the discovered file list, compressed
> hashes, catalog release identity, decoder/git commit, or complete enabled
> tracker/input-selection configuration. A JSON file alone cannot reproduce or
> identify its source population; bind it to an external run receipt and the
> immutable Databento catalog release.

## Key Capabilities

- **13 analysis trackers** covering OFI, spread, returns, volatility, depth, trades, liquidity, order lifecycle, jumps, microstructure noise, VPIN, and cross-scale predictability
- **200+ computed metrics** with academic references (Cont-Kukanov-Stoikov, Barndorff-Nielsen-Shephard, Roll, Kyle, Easley-Lopez de Prado-O'Hara, Hill, Zhang-Mykland-Aït-Sahalia)
- **854K–2.9M events/sec** throughput (single-threaded, release mode)
- **Single-pass processing** — all trackers receive every event simultaneously
- **Bounded memory** — streaming accumulators (Welford, reservoir sampling), no full-dataset storage
- **TOML-driven configuration** — enable/disable trackers, set timescales, tune parameters
- **Comprehensive test suite** — self-contained unit tests (including config schema regression guards + `begin_day` lifecycle regression guards in Spread/Trade trackers) + golden-value integration tests; run `cargo test` / `cargo test -- --ignored` for live counts

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
# Unit tests (self-contained — no external data needed)
cargo test

# Integration tests (require real .dbn data at ../data/hot_store/)
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
└── 13_CrossScaleOfiTracker.json
```

Note: the `write_summaries` config field exists in `OutputConfig` but is currently **unused by profiler code** (reserved for future markdown summary generation — Phase C). The binary writes JSON files only; the `{EXCHANGE}_{SYMBOL}_STATISTICAL_PROFILE.md` files in the committed output directories were generated by external tools.

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

Single-day benchmark (NVDA XNAS, 2025-02-03, 18.5M events):

| Config | Events/sec | Notes |
|--------|-----------|-------|
| QualityTracker only | 2.9M evt/s | Single tracker baseline |
| All 13 trackers | 854K evt/s | Full profiling |
| Python MBO-LOB-analyzer | 72K evt/s | 25+ hours for same dataset |

Full-dataset production runs (233 trading days each, XNAS + ARCX): measured wall time and throughput are recorded in the committed run artifacts — read the `_provenance` block (`runtime_secs`, `throughput_events_per_sec`, `total_events`) in `output_xnas_full/01_QualityTracker.json` and `output_arcx_full/01_QualityTracker.json` rather than any hand-typed figure here (the committed full runs measured materially faster than the dated single-day benchmark above).

Release build: `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`.

## Documentation

| Document | Description |
|----------|-------------|
| [CODEBASE.md](CODEBASE.md) | Complete technical reference: all formulas, statistics, configuration, architecture |
| [NVDA_UNIFIED_ANALYSIS_CONCLUSION.md](NVDA_UNIFIED_ANALYSIS_CONCLUSION.md) | Definitive 233-day cross-exchange (XNAS + ARCX) analysis findings |
| [XNAS ITCH Dataset Analysis Conclusion.md](XNAS%20ITCH%20Dataset%20Analysis%20Conclusion.md) | XNAS-only (Nasdaq ITCH) 233-day analysis conclusion — historical precursor to the unified cross-exchange doc above; retained for reference |
| [CROSS_EXCHANGE_COMPARISON.md](CROSS_EXCHANGE_COMPARISON.md) | XNAS vs ARCX side-by-side comparison (10 sections) |
| [TIER1_ANALYSIS_FINDINGS.md](TIER1_ANALYSIS_FINDINGS.md) | Cross-scale OFI predictability and conditional OFI-return correlation |
| [ZERO_DTE_STRATEGY_BRIDGE.md](ZERO_DTE_STRATEGY_BRIDGE.md) | Bridge from equity microstructure to 0DTE options strategy |
