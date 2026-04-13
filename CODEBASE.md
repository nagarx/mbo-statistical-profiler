# MBO Statistical Profiler: Codebase Reference

> **Version**: 0.1.0 (Phase B Complete — All 13 Trackers Implemented)
> **Last Updated**: 2026-04-14
> **Tests**: 120 passing (100 unit + 20 integration)
> **Performance**: 854K–2.9M events/sec (13 trackers → 1 tracker)

---

## Architecture

```
src/
├── lib.rs                     # AnalysisTracker trait, public API, re-exports hft-statistics
├── config.rs                  # ProfilerConfig (TOML-driven, serde)
├── profiler.rs                # Single-pass processing engine + JSON output
├── trackers/
│   ├── mod.rs                 # Re-exports all 13 trackers
│   ├── quality.rs             # QualityTracker: row counts, action/consistency/regime distribution
│   ├── ofi.rs                 # OfiTracker: multi-scale OFI, OFI-return correlation, components
│   ├── spread.rs              # SpreadTracker: tick distribution, regime-conditional, ACF
│   ├── returns.rs             # ReturnTracker: multi-scale distributions, Hill tail, ACF, VaR/CVaR
│   ├── volatility.rs          # VolatilityTracker: multi-scale RV, vol-of-vol, persistence
│   ├── lifecycle.rs           # LifecycleTracker: lifetime, fill rate, transition matrix
│   ├── trades.rs              # TradeTracker: size distribution, regime rate, directional
│   ├── depth.rs               # DepthTracker: 10-level profile, imbalance, concentration
│   ├── liquidity.rs           # LiquidityTracker: effective spread, VWES, microprice
│   ├── jumps.rs               # JumpTracker: BNS bipower variation test
│   ├── noise.rs               # NoiseTracker: signature plot, SNR, Roll spread
│   ├── vpin.rs                # VpinTracker: VPIN (volume-synchronized informed trading)
│   └── cross_scale_ofi.rs     # CrossScaleOfiTracker: cross-scale OFI correlation matrix
└── bin/
    └── profile_mbo.rs         # CLI: config-driven profiling with all trackers
```

**Supporting files:**
```
tests/
└── integration_real_data.rs   # 20 golden-value regression tests (require real data)

configs/                       # 27 TOML configs (default, full runs, monthly, multi-stock)
scripts/
├── compare_monthly.py         # Monthly signal stability comparison
└── cross_validate.py          # Cross-validation against Python analyzer

schemas/                       # Reserved for future JSON schema definitions
```

---

## Core Trait: AnalysisTracker

All 13 trackers implement the `AnalysisTracker` trait defined in `src/lib.rs`:

```rust
pub trait AnalysisTracker: Send {
    fn process_event(&mut self, msg: &MboMessage, lob_state: &LobState, regime: u8, day_epoch_ns: i64);
    fn end_of_day(&mut self, day_index: u32);
    fn reset_day(&mut self);
    fn finalize(&self) -> serde_json::Value;
    fn name(&self) -> &str;
}
```

**Lifecycle per day:**
```
For each .dbn file (1 file = 1 day):
  1. LobReconstructor processes every MBO event → produces LobState
  2. time_regime(timestamp, utc_offset) → regime (0-6)
  3. tracker.process_event(msg, lob_state, regime, day_epoch_ns)  — for ALL enabled trackers
  4. tracker.end_of_day(day_index)  — at day boundary
  5. tracker.reset_day()            — prepare for next day

After all days:
  6. tracker.finalize() → serde_json::Value
  7. profiler::write_output() → numbered JSON files + provenance metadata
```

**Design principles:**
- **Single pass**: All trackers process events simultaneously — no re-reads
- **Bounded memory**: Streaming accumulators (Welford, reservoir sampling) — no full-dataset storage
- **Composable**: Each tracker is independent, enable/disable via TOML config
- **Deterministic**: Same input → same output (seeded RNG for reservoir sampling)
- **Zero dependency on feature-extractor**: Uses `mbo-lob-reconstructor` library directly

---

## Data Flow

