# NVDA Cross-Exchange Comparison: XNAS vs ARCX

**Instrument**: NVDA (NVIDIA Corporation)
**Period**: 2025-02-03 to 2026-01-06 (233 trading days)
**Source**: mbo-statistical-profiler v0.1.0

This document provides side-by-side comparison of NVDA's microstructure on XNAS (Nasdaq, primary listing, ITCH protocol) versus ARCX (NYSE Arca, secondary venue, PILLAR protocol). All numbers are from identical profiler runs over the same 233-day period.

---

## 1. Scale Comparison

| Metric | XNAS | ARCX | Ratio (XNAS/ARCX) |
|---|---|---|---|
| Total events | 2,867,766,144 | 1,371,860,852 | 2.09× |
| Mean events/day | 12,308,009 | 5,888,247 | 2.09× |
| Total trades | 193,879,475 | 109,084,515 | 1.78× |
| Total volume (shares) | 18,384,430,535 | 6,581,628,311 | 2.79× |
| Mean trades/day | 832,101 | 468,174 | 1.78× |
| Mean trade size (shares) | 94.82 | 60.34 | 1.57× |
| Median trade size (shares) | 37 | 25 | 1.48× |
| Total adds | 1,329,541,122 | 612,917,896 | 2.17× |
| Total cancels | 1,344,345,314 | 615,298,763 | 2.18× |
| Total fills | 63,351,004 | 35,676,240 | 1.78× |

XNAS carries approximately 2× the message traffic and 2.8× the volume of ARCX. The volume ratio is higher than the trade ratio because XNAS trade sizes are 57% larger on average.

### Protocol Differences

| Feature | XNAS (ITCH) | ARCX (PILLAR) |
|---|---|---|
| Modify messages | No (Cancel + Re-Add) | Yes (2.52% of events) |
| Trade share | 6.76% | 7.95% |
| Cancel-to-add ratio | 1.011 | 1.004 |

---

## 2. OFI Correlation Comparison

### 2.1 OFI–Return Correlation (Lag 0)

| Scale | XNAS r | ARCX r | XNAS r² | ARCX r² | Δr² |
|---|---|---|---|---|---|
| 1s | 0.577 | 0.688 | 0.333 | 0.474 | +0.141 |
| 5s | 0.618 | 0.719 | 0.381 | 0.517 | +0.135 |
| 10s | 0.639 | 0.729 | 0.408 | 0.532 | +0.124 |
| 30s | 0.664 | 0.735 | 0.441 | 0.541 | +0.099 |
| 1m | 0.675 | 0.729 | 0.455 | 0.531 | +0.076 |
| 5m | 0.707 | 0.715 | 0.500 | 0.512 | +0.011 |

**Key finding**: ARCX OFI is 14 percentage points more correlated with returns at 1s (r² = 0.474 vs 0.333). The gap narrows with scale and nearly vanishes at 5m (+0.011). This means ARCX order flow has higher per-unit price impact because the book is thinner — each order moves the mid-price more.

**Implication for feature extractor**: When building models at 1s–10s timescales, ARCX OFI features should receive higher weight. At 5m, both exchanges provide comparable signal.

### 2.2 OFI–Spread Correlation (Lag 0)

| Scale | XNAS r | ARCX r |
|---|---|---|
| 1s | 0.005 | 0.008 |
| 5s | 0.007 | 0.011 |
| 10s | 0.013 | 0.012 |
| 30s | 0.008 | 0.011 |
| 1m | 0.018 | 0.016 |
| 5m | 0.008 | −0.011 |

Both near-zero. ARCX turns negative at 5m (−0.011), suggesting sustained directional flow at longer horizons coincides with tighter spreads on ARCX (liquidity providers stepping in to capture the flow).

### 2.3 OFI Persistence (ACF Lag 1)

| Scale | XNAS | ARCX |
|---|---|---|
| 1s | 0.042 | −0.003 |
| 5s | 0.124 | 0.149 |
| 1m | 0.197 | 0.155 |
| 5m | 0.266 | 0.301 |

At 1s, ARCX OFI has near-zero persistence (−0.003) while XNAS has mild positive persistence (0.042). At 5m, ARCX OFI is stickier (0.301 vs 0.266). This means:
- Short-term (1s): ARCX flow is more noise-like → harder to trend-follow
- Long-term (5m): ARCX flow trends persist longer → better for momentum strategies

### 2.4 Component Fractions

