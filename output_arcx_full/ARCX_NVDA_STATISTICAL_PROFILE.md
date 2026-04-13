# ARCX NVDA Statistical Profile

| Field | Value |
|---|---|
| **Instrument** | NVDA (NVIDIA Corporation) |
| **Exchange** | ARCX (NYSE Arca — PILLAR protocol) |
| **Period** | 2025-02-03 to 2026-01-06 (233 trading days) |
| **Total Events** | 1,371,860,852 |
| **Profiler Version** | mbo-statistical-profiler v0.1.0 |
| **Timescales** | 1s, 5s, 10s, 30s, 1m, 5m |
| **Reservoir Capacity** | 10,000 |

---

## 1. Data Characterization

### 1.1 Event Volume

| Metric | Value |
|---|---|
| Total events | 1,371,860,852 |
| Mean events/day | 5,888,247 |
| Days processed | 233 |

### 1.2 Action Distribution

| Action | Count | Share (%) |
|---|---|---|
| Add | 612,917,896 | 44.6793 |
| Cancel | 615,298,763 | 44.8527 |
| Trade | 109,084,515 | 7.9515 |
| Clear | 233 | 0.0000 |
| Fill | 0 | 0.0000 |
| Modify | 34,559,212 | 2.5191 |

PILLAR protocol emits native Modify messages (2.5% of events), unlike ITCH. The trade share (7.95%) is higher than XNAS (6.76%), indicating a higher fraction of aggressive order flow.

### 1.3 Book Consistency

| State | Share (%) |
|---|---|
| Valid | 99.9987 |
| Empty | 0.0013 |
| Crossed | 0.0000 |
| Locked | 0.0000 |

Empty-book fraction is 10× higher than XNAS (0.0013% vs 0.0001%), reflecting ARCX's lower depth.

### 1.4 Regime Distribution

| Regime | Share (%) |
|---|---|
| Pre-Market | 4.50 |
| Open Auction | 2.57 |
| Morning (09:45–11:30 ET) | 23.37 |
| Midday (11:30–14:30 ET) | 55.42 |
| Afternoon (14:30–15:45 ET) | 10.84 |
| Close Auction (15:45–16:00 ET) | 1.65 |
| Post-Market | 1.65 |

Pre-Market share (4.5%) is 2.5× higher than XNAS (1.8%), reflecting ARCX's stronger pre-market participation. Close Auction share (1.65%) is lower than XNAS (2.46%).

---

## 2. Order Flow Imbalance (OFI)

OFI is defined as the net signed order flow: additions at the bid minus additions at the ask, plus cancellations at the ask minus cancellations at the bid, plus trades at the ask minus trades at the bid. Aggregated over non-overlapping time windows (Cont, Kukanov & Stoikov, 2014).

### 2.1 OFI–Return Correlation (Lag 0)

| Scale | r (lag 0) | r² |
|---|---|---|
| 1s | 0.6884 | 0.4739 |
| 5s | 0.7187 | 0.5166 |
| 10s | 0.7292 | 0.5318 |
| 30s | 0.7352 | 0.5405 |
| 1m | 0.7290 | 0.5314 |
| 5m | 0.7154 | 0.5118 |

OFI explains 47–54% of return variance — significantly stronger than XNAS (33–50%). This indicates ARCX order flow is more informative: each unit of OFI carries more price impact on a secondary exchange with lower depth.

### 2.2 OFI–Spread Correlation (Lag 0)

| Scale | r (lag 0) |
|---|---|
| 1s | 0.0084 |
| 5s | 0.0110 |
| 10s | 0.0121 |
| 30s | 0.0114 |
| 1m | 0.0160 |
| 5m | −0.0107 |

OFI–Spread correlation is near-zero but slightly higher than XNAS at most scales. The negative 5m correlation (−0.011) suggests that sustained directional flow at 5m actually coincides with tighter spreads (liquidity providers stepping in).

### 2.3 OFI–Return Lag Structure (5m Scale)

| Lag | r |
|---|---|
| 0 | 0.7154 |
| 1 | −0.0178 |
| 2 | −0.0163 |
| 3 | −0.0259 |
| 4 | −0.0111 |
| 5 | −0.0377 |

Same pattern as XNAS: sharp contemporaneous signal, near-zero predictive lag structure. The negative lag-5 (−0.038) is slightly stronger than XNAS, suggesting more delayed mean-reversion on ARCX.

### 2.4 OFI Persistence (ACF)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | −0.003 | 0.005 | 0.013 | 0.023 | −0.050 |
| 5s | 0.149 | 0.039 | 0.005 | 0.015 | 0.005 |
| 10s | 0.104 | 0.025 | 0.016 | 0.003 | 0.005 |
| 30s | 0.079 | 0.079 | 0.050 | 0.034 | 0.060 |
| 1m | 0.155 | 0.109 | 0.096 | 0.086 | 0.077 |
| 5m | 0.301 | 0.194 | 0.157 | 0.107 | 0.103 |

