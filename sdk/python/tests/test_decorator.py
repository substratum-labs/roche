# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import asyncio
import inspect
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from roche_sandbox.decorator import roche_sandbox
from roche_sandbox.types import ExecOutput


def _make_mock_sandbox():
    sb = MagicMock()
    sb.exec.return_value = ExecOutput(exit_code=0, stdout="hello\n", stderr="")
    sb.__enter__ = MagicMock(return_value=sb)
    sb.__exit__ = MagicMock(return_value=False)
    return sb


def _make_async_mock_sandbox():
    sb = AsyncMock()
    sb.exec.return_value = ExecOutput(exit_code=0, stdout="hello\n", stderr="")
    sb.__aenter__ = AsyncMock(return_value=sb)
    sb.__aexit__ = AsyncMock(return_value=False)
    return sb


class TestSyncDecorator:
    def test_injects_sandbox(self):
        mock_sb = _make_mock_sandbox()

        with patch("roche_sandbox.decorator.Roche") as MockRoche:
            MockRoche.return_value.create.return_value = mock_sb

            @roche_sandbox(image="python:3.12-slim")
            def run_code(code: str, sandbox) -> str:
                return sandbox.exec(["python3", "-c", code]).stdout

            result = run_code("print('hello')")
            assert result == "hello\n"
            mock_sb.__enter__.assert_called_once()
            mock_sb.__exit__.assert_called_once()

    def test_sandbox_param_stripped_from_signature(self):
        @roche_sandbox()
        def run_code(code: str, sandbox) -> str:
            return sandbox.exec(["python3", "-c", code]).stdout

        sig = inspect.signature(run_code)
        assert "sandbox" not in sig.parameters
        assert "code" in sig.parameters

    def test_custom_sandbox_param_name(self):
        mock_sb = _make_mock_sandbox()

        with patch("roche_sandbox.decorator.Roche") as MockRoche:
            MockRoche.return_value.create.return_value = mock_sb

            @roche_sandbox(sandbox_param="sb")
            def run_code(code: str, sb) -> str:
                return sb.exec(["python3", "-c", code]).stdout

            result = run_code("print('hello')")
            assert result == "hello\n"

    def test_preserves_function_name(self):
        @roche_sandbox()
        def my_tool(code: str, sandbox) -> str:
            return ""

        assert my_tool.__name__ == "my_tool"

    def test_passes_create_kwargs(self):
        mock_sb = _make_mock_sandbox()

        with patch("roche_sandbox.decorator.Roche") as MockRoche:
            MockRoche.return_value.create.return_value = mock_sb

            @roche_sandbox(image="node:20", network=True, memory="1g", cpus=2.0)
            def run_code(code: str, sandbox) -> str:
                return ""

            run_code("x")
            call_kwargs = MockRoche.return_value.create.call_args[1]
            assert call_kwargs["image"] == "node:20"
            assert call_kwargs["network"] is True
            assert call_kwargs["memory"] == "1g"
            assert call_kwargs["cpus"] == 2.0


class TestAsyncDecorator:
    @pytest.mark.asyncio
    async def test_injects_sandbox(self):
        mock_sb = _make_async_mock_sandbox()

        with patch("roche_sandbox.decorator.AsyncRoche") as MockRoche:
            MockRoche.return_value.create = AsyncMock(return_value=mock_sb)

            @roche_sandbox(image="python:3.12-slim")
            async def run_code(code: str, sandbox) -> str:
                result = await sandbox.exec(["python3", "-c", code])
                return result.stdout

            result = await run_code("print('hello')")
            assert result == "hello\n"

    @pytest.mark.asyncio
    async def test_sandbox_param_stripped(self):
        @roche_sandbox()
        async def run_code(code: str, sandbox) -> str:
            return ""

        sig = inspect.signature(run_code)
        assert "sandbox" not in sig.parameters

    @pytest.mark.asyncio
    async def test_preserves_function_name(self):
        @roche_sandbox()
        async def my_async_tool(code: str, sandbox) -> str:
            return ""

        assert my_async_tool.__name__ == "my_async_tool"
