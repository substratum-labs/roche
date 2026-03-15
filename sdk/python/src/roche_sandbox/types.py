from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

SandboxStatus = Literal["running", "paused", "stopped", "failed"]


@dataclass
class Mount:
    host_path: str
    container_path: str
    readonly: bool = True


@dataclass
class SandboxConfig:
    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout_secs: int = 300
    network: bool = False
    writable: bool = False
    env: dict[str, str] = field(default_factory=dict)
    mounts: list[Mount] = field(default_factory=list)
    kernel: str | None = None
    rootfs: str | None = None


@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str


@dataclass
class SandboxInfo:
    id: str
    status: SandboxStatus
    provider: str
    image: str
    expires_at: int | None = None
