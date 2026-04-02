#!/usr/bin/env python3
"""Safe Code Evaluator — run untrusted code via HTTP API.

A minimal FastAPI server that accepts code submissions, executes them
in Roche sandboxes, and returns results. Like the backend of LeetCode
or Replit, in 80 lines.

Usage:
    pip install fastapi uvicorn roche-sandbox
    python examples/code-evaluator/server.py

    # Submit code
    curl -X POST http://localhost:8000/run \
      -H "Content-Type: application/json" \
      -d '{"code": "print(2 + 2)", "language": "python"}'

    # Batch evaluate
    curl -X POST http://localhost:8000/batch \
      -H "Content-Type: application/json" \
      -d '{"tasks": [{"code": "print(1)"}, {"code": "print(2)"}]}'
"""

from __future__ import annotations

from fastapi import FastAPI
from pydantic import BaseModel

from roche_sandbox import async_run, async_run_parallel, RunOptions

app = FastAPI(title="Roche Code Evaluator")


class RunRequest(BaseModel):
    code: str
    language: str = "auto"
    timeout_secs: int = 10


class RunResponse(BaseModel):
    stdout: str
    stderr: str
    exit_code: int
    provider: str = ""
    duration_ms: float = 0


class BatchRequest(BaseModel):
    tasks: list[RunRequest]
    max_concurrency: int = 5


class BatchResponse(BaseModel):
    results: list[RunResponse]
    total_succeeded: int
    total_failed: int


@app.post("/run", response_model=RunResponse)
async def run_code(req: RunRequest):
    """Execute a single code snippet in a sandbox."""
    result = await async_run(
        req.code,
        RunOptions(
            language=req.language,
            timeout_secs=req.timeout_secs,
        ),
    )
    duration = result.trace.duration_secs * 1000 if result.trace else 0
    return RunResponse(
        stdout=result.stdout,
        stderr=result.stderr,
        exit_code=result.exit_code,
        duration_ms=round(duration, 1),
    )


@app.post("/batch", response_model=BatchResponse)
async def batch_run(req: BatchRequest):
    """Execute multiple code snippets in parallel sandboxes."""
    tasks = [
        {"code": t.code, "language": t.language, "timeout_secs": t.timeout_secs}
        for t in req.tasks
    ]
    pr = await async_run_parallel(tasks, max_concurrency=req.max_concurrency)
    results = []
    for r in pr.results:
        duration = r.trace.duration_secs * 1000 if r.trace else 0
        results.append(RunResponse(
            stdout=r.stdout,
            stderr=r.stderr,
            exit_code=r.exit_code,
            duration_ms=round(duration, 1),
        ))
    return BatchResponse(
        results=results,
        total_succeeded=pr.total_succeeded,
        total_failed=pr.total_failed,
    )


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
