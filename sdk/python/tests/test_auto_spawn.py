import os
import socket
import time
from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

from roche_sandbox.daemon import (
    _find_bundled_binary,
    _spawn_daemon,
    _wait_for_daemon_ready,
)
from roche_sandbox.client import AsyncRoche
from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.transport.grpc import GrpcTransport


class TestFindBundledBinary:
    def test_returns_path_when_binary_exists(self, tmp_path):
        bin_dir = tmp_path / "bin"
        bin_dir.mkdir()
        binary = bin_dir / "roched"
        binary.write_text("#!/bin/sh\n")
        binary.chmod(0o755)

        with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=bin_dir):
            result = _find_bundled_binary("roched")
        assert result == binary

    def test_returns_none_when_binary_missing(self):
        with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=Path("/nonexistent")):
            result = _find_bundled_binary("roched")
        assert result is None

    def test_returns_none_when_not_executable(self, tmp_path):
        bin_dir = tmp_path / "bin"
        bin_dir.mkdir()
        binary = bin_dir / "roched"
        binary.write_text("not executable")
        binary.chmod(0o644)

        with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=bin_dir):
            result = _find_bundled_binary("roched")
        assert result is None


class TestWaitForDaemonReady:
    def test_returns_true_when_daemon_ready(self):
        # Create a temporary server socket to simulate daemon port
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", 0))
            s.listen(1)
            port = s.getsockname()[1]

            with patch("roche_sandbox.daemon.detect_daemon", return_value={"pid": os.getpid(), "port": port}):
                result = _wait_for_daemon_ready(timeout=1.0)
            assert result is True

    def test_returns_false_on_timeout(self):
        with patch("roche_sandbox.daemon.detect_daemon", return_value=None):
            result = _wait_for_daemon_ready(timeout=0.3)
        assert result is False

    def test_returns_false_when_port_not_open(self):
        with patch("roche_sandbox.daemon.detect_daemon", return_value={"pid": os.getpid(), "port": 19999}):
            result = _wait_for_daemon_ready(timeout=0.5)
        assert result is False


class TestSpawnDaemon:
    def test_spawns_detached_process(self, tmp_path):
        fake_binary = tmp_path / "roched"
        fake_binary.write_text("#!/bin/sh\nsleep 100\n")
        fake_binary.chmod(0o755)

        with patch("subprocess.Popen") as mock_popen, \
             patch("roche_sandbox.daemon._roche_dir", return_value=tmp_path):
            _spawn_daemon(fake_binary)
            mock_popen.assert_called_once()
            call_kwargs = mock_popen.call_args
            assert call_kwargs.kwargs.get("start_new_session") is True

    def test_passes_idle_timeout_from_env(self, tmp_path):
        fake_binary = tmp_path / "roched"
        fake_binary.write_text("#!/bin/sh\n")
        fake_binary.chmod(0o755)

        with patch("subprocess.Popen") as mock_popen, \
             patch("roche_sandbox.daemon._roche_dir", return_value=tmp_path), \
             patch.dict(os.environ, {"ROCHE_DAEMON_IDLE_TIMEOUT": "300"}):
            _spawn_daemon(fake_binary)
            args = mock_popen.call_args[0][0]
            assert "--idle-timeout" in args
            assert "300" in args


class TestAutoSpawnTransport:
    def test_uses_grpc_when_daemon_running(self):
        with patch("roche_sandbox.client.detect_daemon", return_value={"pid": 123, "port": 50051}):
            roche = AsyncRoche()
        assert isinstance(roche.transport, GrpcTransport)

    def test_spawns_daemon_from_bundled_binary(self):
        with patch("roche_sandbox.client.detect_daemon", side_effect=[None, {"pid": 123, "port": 50051}]), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=Path("/fake/roched")), \
             patch("roche_sandbox.client._spawn_daemon") as mock_spawn, \
             patch("roche_sandbox.client._wait_for_daemon_ready", return_value=True):
            roche = AsyncRoche()
        mock_spawn.assert_called_once()

    def test_falls_back_to_cli_when_no_bundled_binary(self):
        with patch("roche_sandbox.client.detect_daemon", return_value=None), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=None):
            roche = AsyncRoche()
        assert isinstance(roche.transport, CliTransport)

    def test_falls_back_to_cli_when_spawn_times_out(self):
        with patch("roche_sandbox.client.detect_daemon", return_value=None), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=Path("/fake/roched")), \
             patch("roche_sandbox.client._spawn_daemon"), \
             patch("roche_sandbox.client._wait_for_daemon_ready", return_value=False):
            roche = AsyncRoche()
        assert isinstance(roche.transport, CliTransport)
