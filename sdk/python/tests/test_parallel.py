# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

import sys
import unittest
from unittest.mock import AsyncMock, patch

from roche_sandbox.run import (
    ParallelResult,
    RunResult,
    async_run_parallel,
)
from roche_sandbox.types import ExecOutput

# Get the actual module object (not the function re-exported from __init__)
_run_mod = sys.modules["roche_sandbox.run"]


class TestParallelResultStructure(unittest.TestCase):
    def test_dataclass_fields(self):
        r = ParallelResult(
            results=[RunResult(exit_code=0, stdout="ok", stderr="")],
            total_succeeded=1,
            total_failed=0,
        )
        assert len(r.results) == 1
        assert r.total_succeeded == 1
        assert r.total_failed == 0

    def test_defaults(self):
        r = ParallelResult(results=[])
        assert r.total_succeeded == 0
        assert r.total_failed == 0


class TestAsyncRunParallel(unittest.IsolatedAsyncioTestCase):
    async def test_creates_correct_number_of_results(self):
        mock_run = AsyncMock(
            return_value=RunResult(exit_code=0, stdout="ok", stderr="")
        )
        with patch.object(_run_mod, "async_run", mock_run):
            tasks = [{"code": f"print({i})"} for i in range(4)]
            result = await async_run_parallel(tasks)

        assert len(result.results) == 4
        assert result.total_succeeded == 4
        assert result.total_failed == 0

    async def test_failed_task_returns_exit_code_1(self):
        """A task that raises an exception should return exit_code=1 without crashing others."""
        call_count = 0

        async def mock_run(code=None, opts=None, **kwargs):
            nonlocal call_count
            call_count += 1
            if call_count == 2:
                raise RuntimeError("container failed")
            return RunResult(exit_code=0, stdout="ok", stderr="")

        with patch.object(_run_mod, "async_run", mock_run):
            tasks = [
                {"code": "print(1)"},
                {"code": "print(2)"},  # this one will fail
                {"code": "print(3)"},
            ]
            result = await async_run_parallel(tasks)

        assert len(result.results) == 3
        assert result.total_failed == 1
        assert result.total_succeeded == 2

        # The failed task should have exit_code=1 and error in stderr
        failed = result.results[1]
        assert failed.exit_code == 1
        assert "container failed" in failed.stderr

    async def test_respects_max_concurrency(self):
        """Verify tasks are dispatched (semaphore doesn't block all)."""
        mock_run = AsyncMock(
            return_value=RunResult(exit_code=0, stdout="", stderr="")
        )
        with patch.object(_run_mod, "async_run", mock_run):
            tasks = [{"code": f"print({i})"} for i in range(10)]
            result = await async_run_parallel(tasks, max_concurrency=2)

        assert len(result.results) == 10
        assert result.total_succeeded == 10

    async def test_empty_tasks_list(self):
        result = await async_run_parallel([])
        assert len(result.results) == 0
        assert result.total_succeeded == 0
        assert result.total_failed == 0


if __name__ == "__main__":
    unittest.main()
