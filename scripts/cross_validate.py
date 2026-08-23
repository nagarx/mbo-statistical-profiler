#!/usr/bin/env python3
"""
Cross-validate Rust profiler output against Python MBO-LOB-analyzer golden values.

Runs the profiler on a single day, then compares every tracker's output against
the per-day golden values from the Python analyzer's 234-day run.

Usage:
    python scripts/cross_validate.py

Requirements:
    - Release binary built: cargo build --release --bin profile_mbo
    - Hot store data available at ../data/hot_store/
    - Python analyzer output at ../MBO-LOB-analyzer/full_234day_output/
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PROFILER_BIN = REPO_ROOT / "target" / "release" / "profile_mbo"
PROFILER_CONFIG = REPO_ROOT / "configs" / "full_single_day.toml"
PYTHON_OUTPUT = REPO_ROOT.parent / "MBO-LOB-analyzer" / "full_234day_output"
GOLDEN_DAY = "2025-02-03"


def run_profiler(output_dir: Path) -> bool:
    """Run the profiler and return True if successful."""
    # Runtime keys MUST be at the top-level (before any [section] header).
    # With ProfilerConfig's `#[serde(deny_unknown_fields)]`, placing these under
    # [trackers] causes a hard parse error instead of being silently dropped.
    config_content = f"""
timescales = [1.0, 5.0, 10.0, 30.0, 60.0, 300.0]
reservoir_capacity = 10000

[input]
hot_store_dir = "../data/hot_store"
filename_pattern = "xnas-itch-{{date}}.mbo.dbn"
symbol = "NVDA"
exchange = "XNAS"
date_start = "{GOLDEN_DAY}"
date_end = "{GOLDEN_DAY}"

[trackers]
quality = true
ofi = true
spread = true
returns = true
volatility = true
lifecycle = true
trades = true
depth = true
liquidity = true
jumps = true
noise = true

[output]
output_dir = "{output_dir}"
write_summaries = false
"""
    config_path = output_dir / "config.toml"
    config_path.write_text(config_content)

    if not PROFILER_BIN.exists():
        print(f"ERROR: Profiler binary not found at {PROFILER_BIN}")
        print("Run: cargo build --release --bin profile_mbo")
        return False

    result = subprocess.run(
        [str(PROFILER_BIN), "--config", str(config_path)],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
        env={**os.environ, "RUST_LOG": "info"},
    )

    if result.returncode != 0:
        print(f"FAIL: Profiler exited with code {result.returncode}")
        print(result.stderr)
        return False

    for line in result.stderr.strip().split("\n"):
        if "Profiling complete" in line or "Done:" in line:
            print(f"  {line.split('] ')[-1] if '] ' in line else line}")

    return True


def load_golden() -> dict:
    """Load the Python analyzer's golden values for the test day."""
    quality_path = PYTHON_OUTPUT / "01_DataQualityAnalyzer.json"
    if not quality_path.exists():
        print(f"ERROR: Python golden output not found at {quality_path}")
        sys.exit(1)

    with open(quality_path) as f:
        data = json.load(f)

    for day in data.get("day_stats", []):
        if day.get("date") == GOLDEN_DAY:
            return day

    print(f"ERROR: Day {GOLDEN_DAY} not found in Python output")
    sys.exit(1)


def check_value(name: str, actual, expected, tolerance=0, relative=False) -> bool:
    """Check a value against expected. Returns True if PASS."""
    if actual is None:
        print(f"  FAIL: {name} — value is None")
        return False

    if tolerance == 0:
        if actual == expected:
            return True
        else:
            print(f"  FAIL: {name} — expected {expected}, got {actual}")
            return False

    if relative:
        if expected == 0:
            diff = abs(actual)
        else:
            diff = abs(actual - expected) / abs(expected)
        if diff <= tolerance:
            return True
        else:
            print(f"  FAIL: {name} — expected {expected}, got {actual} (rel diff {diff:.4f} > {tolerance})")
            return False
    else:
        diff = abs(actual - expected)
        if diff <= tolerance:
            return True
        else:
            print(f"  FAIL: {name} — expected {expected}, got {actual} (abs diff {diff} > {tolerance})")
            return False