OFI persistence is higher on ARCX than XNAS at 5m (ACF(1) = 0.301 vs 0.266). At 1s, ARCX shows near-zero or slightly negative ACF (−0.003), indicating faster mean-reversion of short-timescale flow.

### 2.5 Component Fractions

| Component | Fraction |
|---|---|
| Add | 0.5088 |
| Cancel | 0.4282 |
| Trade | 0.0630 |

Similar to XNAS but with slightly higher cancel fraction (42.8% vs 40.3%) and lower trade fraction (6.3% vs 8.4%).

### 2.6 Regime OFI Intensity

| Regime | Mean OFI | Std OFI | Observations |
|---|---|---|---|
| Pre-Market | 22.57 | 152.60 | 61,674,312 |
| Open Auction | 21.86 | 113.19 | 35,202,218 |
| Morning | 27.53 | 74.92 | 320,555,086 |
| Midday | 33.83 | 77.40 | 760,336,182 |
| Afternoon | 39.22 | 92.70 | 148,709,937 |
| Close Auction | 45.39 | 119.96 | 22,685,282 |
| Post-Market | 23.47 | 286.66 | 22,679,978 |

Close Auction has the highest mean OFI (45.39) — even higher than XNAS (37.43). The monotonic increase from morning (27.5) through close auction (45.4) during RTH reflects intensifying directional pressure through the day.

### 2.7 Aggressor Ratio

| Metric | Value |
|---|---|
| Aggressor ratio (buyer/total) | 0.5000 |

Near-perfect balance.

### 2.8 Cumulative Delta (End-of-Day)

| Metric | Value |
|---|---|
| Mean EOD delta | −54,405 shares |
| Std EOD delta | 1,352,980 shares |

Slight net sell bias (−54K shares/day) on ARCX, contrasting with XNAS's slight buy bias (+18K). Statistically indistinguishable from zero given the std.

---

## 3. Spread Analysis

### 3.1 Spread Distribution (USD)

| Metric | Value |
|---|---|
| Mean | $0.0175 |
| Std | $0.0588 |
| Min | $0.0100 |
| Max | $277.7800 |
| Skewness | 15.62 |
| Kurtosis | 453.17 |

| Percentile | USD |
|---|---|
| p1 | 0.0100 |
| p5 | 0.0100 |
| p10 | 0.0100 |
| p25 | 0.0100 |
| p50 | 0.0100 |
| p75 | 0.0200 |
| p90 | 0.0300 |
| p95 | 0.0300 |
| p99 | 0.0800 |

ARCX median spread is also 1 tick ($0.01), same as XNAS. But the mean ($0.0175) is 10% wider than XNAS ($0.0159), and the max ($277.78) is more extreme.

### 3.2 Spread Distribution (Ticks)

| Metric | Value |
|---|---|
| Mean | 1.750 ticks |
| Std | 5.878 ticks |
| p50 | 1.000 tick |
| p95 | 3.000 ticks |
| p99 | 8.000 ticks |

### 3.3 Spread Distribution (Basis Points)

| Metric | Value |
|---|---|
| Mean | 1.220 bps |
| Std | 3.958 bps |
| Min | 0.471 bps |
| Max | 14,929 bps |

| Percentile | bps |
|---|---|
| p1 | 0.501 |
| p5 | 0.534 |
| p10 | 0.549 |
| p25 | 0.599 |
| p50 | 0.917 |
| p75 | 1.445 |
| p90 | 1.881 |
| p95 | 2.588 |
| p99 | 5.739 |

ARCX median spread in bps (0.917) is 25% wider than XNAS (0.733), reflecting lower competition at the top of book.

### 3.4 Width Classification

| Width | Share (%) |
|---|---|
| 1 tick | 54.54 |
| 2 ticks | 35.40 |
| 3–4 ticks | 7.40 |
| 5+ ticks | 2.66 |

ARCX spends only 54.5% of the time at 1-tick spread (vs 70.3% on XNAS) — significantly more time at 2+ tick spreads. This creates more opportunity for spread-capture strategies but also higher execution costs.

### 3.5 Spread ACF (Sampled at 1s)

| Lag | ACF |
|---|---|
| 1 | 0.933 |
| 2 | 0.870 |
| 3 | 0.825 |
| 4 | 0.785 |
| 5 | 0.753 |

Even more persistent than XNAS (ACF(1) = 0.933 vs 0.889). Spread regimes on ARCX are stickier — once the spread widens, it stays wide longer.

### 3.6 Regime-Conditional Spread (USD)

| Regime | Mean | Std |
|---|---|---|
| Pre-Market | $0.0438 | $0.2659 |
| Open Auction | $0.0290 | $0.0171 |
| Morning | $0.0164 | $0.0080 |
| Midday | $0.0144 | $0.0068 |
| Afternoon | $0.0139 | $0.0079 |
| Close Auction | $0.0161 | $0.0142 |
| Post-Market | $0.0716 | $0.0880 |

