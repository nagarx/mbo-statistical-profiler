# XNAS NVDA Statistical Profile

| Field | Value |
|---|---|
| **Instrument** | NVDA (NVIDIA Corporation) |
| **Exchange** | XNAS (Nasdaq — ITCH protocol) |
| **Period** | 2025-02-03 to 2026-01-06 (233 trading days) |
| **Total Events** | 2,867,766,144 |
| **Profiler Version** | mbo-statistical-profiler v0.1.0 |
| **Timescales** | 1s, 5s, 10s, 30s, 1m, 5m |
| **Reservoir Capacity** | 10,000 |

---

## 1. Data Characterization

### 1.1 Event Volume

| Metric | Value |
|---|---|
| Total events | 2,867,766,144 |
| Mean events/day | 12,308,009 |
| Days processed | 233 |

### 1.2 Action Distribution

| Action | Count | Share (%) |
|---|---|---|
| Add | 1,329,541,122 | 46.3616 |
| Cancel | 1,344,345,314 | 46.8778 |
| Trade | 193,879,475 | 6.7606 |
| Clear | 233 | 0.0000 |
| Fill | 0 | 0.0000 |
| Modify | 0 | 0.0000 |

ITCH protocol does not emit Modify messages; modifications appear as Cancel + Add pairs. The near-equal Add/Cancel counts (ratio 0.9890) confirm this mechanism.

### 1.3 Book Consistency

| State | Share (%) |
|---|---|
| Valid | 99.9999 |
| Empty | 0.0001 |
| Crossed | 0.0000 |
| Locked | 0.0000 |

### 1.4 Regime Distribution

| Regime | Share (%) |
|---|---|
| Pre-Market | 1.83 |
| Open Auction | 3.61 |
| Morning (09:45–11:30 ET) | 24.04 |
| Midday (11:30–14:30 ET) | 55.51 |
| Afternoon (14:30–15:45 ET) | 10.27 |
| Close Auction (15:45–16:00 ET) | 2.46 |
| Post-Market | 2.28 |

Midday dominates with 55.5% of events — the "quiet" session is still the largest by volume because of its duration.

---

## 2. Order Flow Imbalance (OFI)

OFI is defined as the net signed order flow: additions at the bid minus additions at the ask, plus cancellations at the ask minus cancellations at the bid, plus trades at the ask minus trades at the bid. Aggregated over non-overlapping time windows (Cont, Kukanov & Stoikov, 2014).

### 2.1 OFI–Return Correlation (Lag 0)

| Scale | r (lag 0) | r² |
|---|---|---|
| 1s | 0.5774 | 0.3334 |
| 5s | 0.6175 | 0.3813 |
| 10s | 0.6390 | 0.4083 |
| 30s | 0.6644 | 0.4414 |
| 1m | 0.6748 | 0.4554 |
| 5m | 0.7073 | 0.5003 |

OFI explains 33–50% of return variance depending on scale. The monotonic increase with scale indicates that noise averages out at longer horizons while the signal persists.

### 2.2 OFI–Spread Correlation (Lag 0)

| Scale | r (lag 0) |
|---|---|
| 1s | 0.0049 |
| 5s | 0.0073 |
| 10s | 0.0130 |
| 30s | 0.0078 |
| 1m | 0.0178 |
| 5m | 0.0079 |

OFI–Spread correlation is near-zero at all scales, indicating spread widening is not driven by directional flow on XNAS.

### 2.3 OFI–Return Lag Structure (5m Scale)

| Lag | r |
|---|---|
| 0 | 0.7073 |
| 1 | −0.0221 |
| 2 | −0.0109 |
| 3 | −0.0173 |
| 4 | −0.0123 |
| 5 | −0.0452 |

The sharp drop from lag 0 to lag 1 (0.707 → −0.022) confirms OFI is contemporaneous — it predicts the current bar's return, not the next bar's. Weak negative lag-1 suggests mild mean-reversion after large OFI events.

### 2.4 OFI Persistence (ACF)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | 0.042 | 0.019 | 0.020 | 0.011 | −0.015 |
| 5s | 0.124 | 0.055 | 0.019 | 0.036 | 0.042 |
| 10s | 0.057 | 0.051 | 0.058 | 0.009 | 0.037 |
| 30s | 0.101 | 0.063 | 0.074 | 0.037 | 0.048 |
| 1m | 0.197 | 0.113 | 0.101 | 0.093 | 0.071 |
| 5m | 0.266 | 0.166 | 0.145 | 0.095 | 0.081 |

