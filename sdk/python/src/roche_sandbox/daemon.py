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