Core-hours spreads are $0.014–$0.016 (vs $0.012–$0.014 on XNAS). ARCX spreads are consistently wider by $0.002 during RTH.

### 3.7 Trade-Conditional Spread

| Metric | Value |
|---|---|
| Mean spread at trade time | $0.0210 |
| Median spread at trade time | $0.0200 |

Trade-conditional spread ($0.021) is 28% wider than XNAS ($0.016), confirming higher execution costs on ARCX. The median of $0.02 (2 ticks) is notably different from XNAS ($0.01, 1 tick).

---

## 4. Return Distribution

Log mid-price returns: r(t) = ln(mid(t+Δ) / mid(t)).

### 4.1 Multi-Scale Returns

| Scale | N | Mean | Std | Skew | Kurtosis |
|---|---|---|---|---|---|
| 1s | 5,435,511 | 2.87e-8 | 1.535e-4 | 0.64 | 29.97 |
| 5s | 1,089,796 | 1.48e-7 | 3.432e-4 | −0.62 | 45.25 |
| 10s | 544,959 | 2.06e-7 | 4.906e-4 | −0.28 | 10.98 |
| 30s | 181,507 | 6.43e-7 | 8.304e-4 | 0.31 | 13.46 |
| 1m | 90,637 | 4.56e-7 | 1.160e-3 | −0.39 | 104.80 |
| 5m | 17,941 | 8.08e-6 | 2.521e-3 | 0.06 | 20.84 |

Return volatility is nearly identical to XNAS at all scales (within 1%), as expected — it is the same stock, so the price process is fundamentally the same. The key difference is at 1s: ARCX has positive skew (+0.64 vs XNAS −0.27), suggesting brief upward price spikes on ARCX (possibly from aggressive buying lifting a thinner book).

### 4.2 Tail Risk

| Scale | Hill Left | Hill Right | VaR 1% | VaR 5% | CVaR 1% | CVaR 5% |
|---|---|---|---|---|---|---|
| 1s | 2.550 | 2.739 | −4.48 bps | −2.00 bps | −6.64 bps | −3.57 bps |
| 5s | 2.982 | 2.495 | −9.36 bps | −4.80 bps | −13.7 bps | −7.90 bps |
| 10s | 2.866 | 3.161 | −13.9 bps | −6.99 bps | −19.9 bps | −11.5 bps |
| 30s | 2.756 | 2.539 | −22.3 bps | −11.5 bps | −33.6 bps | −19.0 bps |
| 1m | 2.921 | 2.653 | −32.4 bps | −16.4 bps | −44.8 bps | −26.4 bps |
| 5m | 2.716 | 2.474 | −77.5 bps | −37.1 bps | −104.5 bps | −61.3 bps |

Tail risk is essentially identical to XNAS at 10s+ scales. At 1s, ARCX shows slightly lower VaR (−4.48 vs −4.68 bps) due to its wider spread acting as a buffer.

### 4.3 Zero-Return Fraction

| Scale | Zero % |
|---|---|
| 1s | 28.80 |
| 5s | 11.16 |
| 10s | 7.42 |
| 30s | 4.07 |
| 1m | 2.87 |
| 5m | 1.30 |

ARCX has fewer zero-return intervals at every scale (e.g., 28.8% vs 34.5% at 1s). This is because ARCX's thinner book means the mid-price moves more frequently — even small orders can shift the best quote.

### 4.4 Return ACF (Lag 1–5)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | −0.0094 | 0.0119 | 0.0034 | −0.0100 | −0.0015 |
| 5s | 0.0216 | 0.0000 | 0.0092 | 0.0236 | −0.0176 |
| 10s | −0.0097 | 0.0161 | −0.0107 | −0.0066 | 0.0075 |
| 30s | −0.0276 | 0.0255 | −0.0068 | −0.0116 | 0.0241 |
| 1m | 0.0199 | −0.0308 | 0.0214 | 0.0106 | −0.0036 |
| 5m | 0.0187 | −0.0034 | 0.0077 | 0.0070 | −0.0047 |

Returns are uncorrelated at all scales, identical to XNAS — efficient pricing.

### 4.5 Absolute Return ACF (Volatility Clustering)

| Scale | Lag 1 | Lag 2 | Lag 3 | Lag 4 | Lag 5 |
|---|---|---|---|---|---|
| 1s | 0.118 | 0.080 | 0.088 | 0.083 | 0.088 |
| 5s | 0.251 | 0.246 | 0.231 | 0.254 | 0.255 |
| 10s | 0.266 | 0.303 | 0.306 | 0.249 | 0.280 |
| 30s | 0.284 | 0.317 | 0.280 | 0.289 | 0.267 |
| 1m | 0.314 | 0.297 | 0.265 | 0.268 | 0.290 |
| 5m | 0.305 | 0.278 | 0.255 | 0.227 | 0.190 |