```
.dbn file(s)
    │
    ├── hot_store_dir (decompressed .dbn)
    │   └── HotStoreManager::discover_files() → sorted by date
    │
    └── data_dir (compressed .dbn.zst)
        └── discover_files() → sorted by date pattern
    │
    ▼
DbnLoader (from mbo-lob-reconstructor)
    │ iterates MboMessage records
    ▼
LobReconstructor::process_message_into(msg, &mut state_buf)
    │ fills LobState into caller-provided buffer (560B stack-allocated, zero-alloc)
    ▼
┌─────────────────────────────────────────────────┐
│ For each (MboMessage, LobState) pair:           │
│   regime = time_regime(timestamp, utc_offset)   │
│   for tracker in enabled_trackers:              │
│       tracker.process_event(msg, state, regime) │
└─────────────────────────────────────────────────┘
    │
    ▼ (at day boundary)
    tracker.end_of_day(day_index)
    tracker.reset_day()
    │
    ▼ (after all days)
    tracker.finalize() → serde_json::Value
    │
    ▼
profiler::write_output()
    ├── 01_QualityTracker.json
    ├── 02_ReturnTracker.json
    ├── ...
    ├── 13_CrossScaleOfiTracker.json
    └── provenance: { git_hash, config_path, timestamp, total_events, elapsed_secs }
```

---

## Tracker Reference: Formulas & Statistics

### 1. QualityTracker (`quality.rs`)

**Purpose:** Data integrity validation and dataset characterization.

**Statistics:**

| Statistic | Description | Units |
|-----------|-------------|-------|
| Total events | Count of all MBO events processed | count |
| Action distribution | `count(action_i) / total * 100` for each of 7 actions (Add, Modify, Cancel, Trade, Fill, Clear, None) | % |
| Book consistency | `count(state_i) / total * 100` for Valid, Empty, Locked, Crossed | % |
| Time regime distribution | `count(regime_i) / total * 100` for 7 regimes (pre-market, open-auction, morning, midday, afternoon, close-auction, post-market) | % |
| Per-day event counts | Event count for each trading day | count |

**Key validation:** Valid book states should exceed 99.99%. Crossed books indicate data quality issues.

---

### 2. OfiTracker (`ofi.rs`, 1081 lines)

**Purpose:** Order Flow Imbalance at multiple timescales, the most predictive short-term microstructure signal (r = 0.702 at 5m for NVDA).

**OFI per event** (Cont, Kukanov & Stoikov 2014, Eq. 3):

```
OFI_t = (bid_size_t * I(bid_price_t >= bid_price_{t-1})
       - bid_size_{t-1} * I(bid_price_t <= bid_price_{t-1}))
      - (ask_size_t * I(ask_price_t <= ask_price_{t-1})
       - ask_size_{t-1} * I(ask_price_t >= ask_price_{t-1}))
```

**Decomposition:** `OFI = OFI_add + OFI_cancel + OFI_trade` — each component isolated by filtering on `triggering_action`.

**Statistics:**

| Statistic | Formula | Units | Per-Scale? |
|-----------|---------|-------|:---:|
| OFI distribution | Welford + reservoir over all bins | shares | Yes |
| OFI-return correlation | `Pearson(OFI_t, return_t)` at lags 0–5 | dimensionless | Yes |
| OFI-spread cross-correlation | `Pearson(OFI_t, spread_t)` at lags 0–5 | dimensionless | Yes |
| Component fractions | `|OFI_add| / |OFI_total|`, etc. | ratio | No |
| OFI ACF | `ACF(k)` of bin-level OFI | dimensionless | Yes |
| Regime intensity | `mean(|OFI|)` per time regime | shares | No |
| Intraday OFI curve | 390-bin canonical grid (per-minute) | shares | No |
| Intraday OFI-return r curve | Per-minute Pearson r (390 bins) | dimensionless | No |
| Cumulative delta | `sum(trade_size * sign)` per day | shares | No |
| Aggressor ratio | `buyer_vol / (buyer_vol + seller_vol)` | ratio | No |
| Spread-conditional OFI-return r | Pearson r within 4 tick-width buckets (1-tick, 2-tick, 3-4, 5+) | dimensionless | Yes |

**References:**
- Cont, R., Kukanov, A. & Stoikov, S. (2014). "The Price Impact of Order Book Events." *Journal of Financial Econometrics*, 12(1), 47-88.
- Kolm, P., Turiel, J. & Westray, N. (2023). "Deep Order Flow Imbalance." *Mathematical Finance*.

---

### 3. SpreadTracker (`spread.rs`)

**Purpose:** Bid-ask spread characterization for cost modeling and execution timing.

**Formulas:**
- Spread (USD): `S = (best_ask - best_bid) / 1e9` (prices in nanodollars)
- Spread (ticks): `S_ticks = S / tick_size` where `tick_size = $0.01` (SEC Rule 612)
- Spread (bps): `S_bps = S / mid_price * 10000`