OFI persistence increases with scale. At 5m, ACF(1)=0.266 indicates meaningful autocorrelation — institutional order-splitting creates multi-bar OFI trends.

### 2.5 Component Fractions

| Component | Fraction |
|---|---|
| Add | 0.5127 |
| Cancel | 0.4030 |
| Trade | 0.0844 |

Additions dominate OFI (51.3%), with cancellations contributing 40.3% and trades only 8.4%. This matches ITCH protocol mechanics where every order lifecycle begins with an Add.

### 2.6 Regime OFI Intensity

| Regime | Mean OFI | Std OFI | Observations |
|---|---|---|---|
| Pre-Market | 30.74 | 255.23 | 52,374,003 |
| Open Auction | 14.06 | 144.55 | 103,599,218 |
| Morning | 21.18 | 100.92 | 689,314,458 |
| Midday | 24.67 | 102.72 | 1,591,899,502 |
| Afternoon | 32.02 | 114.68 | 294,569,878 |
| Close Auction | 37.43 | 241.70 | 70,529,805 |
| Post-Market | 8.90 | 182.23 | 65,475,828 |

Close Auction has the highest mean OFI (37.43) with extreme variability (std=241.70), consistent with end-of-day positioning. Pre-Market and Post-Market show high noise-to-signal ratios.

### 2.7 Aggressor Ratio

| Metric | Value |
|---|---|
| Aggressor ratio (buyer/total) | 0.4999 |

Near-perfect balance between buyer- and seller-initiated trades over 233 days.

### 2.8 Cumulative Delta (End-of-Day)

| Metric | Value |
|---|---|
| Mean EOD delta | 18,406 shares |
| Std EOD delta | 2,759,088 shares |

Slight net buy bias (+18K shares/day) with enormous daily variation (std = 2.76M), meaning the daily drift is statistically indistinguishable from zero.

---

## 3. Spread Analysis

### 3.1 Spread Distribution (USD)

| Metric | Value |
|---|---|
| Mean | $0.0159 |
| Std | $0.0411 |
| Min | $0.0100 |
| Max | $206.6700 |
| Skewness | 12.18 |
| Kurtosis | 241.25 |

| Percentile | USD |
|---|---|
| p1 | 0.0100 |
| p5 | 0.0100 |
| p10 | 0.0100 |
| p25 | 0.0100 |
| p50 | 0.0100 |
| p75 | 0.0200 |
| p90 | 0.0200 |
| p95 | 0.0300 |
| p99 | 0.0900 |

The median spread is exactly 1 tick ($0.01). 75th percentile is 2 ticks.

### 3.2 Spread Distribution (Ticks)

| Metric | Value |
|---|---|
| Mean | 1.585 ticks |
| Std | 4.114 ticks |
| p50 | 1.000 tick |
| p95 | 3.000 ticks |
| p99 | 9.000 ticks |

### 3.3 Spread Distribution (Basis Points)

| Metric | Value |
|---|---|
| Mean | 1.105 bps |
| Std | 3.063 bps |
| Min | 0.471 bps |
| Max | 19,998 bps |

| Percentile | bps |
|---|---|
| p1 | 0.487 |
| p5 | 0.517 |
| p10 | 0.532 |
| p25 | 0.553 |
| p50 | 0.733 |
| p75 | 1.095 |
| p90 | 1.809 |
| p95 | 2.326 |
| p99 | 7.361 |

### 3.4 Width Classification

| Width | Share (%) |
|---|---|
| 1 tick | 70.27 |
| 2 ticks | 22.20 |
| 3–4 ticks | 4.79 |
| 5+ ticks | 2.74 |

70.3% of the time NVDA trades at minimum tick spread on XNAS — an extremely liquid stock.

### 3.5 Spread ACF (Sampled at 1s)

| Lag | ACF |
|---|---|
| 1 | 0.889 |
| 2 | 0.812 |
| 3 | 0.756 |
| 4 | 0.710 |
| 5 | 0.675 |

Extremely persistent. Spread regimes last for extended periods — when the book is wide, it tends to stay wide.

### 3.6 Regime-Conditional Spread (USD)

| Regime | Mean | Std |
|---|---|---|
| Pre-Market | $0.0491 | $0.2700 |
| Open Auction | $0.0309 | $0.0193 |
| Morning | $0.0140 | $0.0072 |
| Midday | $0.0126 | $0.0060 |
| Afternoon | $0.0123 | $0.0072 |
| Close Auction | $0.0126 | $0.0094 |
| Post-Market | $0.0837 | $0.0852 |

Core trading hours (morning through close auction) show tight spreads of $0.012–$0.014. Pre-market and post-market spreads are 4–7× wider.

