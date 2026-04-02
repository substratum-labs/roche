#!/usr/bin/env python3
"""GitHub Repo Tester — clone and test any public repo.

Pull a GitHub project, detect how to run it, execute in a sandbox,
and report results. CI-as-a-function.

Usage:
    python examples/github-tester/tester.py owner/repo
    python examples/github-tester/tester.py owner/repo --ref main
    python examples/github-tester/tester.py owner/repo --command "pytest -v"
"""

from __future__ import annotations

import argparse
import sys
import time

from roche_sandbox import run, RunResult


def print_result(result: RunResult, elapsed: float) -> None:
    status = "PASSED" if result.exit_code == 0 else "FAILED"
    color = "\033[32m" if result.exit_code == 0 else "\033[31m"
    reset = "\033[0m"

    print(f"\n{'=' * 60}")
    print(f"  Status: {color}{status}{reset} (exit code {result.exit_code})")
    print(f"  Time:   {elapsed:.1f}s")
    print(f"{'=' * 60}")

    if result.stdout:
        print(f"\n--- stdout ---\n{result.stdout[:2000]}", end="")
        if len(result.stdout) > 2000:
            print(f"\n... ({len(result.stdout)} bytes total, truncated)")

    if result.stderr:
        print(f"\n--- stderr ---\n{result.stderr[:2000]}", end="")
        if len(result.stderr) > 2000:
            print(f"\n... ({len(result.stderr)} bytes total, truncated)")


def main():
    parser = argparse.ArgumentParser(description="Test a GitHub repo in a sandbox")
    parser.add_argument("repo", help="GitHub repo (owner/repo)")
    parser.add_argument("--ref", help="Branch, tag, or commit")
    parser.add_argument("--command", help="Explicit test command (e.g. 'pytest -v')")
    parser.add_argument("--timeout", type=int, default=120, help="Timeout in seconds")
    args = parser.parse_args()

    print(f"Testing {args.repo}...")
    if args.ref:
        print(f"  ref: {args.ref}")
    if args.command:
        print(f"  command: {args.command}")

    t0 = time.time()
    try:
        result = run(
            github=args.repo,
            ref=args.ref,
            command=args.command,
            timeout_secs=args.timeout,
            network=True,  # repos often need network for deps
            install=True,
        )
        print_result(result, time.time() - t0)
        sys.exit(result.exit_code)

    except Exception as e:
        elapsed = time.time() - t0
        print(f"\n{'=' * 60}")
        print(f"  Status: \033[31mERROR\033[0m ({elapsed:.1f}s)")
        print(f"  {e}")
        print(f"{'=' * 60}")
        sys.exit(1)


if __name__ == "__main__":
    main()