Virtually identical to XNAS. Volatility clustering is a property of the stock, not the exchange.

### 4.6 Return Percentiles (5m Scale)

| Percentile | Value (bps) |
|---|---|
| p1 | −77.45 |
| p5 | −37.08 |
| p10 | −24.55 |
| p25 | −10.43 |
| p50 | 0.00 |
| p75 | +10.84 |
| p90 | +24.09 |
| p95 | +35.70 |
| p99 | +69.69 |

Nearly identical to XNAS within sampling noise. Confirms returns are driven by the same underlying price process.

### 4.7 Daily Drawdown / Runup

| Metric | Mean | Std | Max |
|---|---|---|---|
| Max drawdown | 6.25% | 7.65% | 70.41% |
| Max runup | 9.13% | 8.34% | 57.13% |

ARCX shows lower mean drawdown (6.3% vs 9.6%) but similar max (70.4% vs 69.6%). ARCX's thinner book may partially absorb directional shocks differently.

---

## 5. Volatility

Daily realized volatility computed as the sum of squared log returns at 1s resolution (Andersen & Bollerslev, 1998).

### 5.1 Daily Realized Volatility

| Metric | Value |
|---|---|
| Mean RV | 5.493e-4 |
| Std RV | 8.070e-4 |
| Min RV | 9.091e-5 |
| Max RV | 8.740e-3 |

### 5.2 Annualized Volatility (%)

| Metric | Value |
|---|---|
| Mean | 33.30% |
| Std | 16.60% |
| Min | 15.14% |
| Max | 148.41% |

Virtually identical to XNAS (33.35%), as expected — same stock, same period.

### 5.3 Vol-of-Vol

| Metric | Value |
|---|---|
| Vol-of-vol (std of daily RV) | 8.070e-4 |

### 5.4 RV Persistence (Daily ACF)

| Lag | ACF |
|---|---|
| 1 | 0.663 |
| 2 | 0.620 |
| 3 | 0.452 |
| 4 | 0.316 |
| 5 | 0.243 |

Identical to XNAS within rounding (XNAS lag-1 = 0.663, ARCX = 0.663). Confirms volatility persistence is a stock-level property.

### 5.5 Per-Scale Daily RV

| Scale | Mean RV | Std RV |
|---|---|---|
| 1s | 5.493e-4 | 8.070e-4 |
| 5s | 5.509e-4 | 8.542e-4 |
| 10s | 5.630e-4 | 9.044e-4 |
| 30s | 5.371e-4 | 9.012e-4 |
| 1m | 5.231e-4 | 9.299e-4 |
| 5m | 4.895e-4 | 9.243e-4 |

### 5.6 Signature Plot (Selected Scales)

| Scale (s) | Mean RV |
|---|---|
| 0.10 | 5.746e-4 |
| 0.14 | 5.690e-4 |
| 0.20 | 5.645e-4 |
| 1.06 | 5.479e-4 |
| 2.07 | 5.440e-4 |
| 2.90 | 5.440e-4 |
| 30.60 | 5.339e-4 |
| 42.85 | 5.294e-4 |
| 60.00 | 5.231e-4 |

The signature plot declines 9.0% from 0.1s to 60s (5.746e-4 → 5.231e-4), slightly more than XNAS (8.2%), indicating marginally higher microstructure noise on ARCX.

### 5.7 Spread–Volatility Correlation

| Metric | Value |
|---|---|
| Spread–Vol correlation | 0.482 |

Slightly higher than XNAS (0.462), indicating ARCX market makers are more sensitive to volatility in their quoting behavior.

---

## 6. Jump Analysis

Jump detection via the Barndorff-Nielsen & Shephard (2006) bipower variation test. Jump fraction = 1 − BV/RV.

### 6.1 BNS Test Results

| Metric | Value |
|---|---|
| Mean daily RV | 5.493e-4 |
| Mean daily BV | 4.790e-4 |
| Mean jump fraction | 16.46% |
| Std jump fraction | 4.54% |
| Min jump fraction | 4.11% |
| Max jump fraction | 27.23% |

### 6.2 Z-Statistic

| Metric | Value |
|---|---|
| Mean z-statistic | 2,988.0 |
| Std z-statistic | 836.2 |

### 6.3 Significant Jump Days

| Metric | Value |
|---|---|
| Days with z > 1.96 | 100.0% |

Jump fraction is slightly lower on ARCX (16.5% vs 17.8%) and z-statistics are lower (2,988 vs 3,262), consistent with ARCX having fewer events per day. The same jumps are detected but with less statistical power.

---

## 7. VPIN

