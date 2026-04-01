# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

from roche_sandbox.trace import ExecutionTrace

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
    network_allowlist: list[str] = field(default_factory=list)
    fs_paths: list[str] = field(default_factory=list)


@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str
    trace: ExecutionTrace | None = None


@dataclass
class ExecEvent:
    """A single event from a streaming exec."""
    type: Literal["output", "heartbeat", "result"]
    # output fields
    stream: str | None = None  # "stdout" or "stderr"
    data: bytes | None = None
    # heartbeat fields
    elapsed_ms: int | None = None
    memory_bytes: int | None = None
    cpu_percent: float | None = None
    # result fields
    exit_code: int | None = None
    trace: ExecutionTrace | None = None


@dataclass
class RetryPolicy:
    max_attempts: int = 1
    backoff: str = "none"
    initial_delay_ms: int = 1000
    retry_on: list[str] = field(default_factory=list)


@dataclass
class OutputLimit:
    max_bytes: int = 0
    action: str = "truncate"


@dataclass
class SandboxInfo:
    id: str
    status: SandboxStatus
    provider: str
    image: str
    expires_at: int | None = None


@dataclass
class PoolInfo:
    provider: str
    image: str
    idle_count: int
    active_count: int
    max_idle: int
    max_total: int


@dataclass
class Budget:
    max_execs: int = 0
    max_total_secs: int = 0
    max_output_bytes: int = 0


@dataclass
class BudgetUsage:
    exec_count: int = 0
    total_secs: float = 0.0
    output_bytes: int = 0


@dataclass
class DynamicPermissions:
    network: bool = False
    network_allowlist: list[str] = field(default_factory=list)
    writable: bool = False
    fs_paths: list[str] = field(default_factory=list)


@dataclass
class SessionInfo:
    session_id: str
    sandbox_id: str
    provider: str
    permissions: DynamicPermissions
    budget: Budget
    usage: BudgetUsage
    created_at_ms: int