**Width classification:**
- 1-tick: `S_ticks ∈ [0.5, 1.5)`
- 2-tick: `S_ticks ∈ [1.5, 2.5)`
- 3-4 tick: `S_ticks ∈ [2.5, 4.5)`
- 5+ tick: `S_ticks >= 4.5`

**Statistics:**

| Statistic | Units | Per-Scale? |
|-----------|-------|:---:|
| Spread distribution (USD, ticks, bps) | various | No |
| Width classification fractions | ratio | No |
| Regime-conditional spread | USD | No |
| Trade-conditional spread | USD | No |
| Spread ACF (20 lags) | dimensionless | No |
| Intraday spread curve (390 bins) | USD | No |

**Reference:** Huang, R.D. & Stoll, H.R. (1997). "The Components of the Bid-Ask Spread."

---

### 4. ReturnTracker (`returns.rs`)

**Purpose:** Mid-price return distributions at multiple timescales. Foundation for volatility analysis, jump detection, and OFI-return correlation.

**Formulas:**
- Log return: `r_t = ln(mid_t / mid_{t-1})` at N timescales
- Hill (1975) estimator: `H = (1/k) * sum_{i=1}^{k} ln(X_{(i)} / X_{(k+1)})` where X is sorted descending by absolute value; reported output is the **tail exponent** `α = 1/H` (separate estimates for left and right tails). Higher α = lighter tails.
- Value at Risk: `VaR_α = quantile(returns, α)` for α = 1%, 5%
- CVaR (Expected Shortfall): `CVaR_α = E[r | r <= VaR_α]` for α = 1%, 5%
- Zero-return fraction: `count(r = 0) / count(r)` — indicates market inactivity
- Max drawdown: peak-to-trough of cumulative intraday returns

**Statistics:**

| Statistic | Units | Per-Scale? |
|-----------|-------|:---:|
| Return distribution (mean, std, skew, kurtosis, percentiles) | dimensionless | Yes |
| ACF (lags 1–20) | dimensionless | Yes |
| Absolute return ACF (volatility clustering) | dimensionless | Yes |
| Hill tail exponent α = 1/H (left, right) | dimensionless | Yes |
| VaR (1%, 5%) | dimensionless | Yes |
| CVaR (1%, 5%) | dimensionless | Yes |
| Zero-return fraction | ratio | Yes |
| Intraday return curve (390 bins) | dim./min | No |
| Intraday absolute return curve | dim./min | No |
| Daily max drawdown and max runup | dimensionless | No |

**References:**
- Hill, B.M. (1975). "A simple general approach to inference about the tail of a distribution." *Annals of Statistics*, 3(5), 1163-1174.
- Cont, R. (2001). "Empirical properties of asset returns: stylized facts and statistical issues." *Quantitative Finance*, 1(2), 223-236.

---

### 5. VolatilityTracker (`volatility.rs`)

**Purpose:** Multi-scale realized volatility with intraday patterns, vol-of-vol, persistence, and spread-volatility correlation.

**Formulas:**
- Realized variance (Barndorff-Nielsen & Shephard 2002): `RV_t = sum_{i=1}^{n} r_{t,i}^2`
- Annualized realized volatility: `σ_annual = sqrt(RV * 252) * 100` (% per annum)
- Vol-of-vol: `std(daily_RV)` via Welford online estimator
- Spread-vol correlation: `Pearson(daily_RV, daily_mean_spread)`

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Multi-scale daily RV | dimensionless |
| Annualized volatility distribution | % per annum |
| Intraday volatility curve (390 bins, mean r² per minute) | dimensionless |
| Vol-of-vol | dimensionless |
| RV persistence ACF (20 lags) | dimensionless |
| Spread-volatility Pearson correlation | dimensionless |

**Reference:** Barndorff-Nielsen, O.E. & Shephard, N. (2002). "Econometric analysis of realized volatility." *JRSS-B*, 64(2), 253-280.

---

### 6. LifecycleTracker (`lifecycle.rs`)

**Purpose:** Individual order lifecycle from placement to resolution (fill, cancel, or expiry). Most complex tracker — maintains `AHashMap<u64, ActiveOrder>` with 500K eviction cap.

