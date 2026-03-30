# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

import unittest
from unittest.mock import AsyncMock, patch

from roche_sandbox.run import RunOptions, _detect_language, _detect_provider, async_run
from roche_sandbox.types import ExecOutput


class TestDetectLanguage(unittest.TestCase):
    def test_python(self):
        assert _detect_language("import os\nprint('hello')") == "python"

    def test_node(self):
        assert _detect_language("console.log('hello')") == "node"

    def test_bash(self):
        assert _detect_language("#!/bin/bash\necho hello") == "bash"

    def test_ambiguous_defaults_to_python(self):
        assert _detect_language("x = 1") == "python"


class TestDetectProvider(unittest.TestCase):
    @patch("shutil.which", return_value="/usr/bin/docker")
    def test_docker_available(self, _):
        assert _detect_provider() == "docker"

    @patch("shutil.which", return_value=None)
    def test_docker_fallback(self, _):
        assert _detect_provider() == "docker"  # fallback for now


class TestAsyncRun(unittest.IsolatedAsyncioTestCase):
    @patch("roche_sandbox.run.AsyncRoche")
    async def test_run_basic(self, MockClient):
        mock_sandbox = AsyncMock()
        mock_sandbox.exec.return_value = ExecOutput(exit_code=0, stdout="4\n", stderr="")
        mock_sandbox.destroy = AsyncMock()

        mock_instance = MockClient.return_value
        mock_instance.create = AsyncMock(return_value=mock_sandbox)

        result = await async_run("print(2+2)")

        assert result.stdout == "4\n"
        assert result.exit_code == 0
        mock_sandbox.destroy.assert_called_once()

    @patch("roche_sandbox.run.AsyncRoche")
    async def test_run_with_options(self, MockClient):
        mock_sandbox = AsyncMock()
        mock_sandbox.exec.return_value = ExecOutput(exit_code=0, stdout="ok", stderr="")
        mock_sandbox.destroy = AsyncMock()

        mock_instance = MockClient.return_value
        mock_instance.create = AsyncMock(return_value=mock_sandbox)

        result = await async_run(
            "console.log('ok')",
            RunOptions(language="node", timeout_secs=10, network=True),
        )

        assert result.stdout == "ok"
        mock_instance.create.assert_called_once()
        call_kwargs = mock_instance.create.call_args[1]
        assert call_kwargs["image"] == "node:20-slim"
        assert call_kwargs["network"] is True
        assert call_kwargs["timeout_secs"] == 10

    @patch("roche_sandbox.run.AsyncRoche")
    async def test_run_destroys_on_error(self, MockClient):
        mock_sandbox = AsyncMock()
        mock_sandbox.exec.side_effect = RuntimeError("boom")
        mock_sandbox.destroy = AsyncMock()

        mock_instance = MockClient.return_value
        mock_instance.create = AsyncMock(return_value=mock_sandbox)

        with self.assertRaises(RuntimeError):
            await async_run("bad code")

        mock_sandbox.destroy.assert_called_once()

    @patch("roche_sandbox.run.AsyncRoche")
    async def test_run_kwargs_shorthand(self, MockClient):
        mock_sandbox = AsyncMock()
        mock_sandbox.exec.return_value = ExecOutput(exit_code=0, stdout="", stderr="")
        mock_sandbox.destroy = AsyncMock()

        mock_instance = MockClient.return_value
        mock_instance.create = AsyncMock(return_value=mock_sandbox)

        await async_run("echo hello", language="bash", timeout_secs=5)

        call_kwargs = mock_instance.create.call_args[1]
        assert call_kwargs["image"] == "ubuntu:22.04"


if __name__ == "__main__":
    unittest.main()
