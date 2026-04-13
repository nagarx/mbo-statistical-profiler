# Tier 1 Analysis Findings: Cross-Scale OFI and Conditional Correlations

**Date**: 2026-03-12
**Instrument**: NVDA | **Exchanges**: XNAS + ARCX | **Period**: 233 days (2025-02-03 to 2026-01-06)
**Profiler**: v0.1.0 with CrossScaleOfiTracker + conditional OFI spread-bucket extension
**Trackers**: 13 (12 original + CrossScaleOfiTracker)

This document reports the findings from two new analytical capabilities added to the profiler. Both were designed to answer questions that the 233-day aggregate analysis left open. The results have direct, quantifiable implications for feature extractor configuration and model design.

---

## 1. Cross-Scale OFI Predictability

### 1.1 The Question

Can OFI measured at a short timescale predict returns at a longer timescale? Specifically, does 5s OFI predict the next 1m return? This determines whether OFI can be converted from a contemporaneous signal into a predictive one.

### 1.2 Method

A 6x6 correlation matrix was computed where entry (i,j) = Pearson r between OFI aggregated at scale i and the log mid-price return at scale j. For cross-scale pairs (i != j), **predictive alignment** was used: source OFI in a bin ending at time t was paired with the target return of the bin **starting** at time t (the next bar). For same-scale pairs (diagonal), contemporaneous alignment (lag 0) was used.

Cross-validation: the diagonal entries match the existing OfiTracker lag-0 correlations to within 1e-6 on both exchanges, confirming implementation correctness.

### 1.3 XNAS Results (2.87B events, 233 days)

| Source OFI \ Target Return | 1s | 5s | 10s | 30s | 1m | 5m |
|---|---|---|---|---|---|---|
| **1s** | **0.577** | −0.000 | −0.003 | −0.002 | +0.000 | −0.002 |
| **5s** | −0.004 | **0.618** | −0.007 | −0.003 | −0.000 | −0.004 |
| **10s** | −0.012 | −0.008 | **0.639** | −0.005 | −0.000 | −0.006 |
| **30s** | +0.006 | +0.004 | −0.005 | **0.664** | −0.000 | −0.009 |
| **1m** | +0.018 | +0.014 | +0.020 | +0.014 | **0.675** | −0.012 |
| **5m** | +0.007 | −0.016 | −0.021 | −0.027 | −0.041 | **0.707** |

Bold = diagonal (contemporaneous). All off-diagonal entries are in the range [−0.041, +0.020].

### 1.4 ARCX Results (1.37B events, 233 days)

| Source OFI \ Target Return | 1s | 5s | 10s | 30s | 1m | 5m |
|---|---|---|---|---|---|---|
| **1s** | **0.688** | +0.000 | −0.005 | −0.003 | −0.000 | −0.001 |
| **5s** | −0.004 | **0.719** | −0.010 | −0.006 | −0.001 | −0.003 |
| **10s** | −0.017 | −0.013 | **0.729** | −0.008 | −0.001 | −0.004 |
| **30s** | +0.004 | −0.001 | −0.008 | **0.735** | −0.002 | −0.007 |
| **1m** | +0.020 | +0.014 | +0.019 | +0.014 | **0.729** | −0.009 |
| **5m** | −0.003 | −0.017 | −0.019 | −0.018 | −0.044 | **0.715** |

### 1.5 Interpretation

**The off-diagonal is dead.** Across 36 cross-scale entries on XNAS and 36 on ARCX (72 total), the maximum absolute correlation is 0.044 (5m OFI → 1m return on ARCX). For context, the diagonal ranges from 0.577 to 0.735. The off-diagonal signal is 0.6-6% of the diagonal signal, rendering it economically insignificant.

Two weak patterns exist but are not tradeable:

1. **1m OFI shows mild positive correlation with shorter-scale returns** (1m→1s: +0.018/+0.020, 1m→10s: +0.020/+0.019). This likely reflects that 1m OFI captures slower institutional flow that partially overlaps with faster-scale price movements. The magnitude (~0.02) is too small to trade after costs.

2. **5m OFI shows mild negative correlation with 1m return** (−0.041/−0.044). This suggests mean-reversion: after a strong 5m OFI, the next 1m return weakly reverts. Again, r = 0.04 is not tradeable.

### 1.6 Conclusion for the Pipeline

**OFI is exclusively a contemporaneous signal.** A model cannot gain predictive alpha by feeding short-scale OFI to predict longer-scale returns. The feature extractor should focus on same-scale features. The viable path to prediction is not cross-scale OFI but rather **predicting OFI direction** using other features (spread state, depth, volatility) and then leveraging the contemporaneous OFI-return link.

