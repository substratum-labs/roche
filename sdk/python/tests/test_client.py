"""Unit tests for the Roche Python SDK (no Docker required)."""

import subprocess
from unittest.mock import MagicMock, patch

import pytest

from roche import ExecOutput, Roche, RocheError, Sandbox, SandboxConfig


class TestRocheClient:
    def test_create_default_config(self):
        mock_result = MagicMock()
        mock_result.stdout = "abc123def456\n"
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche(binary="/usr/bin/roche")
            sandbox_id = client.create()

        assert sandbox_id == "abc123def456"
        args = mock_run.call_args[0][0]
        assert args[0] == "/usr/bin/roche"
        assert "create" in args
        assert "--provider" in args
        assert "docker" in args
        assert "--image" in args
        assert "python:3.12-slim" in args
        # Network and writable flags should NOT be present (defaults off)
        assert "--network" not in args
        assert "--writable" not in args

    def test_create_custom_config(self):
        mock_result = MagicMock()
        mock_result.stdout = "xyz789\n"
        mock_result.returncode = 0

        config = SandboxConfig(
            memory="1g",
            cpus=2.0,
            network=True,
            writable=True,
        )

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            sandbox_id = client.create(config)

        assert sandbox_id == "xyz789"
        args = mock_run.call_args[0][0]
        assert "--memory" in args
        assert "1g" in args
        assert "--cpus" in args
        assert "2.0" in args
        assert "--network" in args
        assert "--writable" in args

    def test_exec_returns_output(self):
        mock_result = MagicMock()
        mock_result.stdout = "4\n"
        mock_result.stderr = ""
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            output = client.exec("abc123", ["python3", "-c", "print(2+2)"])

        assert isinstance(output, ExecOutput)
        assert output.exit_code == 0
        assert output.stdout == "4\n"

    def test_exec_nonzero_exit(self):
        mock_result = MagicMock()
        mock_result.stdout = ""
        mock_result.stderr = "error\n"
        mock_result.returncode = 1

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            output = client.exec("abc123", ["false"])

        assert output.exit_code == 1
        assert output.stderr == "error\n"

    def test_destroy_calls_cli(self):
        mock_result = MagicMock()
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            client.destroy("abc123")

        args = mock_run.call_args[0][0]
        assert "destroy" in args
        assert "abc123" in args

    def test_list_parses_json(self):
        mock_result = MagicMock()
        mock_result.stdout = '[{"id":"abc","status":"running","provider":"docker","image":"python:3.12-slim"}]'
        mock_result.returncode = 0

        with patch("subprocess.run", return_value=mock_result):
            client = Roche()
            sandboxes = client.list()

        assert len(sandboxes) == 1
        assert sandboxes[0]["id"] == "abc"

    def test_binary_not_found_raises_error(self):
        with patch("subprocess.run", side_effect=FileNotFoundError):
            client = Roche(binary="nonexistent")
            with pytest.raises(RocheError, match="not found"):
                client.create()

    def test_cli_error_raises_roche_error(self):
        with patch(
            "subprocess.run",
            side_effect=subprocess.CalledProcessError(1, "roche", stderr="provider unavailable"),
        ):
            client = Roche()
            with pytest.raises(RocheError, match="provider unavailable"):
                client.create()

    def test_create_with_env_vars(self):
        mock_result = MagicMock()
        mock_result.stdout = "env123\n"
        mock_result.returncode = 0

        config = SandboxConfig(env={"FOO": "bar", "DB": "localhost"})

        with patch("subprocess.run", return_value=mock_result) as mock_run:
            client = Roche()
            sandbox_id = client.create(config)

        assert sandbox_id == "env123"
        args = mock_run.call_args[0][0]
        assert "--env" in args
        assert "FOO=bar" in args
        assert "DB=localhost" in args


class TestSandboxContextManager:
    def test_sandbox_creates_and_destroys(self):
        mock_create = MagicMock()
        mock_create.stdout = "sandbox123\n"
        mock_create.returncode = 0

        mock_destroy = MagicMock()
        mock_destroy.returncode = 0

        with patch("subprocess.run", side_effect=[mock_create, mock_destroy]):
            client = Roche()
            with Sandbox(client) as sb:
                assert sb.id == "sandbox123"

    def test_sandbox_id_before_enter_raises(self):
        client = Roche()
        sb = Sandbox(client)
        with pytest.raises(RocheError, match="not created"):
            _ = sb.id