def validate_quality(rust: dict, golden: dict) -> tuple:
    """Validate QualityTracker against golden values."""
    checks = 0
    passes = 0

    # ⚠ THE `trade_count` COMPARISON WAS LOUD IN THE WRONG DIRECTION.
    #
    # The golden's `action_counts["Trade"]` was produced BEFORE the carrier
    # split, when the decoder merged `b'T' | b'F' => Action::Trade`, so it is the
    # MERGED population. Post-split the Rust `trade_count` is bucket 3 =
    # `TradeAggregate` alone, which is smaller by the whole `F` population
    # (−44.05% of executions on XNAS). Comparing them at tolerance 0 fails on a
    # CORRECT number — and a gate that reds on correct code is one that gets
    # routed around (hft-rules §1).
    #
    # ⭐ THE REPAIR IS STRICTLY STRONGER THAN THE ORIGINAL CHECK. Comparing
    # `trade_count + fill_count` against the merged golden asserts that the
    # split PARTITIONS the same population — nothing invented, nothing dropped.
    # The old check could only see the total; this one would also catch a
    # record leaking out of both buckets.
    #
    # ⚠ ABSENT IS NOT ZERO (hft-rules §9). If either key is missing the sum is
    # `None`, so `check_value` reports a missing measurement rather than
    # silently comparing a fabricated 0.
    _actions = rust.get("action_distribution", {})
    _t = _actions.get("trade_count")
    _f = _actions.get("fill_count")
    _carrier_total = None if (_t is None or _f is None) else _t + _f

    tests = [
        ("total_events", rust.get("total_events"), golden["n_mbo_rows"], 0),
        ("add_count", _actions.get("add_count"),
         golden["action_counts"].get("Add"), 0),
        ("cancel_count", _actions.get("cancel_count"),
         golden["action_counts"].get("Cancel"), 0),
        ("trade_count+fill_count", _carrier_total,
         golden["action_counts"].get("Trade"), 0),
    ]

    for name, actual, expected, tol in tests:
        checks += 1
        if check_value(name, actual, expected, tol):
            passes += 1

    return passes, checks


def validate_spread(rust: dict, golden: dict) -> tuple:
    """Validate SpreadTracker against golden values."""
    checks = 0
    passes = 0

    spread_mean = rust.get("distribution_usd", {}).get("mean")
    golden_mean = golden.get("spread_mean_usd")
    checks += 1
    if check_value("spread_mean_usd", spread_mean, golden_mean, 0.05, relative=True):
        passes += 1

    return passes, checks


def validate_lifecycle(rust: dict, golden: dict) -> tuple:
    """Validate LifecycleTracker ranges."""
    checks = 0
    passes = 0

    fr = rust.get("fill_rate")
    checks += 1
    if fr is not None and 0.02 < fr < 0.10:
        passes += 1
    else:
        print(f"  FAIL: fill_rate — {fr} outside [0.02, 0.10]")

    cta = rust.get("cancel_to_add_ratio")
    checks += 1
    if cta is not None and 0.9 < cta < 1.2:
        passes += 1
    else:
        print(f"  FAIL: cancel_to_add_ratio — {cta} outside [0.9, 1.2]")

    return passes, checks


def main():
    print("=" * 60)
    print("Cross-Validation: Rust Profiler vs Python MBO-LOB-Analyzer")
    print(f"Golden day: {GOLDEN_DAY}")
    print("=" * 60)

    golden = load_golden()
    print(f"\nGolden reference loaded: {golden['n_mbo_rows']:,} MBO rows")

    with tempfile.TemporaryDirectory(prefix="profiler_xval_") as tmpdir:
        output_dir = Path(tmpdir)
        print(f"\nRunning profiler...")
        if not run_profiler(output_dir):
            sys.exit(1)

        rust_files = sorted(output_dir.glob("*.json"))
        print(f"\nRust output: {len(rust_files)} tracker files")

        rust_data = {}
        for f in rust_files:
            with open(f) as fh:
                d = json.load(fh)
            tracker = d.get("tracker", f.stem)
            rust_data[tracker] = d

        total_pass = 0
        total_check = 0

        print("\n--- QualityTracker ---")
        p, c = validate_quality(rust_data.get("QualityTracker", {}), golden)
        total_pass += p
        total_check += c
        print(f"  {p}/{c} PASS")

        print("\n--- SpreadTracker ---")
        p, c = validate_spread(rust_data.get("SpreadTracker", {}), golden)
        total_pass += p
        total_check += c
        print(f"  {p}/{c} PASS")

        print("\n--- LifecycleTracker ---")
        p, c = validate_lifecycle(rust_data.get("LifecycleTracker", {}), golden)
        total_pass += p
        total_check += c
        print(f"  {p}/{c} PASS")

        print("\n--- TradeTracker ---")
        total_check += 1
        trades = rust_data.get("TradeTracker", {}).get("total_trades")
        if trades == golden["action_counts"].get("Trade"):
            total_pass += 1
            print(f"  1/1 PASS (total_trades={trades})")
        else:
            print(f"  FAIL: total_trades expected {golden['action_counts'].get('Trade')}, got {trades}")

        print(f"\n{'=' * 60}")
        print(f"TOTAL: {total_pass}/{total_check} checks PASSED")
        if total_pass == total_check:
            print("STATUS: ALL CHECKS PASSED")
        else:
            print(f"STATUS: {total_check - total_pass} FAILURES")
            sys.exit(1)


if __name__ == "__main__":
    main()