### 3.7 Trade-Conditional Spread

| Metric | Value |
|---|---|
| Mean spread at trade time | $0.0164 |
| Median spread at trade time | $0.0100 |

Trades occur slightly above the unconditional mean spread ($0.0164 vs $0.0159), indicating mild adverse selection — trades tend to hit wider books.

---

## 4. Return Distribution

Log mid-price returns: r(t) = ln(mid(t+Δ) / mid(t)).

### 4.1 Multi-Scale Returns

| Scale | N | Mean | Std | Skew | Kurtosis |
|---|---|---|---|---|---|
| 1s | 5,432,635 | 2.81e-8 | 1.537e-4 | −0.27 | 17.45 |
| 5s | 1,088,976 | 1.48e-7 | 3.438e-4 | −0.48 | 45.45 |
| 10s | 544,749 | 2.10e-7 | 4.912e-4 | −0.37 | 9.74 |
| 30s | 181,506 | 6.51e-7 | 8.307e-4 | 0.33 | 13.48 |
| 1m | 90,637 | 4.82e-7 | 1.160e-3 | −0.29 | 105.79 |
| 5m | 17,941 | 8.04e-6 | 2.523e-3 | 0.05 | 20.90 |

Excess kurtosis is extreme at all scales (9.7–105.8), confirming heavy tails. The 1m outlier (kurtosis=105.8) likely reflects a single extreme-return event. Slight negative skew at short scales indicates flash-crash asymmetry.

### 4.2 Tail Risk

| Scale | Hill Left | Hill Right | VaR 1% | VaR 5% | CVaR 1% | CVaR 5% |
|---|---|---|---|---|---|---|
| 1s | 2.485 | 2.674 | −4.68 bps | −2.22 bps | −7.24 bps | −3.87 bps |
| 5s | 2.762 | 2.505 | −9.71 bps | −4.70 bps | −14.0 bps | −7.97 bps |
| 10s | 2.817 | 3.284 | −13.8 bps | −6.99 bps | −19.4 bps | −11.4 bps |
| 30s | 2.739 | 2.607 | −22.3 bps | −11.5 bps | −33.7 bps | −19.1 bps |
| 1m | 2.924 | 2.664 | −32.2 bps | −16.4 bps | −44.8 bps | −26.4 bps |
| 5m | 2.730 | 2.484 | −77.0 bps | −37.1 bps | −104.6 bps | −61.3 bps |

Hill tail indices are in the range 2.4–3.3, consistent with power-law tails (finite variance but infinite higher moments). Left tails tend to be slightly heavier than right tails (Hill left < Hill right at 1s, 5s), indicating asymmetric crash risk at short timescales.

### 4.3 Zero-Return Fraction

| Scale | Zero % |
|---|---|
| 1s | 34.47 |
| 5s | 13.90 |
| 10s | 9.27 |
| 30s | 5.15 |
| 1m | 3.61 |
| 5m | 1.62 |

At 1s, 34.5% of intervals have zero return — the mid-price doesn't move. This drops rapidly with scale.

### 4.4 Return ACF (Lag 1–5)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | 0.0004 | 0.0085 | 0.0019 | −0.0078 | −0.0048 |
| 5s | 0.0227 | −0.0003 | 0.0075 | 0.0225 | −0.0159 |
| 10s | −0.0121 | 0.0163 | −0.0093 | −0.0067 | 0.0079 |
| 30s | −0.0279 | 0.0253 | −0.0067 | −0.0114 | 0.0241 |
| 1m | 0.0203 | −0.0313 | 0.0216 | 0.0106 | −0.0030 |
| 5m | 0.0188 | −0.0032 | 0.0075 | 0.0072 | −0.0050 |

Returns are essentially uncorrelated at all scales — consistent with efficient pricing on XNAS.

### 4.5 Absolute Return ACF (Volatility Clustering)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | 0.116 | 0.075 | 0.089 | 0.082 | 0.084 |
| 5s | 0.254 | 0.242 | 0.230 | 0.255 | 0.254 |
| 10s | 0.267 | 0.301 | 0.305 | 0.255 | 0.278 |
| 30s | 0.284 | 0.316 | 0.279 | 0.287 | 0.267 |
| 1m | 0.316 | 0.296 | 0.265 | 0.269 | 0.290 |
| 5m | 0.305 | 0.277 | 0.255 | 0.226 | 0.190 |

Strong volatility clustering at all scales. At 5s+, absolute return ACF exceeds 0.25 at lag 1, decaying slowly — confirming GARCH-type dynamics. This is exploitable for volatility prediction.