**Formulas:**
- Order lifetime: `duration = resolve_timestamp - add_timestamp` (seconds)
- Fill rate: `fill_rate = n_filled / n_resolved`
- Cancel-to-add ratio: `CTA = n_cancels / n_adds`
- Transition matrix states: {Add=0, Modify=1, Cancel=2, Trade=3} — `P[i][j] = count(i → j) / sum_j(count(i → j))`
- Duration-size correlation: `Pearson(log(duration), log(size))`

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Order lifetime distribution | seconds |
| Fill rate | ratio [0, 1] |
| Cancel-to-add ratio | ratio |
| 4×4 action transition matrix | probability |
| Duration-size log-log correlation | dimensionless |
| Modify count distribution | count |
| Partial fill fraction | ratio |
| Regime-conditional lifetime | seconds |
| Regime-conditional fill rate | ratio |

**References:**
- Cont, R., Stoikov, S. & Talreja, R. (2014). "A stochastic model for order book dynamics." *Operations Research*, 58(3), 549-563.
- Hasbrouck, J. (2018). "High-frequency quoting." *JFQA*, 53(2), 613-641.

---

### 7. TradeTracker (`trades.rs`)

**Purpose:** Trade flow characterization — size distributions, arrival rates, directional classification, clustering, and large trade impact.

**Formulas:**
- Trade rate per regime: `rate_r = n_trades_r / duration_r_seconds`
- Price classification (trade price P, best bid B, best ask A):
  - `at_bid`: P == B
  - `at_ask`: P == A
  - `inside`: B < P < A
  - `outside`: P < B or P > A
- Trade clustering: cluster boundary at inter-trade gap > 1 second
- Large trade impact: impact in bps for trades > 95th percentile size

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Trade size distribution (all, buyer-initiated, seller-initiated) | shares |
| Trade value distribution | USD |
| Price-level classification (at_bid, at_ask, inside, outside) | count/% |
| Trade-through count and percentage | count/% |
| Inter-trade time distribution | seconds |
| Trade clustering (cluster size distribution, cluster fraction) | count/ratio |
| Large trade impact (>p95) | bps |
| Intraday trade rate curve (390 bins) | trades/min |
| Regime-conditional trade size | shares |

**References:**
- Kyle, A.S. (1985). "Continuous auctions and insider trading." *Econometrica*, 53(6), 1315-1335.
- Lee, C.M.C. & Ready, M.J. (1991). "Inferring trade direction from intraday data." *Journal of Finance*, 46(2), 733-746.

---

### 8. DepthTracker (`depth.rs`)

**Purpose:** Order book depth structure, imbalance, L1 concentration, and depth stability across regimes.

**Formulas:**
- Depth imbalance: `DI = (sum(bid_sizes) - sum(ask_sizes)) / (sum(bid_sizes) + sum(ask_sizes))` — range [-1, 1]
- L1 concentration: `C_L1 = (bid_size[0] + ask_size[0]) / (total_bid_vol + total_ask_vol)` — range [0, 1]
- Coefficient of variation: `CV = std(total_depth) / mean(total_depth)`

**Statistics:**

| Statistic | Units |
|-----------|-------|
| 10-level mean depth profile (bid and ask) | shares |
| Depth imbalance distribution | ratio [-1, 1] |
| L1 concentration | ratio [0, 1] |
| Total depth statistics (mean, std, CV) | shares |
| Regime-conditional imbalance | ratio |
| Regime-conditional total depth | shares |

**Reference:** Cao, C., Hansch, O. & Wang, X. (2009). "The information content of an open limit-order book." *Journal of Futures Markets*, 29(1), 16-41.

---

### 9. LiquidityTracker (`liquidity.rs`)

**Purpose:** Execution quality and liquidity cost through effective spread, volume-weighted spread, and microprice deviation.

**Formulas:**
- Effective spread (unsigned, Kyle 1985): `ES_i = 2 * |P_i - M_i| / M_i * 10000` (bps)
  where P_i = trade price, M_i = midpoint before trade
- Volume-weighted effective spread: `VWES = sum(ES_i * size_i) / sum(size_i)` (bps)
- Microprice deviation: `MPD = |microprice - mid| / mid * 10000` (bps)
  — indicates asymmetry in liquidity provision

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Effective spread distribution | bps |
| Volume-weighted effective spread (daily) | bps |
| Microprice deviation distribution | bps |

**References:**
- Kyle, A.S. (1985). "Continuous auctions and insider trading." *Econometrica*, 53(6), 1315-1335.
- Amihud, Y. (2002). "Illiquidity and stock returns." *Journal of Financial Markets*, 5(1), 31-56.

---

### 10. JumpTracker (`jumps.rs`)

**Purpose:** Price jump detection using the Barndorff-Nielsen & Shephard (2004) bipower variation test. Quantifies variance from jumps vs. continuous diffusion.

