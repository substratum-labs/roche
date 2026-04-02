# Code Evaluator

A safe code execution API — submit code via HTTP, get results back. Like the backend of LeetCode or Replit.

## Quick Start

```bash
pip install fastapi uvicorn roche-sandbox
python examples/code-evaluator/server.py
```

```bash
# Run a single snippet
curl -X POST http://localhost:8000/run \
  -H "Content-Type: application/json" \
  -d '{"code": "print(2 + 2)", "language": "python"}'
# → {"stdout": "4\n", "stderr": "", "exit_code": 0, "duration_ms": 150.3}

# Batch evaluate
curl -X POST http://localhost:8000/batch \
  -H "Content-Type: application/json" \
  -d '{"tasks": [{"code": "print(1)"}, {"code": "print(2)"}], "max_concurrency": 5}'
```

## Batch Test Runner

Run multiple submissions in parallel without the HTTP server:

```bash
python examples/code-evaluator/test_cases.py
```

```
Running 8 submissions in parallel...

  [PASS] #1: def solve(nums): return sum(nums)\nprint(solve...
         stdout: 6
  [PASS] #2: def solve(nums):\n  total = 0\n  for n in num...
         stdout: 6
  [PASS] #3: def solve(nums): return len(nums)\nprint(solve...
         stdout: 3
  [FAIL] #4: def solve(nums): return nums[999]\nprint(solve...
         stderr: IndexError: list index out of range
  [FAIL] #5: while True: pass...
         stderr: timeout
  [FAIL] #6: import socket; socket.connect(('evil.com', 80))...
         stderr: ConnectionRefusedError
  [PASS] #7: import sys; print(f'Python {sys.version}')...
         stdout: Python 3.12.x
  [PASS] #8: console.log(1 + 2 + 3)...
         stdout: 6

5/8 passed, 3 failed
```

## What Roche Does

- Each submission runs in an **isolated Docker container**
- **Network disabled** by default — `socket.connect()` fails
- **Filesystem readonly** — can't write to disk
- **Timeout enforced** — infinite loops killed after N seconds
- **Parallel** — `run_parallel()` runs all submissions concurrently
- **Multi-language** — Python, Node.js, Bash auto-detected