### 4.6 Return Percentiles (5m Scale)

| Percentile | Value (bps) |
|---|---|
| p1 | −77.02 |
| p5 | −37.12 |
| p10 | −24.55 |
| p25 | −10.47 |
| p50 | 0.00 |
| p75 | +10.85 |
| p90 | +24.05 |
| p95 | +35.71 |
| p99 | +69.97 |

Distribution is slightly asymmetric: |p1| > |p99| (77 bps vs 70 bps).

### 4.7 Daily Drawdown / Runup

| Metric | Mean | Std | Max |
|---|---|---|---|
| Max drawdown | 9.62% | 10.92% | 69.56% |
| Max runup | 10.35% | 12.22% | 79.74% |

Average daily max drawdown is ~10%, but the worst day saw 69.6% — likely a major earnings or macro event.

---

## 5. Volatility

Daily realized volatility computed as the sum of squared log returns at 1s resolution (Andersen & Bollerslev, 1998).

### 5.1 Daily Realized Volatility

| Metric | Value |
|---|---|
| Mean RV | 5.510e-4 |
| Std RV | 8.096e-4 |
| Min RV | 8.983e-5 |
| Max RV | 8.786e-3 |

### 5.2 Annualized Volatility (%)

| Metric | Value |
|---|---|
| Mean | 33.35% |
| Std | 16.63% |
| Min | 15.05% |
| Max | 148.80% |

Typical NVDA vol is 33% annualized, but ranges from 15% (calm) to 149% (extreme).

### 5.3 Vol-of-Vol

| Metric | Value |
|---|---|
| Vol-of-vol (std of daily RV) | 8.096e-4 |

### 5.4 RV Persistence (Daily ACF)

| Lag | ACF |
|---|---|
| 1 | 0.663 |
| 2 | 0.621 |
| 3 | 0.453 |
| 4 | 0.316 |
| 5 | 0.244 |

Strong persistence: lag-1 ACF = 0.663. Yesterday's volatility is the single best predictor of today's. Decays slowly — consistent with long-memory volatility processes (Corsi, 2009, HAR model).

### 5.5 Per-Scale Daily RV

| Scale | Mean RV | Std RV |
|---|---|---|
| 1s | 5.510e-4 | 8.096e-4 |
| 5s | 5.523e-4 | 8.571e-4 |
| 10s | 5.640e-4 | 9.066e-4 |
| 30s | 5.376e-4 | 9.018e-4 |
| 1m | 5.233e-4 | 9.287e-4 |
| 5m | 4.900e-4 | 9.277e-4 |

RV slightly increases from 1s to 10s (microstructure noise inflates fast-scale RV) then decreases from 10s to 5m (returns average out). The signature plot (§5.6) shows this more precisely.

### 5.6 Signature Plot (Selected Scales)

| Scale (s) | Mean RV |
|---|---|
| 0.10 | 5.703e-4 |
| 0.14 | 5.659e-4 |
| 0.20 | 5.626e-4 |
| 1.06 | 5.497e-4 |
| 2.07 | 5.460e-4 |
| 2.90 | 5.457e-4 |
| 30.60 | 5.339e-4 |
| 42.85 | 5.294e-4 |
| 60.00 | 5.233e-4 |

The signature plot decreases monotonically from 5.703e-4 (0.1s) to 5.233e-4 (60s), a 8.2% decline. This moderate decline indicates relatively low microstructure noise contamination. The RV is approximately stable from ~2s to ~30s, making that range optimal for volatility estimation.

### 5.7 Spread–Volatility Correlation

| Metric | Value |
|---|---|
| Spread–Vol correlation | 0.462 |

Moderate positive correlation: high-vol days tend to have wider spreads, as market makers widen quotes to manage adverse selection risk.

---

## 6. Jump Analysis

Jump detection via the Barndorff-Nielsen & Shephard (2006) bipower variation test. Jump fraction = 1 − BV/RV.

### 6.1 BNS Test Results

| Metric | Value |
|---|---|
| Mean daily RV | 5.510e-4 |
| Mean daily BV | 4.755e-4 |
| Mean jump fraction | 17.85% |
| Std jump fraction | 5.07% |
| Min jump fraction | 4.40% |
| Max jump fraction | 29.12% |

### 6.2 Z-Statistic

| Metric | Value |
|---|---|
| Mean z-statistic | 3,262.3 |
| Std z-statistic | 940.9 |

Z-statistics are astronomically large because the test is applied to ultra-high-frequency data with millions of observations per day. Every day is statistically significant.

### 6.3 Significant Jump Days