**Formulas:**
- Realized variance: `RV = sum(r_t²)`
- Bipower variation: `BV = (π/2) * sum_{i=2}^{n} |r_i| * |r_{i-1}|`
- Jump component: `J = max(RV - BV, 0)`
- Jump fraction: `J / RV` — ratio of variance attributable to jumps
- BNS z-statistic: `z = (RV - BV) / sqrt(var_est)` where:
  - `var_est = (π²/4 + π - 5) * max(TP, 0) / n`
  - Tripower quarticity (BNS 2006): `TP = (n / (n-2)) * μ_{4/3}^{-3} * sum_{i=3}^{n} |r_i|^{4/3} * |r_{i-1}|^{4/3} * |r_{i-2}|^{4/3}`
  - Scaling constant: `μ_{4/3} = 2^{2/3} * Γ(7/6) / Γ(1/2)`
- Significant jump: `z > z_threshold` (configurable, default from normal quantile)

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Daily bipower variation | dimensionless |
| Daily jump fraction distribution | ratio [0, 1] |
| BNS z-statistic distribution | dimensionless |
| Significant jump day count | count |
| Regime-conditional jump rate | ratio |

**References:**
- Barndorff-Nielsen, O.E. & Shephard, N. (2004). "Power and bipower variation with stochastic volatility and jumps." *Journal of Financial Econometrics*, 2(1), 1-37.
- Barndorff-Nielsen, O.E. & Shephard, N. (2006). "Econometrics of testing for jumps." *Journal of Financial Econometrics*, 4(1), 1-30.

---

### 11. NoiseTracker (`noise.rs`)

**Purpose:** Microstructure noise characterization using the signature plot method, noise variance estimation, signal-to-noise ratio, and Roll's implied spread.

**Formulas:**
- Signature plot (Zhang et al. 2005): Compute `RV(δ) = sum(r_{t,δ}²)` at 20 log-spaced timescales δ ∈ [0.1s, 60s]. In the presence of noise, RV increases as δ decreases.
- Noise variance: `σ²_noise = (RV_fast - RV_slow) / (2 * n_fast)` — using fastest scale as RV_fast, slowest as RV_slow
- Signal-to-noise ratio: `SNR = σ²_true / σ²_noise` where `σ²_true = RV_slow`
- Roll (1984) implied spread: `S_Roll = 2 * sqrt(-γ₁)` if `γ₁ < 0`, else `NaN` — where `γ₁ = cov(r_t, r_{t-1})` (first-order autocovariance of tick returns)

**Statistics:**

| Statistic | Units |
|-----------|-------|
| Signature plot (20 points) | dimensionless |
| Noise variance estimate | dimensionless |
| Signal-to-noise ratio | dimensionless |
| Roll implied spread | dollars |

**References:**
- Zhang, L., Mykland, P.A. & Aït-Sahalia, Y. (2005). "A tale of two time scales." *JASA*, 100(472), 1394-1411.
- Roll, R. (1984). "A simple implicit measure of the effective bid-ask spread." *Journal of Finance*, 39(4), 1127-1139.

---

### 12. VpinTracker (`vpin.rs`)

**Purpose:** Volume-Synchronized Probability of Informed Trading. VPIN is the #1 out-of-sample predictor for spread, volatility, kurtosis, skewness, and serial correlation changes (Easley et al. 2019).

**Formulas:**
- Volume bars: Aggregate trade events into bars of fixed volume `V_bar` (default: 5,000 shares). Each bar records: total_volume, buy_volume, sell_volume, vwap, close_price.
- VPIN: `VPIN = (1/n) * sum_{i=t-n+1}^{t} |V_buy_i - V_sell_i| / V_bar` — rolling average of |order imbalance| over n volume bars (default n=50).
- MBO convention: Side::Ask resting order hit by trade = buyer-initiated aggressor.

**Statistics:**

| Statistic | Units |
|-----------|-------|
| VPIN distribution | ratio [0, 1] |
| Intraday VPIN curve | ratio |
| Daily mean VPIN | ratio |
| VPIN-spread correlation | dimensionless |
| Regime-conditional VPIN | ratio |
| Volume bar count | count |

**References:**
- Easley, D., Lopez de Prado, M., O'Hara, M. & Zhang, Z. (2019). "Microstructure in the Machine Age." *Review of Financial Studies*.
- Easley, D., Lopez de Prado, M. & O'Hara, M. (2012). "Flow Toxicity and Liquidity in a High-Frequency World." *Review of Financial Studies*, 25(5), 1457-1493.

---