| Component | XNAS | ARCX |
|---|---|---|
| Add | 0.513 | 0.509 |
| Cancel | 0.403 | 0.428 |
| Trade | 0.084 | 0.063 |

ARCX has higher cancel contribution (42.8% vs 40.3%) and lower trade contribution (6.3% vs 8.4%) to OFI. ARCX's cancel-driven OFI partially explains its higher return correlation — cancellations reveal market maker repositioning, which is more predictive of short-term direction than trade execution.

### 2.5 Regime OFI Intensity

| Regime | XNAS Mean | ARCX Mean | XNAS Std | ARCX Std |
|---|---|---|---|---|
| Pre-Market | 30.74 | 22.57 | 255.23 | 152.60 |
| Open Auction | 14.06 | 21.86 | 144.55 | 113.19 |
| Morning | 21.18 | 27.53 | 100.92 | 74.93 |
| Midday | 24.67 | 33.83 | 102.72 | 77.40 |
| Afternoon | 32.02 | 39.22 | 114.68 | 92.70 |
| Close Auction | 37.43 | 45.39 | 241.70 | 119.96 |
| Post-Market | 8.90 | 23.47 | 182.23 | 286.66 |

ARCX has higher mean OFI in all RTH regimes (Morning through Close Auction) and lower OFI variability. This suggests more concentrated, directional flow on ARCX during regular hours.

---

## 3. Spread Comparison

### 3.1 Spread Distribution

| Metric | XNAS | ARCX | Unit |
|---|---|---|---|
| Mean | 0.0159 | 0.0175 | USD |
| Median (p50) | 0.0100 | 0.0100 | USD |
| p95 | 0.0300 | 0.0300 | USD |
| p99 | 0.0900 | 0.0800 | USD |
| Mean (bps) | 1.105 | 1.220 | bps |
| Median (bps) | 0.733 | 0.917 | bps |

ARCX mean spread is 10% wider in USD and 10.4% wider in bps.

### 3.2 Width Classification

| Width | XNAS (%) | ARCX (%) |
|---|---|---|
| 1 tick | 70.27 | 54.54 |
| 2 ticks | 22.20 | 35.40 |
| 3–4 ticks | 4.79 | 7.40 |
| 5+ ticks | 2.74 | 2.66 |

ARCX spends 15.7 fewer percentage points at the minimum tick. The "extra" time goes primarily to 2-tick spread (35.4% vs 22.2%). This is the most actionable structural difference — ARCX provides more spread-capture opportunity.

### 3.3 Spread Persistence

| Lag | XNAS ACF | ARCX ACF |
|---|---|---|
| 1 | 0.889 | 0.933 |
| 2 | 0.812 | 0.870 |
| 3 | 0.756 | 0.825 |
| 4 | 0.710 | 0.785 |
| 5 | 0.675 | 0.753 |

ARCX spreads are more persistent (ACF(1) = 0.933 vs 0.889). When ARCX widens to 2+ ticks, it stays there longer — beneficial for market-making strategies that require time to capture the edge.

### 3.4 Regime-Conditional Spread (USD)

| Regime | XNAS | ARCX |
|---|---|---|
| Pre-Market | $0.0491 | $0.0438 |
| Open Auction | $0.0309 | $0.0290 |
| Morning | $0.0140 | $0.0164 |
| Midday | $0.0126 | $0.0144 |
| Afternoon | $0.0123 | $0.0139 |
| Close Auction | $0.0126 | $0.0161 |
| Post-Market | $0.0837 | $0.0716 |

During extended hours, ARCX has tighter spreads (Pre-Market: $0.044 vs $0.049; Post-Market: $0.072 vs $0.084). During RTH, ARCX is wider ($0.014–$0.016 vs $0.012–$0.014). This reflects ARCX's stronger extended-hours participation.

### 3.5 Trade-Conditional Spread

| Metric | XNAS | ARCX | Unit |
|---|---|---|---|
| Mean | 0.0164 | 0.0210 | USD |
| Median (p50) | 0.0100 | 0.0200 | USD |

At trade time, ARCX median spread is 2 ticks vs 1 tick on XNAS. This means the average ARCX trade crosses a wider spread — higher execution cost but more edge for passive strategies.

---

## 4. Volatility Comparison

Volatility should be identical across exchanges (same stock, same price process). Any differences reflect microstructure artifacts.

### 4.1 Daily RV and Annualized Vol

