#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche performance benchmarks.

Measures creation latency, exec latency, pool hit/miss, parallel throughput,
and provider comparison. Requires Docker running.

Usage:
    python benchmarks/bench.py              # run all benchmarks
    python benchmarks/bench.py --quick      # fast subset
    python benchmarks/bench.py --json       # machine-readable output
"""

from __future__ import annotations

import argparse
import asyncio
import json
import statistics
import sys
import time
from dataclasses import dataclass, field


@dataclass
class BenchResult:
    name: str
    iterations: int
    times_ms: list[float] = field(default_factory=list)

    @property
    def mean_ms(self) -> float:
        return statistics.mean(self.times_ms) if self.times_ms else 0

    @property
    def median_ms(self) -> float:
        return statistics.median(self.times_ms) if self.times_ms else 0

    @property
    def min_ms(self) -> float:
        return min(self.times_ms) if self.times_ms else 0

    @property
    def max_ms(self) -> float:
        return max(self.times_ms) if self.times_ms else 0

    @property
    def p95_ms(self) -> float:
        if not self.times_ms:
            return 0
        sorted_t = sorted(self.times_ms)
        idx = int(len(sorted_t) * 0.95)
        return sorted_t[min(idx, len(sorted_t) - 1)]

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "iterations": self.iterations,
            "mean_ms": round(self.mean_ms, 2),
            "median_ms": round(self.median_ms, 2),
            "min_ms": round(self.min_ms, 2),
            "max_ms": round(self.max_ms, 2),
            "p95_ms": round(self.p95_ms, 2),
        }

    def print(self) -> None:
        print(f"  {self.name}")
        print(f"    n={self.iterations}  mean={self.mean_ms:.1f}ms  "
              f"median={self.median_ms:.1f}ms  min={self.min_ms:.1f}ms  "
              f"max={self.max_ms:.1f}ms  p95={self.p95_ms:.1f}ms")


# ---------------------------------------------------------------------------
# Individual benchmarks
# ---------------------------------------------------------------------------


async def bench_sandbox_create_destroy(n: int = 5, provider: str = "docker", image: str = "python:3.12-slim") -> BenchResult:
    """Measure sandbox creation + destruction latency (cold start)."""
    from roche_sandbox import AsyncRoche

    result = BenchResult(name=f"create+destroy ({provider}, {image})", iterations=n)
    client = AsyncRoche(provider=provider)

    for _ in range(n):
        t0 = time.perf_counter()
        sandbox = await client.create(provider=provider, image=image, timeout_secs=60)
        await sandbox.destroy()
        result.times_ms.append((time.perf_counter() - t0) * 1000)

    return result


async def bench_provider_comparison(n: int = 3) -> list[BenchResult]:
    """Compare create+destroy latency across available providers."""
    import shutil

    providers = [
        ("docker", "python:3.12-slim", lambda: shutil.which("docker") is not None),
        ("docker", "node:20-slim", lambda: shutil.which("docker") is not None),
        ("docker", "ubuntu:22.04", lambda: shutil.which("docker") is not None),
        ("wasm", "python.wasm", lambda: False),  # TODO: detect WASM runtime
    ]

    results: list[BenchResult] = []
    for provider, image, check in providers:
        if not check():
            r = BenchResult(name=f"create+destroy ({provider}, {image})", iterations=0)
            r.times_ms = []
            results.append(r)
            print(f"  SKIP {provider}/{image} (not available)")
            continue
        try:
            r = await bench_sandbox_create_destroy(n, provider, image)
            r.print()
            results.append(r)
        except Exception as e:
            print(f"  FAIL {provider}/{image}: {e}")
            results.append(BenchResult(name=f"create+destroy ({provider}, {image})", iterations=0))

    return results


async def bench_exec_comparison(n: int = 5) -> list[BenchResult]:
    """Compare exec latency across different images/languages."""
    from roche_sandbox import AsyncRoche

    configs = [
        ("python:3.12-slim", ["python3", "-c", "print('hello')"], "python"),
        ("node:20-slim", ["node", "-e", "console.log('hello')"], "node"),
        ("ubuntu:22.04", ["echo", "hello"], "bash/echo"),
    ]

    results: list[BenchResult] = []
    client = AsyncRoche()

    for image, command, label in configs:
        result = BenchResult(name=f"exec ({label}, {image})", iterations=n)
        try:
            sandbox = await client.create(image=image, timeout_secs=120)
            try:
                for _ in range(n):
                    t0 = time.perf_counter()
                    await sandbox.exec(command)
                    result.times_ms.append((time.perf_counter() - t0) * 1000)
            finally:
                await sandbox.destroy()
            result.print()
        except Exception as e:
            print(f"  FAIL exec ({label}): {e}")
        results.append(result)

    return results


async def bench_exec_hello(n: int = 5) -> BenchResult:
    """Measure exec latency for a trivial command."""
    from roche_sandbox import AsyncRoche

    result = BenchResult(name="exec print('hello') (Docker)", iterations=n)
    client = AsyncRoche()
    sandbox = await client.create(image="python:3.12-slim", timeout_secs=120)

    try:
        for _ in range(n):
            t0 = time.perf_counter()
            out = await sandbox.exec(["python3", "-c", "print('hello')"])
            result.times_ms.append((time.perf_counter() - t0) * 1000)
            assert out.stdout.strip() == "hello", f"unexpected: {out.stdout!r}"
    finally:
        await sandbox.destroy()

    return result


async def bench_run_inline(n: int = 5) -> BenchResult:
    """Measure end-to-end run() latency (create + exec + destroy)."""
    from roche_sandbox import async_run

    result = BenchResult(name="run('print(42)') end-to-end", iterations=n)

    for _ in range(n):
        t0 = time.perf_counter()
        out = await async_run("print(42)")
        result.times_ms.append((time.perf_counter() - t0) * 1000)
        assert out.stdout.strip() == "42"

    return result


async def bench_run_parallel_throughput(task_count: int = 10, concurrency: int = 5) -> BenchResult:
    """Measure parallel execution throughput."""
    from roche_sandbox import async_run_parallel

    result = BenchResult(name=f"run_parallel({task_count} tasks, concurrency={concurrency})", iterations=1)

    tasks = [{"code": f"print({i})"} for i in range(task_count)]

    t0 = time.perf_counter()
    pr = await async_run_parallel(tasks, max_concurrency=concurrency)
    elapsed = (time.perf_counter() - t0) * 1000
    result.times_ms.append(elapsed)
    result.iterations = task_count

    print(f"    throughput: {task_count / (elapsed / 1000):.1f} tasks/sec  "
          f"succeeded={pr.total_succeeded}  failed={pr.total_failed}")

    return result


async def bench_intent_analysis(n: int = 100) -> BenchResult:
    """Measure intent analysis speed (no sandbox, pure CPU)."""
    from roche_sandbox.intent import analyze

    codes = [
        'print(2+2)',
        'import requests\nrequests.get("https://api.github.com")',
        'import pandas as pd\ndf = pd.read_csv("data.csv")\ndf.to_csv("/tmp/out.csv")',
        'import subprocess\nsubprocess.run(["pip", "install", "numpy"])',
        'with open("/tmp/x.txt", "w") as f:\n    f.write("hello")',
    ]

    result = BenchResult(name="intent analyze() (Python AST)", iterations=n)

    for i in range(n):
        code = codes[i % len(codes)]
        t0 = time.perf_counter()
        analyze(code, "python")
        result.times_ms.append((time.perf_counter() - t0) * 1000)

    return result


async def bench_snapshot_restore(n: int = 3) -> BenchResult:
    """Measure snapshot + restore cycle."""
    from roche_sandbox import AsyncRoche, async_snapshot, async_restore, async_delete_snapshot

    result = BenchResult(name="snapshot + restore cycle", iterations=n)
    client = AsyncRoche()

    # Create and set up a sandbox
    sandbox = await client.create(image="python:3.12-slim", timeout_secs=120, writable=True)
    await sandbox.exec(["python3", "-c", "open('/tmp/marker.txt','w').write('bench')"])

    try:
        for _ in range(n):
            # Snapshot
            t0 = time.perf_counter()
            snap = await async_snapshot(sandbox.id)
            snap_ms = (time.perf_counter() - t0) * 1000

            # Restore + exec
            t1 = time.perf_counter()
            out = await async_restore(snap, ["python3", "-c", "print(open('/tmp/marker.txt').read())"])
            restore_ms = (time.perf_counter() - t1) * 1000

            result.times_ms.append(snap_ms + restore_ms)
            await async_delete_snapshot(snap)

            assert out.stdout.strip() == "bench", f"unexpected: {out.stdout!r}"
            print(f"    iter: snapshot={snap_ms:.0f}ms  restore+exec={restore_ms:.0f}ms")
    finally:
        await sandbox.destroy()

    return result


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


async def run_benchmarks(quick: bool = False) -> list[BenchResult]:
    results: list[BenchResult] = []

    print("\n=== Roche Performance Benchmarks ===\n")

    # Intent analysis (always fast, no Docker needed)
    r = await bench_intent_analysis(100 if not quick else 20)
    r.print()
    results.append(r)

    # Provider comparison — create+destroy across images
    print("\n--- Provider Comparison: create+destroy ---\n")
    n = 3 if quick else 5
    provider_results = await bench_provider_comparison(n)
    results.extend(provider_results)

    # Exec comparison — same command, different images
    print("\n--- Exec Comparison: by language/image ---\n")
    exec_results = await bench_exec_comparison(n * 2 if not quick else n)
    results.extend(exec_results)

    # End-to-end
    print("\n--- End-to-end ---\n")
    r = await bench_run_inline(n)
    r.print()
    results.append(r)

    if not quick:
        print()
        r = await bench_run_parallel_throughput(10, 5)
        r.print()
        results.append(r)

        print()
        r = await bench_snapshot_restore(3)
        r.print()
        results.append(r)

    print("\n=== Summary ===\n")
    print(f"{'Benchmark':<45} {'Mean':>8} {'Median':>8} {'P95':>8} {'N':>4}")
    print("-" * 78)
    for r in results:
        print(f"{r.name:<45} {r.mean_ms:>7.1f}ms {r.median_ms:>7.1f}ms {r.p95_ms:>7.1f}ms {r.iterations:>4}")

    return results


def main():
    parser = argparse.ArgumentParser(description="Roche performance benchmarks")
    parser.add_argument("--quick", action="store_true", help="Run fast subset")
    parser.add_argument("--json", action="store_true", help="Output JSON")
    args = parser.parse_args()

    results = asyncio.run(run_benchmarks(quick=args.quick))

    if args.json:
        print(json.dumps([r.to_dict() for r in results], indent=2))


if __name__ == "__main__":
    main()