### 13. CrossScaleOfiTracker (`cross_scale_ofi.rs`)

**Purpose:** Determines whether short-timescale OFI predicts longer-timescale returns. Computes an N×N correlation matrix where entry (i,j) = Pearson(OFI at scale i, return at scale j).

**Key design:** Predictive alignment for off-diagonal entries — OFI in source bin ending at time t is paired with the return of the target-scale bin **starting** at time t (next bar's return). Diagonal entries use contemporaneous alignment (lag 0).

**Formulas:**
- OFI per event: Cont, Kukanov & Stoikov (2014), Eq. 3 (same as OfiTracker)
- Matrix entry: `r(i,j) = Pearson(OFI_scale_i, return_scale_j)` with predictive alignment for i ≠ j
- Cross-day aggregation: weighted average of per-day correlations (weighted by sample count)

**Statistics:**

| Statistic | Units |
|-----------|-------|
| N×N cross-scale correlation matrix | dimensionless |
| Per-scale sample counts | count |
| Diagonal (contemporaneous) correlations | dimensionless |
| Off-diagonal (predictive) correlations | dimensionless |

**Reference:** Cont, R., Kukanov, A. & Stoikov, S. (2014). "The Price Impact of Order Book Events." *Journal of Financial Econometrics*, 12(1), 47-88.

---

## Statistical Primitives (from hft-statistics)

The profiler re-exports statistical primitives from the [`hft-statistics`](https://github.com/nagarx/hft-statistics) crate:

| Primitive | Purpose | Used By |
|-----------|---------|---------|
| `WelfordAccumulator` | Online mean/variance (numerically stable) | All trackers |
| `StreamingDistribution` | Welford + reservoir sampling for percentiles | Returns, Spread, Trades, Lifecycle, Depth, Liquidity, VPIN |
| `ReservoirSampler` | Uniform random sampling from stream (seeded RNG) | StreamingDistribution |
| `AcfComputer` | Autocorrelation function (configurable lags) | Returns, Spread, Volatility, OFI |
| `IntradayCurveAccumulator` | 390-bin per-minute canonical grid accumulator | OFI, Returns, Spread, Volatility, Trades, VPIN |
| `IntradayCorrelationAccumulator` | Per-minute Pearson r accumulator (390 bins) | OFI |
| `RegimeAccumulator` | Per-regime (0-6) statistics accumulator | All regime-conditional trackers |
| `TransitionMatrix<N>` | N×N transition count matrix → probabilities | Lifecycle |

**Time utilities** (also from hft-statistics):
- `time_regime(timestamp_ns, utc_offset) → u8` — 7-regime intraday classification
- `utc_offset_for_date(year, month, day) → i32` — DST-aware UTC offset (2nd Sunday March, 1st Sunday November)
- `infer_utc_offset(timestamps) → i32` — infer UTC offset from first timestamp
- `infer_day_params(timestamps) → (utc_offset: i32, day_epoch_ns: i64)` — infer both from timestamps
- `day_epoch_ns(year, month, day, utc_offset) → i64` — midnight UTC in nanoseconds for a trading day
- `resample_to_grid(timestamps_ns, values, bin_width_ns, day_epoch_ns, utc_offset_hours, mode) → ResampledBins` — canonical-grid resampling with 390-bin intraday support
- `N_REGIMES = 7` — number of time regimes

---

## Configuration Reference

Configuration is TOML-driven via `ProfilerConfig` (defined in `src/config.rs`).

### ProfilerConfig (top-level)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `input` | `InputConfig` | — | Input data source (required) |
| `trackers` | `TrackerConfig` | all true except `cross_scale_ofi` | Which trackers to enable |
| `timescales` | `Vec<f64>` | `[1, 5, 10, 30, 60, 300]` | Multi-scale analysis timescales (seconds) |
| `reservoir_capacity` | `usize` | `10,000` | Reservoir sampling capacity for distributions |
| `vpin_volume_bar_size` | `u64` | `5,000` | VPIN volume bar size (shares) |
| `vpin_window_bars` | `usize` | `50` | VPIN rolling window (number of volume bars) |
| `output` | `OutputConfig` | see below | Output directory and format |

### InputConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `hot_store_dir` | `Option<PathBuf>` | `None` | Path to decompressed .dbn files |
| `data_dir` | `Option<PathBuf>` | `None` | Path to compressed .dbn.zst files |
| `filename_pattern` | `String` | — | e.g., `"xnas-itch-{date}.mbo.dbn"` |
| `symbol` | `String` | `"NVDA"` | Symbol name (metadata only) |
| `exchange` | `String` | `"XNAS"` | Exchange identifier (metadata only) |
| `date_start` | `Option<String>` | `None` | Inclusive start date filter (`"YYYY-MM-DD"`) |
| `date_end` | `Option<String>` | `None` | Inclusive end date filter |

### TrackerConfig

| Tracker | Default | Notes |
|---------|---------|-------|
| `quality` | `true` | Always recommended |
| `ofi` | `true` | Core signal tracker |
| `spread` | `true` | |
| `returns` | `true` | |
| `volatility` | `true` | |
| `lifecycle` | `true` | Memory-intensive (AHashMap) |
| `trades` | `true` | |
| `depth` | `true` | |
| `liquidity` | `true` | |
| `jumps` | `true` | |
| `noise` | `true` | |
| `vpin` | `true` | |
| `cross_scale_ofi` | **`false`** | Expensive; enable explicitly |

### OutputConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `output_dir` | `PathBuf` | `"profiler_output"` | Output directory for JSON files |
| `write_summaries` | `bool` | `true` | **Currently unused** (reserved for future markdown summary generation — Phase C) |

---

## Output Format

Each tracker produces a numbered JSON file: `{NN}_{TrackerName}.json`.

**Provenance metadata** (emitted by `profiler.rs::write_output()`, appended as `_provenance` key to each tracker's JSON):
```json
{
  "profiler_version": "0.1.0",
  "symbol": "NVDA",
  "exchange": "XNAS",
  "n_days": 233,
  "total_events": 2873641234,
  "runtime_secs": 3367.2,
  "throughput_events_per_sec": 853487.5,
  "timescales": [1.0, 5.0, 10.0, 30.0, 60.0, 300.0],
  "reservoir_capacity": 10000
}
```

Note: the `write_summaries` config field exists in `OutputConfig` but is currently **unused by profiler code** — markdown summary files in committed `output_*/` directories were generated by external tools. Adding markdown summary generation is part of Phase C.

---

## Dependencies

| Crate | Source | Purpose |
|-------|--------|---------|
| [`mbo-lob-reconstructor`](https://github.com/nagarx/MBO-LOB-reconstructor) | git (main) | `LobState`, `MboMessage`, `Action`, `Side`, `BookConsistency`, `DbnLoader`, `HotStoreManager` |
| [`hft-statistics`](https://github.com/nagarx/hft-statistics) | git (main) | Statistical primitives (Welford, reservoir, ACF, regime, resampler, transition matrix) |
| `ahash` 0.8 | crates.io | Fast hashmap for LifecycleTracker order tracking |
| `serde` 1.0 | crates.io | Serialization (derive) |
| `serde_json` 1.0 | crates.io | JSON output |
| `toml` 0.8 | crates.io | Configuration parsing |
| `log` 0.4 + `env_logger` 0.10 | crates.io | Logging |
| `rand` 0.8 | crates.io | Reservoir sampling RNG |
| `chrono` 0.4 | crates.io | Date arithmetic for DST computation |

**Local development:** `.cargo/config.toml` (gitignored) patches both git dependencies to sibling directories for fast iteration. See README.md for monorepo setup.

---

## Performance

**Single-day benchmark** (NVDA XNAS, 2025-02-03, 18.5M events):

| Config | Events/sec | Wall Time |
|--------|-----------|-----------|
| QualityTracker only | 2.9M evt/s | 6.3s |
| All 12 default trackers | 854K evt/s | 21.6s |
| Python MBO-LOB-analyzer (equivalent) | 72K evt/s | ~25 hours |

**Full dataset runs** (233 trading days, all 13 trackers):

| Dataset | Events | Wall Time | Throughput |
|---------|--------|-----------|------------|
| NVDA XNAS | 2.87B | ~56 min | ~854K evt/s |
| NVDA ARCX | 1.37B | ~30 min | ~760K evt/s |

**Build profile** (release): `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `strip = true`.

---

## Test Inventory

**Total: 120 tests** (100 unit + 20 integration)

### Unit Tests (98, self-contained)

| Tracker | Tests | Key Validations |
|---------|-------|-----------------|
| OfiTracker | 15 | OFI on bid/ask changes, book filter, finalize, reset, spread-bucket classification, conditional correlations |
| LifecycleTracker | 10 | Add-cancel, add-trade, partial fill, transition matrix, fill rate, CTA, duration |
| ReturnTracker | 9 | Mid collection, scale labels, Hill index, log return, CVaR, zero-fraction, drawdown |
| CrossScaleOfiTracker | 8 | Matrix creation, book filter, OFI+mid collection, finalize, contemporaneous/predictive alignment |
| VolatilityTracker | 8 | Collection, Pearson r, insufficient data, exact RV, annualized vol, constant prices |
| LiquidityTracker | 8 | Effective spread at mid/ask/bid, microprice deviation, VWES, non-trade filtering |
| JumpTracker | 7 | BV < RV for jumps, BV exact for constant, jump fraction, gamma ratio |
| DepthTracker | 7 | Symmetric/imbalanced book, L1 concentration, empty book, exact values |
| TradeTracker | 6 | Count, filter, price classification, directional size, finalize |
| NoiseTracker | 6 | Log-spaced scales, collection, Roll spread (negative autocovariance, alternating returns) |
| VpinTracker | 5 | Volume bar construction, range [0,1], all-buy=1, balanced≈0, finalize |
| SpreadTracker | 5 | 1-tick spread, finalize, intraday curve, exact conversions, 3-tick classification |
| QualityTracker | 4 | Event counting, day boundary, finalize JSON, book consistency |

### Integration Tests (20, require real data)

File: `tests/integration_real_data.rs` — all marked `#[ignore]`.
Uses `OnceLock` singleton to run profiler once across all 20 tests. Validates against golden values from Python `MBO-LOB-analyzer` for 2025-02-03 NVDA (18.5M events).

Tests: exact integer counts (events, actions, trades), float tolerances (spread mean, vol), structural completeness (all trackers present, expected JSON fields), range checks (fill rate, VPIN, jump fraction).

---

## Analysis Results

The `output_*/` directories contain profiler output from production runs:

| Directory | Dataset | Days | Trackers | Size |
|-----------|---------|------|----------|------|
| `output_xnas_full/` | NVDA XNAS | 233 | 13 | 512 KB |
| `output_arcx_full/` | NVDA ARCX | 233 | 13 | 368 KB |
| `output_xnas_monthly/` | NVDA XNAS (per month) | 12 × ~20 | 11 | 4.8 MB |
| `output_CRSP_134day/` | Multi-stock universality | 134 | 13 | 480 KB |

See `NVDA_UNIFIED_ANALYSIS_CONCLUSION.md` for the definitive cross-exchange analysis.

### Historical Note: Multi-Stock VPIN Bar Sizes

On 2026-04-14, a TOML schema misplacement was discovered: all committed configs had runtime keys (`timescales`, `reservoir_capacity`, `vpin_volume_bar_size`, `vpin_window_bars`) placed after the `[trackers]` section header. Because these keys don't exist in `TrackerConfig`, serde silently dropped them and used defaults.

**Affected runs**: The NVDA XNAS + ARCX full runs used defaults (`vpin_volume_bar_size = 5000`, matching the intended values), so their results are correct. However, the multi-stock universality configs intended stock-calibrated VPIN bar sizes that were not applied:

| Stock | Intended | Actually Ran With |
|-------|----------|-------------------|
| CRSP, ISRG | 500 | 5000 (10× larger) |
| ZM, FANG | 1000 | 5000 (5× larger) |
| PEP, IBKR | 1500 | 5000 (3.3× larger) |
| MRNA, DKNG | 2000 | 5000 (2.5× larger) |
| SNAP | 4000 | 5000 (1.25× larger) |
| HOOD | 5000 | 5000 (correct) |

The VPIN values in these multi-stock JSON outputs therefore reflect larger-volume bars than intended. To re-run with correct bar sizes, execute: `cargo run --release --bin profile_mbo -- --config configs/xnas_{stock}_134day.toml` after pulling the updated configs. The config files have been corrected and `#[serde(deny_unknown_fields)]` now prevents this class of silent drop at parse time.

---

## Roadmap

### Complete
- Phase A: Foundation (AnalysisTracker trait, QualityTracker, CLI, statistical primitives)
- Phase B: All 13 trackers implemented (120 tests)
- Golden-value regression tests (20 integration tests vs Python analyzer)
- Full 233-day XNAS + ARCX runs
- Monthly signal stability analysis (12 months)
- Multi-stock universality study (CRSP + 10 individual symbols)
- Cross-scale OFI predictability analysis
- Conditional OFI-return correlation by spread state

### Pending
- Rayon parallelism for multi-day processing
- JSON schema definitions (schemas/ directory)
- Configurable z-threshold for jump significance
- Additional cross-validation against Python analyzer (currently 1 day)