| Metric | Value |
|---|---|
| Days with z > 1.96 | 100.0% |

Every trading day has statistically significant jump activity. On average, 17.8% of daily variance is attributed to jumps (discrete price discontinuities), with the remaining 82.2% from diffusive (continuous) price movements.

---

## 7. VPIN

Volume-Synchronized Probability of Informed Trading (Easley, López de Prado & O'Hara, 2012). Computed with volume bars of 5,000 shares and a rolling window of 50 bars.

### 7.1 VPIN Distribution

| Metric | Value |
|---|---|
| Mean | 0.2977 |
| Std | 0.3757 |
| Min | 0.0079 |
| Max | 1.0000 |

| Percentile | VPIN |
|---|---|
| p1 | 0.0202 |
| p5 | 0.0296 |
| p10 | 0.0370 |
| p25 | 0.0559 |
| p50 | 0.0981 |
| p75 | 0.3026 |
| p90 | 1.0000 |
| p95 | 1.0000 |
| p99 | 1.0000 |

The distribution is extremely right-skewed: median VPIN is only 0.098 but 10% of volume bars have VPIN = 1.0 (maximum toxicity). This occurs during low-volume periods where individual bars are dominated by one-sided flow.

### 7.2 Daily Mean VPIN

| Metric | Value |
|---|---|
| Mean | 0.2991 |
| Std | 0.0624 |
| Min | 0.1957 |
| Max | 0.6192 |

Daily mean VPIN is remarkably stable (CV = 0.209), ranging from 0.20 to 0.62 across 233 days.

### 7.3 VPIN Intraday Pattern

| Period | Mean VPIN |
|---|---|
| Open (minute 0) | 0.7512 |
| Midday (minute 195) | 0.1044 |
| Close (minute 389) | 0.1418 |

VPIN is highest at the open (0.75) — consistent with accumulated overnight information being impounded into prices. It drops to ~0.10 during midday and rises slightly into the close (0.14).

### 7.4 VPIN–Spread Correlation

| Metric | Value |
|---|---|
| VPIN–Spread correlation | 0.440 |

Moderate positive correlation: high VPIN periods coincide with wider spreads, as market makers detect informed flow and widen quotes.

### 7.5 Volume Bar Statistics

| Metric | Value |
|---|---|
| Volume bar size | 5,000 shares |
| Rolling window | 50 bars |
| Total volume bars | 3,676,994 |
| Mean bars/day | 15,781 |

---

## 8. Microstructure Noise

Noise estimation via the realized variance ratio and Roll (1984) implied spread.

### 8.1 Noise Metrics

| Metric | Mean | Std |
|---|---|---|
| Noise variance | 1.646e-10 | 2.084e-10 |
| Signal-to-noise ratio | 7,456,774 | 17,989,523 |
| Roll implied spread | $0.0000474 | $0.0000657 |

### 8.2 Signature Plot Summary

| Scale | Mean RV |
|---|---|
| Fastest (0.10s) | 5.703e-4 |
| Slowest (60.0s) | 5.233e-4 |
| Ratio (fast/slow) | 1.090 |

The fast/slow RV ratio of 1.09 indicates only ~9% noise inflation at the fastest timescale — NVDA on XNAS has very low microstructure noise, consistent with its deep liquidity. The Roll implied spread ($0.000047) is far below the actual quoted spread ($0.016), confirming that trades execute efficiently near the mid.

---

## 9. Order Book Depth

### 9.1 Bid Depth Profile (Shares)

| Level | Mean | Std |
|---|---|---|
| L1 | 544 | 6,075 |
| L2 | 659 | 4,286 |
| L3 | 727 | 3,840 |
| L4 | 787 | 3,577 |
| L5 | 846 | 3,926 |
| L6 | 902 | 4,180 |
| L7 | 937 | 3,966 |
| L8 | 987 | 4,511 |
| L9 | 982 | 4,385 |
| L10 | 957 | 4,514 |

### 9.2 Ask Depth Profile (Shares)

| Level | Mean | Std |
|---|---|---|
| L1 | 610 | 14,107 |
| L2 | 766 | 11,236 |
| L3 | 853 | 11,865 |
| L4 | 902 | 9,373 |
| L5 | 957 | 9,226 |
| L6 | 1,057 | 11,548 |
| L7 | 1,134 | 13,214 |
| L8 | 1,200 | 14,610 |
| L9 | 1,195 | 11,989 |
| L10 | 1,187 | 11,784 |

Depth increases with distance from mid (L1 < L10) on both sides — typical of limit order books. Ask depth is consistently higher than bid depth at every level, with notably higher variance (ask L1 std=14,107 vs bid L1 std=6,075), reflecting occasional large resting sell orders (possibly institutional).

### 9.3 L1 Concentration

| Metric | Value |
|---|---|
| Mean L1 concentration | 0.0589 |
| Std L1 concentration | 0.0679 |

Only 5.9% of total 10-level depth sits at L1. This means ~94% of visible liquidity is behind the best quote — multi-level depth features (MLOFI) are critical.

### 9.4 Depth Imbalance

Depth imbalance = (bid_depth − ask_depth) / (bid_depth + ask_depth), computed at L1–L10.

| Metric | Value |
|---|---|
| Mean | −0.0111 |
| Std | 0.3500 |
| Skewness | −0.0050 |

| Percentile | Value |
|---|---|
| p1 | −0.846 |
| p5 | −0.611 |
| p10 | −0.464 |
| p25 | −0.225 |
| p50 | −0.008 |
| p75 | +0.207 |
| p90 | +0.446 |
| p95 | +0.587 |
| p99 | +0.816 |

Near-symmetric (mean ≈ −0.01), indicating no persistent directional imbalance. High variance (std=0.35) means the imbalance swings widely — this is a high-information-content signal for short-term prediction.

### 9.5 Total Depth

| Metric | Value |
|---|---|
| Mean (shares) | 18,189 |
| Std (shares) | 44,001 |
| CV | 2.419 |

Extremely high coefficient of variation (2.42) — total depth varies dramatically, often driven by large iceberg or institutional orders temporarily inflating one side.

---

## 10. Trade Microstructure

### 10.1 Trade Volume

| Metric | Value |
|---|---|
| Total trades | 193,879,475 |
| Total volume | 18,384,430,535 shares |
| Mean trades/day | 832,101 |

### 10.2 Trade Size Distribution

| Metric | Value (shares) |
|---|---|
| Mean | 94.82 |
| Std | 18,628 |

| Percentile | Shares |
|---|---|
| p50 | 37 |
| p95 | 200 |
| p99 | 600 |

Median trade is only 37 shares (~$5K at ~$135/share). The mean (95 shares) is pulled up by occasional very large trades (std=18,628). This is characteristic of algorithmic order-splitting.

### 10.3 Price Level Classification

| Level | Share (%) |
|---|---|
| At bid | 40.47 |
| At ask | 37.01 |
| Inside spread | 22.51 |
| Outside spread | 0.002 |

22.5% of trades execute inside the spread — significant midpoint crossing / hidden order activity.

### 10.4 Trade-Through Rate

| Metric | Value |
|---|---|
| Trade-through count | 4,325 |
| Trade-through rate | 0.0022% |

Negligible trade-through rate, confirming clean price discovery.

### 10.5 Inter-Trade Time (seconds)

| Metric | Value |
|---|---|
| Mean | 0.247s |
| Std | 1.873s |

| Percentile | Seconds |
|---|---|
| p1 | 0.000001 |
| p5 | 0.000004 |
| p10 | 0.000016 |
| p25 | 0.000048 |
| p50 | 0.001731 |
| p75 | 0.088506 |
| p90 | 0.410523 |
| p95 | 0.894961 |
| p99 | 4.156938 |

Median inter-trade time is 1.7ms — extremely fast. 25% of trades arrive within 48μs of the previous trade. The heavy right tail (mean=247ms vs median=1.7ms) reflects pre/post-market quiet periods.

### 10.6 Trade Clustering

| Metric | Value |
|---|---|
| Cluster fraction | 27.15% |
| Mean cluster size | 34.6 trades |
| Max cluster size | 265,422 |
| Total clusters | 1,519,157 |

27.2% of trades occur in clusters (multiple fills within the same timestamp). The extremely large max cluster (265K) likely corresponds to a market-on-close or similar bulk execution.

### 10.7 Large Trade Impact

Threshold: ≥200 shares.

| Metric | Value (bps) |
|---|---|
| Mean impact | 0.943 |
| p50 impact | 0.690 |
| p75 impact | 0.922 |
| p95 impact | 2.766 |
| p99 impact | 6.816 |

Median large-trade impact is 0.69 bps — roughly half the quoted spread. The p99 at 6.8 bps indicates that extreme-size trades can move the market 7 bps, which is significant for execution cost modeling.

---

## 11. Order Lifecycle

### 11.1 Aggregate Metrics

| Metric | Value |
|---|---|
| Fill rate | 4.776% |
| Cancel-to-add ratio | 1.0111 |
| Partial fill fraction | 15.79% |
| Duration–size correlation | −0.205 |
| Total adds | 1,329,541,122 |
| Total cancels | 1,344,345,314 |
| Total fills | 63,351,004 |
| Total resolved | 1,326,378,966 |

Only 4.8% of orders get filled. The cancel-to-add ratio >1 (1.011) is consistent with ITCH mechanics where some cancels correspond to modifications (cancel + re-add). Negative duration–size correlation (−0.205) means larger orders have shorter lifetimes — they are either filled quickly or cancelled quickly.

### 11.2 Lifetime Distribution (seconds)

| Metric | Value |
|---|---|
| Mean | 641.9s |
| p50 | 0.018s |
| p95 | 151.2s |
| p99 | 23,352.8s |

The median order lifetime is 18ms — most orders are cancelled almost immediately (HFT market making). The mean (642s ≈ 10.7 min) is pulled up by long-lived resting orders. The extreme p99 (23,353s ≈ 6.5 hours) represents orders placed at market open and cancelled at close.

### 11.3 Transition Matrix (Probabilities)

| From → To | Cancel | Trade |
|---|---|---|
| **Add** | 0.9433 | 0.0567 |
| **Trade** | 1.0000 | — |

In XNAS (ITCH), 94.3% of adds result in a cancel, and 5.7% result in a trade. After a partial fill (Trade), the remaining quantity always gets cancelled (100%). No Modify state exists in ITCH.

### 11.4 Regime-Conditional Fill Rate

| Regime | Fill Rate (%) |
|---|---|
| Pre-Market | 7.80 |
| Open Auction | 6.97 |
| Morning | 5.18 |
| Midday | 4.42 |
| Afternoon | 5.43 |
| Close Auction | 10.62 |
| Post-Market | 0.94 |

Close Auction has the highest fill rate (10.6%) — end-of-day crossing activity. Post-Market has the lowest (0.9%) — orders are placed but rarely matched.

### 11.5 Regime-Conditional Lifetime (seconds)

| Regime | Mean Lifetime |
|---|---|
| Pre-Market | 1,325.5s |
| Open Auction | 11,327.2s |
| Morning | 129.9s |
| Midday | 69.4s |
| Afternoon | 90.3s |
| Close Auction | 74.4s |
| Post-Market | 192.4s |

Open Auction orders have extremely long lifetimes (11,327s ≈ 3.1 hours) because orders accumulate pre-open. During core hours, midday has the shortest lifetime (69s) and morning the longest (130s).

---

## 12. Liquidity Costs

### 12.1 Effective Spread (bps)

Effective spread = 2 × |trade_price − mid_price| / mid_price × 10,000.

| Metric | Value (bps) |
|---|---|
| Mean | 0.798 |
| Std | 3.000 |

| Percentile | bps |
|---|---|
| p1 | 0.000 |
| p5 | 0.000 |
| p10 | 0.000 |
| p25 | 0.529 |
| p50 | 0.574 |
| p75 | 0.889 |
| p90 | 1.469 |
| p95 | 2.061 |
| p99 | 5.101 |

Many trades execute at the mid (p1–p10 = 0.000 bps), consistent with the 22.5% inside-spread execution rate.

### 12.2 Volume-Weighted Effective Spread

| Metric | Value (bps) |
|---|---|
| VWES | 1.969 |

The VWES (1.97 bps) is 2.5× the simple mean (0.80 bps), indicating that large-volume trades pay substantially wider effective spreads.

### 12.3 Microprice Deviation

| Metric | Value (bps) |
|---|---|
| Mean | 0.239 |
| Std | 0.911 |

The microprice (volume-weighted mid) deviates from the mid by only 0.24 bps on average, indicating that L1 depth is reasonably balanced.

---

## 13. Feature Extractor Configuration

Based on the 233-day XNAS statistical profile, the following configurations are recommended for the feature extractor pipeline.

### 13.1 Sampling Recommendation

| Parameter | Recommendation | Justification |
|---|---|---|
| Primary timescale | 1s–5s | OFI–return r > 0.57 at 1s; noise contamination only 9% |
| Labeling horizon | 30s–5m | Return std 8.3–25.2 bps provides sufficient signal for classification |
| Min events/sample | 100 | Ensures meaningful OFI aggregation per bin |

### 13.2 Feature Groups

| Group | Enable | Priority | Justification |
|---|---|---|---|
| OFI (L1) | Yes | Critical | r = 0.577–0.707, explains 33–50% of return variance |
| MLOFI (L1–L10) | Yes | Critical | L1 concentration only 5.9% — 94% of depth is behind L1 |
| Spread features | Yes | High | 70% 1-tick, ACF(1)=0.889, regime-dependent |
| Depth imbalance | Yes | High | std=0.35, high information content |
| Return features | Yes | High | Volatility clustering (abs ACF > 0.25 at 5s+) |
| VPIN | Yes | Medium | r(VPIN,spread)=0.44, but high variance |
| Trade size/clustering | Yes | Medium | 27% clustering, median=37 shares (algo signature) |
| Lifecycle | No | Low | Fill rate 4.8% too slow for short-term signals |

### 13.3 Labeling Thresholds (5m Horizon)

| Label | Threshold | Justification |
|---|---|---|
| Up | > +25 bps | ~p90 of 5m returns |
| Down | < −25 bps | ~p10 of 5m returns |
| Neutral | −25 to +25 bps | Middle 80% |

### 13.4 Horizon Recommendations

| Horizon | Use Case |
|---|---|
| 10s | Ultra-short alpha capture; requires sub-ms execution |
| 1m | Primary prediction horizon; good signal with manageable execution |
| 5m | Volatility/regime prediction; label construction |

---

## 14. Actionable Signals for Statistical Arbitrage

### Signal 1: OFI Momentum

- **Evidence**: OFI ACF(1) at 5m = 0.266, OFI–return r = 0.707 at 5m
- **Strategy**: When 5m OFI exceeds 1σ, enter in direction of flow; hold for 1–2 bars
- **Risk**: OFI lag-1 return correlation is −0.022 (mild reversion); requires fast execution to capture contemporaneous move
- **Priority**: Highest

### Signal 2: Volatility Regime Switching

- **Evidence**: RV ACF(1) = 0.663; annualized vol ranges 15–149%; abs-return ACF at 5s > 0.25
- **Strategy**: Use HAR-type model to predict next-day vol; size positions inversely to predicted vol; switch between mean-reversion (low vol) and momentum (high vol) strategies
- **Risk**: Vol-of-vol is high (std = 8.1e-4); regime transitions are sudden
- **Priority**: High

### Signal 3: Spread Regime Exploitation

- **Evidence**: 70.3% of time at 1-tick spread; spread ACF(1) = 0.889; regime spreads range $0.012–$0.084
- **Strategy**: Market-make during wide-spread regimes (>2 ticks) where edge > execution cost; avoid market-making when spread compresses to 1 tick
- **Risk**: Wide-spread regimes coincide with higher volatility (spread–vol r = 0.462)
- **Priority**: High

### Signal 4: VPIN-Based Informed Trading Detection

- **Evidence**: Mean VPIN = 0.30; open VPIN = 0.75 vs midday = 0.10; VPIN–spread r = 0.44
- **Strategy**: Avoid providing liquidity when VPIN > 0.5 (top 25%); increase participation when VPIN < 0.05 (bottom 25%)
- **Risk**: VPIN is volume-synchronized and may lag during flash events
- **Priority**: Medium

### Signal 5: Depth Imbalance Alpha

- **Evidence**: Depth imbalance std = 0.35 with near-zero mean; L1 concentration only 5.9%
- **Strategy**: Use multi-level depth imbalance as a short-term directional predictor; combine with OFI for stronger signal
- **Risk**: Depth can be spoofed; imbalance predictive power decays within seconds
- **Priority**: Medium

---

## 15. Summary

NVDA on XNAS is an ultra-liquid, heavily traded stock with 12.3M events/day and a 70.3% one-tick spread regime. OFI is the dominant signal, explaining 33–50% of return variance across timescales with near-zero lag structure — confirming contemporaneous price impact. Returns are heavy-tailed (Hill index ~2.5–3.3) with strong volatility clustering (absolute-return ACF > 0.25), making volatility prediction a viable secondary signal. The order book is deep but concentrated behind L1 (only 5.9% at best quote), making multi-level features essential. VPIN averages 0.30 with extreme intraday variation (open: 0.75 → midday: 0.10), serving as a regime indicator for informed trading detection. Only 4.8% of orders are filled, with median lifetime of 18ms, reflecting dominant HFT activity. The optimal feature extraction configuration focuses on OFI, MLOFI, spread dynamics, and depth imbalance at 1s–5s timescales, with labeling at 30s–5m horizons using ±25 bps thresholds.

| Key Metric | Value |
|---|---|
| OFI–Return r (5m) | 0.707 |
| Median spread | $0.01 (1 tick) |
| Annualized vol | 33.4% |
| Fill rate | 4.78% |
| Mean VPIN | 0.299 |
| Daily events | 12.3M |
| Median trade size | 37 shares |
