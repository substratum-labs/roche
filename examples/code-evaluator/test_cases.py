#!/usr/bin/env python3
"""Batch test case runner — evaluates multiple code submissions.

Demonstrates run_parallel() for concurrent evaluation of student
submissions or coding challenge test cases.

Usage:
    python examples/code-evaluator/test_cases.py
"""

from roche_sandbox import run_parallel


def main():
    # Simulate coding challenge: "Write a function that returns the sum of a list"
    submissions = [
        # Correct
        {"code": "def solve(nums): return sum(nums)\nprint(solve([1,2,3]))"},
        # Correct but slow
        {"code": "def solve(nums):\n  total = 0\n  for n in nums: total += n\n  return total\nprint(solve([1,2,3]))"},
        # Wrong answer
        {"code": "def solve(nums): return len(nums)\nprint(solve([1,2,3]))"},
        # Runtime error
        {"code": "def solve(nums): return nums[999]\nprint(solve([1,2,3]))"},
        # Timeout (infinite loop)
        {"code": "while True: pass", "timeout_secs": 3},
        # Security attempt (blocked — network disabled)
        {"code": "import socket; socket.connect(('evil.com', 80))"},
        # Python version check
        {"code": "import sys; print(f'Python {sys.version}')"},
        # Multi-language: Node.js
        {"code": "console.log(1 + 2 + 3)", "language": "node"},
    ]

    print("Running 8 submissions in parallel...\n")
    results = run_parallel(submissions, max_concurrency=4)

    for i, (task, result) in enumerate(zip(submissions, results.results)):
        code_preview = task["code"][:50].replace("\n", "\\n")
        status = "PASS" if result.exit_code == 0 else "FAIL"
        stdout = result.stdout.strip()[:80]
        stderr = result.stderr.strip()[:80] if result.stderr else ""

        print(f"  [{status}] #{i+1}: {code_preview}...")
        if stdout:
            print(f"         stdout: {stdout}")
        if stderr:
            print(f"         stderr: {stderr}")

    print(f"\n{results.total_succeeded}/{len(submissions)} passed, "
          f"{results.total_failed} failed")


if __name__ == "__main__":
    main()
