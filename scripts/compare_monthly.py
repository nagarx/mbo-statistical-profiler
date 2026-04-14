#!/usr/bin/env python3
"""
Monthly Signal Stability Comparison

Reads profiler JSON output from each monthly run and produces a summary
table showing how key metrics vary month-to-month. The primary question:
is the OFI-return correlation stable or does it fluctuate?

Usage:
    python scripts/compare_monthly.py [--output-dir output_xnas_monthly]

Output:
    - Prints summary table to stdout
    - Writes output_xnas_monthly/monthly_stability_report.json
"""

import argparse
import json
import math
import os
import sys
from pathlib import Path


MONTHS = [
    "2025_02", "2025_03", "2025_04", "2025_05", "2025_06", "2025_07",
    "2025_08", "2025_09", "2025_10", "2025_11", "2025_12", "2026_01",
]

SCALES = ["1s", "5s", "10s", "30s", "1m", "5m"]


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def coerce_finite(v) -> float:
    """Coerce a JSON value to a float, mapping null/missing to NaN.

    Hardens against `json.load` yielding `None` for JSON `null` values — since
    hft-statistics 0.2.0, the profiler emits `null` (not `0.0`) for correlations
    on degenerate variance (e.g., duration_size_correlation when no data) and
    `math.isfinite(None)` raises `TypeError: must be real number, not NoneType`.
    This helper normalises everything to f64 at ingestion so downstream code
    (filtering, format strings, arithmetic) can treat values uniformly.
    """
    if v is None:
        return float("nan")
    try:
        return float(v)
    except (TypeError, ValueError):
        return float("nan")


def find_tracker_file(month_dir: Path, tracker_name: str) -> Path | None:
    """Find a tracker JSON by name, ignoring the numeric prefix."""
    for p in sorted(month_dir.glob(f"*_{tracker_name}.json")):
        return p
    return None


def extract_monthly_metrics(month_dir: Path) -> dict | None:
    """Extract key metrics from a single month's profiler output."""
    tracker_names = ["QualityTracker", "OfiTracker", "SpreadTracker",
                     "VolatilityTracker", "VpinTracker"]
    paths = {name: find_tracker_file(month_dir, name) for name in tracker_names}
    if any(p is None for p in paths.values()):
        return None

    quality_path = paths["QualityTracker"]
    ofi_path = paths["OfiTracker"]
    spread_path = paths["SpreadTracker"]
    vol_path = paths["VolatilityTracker"]
    vpin_path = paths["VpinTracker"]

    q = load_json(quality_path)
    o = load_json(ofi_path)
    s = load_json(spread_path)
    v = load_json(vol_path)
    vp = load_json(vpin_path)

    result = {
        "n_days": q["n_days"],
        "total_events": q["total_events"],
        "mean_events_per_day": q["mean_events_per_day"],
    }

    ofi_return_r = {}
    ofi_return_r2 = {}
    ofi_acf1 = {}
    for sc in SCALES:
        ps = o.get("per_scale", {}).get(sc, {})
        rc = ps.get("ofi_return_correlation", {})
        r0 = coerce_finite(rc.get("lag_0"))
        ofi_return_r[sc] = r0
        # r0 * r0 is NaN iff r0 is NaN — math.isfinite guard later handles this.
        ofi_return_r2[sc] = r0 * r0
        acf = ps.get("acf", [])
        ofi_acf1[sc] = coerce_finite(acf[0]) if acf else float("nan")

    result["ofi_return_r"] = ofi_return_r
    result["ofi_return_r2"] = ofi_return_r2
    result["ofi_acf1"] = ofi_acf1

    result["spread_mean_usd"] = coerce_finite(s.get("distribution_usd", {}).get("mean"))
    wc = s.get("width_classification", {})
    result["spread_1tick_pct"] = coerce_finite(wc.get("one_tick_pct"))

    result["annualized_vol_mean"] = coerce_finite(
        v.get("daily_annualized_vol", {}).get("mean")
    )
    result["rv_mean"] = coerce_finite(v.get("daily_rv", {}).get("mean"))

    result["vpin_mean"] = coerce_finite(vp.get("daily_mean_vpin", {}).get("mean"))
    # Critical: vpin_spread_correlation can be JSON null (since 0.2.0) when
    # input variance is degenerate. coerce_finite maps that to NaN.
    result["vpin_spread_corr"] = coerce_finite(vp.get("vpin_spread_correlation"))

    return result