Volume-Synchronized Probability of Informed Trading (Easley, López de Prado & O'Hara, 2012). Computed with volume bars of 5,000 shares and a rolling window of 50 bars.

### 7.1 VPIN Distribution

| Metric | Value |
|---|---|
| Mean | 0.0787 |
| Std | 0.0771 |
| Min | 0.0076 |
| Max | 0.9999 |

| Percentile | VPIN |
|---|---|
| p1 | 0.0155 |
| p5 | 0.0213 |
| p10 | 0.0262 |
| p25 | 0.0373 |
| p50 | 0.0563 |
| p75 | 0.0903 |
| p90 | 0.1471 |
| p95 | 0.2051 |
| p99 | 0.4054 |

ARCX VPIN (mean=0.079) is dramatically lower than XNAS (mean=0.298). The distribution is far less skewed — p90 = 0.147 vs XNAS p90 = 1.000. This indicates ARCX trade flow is more balanced within volume bars, likely because informed traders preferentially route to XNAS (the primary listing exchange).

### 7.2 Daily Mean VPIN

| Metric | Value |
|---|---|
| Mean | 0.0748 |
| Std | 0.0203 |
| Min | 0.0333 |
| Max | 0.1484 |

ARCX daily mean VPIN (0.075) is 4× lower than XNAS (0.299). The low max (0.148) means ARCX never experiences the sustained high-toxicity flow seen on XNAS.

### 7.3 VPIN Intraday Pattern

| Period | Mean VPIN |
|---|---|
| Open (minute 0) | 0.1043 |
| Midday (minute 195) | 0.0839 |
| Close (minute 389) | 0.0555 |

Unlike XNAS where VPIN spikes at open (0.75) and is elevated at close (0.14), ARCX VPIN decreases monotonically through the day (0.10 → 0.06). Informed traders do not concentrate their flow through ARCX even at the open.

### 7.4 VPIN–Spread Correlation

| Metric | Value |
|---|---|
| VPIN–Spread correlation | 0.099 |

Very weak correlation (0.10 vs XNAS 0.44). On ARCX, VPIN does not drive spread widening — spreads are driven by other factors (depth, volatility).

### 7.5 Volume Bar Statistics

| Metric | Value |
|---|---|
| Volume bar size | 5,000 shares |
| Rolling window | 50 bars |
| Total volume bars | 1,316,171 |
| Mean bars/day | 5,649 |

ARCX generates 2.8× fewer volume bars than XNAS (1.32M vs 3.68M), proportional to its lower volume.

---

## 8. Microstructure Noise

Noise estimation via the realized variance ratio and Roll (1984) implied spread.

### 8.1 Noise Metrics

| Metric | Mean | Std |
|---|---|---|
| Noise variance | 1.749e-10 | 2.191e-10 |
| Signal-to-noise ratio | 8,008,647 | 25,082,063 |
| Roll implied spread | $0.0000101 | $0.0000144 |

### 8.2 Signature Plot Summary

| Scale | Mean RV |
|---|---|
| Fastest (0.10s) | 5.746e-4 |
| Slowest (60.0s) | 5.231e-4 |
| Ratio (fast/slow) | 1.098 |

The fast/slow ratio (1.098) is slightly higher than XNAS (1.090), indicating marginally more microstructure noise. However, the Roll implied spread ($0.000010) is 4.7× smaller than XNAS ($0.000047), which seems paradoxical but reflects ARCX's lower trade frequency — fewer trades mean less noise accumulation per unit time.

---

## 9. Order Book Depth

### 9.1 Bid Depth Profile (Shares)

| Level | Mean | Std |
|---|---|---|
| L1 | 297 | 1,999 |
| L2 | 356 | 1,220 |
| L3 | 371 | 1,260 |
| L4 | 389 | 1,352 |
| L5 | 403 | 1,438 |
| L6 | 415 | 1,513 |
| L7 | 403 | 1,621 |
| L8 | 389 | 1,761 |
| L9 | 382 | 1,873 |
| L10 | 375 | 1,962 |

### 9.2 Ask Depth Profile (Shares)

| Level | Mean | Std |
|---|---|---|
| L1 | 307 | 3,249 |
| L2 | 391 | 3,130 |
| L3 | 404 | 2,833 |
| L4 | 421 | 2,809 |
| L5 | 437 | 2,911 |
| L6 | 459 | 3,493 |
| L7 | 454 | 3,840 |
| L8 | 443 | 4,094 |
| L9 | 434 | 4,034 |
| L10 | 425 | 3,966 |

ARCX depth is roughly half of XNAS at every level (L1 bid: 297 vs 544; L1 ask: 307 vs 610). ARCX shows a distinctive hump-shaped depth profile: depth peaks at L5–L7 then decreases, unlike XNAS where it increases monotonically to L8–L10.

### 9.3 L1 Concentration

| Metric | Value |
|---|---|
| Mean L1 concentration | 0.0764 |
| Std L1 concentration | 0.0756 |

ARCX has higher L1 concentration (7.6% vs 5.9%) — a larger fraction of total depth sits at the best quote, suggesting fewer deep resting orders.

### 9.4 Depth Imbalance

Depth imbalance = (bid_depth − ask_depth) / (bid_depth + ask_depth), computed at L1–L10.

| Metric | Value |
|---|---|
| Mean | −0.0049 |
| Std | 0.3260 |
| Skewness | 0.0146 |

| Percentile | Value |
|---|---|
| p1 | −0.793 |
| p5 | −0.557 |
| p10 | −0.423 |
| p25 | −0.206 |
| p50 | −0.010 |
| p75 | +0.185 |
| p90 | +0.398 |
| p95 | +0.545 |
| p99 | +0.799 |

Slightly narrower distribution than XNAS (std=0.326 vs 0.350), indicating more balanced books on ARCX. The slight positive skew (+0.015 vs −0.005) suggests marginally more bid-heavy snapshots.

### 9.5 Total Depth

| Metric | Value |
|---|---|
| Mean (shares) | 7,953 |
| Std (shares) | 13,261 |
| CV | 1.667 |

Total depth is 56% lower than XNAS (7,953 vs 18,189). CV is also lower (1.667 vs 2.419), meaning depth is less variable — fewer extreme-depth events.

---

## 10. Trade Microstructure

### 10.1 Trade Volume

| Metric | Value |
|---|---|
| Total trades | 109,084,515 |
| Total volume | 6,581,628,311 shares |
| Mean trades/day | 468,174 |

ARCX handles 56% of XNAS trade count and 36% of XNAS volume — the average ARCX trade is smaller.

### 10.2 Trade Size Distribution

| Metric | Value (shares) |
|---|---|
| Mean | 60.34 |
| Std | 241.09 |

| Percentile | Shares |
|---|---|
| p50 | 25 |
| p95 | 169 |
| p99 | 500 |

Median trade (25 shares, ~$3.4K) is 32% smaller than XNAS (37 shares). Mean (60 shares) is 36% smaller. This confirms ARCX attracts more granular, retail-like order flow.

### 10.3 Price Level Classification

| Level | Share (%) |
|---|---|
| At bid | 38.89 |
| At ask | 32.77 |
| Inside spread | 28.35 |
| Outside spread | 0.0004 |

Inside-spread executions are higher on ARCX (28.4% vs 22.5%), reflecting more midpoint crossing and hidden order activity. The bid/ask asymmetry (38.9% bid vs 32.8% ask) suggests more selling pressure on ARCX — consistent with the slight negative EOD delta.

### 10.4 Trade-Through Rate

| Metric | Value |
|---|---|
| Trade-through count | 479 |
| Trade-through rate | 0.0004% |

Even lower than XNAS (0.0022%), indicating extremely clean execution on ARCX.

### 10.5 Inter-Trade Time (seconds)

| Metric | Value |
|---|---|
| Mean | 0.390s |
| Std | 1.580s |

| Percentile | Seconds |
|---|---|
| p1 | 0.000001 |
| p5 | 0.000012 |
| p10 | 0.000031 |
| p25 | 0.000153 |
| p50 | 0.009064 |
| p75 | 0.220044 |
| p90 | 0.808026 |
| p95 | 1.596074 |
| p99 | 5.873025 |

Median inter-trade time (9.1ms) is 5.2× slower than XNAS (1.7ms). The 25th percentile (153μs) is 3.2× slower than XNAS (48μs). ARCX trades arrive at lower frequency, giving strategies more time to react.

### 10.6 Trade Clustering

| Metric | Value |
|---|---|
| Cluster fraction | 29.98% |
| Mean cluster size | 17.0 trades |
| Max cluster size | 92,997 |
| Total clusters | 1,923,427 |

Higher cluster fraction (30.0% vs 27.2%) but smaller clusters (mean=17 vs 35). ARCX has more frequent but smaller bursts — consistent with retail order flow patterns.

### 10.7 Large Trade Impact

Threshold: ≥150 shares.

| Metric | Value (bps) |
|---|---|
| Mean impact | 1.217 |
| p50 impact | 0.733 |
| p75 impact | 1.090 |
| p95 impact | 4.076 |
| p99 impact | 9.650 |

Large-trade impact is 29% higher on ARCX (mean=1.22 bps vs 0.94 bps on XNAS with 200-share threshold), as expected for a thinner book. The p99 at 9.65 bps (vs 6.82 on XNAS) shows that extreme trades have much larger impact.

---

## 11. Order Lifecycle

### 11.1 Aggregate Metrics

| Metric | Value |
|---|---|
| Fill rate | 5.886% |
| Cancel-to-add ratio | 1.0039 |
| Partial fill fraction | 15.38% |
| Duration–size correlation | −0.141 |
| Total adds | 612,917,896 |
| Total cancels | 615,298,763 |
| Total fills | 35,676,240 |
| Total resolved | 606,160,866 |

ARCX fill rate (5.9%) is 23% higher than XNAS (4.8%). The cancel-to-add ratio (1.004) is closer to 1.0 than XNAS (1.011), because PILLAR's native Modify reduces cancel/re-add cycles. Weaker duration–size correlation (−0.141 vs −0.205) indicates less aggressive size-dependent cancellation behavior.

### 11.2 Lifetime Distribution (seconds)

| Metric | Value |
|---|---|
| Mean | 484.5s |
| p50 | 0.031s |
| p95 | 26.7s |
| p99 | 25,718.1s |

Median lifetime (31ms) is 1.7× longer than XNAS (18ms), but p95 is much shorter (26.7s vs 151.2s). ARCX orders are either filled/cancelled quickly or rest for a very long time — a more bimodal distribution.

### 11.3 Transition Matrix (Probabilities)

| From → To | Modify | Cancel | Trade |
|---|---|---|---|
| **Add** | 0.0560 | 0.8798 | 0.0642 |
| **Modify** | 0.0104 | 0.8951 | 0.0946 |
| **Trade** | — | 1.0000 | — |

Key differences from XNAS: ARCX has Modify as a valid state. 5.6% of adds lead to a modify (price/size change). Once modified, orders have a higher trade probability (9.5%) than unmodified orders (6.4%), suggesting modifications are strategic repositioning that improves fill likelihood.

### 11.4 Regime-Conditional Fill Rate

| Regime | Fill Rate (%) |
|---|---|
| Pre-Market | 18.13 |
| Open Auction | 11.83 |
| Morning | 5.93 |
| Midday | 4.74 |
| Afternoon | 4.93 |
| Close Auction | 8.98 |
| Post-Market | 19.44 |

Pre-Market and Post-Market fill rates are dramatically higher on ARCX (18.1% and 19.4%) vs XNAS (7.8% and 0.9%). ARCX provides significant fill opportunity during extended hours.

### 11.5 Regime-Conditional Lifetime (seconds)

| Regime | Mean Lifetime |
|---|---|
| Pre-Market | 7,641.9s |
| Open Auction | 1,955.5s |
| Morning | 224.8s |
| Midday | 81.3s |
| Afternoon | 12.5s |
| Close Auction | 6.0s |
| Post-Market | 948.0s |

ARCX lifetimes decrease dramatically through the day: Pre-Market (7,642s ≈ 2.1 hours) down to Close Auction (6s). This pattern is more pronounced than XNAS, reflecting ARCX's role as an early-morning venue that transitions to rapid turnover during closing.

---

## 12. Liquidity Costs

### 12.1 Effective Spread (bps)

Effective spread = 2 × |trade_price − mid_price| / mid_price × 10,000.

| Metric | Value (bps) |
|---|---|
| Mean | 1.032 |
| Std | 1.848 |

| Percentile | bps |
|---|---|
| p1 | 0.000 |
| p5 | 0.000 |
| p10 | 0.000 |
| p25 | 0.531 |
| p50 | 0.695 |
| p75 | 1.077 |
| p90 | 1.992 |
| p95 | 2.946 |
| p99 | 6.902 |

Effective spread is 29% wider than XNAS (1.03 bps vs 0.80 bps). The p99 at 6.90 bps (vs 5.10) shows a longer tail of expensive executions.

### 12.2 Volume-Weighted Effective Spread

| Metric | Value (bps) |
|---|---|
| VWES | 1.104 |

ARCX VWES (1.10 bps) is 44% lower than XNAS (1.97 bps). This is notable: despite wider simple effective spreads, the volume-weighted spread is lower — indicating that large-volume trades on ARCX actually capture better prices (possibly through hidden/midpoint orders).

### 12.3 Microprice Deviation

| Metric | Value (bps) |
|---|---|
| Mean | 0.277 |
| Std | 1.447 |

Microprice deviation (0.28 bps) is similar to XNAS (0.24 bps) but with higher variance (std=1.45 vs 0.91), reflecting ARCX's more volatile depth profile.

---

## 13. Feature Extractor Configuration

Based on the 233-day ARCX statistical profile, the following configurations are recommended for the feature extractor pipeline.

### 13.1 Sampling Recommendation

| Parameter | Recommendation | Justification |
|---|---|---|
| Primary timescale | 1s–5s | OFI–return r > 0.69 at 1s; even stronger than XNAS |
| Labeling horizon | 30s–5m | Return std 8.3–25.2 bps provides sufficient signal |
| Min events/sample | 50 | Lower event density than XNAS requires lower threshold |

### 13.2 Feature Groups

| Group | Enable | Priority | Justification |
|---|---|---|---|
| OFI (L1) | Yes | Critical | r = 0.688–0.735, explains 47–54% of return variance |
| MLOFI (L1–L10) | Yes | Critical | L1 concentration 7.6% — 92% of depth is behind L1 |
| Spread features | Yes | Critical | Only 54.5% 1-tick; more informative than XNAS |
| Depth imbalance | Yes | High | std=0.33, high information content |
| Return features | Yes | High | Volatility clustering identical to XNAS |
| Trade size/clustering | Yes | Medium | 30% clustering; median=25 shares |
| VPIN | Yes | Low | Mean only 0.079; weak spread correlation (0.10) |
| Lifecycle | Yes | Medium | Fill rate 5.9% with Modify transitions — more informative than XNAS |

### 13.3 Labeling Thresholds (5m Horizon)

| Label | Threshold | Justification |
|---|---|---|
| Up | > +24 bps | ~p90 of 5m returns |
| Down | < −25 bps | ~p10 of 5m returns |
| Neutral | −25 to +24 bps | Middle 80% |

### 13.4 Horizon Recommendations

| Horizon | Use Case |
|---|---|
| 10s | Ultra-short alpha capture; ARCX's slower trade pace provides more reaction time |
| 1m | Primary prediction horizon; ARCX OFI slightly more predictive than XNAS |
| 5m | Volatility/regime prediction; label construction |

---

## 14. Actionable Signals for Statistical Arbitrage

### Signal 1: OFI Momentum (Higher Alpha on ARCX)

- **Evidence**: OFI ACF(1) at 5m = 0.301, OFI–return r = 0.715 at 5m; both stronger than XNAS
- **Strategy**: When 5m OFI exceeds 1σ, enter in direction of flow; hold for 1–2 bars. ARCX flow is stickier (higher ACF) so trends persist longer.
- **Risk**: Thinner book means execution impact is larger; need to control position size
- **Priority**: Highest

### Signal 2: Volatility Regime Switching

- **Evidence**: RV ACF(1) = 0.663; annualized vol ranges 15–148%; identical to XNAS
- **Strategy**: Same as XNAS — HAR-type model for vol prediction; size inversely to vol; regime-switch strategy selection
- **Risk**: Identical to XNAS (same stock, same vol process)
- **Priority**: High

### Signal 3: Spread Regime Exploitation (More Opportunity on ARCX)

- **Evidence**: Only 54.5% at 1-tick (vs 70.3% XNAS); spread ACF(1) = 0.933; trade-conditional spread is 2 ticks
- **Strategy**: Market-make during 2+ tick spread periods (45.5% of the time on ARCX vs 29.7% on XNAS). Wider spreads provide more edge per trade.
- **Risk**: Wider spreads coincide with higher volatility (spread–vol r = 0.482). Lower depth means adverse selection is more punishing.
- **Priority**: High

### Signal 4: Cross-Exchange VPIN Divergence

- **Evidence**: XNAS VPIN mean=0.30 vs ARCX VPIN mean=0.08; informed traders route to XNAS
- **Strategy**: Monitor XNAS VPIN in real-time; when it spikes (>0.5), reduce ARCX market-making activity because ARCX prices will lag XNAS
- **Risk**: Requires cross-exchange data feed; latency-sensitive
- **Priority**: Medium

### Signal 5: Extended-Hours Fill Opportunity

- **Evidence**: ARCX pre-market fill rate = 18.1% (vs XNAS 7.8%); post-market = 19.4% (vs XNAS 0.9%)
- **Strategy**: Route limit orders through ARCX during extended hours for better fill probability. Use ARCX as the primary venue for pre/post-market execution.
- **Risk**: Extended-hours spreads are 3–5× wider ($0.044–$0.072)
- **Priority**: Medium

---

## 15. Summary

NVDA on ARCX is a secondary-exchange venue with 5.9M events/day (48% of XNAS volume) and a 54.5% one-tick spread regime. Despite lower volume, ARCX OFI is more strongly correlated with returns (r = 0.688–0.735, explaining 47–54% of variance) than XNAS (r = 0.577–0.707), making it a higher-alpha venue for OFI-based strategies at the cost of higher execution impact. Returns and volatility are identical to XNAS (same stock, same price process — annualized vol 33.3%, RV ACF(1) = 0.663). VPIN is dramatically lower (mean 0.079 vs 0.299), confirming that informed flow routes primarily through XNAS. ARCX fills more orders (5.9% vs 4.8%) with a native Modify mechanism, and provides significantly better extended-hours fill rates (18–19% vs 1–8%). The optimal feature configuration emphasizes OFI, spread dynamics, and MLOFI, with VPIN downgraded to low priority (weak spread correlation at 0.10).

| Key Metric | Value |
|---|---|
| OFI–Return r (5m) | 0.715 |
| Median spread | $0.01 (1 tick) |
| Annualized vol | 33.3% |
| Fill rate | 5.89% |
| Mean VPIN | 0.075 |
| Daily events | 5.9M |
| Median trade size | 25 shares |