| Metric | XNAS | ARCX | Diff (%) |
|---|---|---|---|
| Mean daily RV | 5.510e-4 | 5.493e-4 | −0.3% |
| Annualized vol mean | 33.35% | 33.30% | −0.2% |
| Annualized vol std | 16.63% | 16.60% | −0.2% |
| Min annualized vol | 15.05% | 15.14% | +0.6% |
| Max annualized vol | 148.80% | 148.41% | −0.3% |

Differences are <1% — confirming the same underlying price process.

### 4.2 RV Persistence

| Lag | XNAS ACF | ARCX ACF |
|---|---|---|
| 1 | 0.663 | 0.663 |
| 2 | 0.621 | 0.620 |
| 3 | 0.453 | 0.452 |
| 4 | 0.316 | 0.316 |
| 5 | 0.244 | 0.243 |

Identical within rounding. Volatility dynamics are purely a stock-level property.

### 4.3 Signature Plot

| Scale (s) | XNAS RV | ARCX RV | Diff (%) |
|---|---|---|---|
| 0.10 | 5.703e-4 | 5.746e-4 | +0.8% |
| 1.06 | 5.497e-4 | 5.479e-4 | −0.3% |
| 2.90 | 5.457e-4 | 5.440e-4 | −0.3% |
| 60.00 | 5.233e-4 | 5.231e-4 | −0.0% |
| Fast/Slow ratio | 1.090 | 1.098 | +0.8% |

ARCX has slightly higher noise contamination at the fastest scale (ratio 1.098 vs 1.090). At 60s they converge completely.

### 4.4 Spread–Vol Correlation

| Metric | XNAS | ARCX |
|---|---|---|
| Spread–Vol r | 0.462 | 0.482 |

ARCX market makers are marginally more reactive to volatility in their quoting.

---

## 5. VPIN Comparison

### 5.1 Distribution

| Metric | XNAS | ARCX | Ratio |
|---|---|---|---|
| Mean | 0.298 | 0.079 | 3.77× |
| Std | 0.376 | 0.077 | 4.88× |
| Min | 0.008 | 0.008 | — |
| Max | 1.000 | 1.000 | — |
| p25 | 0.056 | 0.037 | 1.50× |
| p50 | 0.098 | 0.056 | 1.75× |
| p75 | 0.303 | 0.090 | 3.36× |
| p90 | 1.000 | 0.147 | 6.80× |

XNAS VPIN is 3.8× higher than ARCX on average and 6.8× higher at p90. This is the single largest structural difference between the exchanges.

**Interpretation**: Informed traders route their orders to XNAS (the primary listing exchange) where liquidity is deepest and execution is fastest. ARCX receives more balanced, less information-laden flow.

### 5.2 Daily Mean VPIN

| Metric | XNAS | ARCX | Ratio |
|---|---|---|---|
| Mean | 0.299 | 0.075 | 3.99× |
| Std | 0.062 | 0.020 | 3.10× |
| Min | 0.196 | 0.033 | 5.88× |
| Max | 0.619 | 0.148 | 4.18× |

Even on ARCX's "worst" day (VPIN=0.148), it is below XNAS's average day (0.299).

### 5.3 Intraday Pattern

| Period | XNAS | ARCX |
|---|---|---|
| Open (min 0) | 0.751 | 0.104 |
| Midday (min 195) | 0.104 | 0.084 |
| Close (min 389) | 0.142 | 0.055 |

XNAS shows a dramatic open spike (0.75) that ARCX does not (0.10). At midday both converge. XNAS rises into close while ARCX continues to decline.

### 5.4 VPIN–Spread Correlation

| Metric | XNAS | ARCX |
|---|---|---|
| VPIN–Spread r | 0.440 | 0.099 |

Strong link on XNAS, near-zero on ARCX. XNAS market makers actively widen spreads in response to detected informed flow; ARCX market makers do not (or the flow is too benign to trigger widening).

### 5.5 Volume Bar Count

| Metric | XNAS | ARCX | Ratio |
|---|---|---|---|
| Total bars | 3,676,994 | 1,316,171 | 2.79× |
| Mean bars/day | 15,781 | 5,649 | 2.79× |

Proportional to volume ratio.

---

## 6. Lifecycle Comparison

### 6.1 Aggregate Metrics

| Metric | XNAS | ARCX |
|---|---|---|
| Fill rate | 4.776% | 5.886% |
| Cancel-to-add ratio | 1.011 | 1.004 |
| Partial fill fraction | 15.79% | 15.38% |
| Duration–size correlation | −0.205 | −0.141 |
| Median lifetime | 0.018s | 0.031s |
| Mean lifetime | 641.9s | 484.5s |