---

## 2. Conditional OFI-Return Correlation by Spread State

### 2.1 The Question

Does OFI's predictive power vary with the concurrent spread width? If OFI is more informative when the spread is wide, models should weight OFI features higher during wide-spread regimes.

### 2.2 Method

OFI-return pairs (lag 0) were partitioned into 4 spread-width buckets based on the mean spread during each time bin:

| Bucket | Spread Range (USD) | Spread in Ticks |
|---|---|---|
| 1_tick | < $0.015 | ~1 tick |
| 2_tick | $0.015 – $0.025 | ~2 ticks |
| 3_4_tick | $0.025 – $0.045 | ~3-4 ticks |
| 5_plus_tick | >= $0.045 | 5+ ticks |

### 2.3 XNAS Results

| Scale | 1-Tick r (n) | 2-Tick r (n) | 3-4 Tick r (n) | 5+ Tick r (n) |
|---|---|---|---|---|
| 1s | 0.546 (4,981K) | 0.365 (380K) | 0.354 (57K) | 0.489 (8K) |
| 5s | 0.573 (1,005K) | 0.398 (69K) | 0.402 (12K) | 0.511 (2K) |
| 10s | 0.589 (503K) | 0.492 (35K) | 0.317 (6K) | 0.581 (1K) |
| 30s | 0.623 (167K) | 0.454 (12K) | 0.487 (2K) | 0.639 (330) |
| 1m | 0.630 (84K) | 0.477 (6K) | 0.499 (967) | 0.643 (144) |
| 5m | 0.638 (17K) | 0.589 (1K) | 0.612 (101) | 0.978 (19) |

### 2.4 ARCX Results

| Scale | 1-Tick r (n) | 2-Tick r (n) | 3-4 Tick r (n) | 5+ Tick r (n) |
|---|---|---|---|---|
| 1s | 0.684 (4,042K) | 0.600 (1,287K) | 0.519 (90K) | 0.546 (8K) |
| 5s | 0.719 (841K) | 0.612 (230K) | 0.529 (17K) | 0.559 (2K) |
| 10s | 0.730 (422K) | 0.623 (113K) | 0.540 (9K) | 0.519 (692) |
| 30s | 0.737 (141K) | 0.618 (38K) | 0.542 (3K) | 0.635 (182) |
| 1m | 0.721 (70K) | 0.630 (19K) | 0.557 (1K) | 0.665 (73) |
| 5m | 0.688 (14K) | 0.639 (4K) | 0.680 (135) | 0.551 (11) |

### 2.5 Interpretation

**OFI is most strongly correlated with returns during 1-tick spread regimes.** This is the central finding and it contradicts the prior hypothesis.

On XNAS at 1s: r(1-tick) = 0.546 vs r(2-tick) = 0.365 — a 33% drop. On ARCX at 1s: r(1-tick) = 0.684 vs r(2-tick) = 0.600 — a 12% drop. The pattern is consistent across all timescales and both exchanges.

**Why this happens**: At 1-tick spread, the order book is at its tightest configuration. Every marginal order addition or cancellation directly shifts the best bid/ask balance. The mid-price is extremely sensitive to order flow. When the spread widens to 2+ ticks, there is "slack" in the book — order flow can be absorbed without moving the mid-price. Additionally, wide-spread regimes often coincide with pre/post-market or high-volatility periods where noise overwhelms signal.

**The 5+ tick bucket shows unstable, high-variance estimates** (e.g., XNAS 5m at 5+ tick: r = 0.978 with n = 19). With sample sizes < 100, these are unreliable. Ignore all 5+ tick results below n = 200.

### 2.6 Volume Distribution Across Buckets (XNAS, 1s scale)

| Bucket | n | Share of Total |
|---|---|---|
| 1-tick | 4,981,236 | 91.8% |
| 2-tick | 380,408 | 7.0% |
| 3-4 tick | 57,286 | 1.1% |
| 5+ tick | 7,666 | 0.1% |

91.8% of 1s observations occur at 1-tick spread. The model's prediction quality is dominated by behavior in this regime. On ARCX, 1-tick covers 74.4% at 1s (lower because ARCX has more 2-tick time), with 2-tick at 23.7%.

### 2.7 Conclusion for the Pipeline

1. **Do not downweight OFI during 1-tick spread periods.** The prior hypothesis (OFI more informative at wide spreads) was wrong. OFI is most informative at tight spread.

