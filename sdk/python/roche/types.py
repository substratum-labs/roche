"""Core data types for the Roche Python SDK."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class SandboxConfig:
    """Configuration for creating a new sandbox."""

    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout: int = 300
    network: bool = False
    writable: bool = False
    env: dict[str, str] = field(default_factory=dict)


@dataclass
class ExecOutput:
    """Output from executing a command in a sandbox."""

    exit_code: int
    stdout: str
    stderr: str