ARCX has 23% higher fill rate and 72% longer median order lifetime. ARCX's native Modify support (5.6% of adds lead to Modify) reduces spurious cancel/re-add activity, bringing the cancel-to-add ratio closer to 1.0.

### 6.2 Regime Fill Rates

| Regime | XNAS (%) | ARCX (%) | ARCX/XNAS |
|---|---|---|---|
| Pre-Market | 7.80 | 18.13 | 2.32× |
| Open Auction | 6.97 | 11.83 | 1.70× |
| Morning | 5.18 | 5.93 | 1.14× |
| Midday | 4.42 | 4.74 | 1.07× |
| Afternoon | 5.43 | 4.93 | 0.91× |
| Close Auction | 10.62 | 8.98 | 0.85× |
| Post-Market | 0.94 | 19.44 | 20.7× |

The most dramatic difference is Post-Market: ARCX fills 19.4% of orders vs XNAS 0.9% — a 20.7× ratio. Pre-Market is 2.3×. During RTH, differences are modest (0.85–1.14×).

### 6.3 Regime Lifetimes

| Regime | XNAS (s) | ARCX (s) |
|---|---|---|
| Pre-Market | 1,326 | 7,642 |
| Open Auction | 11,327 | 1,956 |
| Morning | 130 | 225 |
| Midday | 69 | 81 |
| Afternoon | 90 | 13 |
| Close Auction | 74 | 6 |
| Post-Market | 192 | 948 |

ARCX Pre-Market lifetimes (7,642s) are 5.8× longer than XNAS (1,326s) — orders queue for hours. ARCX Close Auction is extremely short (6s) vs XNAS (74s). ARCX Open Auction is much shorter (1,956s vs 11,327s) — ARCX opens faster.

### 6.4 Transition Matrices

**XNAS (no Modify state)**:

| From → To | Cancel | Trade |
|---|---|---|
| Add | 94.33% | 5.67% |
| Trade | 100.00% | — |

**ARCX (with Modify state)**:

| From → To | Modify | Cancel | Trade |
|---|---|---|---|
| Add | 5.60% | 87.98% | 6.42% |
| Modify | 1.04% | 89.51% | 9.46% |
| Trade | — | 100.00% | — |

After modification, ARCX orders have 9.46% trade probability (vs 6.42% unmodified) — modifications increase fill probability by 47%. This suggests modifications are strategic repositioning to improve queue priority.

---

## 7. Depth Comparison

### 7.1 Total Depth

| Metric | XNAS | ARCX | Ratio |
|---|---|---|---|
| Mean (shares) | 18,189 | 7,953 | 2.29× |
| Std (shares) | 44,001 | 13,261 | 3.32× |
| CV | 2.419 | 1.667 | 1.45× |

XNAS has 2.3× more depth. Its higher CV (2.42 vs 1.67) reflects more extreme depth fluctuations (institutional iceberg orders).

### 7.2 Bid Depth Profile (Mean Shares)

| Level | XNAS | ARCX | Ratio |
|---|---|---|---|
| L1 | 544 | 297 | 1.83× |
| L2 | 659 | 356 | 1.85× |
| L3 | 727 | 371 | 1.96× |
| L5 | 846 | 403 | 2.10× |
| L7 | 937 | 403 | 2.33× |
| L10 | 957 | 375 | 2.55× |

XNAS depth advantage grows with distance from mid: 1.83× at L1, 2.55× at L10. XNAS has monotonically increasing depth; ARCX peaks at L6–L8 then decreases.

### 7.3 Ask Depth Profile (Mean Shares)

| Level | XNAS | ARCX | Ratio |
|---|---|---|---|
| L1 | 610 | 307 | 1.99× |
| L2 | 766 | 391 | 1.96× |
| L5 | 957 | 437 | 2.19× |
| L10 | 1,187 | 425 | 2.79× |

Same pattern — XNAS ask advantage accelerates with depth.

### 7.4 L1 Concentration

| Metric | XNAS | ARCX |
|---|---|---|
| L1 concentration | 5.89% | 7.64% |

ARCX has higher L1 concentration (7.6% vs 5.9%) — a larger fraction of its thinner book sits at the best quote. Both are low enough that multi-level features (MLOFI) are essential for both exchanges.

### 7.5 Depth Imbalance

