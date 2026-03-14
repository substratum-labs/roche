import json
import os
from unittest.mock import patch

import pytest

from roche_sandbox.daemon import detect_daemon


class TestDetectDaemon:
    def test_returns_none_when_file_missing(self, tmp_path):
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=tmp_path / "daemon.json"):
            result = detect_daemon()
        assert result is None

    def test_returns_none_when_file_malformed(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text("not json")
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is None

    def test_returns_info_when_valid_and_alive(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text(json.dumps({"pid": os.getpid(), "port": 50051}))
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is not None
        assert result["pid"] == os.getpid()
        assert result["port"] == 50051

    def test_returns_none_when_pid_dead(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text(json.dumps({"pid": 999999999, "port": 50051}))
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is None