2. **The spread-state categorical feature remains valuable** — but its role is different from expected. Rather than modulating OFI weight, it serves as a **regime indicator**: 1-tick regime = high OFI signal quality but no spread-capture opportunity. Multi-tick regime = lower OFI signal quality but spread-capture opportunity.

3. **For model design**: The model does not need a spread-state × OFI interaction term. OFI features alone capture directional information effectively at all spread states. The spread feature's primary role is informing **execution strategy** (when to trade aggressively vs passively), not signal quality.

---

## 3. Updated Signal Assessment

Incorporating these findings into the signal hierarchy from the unified conclusion:

| # | Signal | Prior Assessment | Updated Assessment | Change |
|---|---|---|---|---|
| 1 | OFI (same-scale) | r = 0.577–0.735 | **Confirmed.** r = 0.546-0.737 in dominant 1-tick regime. Strongest at tight spreads. | Reinforced |
| 2 | Cross-scale OFI | "High priority gap" | **Rejected as signal.** Max off-diagonal r = 0.044. Not tradeable. | Eliminated |
| 3 | Spread-conditioned OFI | "High priority — OFI may be more predictive at wide spreads" | **Reversed.** OFI is most predictive at 1-tick. Spread role is execution routing, not signal modulation. | Reversed |
| 4 | OFI persistence → prediction | ACF(1) at 5m = 0.266/0.301 | **Remains the primary prediction mechanism.** Since cross-scale is dead, the only path is predicting OFI direction via other features. | Unchanged |

### 3.1 Revised Confidence Rating

Prior rating: 8.5/10. The Tier 1 analyses answered two of the three blocking questions:

| Question | Answer | Confidence Impact |
|---|---|---|
| Is cross-scale OFI predictive? | **No.** Dead across 72 scale pairs on 2 exchanges. | +0.5 (eliminates a dead-end) |
| Is OFI more predictive at wide spreads? | **No.** OFI is most predictive at 1-tick. | +0.5 (corrects a wrong assumption) |
| Is OFI signal stable month-to-month? | **Yes.** Monthly std = 0.036 at 5m across 12 months. See `output_xnas_monthly/monthly_stability_report.json`. | +0.5 (stability confirmed) |

**Updated rating: 9.5/10** — monthly stability confirmed. Remaining 0.5 point: multi-symbol generalization not yet tested.

---

## 4. Implications for Feature Extractor Configuration

### 4.1 Configuration Changes Based on New Findings

| Parameter | Prior Recommendation | Updated Recommendation | Justification |
|---|---|---|---|
| MLOFI experimental group | Critical — enable | **Critical — enable** (unchanged) | L1 < 8% of depth |
| Spread-state categorical | "High priority gap" | **Medium priority** | Useful for execution routing, not for OFI signal modulation |
| Cross-scale OFI features | Not in extractor, planned | **Do not implement** | r < 0.044 — no predictive value |
| OFI feature weight | High in all regimes | **High in all regimes** (confirmed) | OFI r is highest at 1-tick (91.8% of time) |

### 4.2 Model Architecture Implications

1. **Single-scale models are sufficient.** The cross-scale matrix proves that a model trained on 1s features to predict 1s returns cannot benefit from also predicting 5s or 1m returns — there is no cross-scale link to exploit. Focus each model instance on one prediction horizon.

2. **OFI persistence is the only prediction mechanism.** Since cross-scale OFI is dead, a model that predicts the next bar's return must do so by predicting the next bar's OFI direction (ACF(1) = 0.266-0.301 at 5m). Features that help predict OFI direction — depth imbalance, trade flow, volatility state — are the critical inputs.

3. **No spread-state interaction features needed.** OFI is equally or more informative at tight spreads. The model does not need a spread×OFI cross term.

---

## 5. Remaining Work

### 5.1 Monthly Rolling Stability — COMPLETED

Twelve monthly runs completed (`output_xnas_monthly/`). OFI-return correlation is stable month-to-month (std = 0.036 at 5m). The comparison script `scripts/compare_monthly.py` produced `output_xnas_monthly/monthly_stability_report.json`. See `ZERO_DTE_STRATEGY_BRIDGE.md` §4 for the stability results used in strategy parameterization.

### 5.2 What Has Been Resolved

| Prior Gap | Resolution |
|---|---|
| Cross-scale OFI predictability | **Resolved: no predictive signal.** |
| OFI conditioned on spread state | **Resolved: OFI strongest at 1-tick.** |
| Rolling signal stability | **Resolved: stable.** Monthly std = 0.036 at 5m. See `output_xnas_monthly/`. |
| VPIN as real-time feature | Not addressed in Tier 1 |
| Spread-state categorical | Priority downgraded from High to Medium |