| Metric | XNAS | ARCX |
|---|---|---|
| Mean | −0.011 | −0.005 |
| Std | 0.350 | 0.326 |
| p1 | −0.846 | −0.793 |
| p99 | +0.816 | +0.799 |

XNAS has wider depth imbalance extremes (std=0.35 vs 0.33). Both are centered near zero.

---

## 8. Trade Comparison

### 8.1 Trade Size

| Metric | XNAS | ARCX | Unit |
|---|---|---|---|
| Mean | 94.82 | 60.34 | shares |
| Median (p50) | 37 | 25 | shares |
| p95 | 200 | 169 | shares |
| p99 | 600 | 500 | shares |

ARCX trades are 36% smaller on average. This is consistent with ARCX attracting more retail/small-order flow.

### 8.2 Price Level Classification

| Level | XNAS (%) | ARCX (%) |
|---|---|---|
| At bid | 40.47 | 38.89 |
| At ask | 37.01 | 32.77 |
| Inside spread | 22.51 | 28.35 |
| Outside | 0.002 | 0.0004 |

ARCX has 26% more inside-spread executions (28.4% vs 22.5%). This reflects more hidden/midpoint order activity on ARCX — beneficial for execution quality.

### 8.3 Inter-Trade Time

| Metric | XNAS | ARCX | Ratio |
|---|---|---|---|
| Mean | 0.247s | 0.390s | 1.58× |
| Median (p50) | 0.0017s | 0.0091s | 5.24× |
| p25 | 0.000048s | 0.000153s | 3.19× |

ARCX trades arrive 5.2× slower at the median. This longer inter-trade time gives slower strategies more room to operate.

### 8.4 Clustering

| Metric | XNAS | ARCX |
|---|---|---|
| Cluster fraction | 27.15% | 29.98% |
| Mean cluster size | 34.6 | 17.0 |
| Max cluster size | 265,422 | 92,997 |

ARCX has more frequent but smaller clusters. XNAS has rarer but much larger clusters (265K vs 93K max).

### 8.5 Large Trade Impact

| Metric | XNAS (≥200 sh) | ARCX (≥150 sh) | Unit |
|---|---|---|---|
| Mean impact | 0.943 | 1.217 | bps |
| p50 impact | 0.690 | 0.733 | bps |
| p95 impact | 2.766 | 4.076 | bps |
| p99 impact | 6.816 | 9.650 | bps |

Large-trade impact is 29% higher on ARCX at the mean and 42% higher at p99, despite a lower threshold. Thinner books amplify price impact.

---

## 9. Cross-Exchange Signal Strategy

Based on the structural differences identified above, the following cross-exchange strategies are viable.

### Strategy 1: XNAS VPIN → ARCX Spread Adjustment

- **Signal**: Monitor XNAS VPIN in real-time. When VPIN > 0.5 (informed trading detected on primary exchange), ARCX prices will adjust with a lag.
- **Mechanism**: XNAS is where informed traders execute; ARCX market makers update quotes after XNAS price movement is confirmed.
- **Edge**: The VPIN divergence (4× higher on XNAS) creates a detectable information asymmetry.
- **Execution**: Widen ARCX market-making quotes or withdraw liquidity when XNAS VPIN spikes; tighten when XNAS VPIN is low.
- **Expected alpha**: VPIN–spread r on XNAS (0.44) vs ARCX (0.10) creates a 0.34 correlation gap exploitable for timing.

### Strategy 2: OFI Convergence Across Exchanges

- **Signal**: When XNAS and ARCX OFI diverge (e.g., XNAS bullish, ARCX neutral), the ARCX price tends to follow XNAS.
- **Mechanism**: ARCX OFI–return r at 1s is 0.688 vs XNAS 0.577. ARCX book is thinner so flow has more impact — but XNAS leads the price discovery.
- **Execution**: Compute OFI on both exchanges simultaneously; trade on ARCX in the direction of XNAS OFI when ARCX OFI lags behind.
- **Expected alpha**: The 5m OFI ACF(1) on ARCX (0.301) is higher than XNAS (0.266), meaning ARCX flow trends persist longer — once ARCX catches up, the move continues.

### Strategy 3: Extended-Hours Venue Routing

- **Signal**: During pre-market and post-market, route passive limit orders to ARCX for dramatically higher fill rates.
- **Mechanism**: ARCX pre-market fill rate (18.1%) is 2.3× higher than XNAS (7.8%); post-market (19.4%) is 20.7× higher.
- **Execution**: Maintain dual-venue infrastructure; shift order routing to ARCX during 04:00–09:30 and 16:00–20:00 ET.
- **Risk**: Extended-hours spreads on ARCX are still wide ($0.044 pre-market, $0.072 post-market) but tighter than XNAS ($0.049, $0.084).