def compute_stability(monthly_data: list[dict]) -> dict:
    """Compute mean and std of each metric across months."""

    def mean_std(values):
        # Belt-and-suspenders: even though coerce_finite normalised ingestion,
        # guard against any None that may slip through future refactors.
        finite = [v for v in values if v is not None and math.isfinite(v)]
        if not finite:
            return float("nan"), float("nan"), 0
        m = sum(finite) / len(finite)
        if len(finite) < 2:
            return m, 0.0, len(finite)
        var = sum((x - m) ** 2 for x in finite) / (len(finite) - 1)
        return m, math.sqrt(var), len(finite)

    stability = {}

    for sc in SCALES:
        vals = [d["ofi_return_r"][sc] for d in monthly_data]
        m, s, n = mean_std(vals)
        stability[f"ofi_return_r_{sc}"] = {"mean": m, "std": s, "n": n,
                                            "values": vals}

    for sc in SCALES:
        vals = [d["ofi_acf1"][sc] for d in monthly_data]
        m, s, n = mean_std(vals)
        stability[f"ofi_acf1_{sc}"] = {"mean": m, "std": s, "n": n}

    for key in ["spread_mean_usd", "spread_1tick_pct", "annualized_vol_mean",
                "rv_mean", "vpin_mean", "vpin_spread_corr"]:
        vals = [d[key] for d in monthly_data]
        m, s, n = mean_std(vals)
        stability[key] = {"mean": m, "std": s, "n": n, "values": vals}

    return stability


def print_report(months_loaded: list[str], monthly_data: list[dict],
                 stability: dict):
    """Print a human-readable stability report."""
    print("=" * 80)
    print("  MONTHLY SIGNAL STABILITY REPORT — XNAS NVDA")
    print("=" * 80)
    print(f"\n  Months loaded: {len(months_loaded)}")
    print(f"  Total days: {sum(d['n_days'] for d in monthly_data)}")
    print()

    print("  OFI-Return r(lag 0) by Month:")
    print(f"  {'Month':<10}", end="")
    for sc in SCALES:
        print(f"  {sc:>6}", end="")
    print(f"  {'Days':>6}")
    print("  " + "-" * (10 + 7 * len(SCALES) + 7))

    for month, data in zip(months_loaded, monthly_data):
        print(f"  {month:<10}", end="")
        for sc in SCALES:
            r = data["ofi_return_r"][sc]
            print(f"  {r:>6.3f}", end="")
        print(f"  {data['n_days']:>6}")

    print("  " + "-" * (10 + 7 * len(SCALES) + 7))

    print(f"  {'MEAN':<10}", end="")
    for sc in SCALES:
        s = stability[f"ofi_return_r_{sc}"]
        print(f"  {s['mean']:>6.3f}", end="")
    print()

    print(f"  {'STD':<10}", end="")
    for sc in SCALES:
        s = stability[f"ofi_return_r_{sc}"]
        print(f"  {s['std']:>6.3f}", end="")
    print()

    print(f"\n  Signal Stability Verdict (OFI-return r std):")
    for sc in SCALES:
        s = stability[f"ofi_return_r_{sc}"]
        verdict = "STABLE" if s["std"] < 0.05 else ("MARGINAL" if s["std"] < 0.10 else "UNSTABLE")
        print(f"    {sc}: std = {s['std']:.4f} → {verdict}")

    print(f"\n  Other Monthly Metrics (mean ± std):")
    for key in ["spread_mean_usd", "spread_1tick_pct", "annualized_vol_mean",
                "vpin_mean"]:
        s = stability[key]
        print(f"    {key}: {s['mean']:.4f} ± {s['std']:.4f}")

    print()


def main():
    parser = argparse.ArgumentParser(description="Monthly signal stability comparison")
    parser.add_argument("--output-dir", default="output_xnas_monthly",
                        help="Base directory containing monthly subdirectories")
    args = parser.parse_args()

    base = Path(args.output_dir)
    if not base.exists():
        print(f"Error: {base} does not exist. Run monthly profiler first.", file=sys.stderr)
        sys.exit(1)

    months_loaded = []
    monthly_data = []

    for month in MONTHS:
        month_dir = base / month
        metrics = extract_monthly_metrics(month_dir)
        if metrics is not None:
            months_loaded.append(month)
            monthly_data.append(metrics)

    if not monthly_data:
        print("Error: No monthly data found. Run the profiler for each month first.",
              file=sys.stderr)
        sys.exit(1)

    stability = compute_stability(monthly_data)

    print_report(months_loaded, monthly_data, stability)

    report = {
        "months_loaded": months_loaded,
        "monthly_data": monthly_data,
        "stability": {k: {kk: vv for kk, vv in v.items() if kk != "values"}
                      for k, v in stability.items()},
        "stability_with_values": stability,
    }

    report_path = base / "monthly_stability_report.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2, default=str)
    print(f"  Report written to: {report_path}")


if __name__ == "__main__":
    main()
