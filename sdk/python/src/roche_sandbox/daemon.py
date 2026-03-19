# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import TypedDict


class DaemonInfo(TypedDict):
    pid: int
    port: int


def daemon_json_path() -> Path:
    return Path.home() / ".roche" / "daemon.json"


def detect_daemon() -> DaemonInfo | None:
    path = daemon_json_path()
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text())
    except (json.JSONDecodeError, OSError):
        return None
    pid = data.get("pid")
    port = data.get("port")
    if not isinstance(pid, int) or not isinstance(port, int):
        return None
    if not _is_process_alive(pid):
        return None
    return DaemonInfo(pid=pid, port=port)


def _is_process_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ProcessLookupError):
        return False


import socket
import subprocess
import time


def _bundled_bin_dir() -> Path:
    """Return the bin/ directory inside the roche_sandbox package."""
    return Path(__file__).parent / "bin"


def _roche_dir() -> Path:
    """Return ~/.roche directory, creating it if needed."""
    d = Path.home() / ".roche"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _find_bundled_binary(name: str) -> Path | None:
    """Locate a bundled binary in the roche_sandbox package."""
    binary = _bundled_bin_dir() / name
    if binary.exists() and os.access(binary, os.X_OK):
        return binary
    return None


def _spawn_daemon(roched_path: Path) -> None:
    """Spawn roched as a detached background process."""
    roche_dir = _roche_dir()
    log_path = roche_dir / "daemon.log"

    args = [str(roched_path)]
    idle_timeout = os.environ.get("ROCHE_DAEMON_IDLE_TIMEOUT")
    if idle_timeout:
        args.extend(["--idle-timeout", idle_timeout])

    with open(log_path, "a") as log_file:
        subprocess.Popen(
            args,
            stdout=log_file,
            stderr=log_file,
            start_new_session=True,
        )


def _wait_for_daemon_ready(timeout: float = 3.0) -> bool:
    """Poll until daemon is ready to accept gRPC connections."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        daemon = detect_daemon()
        if daemon is not None:
            try:
                with socket.create_connection(
                    ("127.0.0.1", daemon["port"]), timeout=0.5
                ):
                    return True
            except (ConnectionRefusedError, OSError):
                pass
        time.sleep(0.1)
    return False