### Strategy 4: Spread Regime Arbitrage

- **Signal**: ARCX spends 45.5% of time at 2+ tick spreads (vs 29.7% on XNAS). When ARCX spread widens and XNAS remains at 1-tick, the ARCX spread will compress.
- **Mechanism**: ARCX spread ACF(1) = 0.933 (very persistent), so wide-spread regimes are predictable. Trade the reversion to XNAS-implied fair spread.
- **Execution**: Post passive bids and offers on ARCX at 2-tick when XNAS shows 1-tick; capture the spread compression.
- **Expected alpha**: The 15.7pp difference in 1-tick time (70.3% vs 54.5%) represents persistent structural edge.

### Strategy 5: ARCX Modify Signal

- **Signal**: ARCX's native Modify messages (2.5% of events) reveal order intent. When Modify rate increases, it signals active repositioning by market makers.
- **Mechanism**: After modification, orders have 47% higher fill probability (9.46% vs 6.42%), suggesting strategic queue jumping.
- **Execution**: Track Modify rates per price level; detect when market makers are aggressively repositioning — use as short-term direction signal.
- **Limitation**: This signal is ARCX-specific; XNAS has no Modify messages.

---

## 10. Feature Extractor Implications

### 10.1 Exchange-Specific Feature Weights

| Feature | XNAS Weight | ARCX Weight | Rationale |
|---|---|---|---|
| OFI (L1) | High | Critical | ARCX r² +14pp higher at 1s |
| MLOFI (multi-level) | Critical | Critical | L1 concentration <8% on both |
| Spread state | Medium | Critical | ARCX 2-tick 35% of time vs 22% |
| VPIN | High | Low | ARCX mean 0.08 vs XNAS 0.30 |
| Depth imbalance | High | High | Similar information content |
| Trade clustering | Medium | Medium | Both ~28–30% cluster fraction |
| Modify rate | N/A | Medium | ARCX-only signal |
| Lifecycle features | Low | Medium | ARCX fill rate 23% higher |

### 10.2 Cross-Exchange Features

If the feature extractor supports multi-exchange inputs, these cross-exchange features provide unique alpha:

| Feature | Description | Expected Signal |
|---|---|---|
| VPIN_XNAS − VPIN_ARCX | Information asymmetry indicator | High delta → informed flow detected |
| OFI_XNAS − OFI_ARCX | Cross-exchange flow divergence | Non-zero → pending price adjustment |
| Spread_ARCX / Spread_XNAS | Relative liquidity | Ratio > 1.5 → ARCX withdrawal |
| FillRate_ARCX / FillRate_XNAS | Venue attractiveness | High ratio → ARCX more active |

### 10.3 Unified Labeling Strategy

Since returns are identical across exchanges (same stock), labels should be constructed from XNAS mid-price returns (primary listing, highest liquidity, lowest noise). Features can be sourced from either or both exchanges.

| Parameter | Value | Justification |
|---|---|---|
| Label source | XNAS mid-price | Primary listing, lowest noise |
| Label horizon | 1m, 5m | Sufficient return variance |
| Up threshold (5m) | +25 bps | ~p90 on both exchanges |
| Down threshold (5m) | −25 bps | ~p10 on both exchanges |
| Feature source | Both XNAS and ARCX | Complementary information |

### 10.4 Key Numerical Constants for Configuration

| Constant | XNAS | ARCX | Use |
|---|---|---|---|
| Median spread (USD) | $0.01 | $0.01 | Transaction cost model |
| Mean spread (bps) | 1.11 | 1.22 | Spread normalization |
| VWES (bps) | 1.97 | 1.10 | Execution cost estimate |
| Mean trade size | 95 | 60 | Position sizing |
| Median inter-trade (ms) | 1.7 | 9.1 | Latency budget |
| Mean daily RV | 5.51e-4 | 5.49e-4 | Volatility normalization |
| 5m return std (bps) | 25.2 | 25.2 | Label calibration |
| OFI–return r (1s) | 0.577 | 0.688 | Feature weight |
| OFI–return r (5m) | 0.707 | 0.715 | Feature weight |
| Abs-return ACF(1) at 5s | 0.254 | 0.251 | Vol model calibration |
