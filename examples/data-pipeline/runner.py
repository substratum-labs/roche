#!/usr/bin/env python3
"""Data Pipeline Runner — execute data processing scripts safely.

Accepts a Python script + optional data file, runs in a sandbox with
pandas/numpy pre-cached, and returns the output files.

Usage:
    python examples/data-pipeline/runner.py examples/data-pipeline/sample_scripts/analyze.py

    # With data file
    python examples/data-pipeline/runner.py script.py --data input.csv --download /app/output.csv

    # With dependency caching (fast on repeat runs)
    python examples/data-pipeline/runner.py script.py --cached
"""

from __future__ import annotations

import argparse
import sys

from roche_sandbox import run, run_cached, snapshot, restore, RunResult


def print_result(result: RunResult, label: str = "") -> None:
    if label:
        print(f"\n--- {label} ---")
    if result.stdout:
        print(result.stdout, end="")
    if result.stderr:
        print(result.stderr, end="", file=sys.stderr)
    print(f"\n[exit_code={result.exit_code}]")
    if result.files:
        for name, data in result.files.items():
            print(f"[downloaded {name}: {len(data)} bytes]")


def main():
    parser = argparse.ArgumentParser(description="Run data processing scripts safely")
    parser.add_argument("script", help="Python script to execute")
    parser.add_argument("--data", help="Data file to upload alongside the script")
    parser.add_argument("--download", action="append", help="Sandbox path to download after execution")
    parser.add_argument("--cached", action="store_true", help="Use dependency caching")
    parser.add_argument("--snapshot", action="store_true", help="Save environment snapshot after run")
    args = parser.parse_args()

    download = args.download or []
    run_fn = run_cached if args.cached else run

    print(f"Running {args.script} in sandbox...")

    if args.data:
        # Upload data file alongside the script — use project mode
        import shutil
        import tempfile
        from pathlib import Path

        with tempfile.TemporaryDirectory() as tmp:
            # Copy script and data into a temp project dir
            shutil.copy(args.script, tmp)
            shutil.copy(args.data, tmp)
            script_name = Path(args.script).name

            result = run_fn(
                path=tmp,
                entry=script_name,
                install=True,
                download=download,
                timeout_secs=120,
            )
    else:
        result = run_fn(
            file=args.script,
            install=True,
            download=download,
            timeout_secs=120,
        )

    print_result(result, args.script)

    # Save snapshot if requested
    if args.snapshot and result.exit_code == 0:
        print("\n[Snapshot not available in file mode — use SDK directly]")


if __name__ == "__main__":
    main()
