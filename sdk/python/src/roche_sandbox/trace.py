from __future__ import annotations
from dataclasses import dataclass, field


class TraceLevel:
    OFF = "off"
    SUMMARY = "summary"
    STANDARD = "standard"
    FULL = "full"


@dataclass
class ResourceUsage:
    peak_memory_bytes: int
    cpu_time_secs: float
    network_rx_bytes: int
    network_tx_bytes: int


@dataclass
class FileAccess:
    path: str
    op: str
    size_bytes: int | None = None


@dataclass
class NetworkAttempt:
    address: str
    protocol: str
    allowed: bool


@dataclass
class BlockedOperation:
    op_type: str
    detail: str


@dataclass
class SyscallEvent:
    name: str
    args: list[str]
    result: str
    timestamp_ms: int


@dataclass
class ResourceSnapshot:
    timestamp_ms: int
    memory_bytes: int
    cpu_percent: float


@dataclass
class ExecutionTrace:
    duration_secs: float
    resource_usage: ResourceUsage
    file_accesses: list[FileAccess] = field(default_factory=list)
    network_attempts: list[NetworkAttempt] = field(default_factory=list)
    blocked_ops: list[BlockedOperation] = field(default_factory=list)
    syscalls: list[SyscallEvent] = field(default_factory=list)
    resource_timeline: list[ResourceSnapshot] = field(default_factory=list)

    def summary(self) -> str:
        parts = [f"{self.duration_secs:.1f}s"]
        parts.append(f"mem {self.resource_usage.peak_memory_bytes // 1_000_000}MB")
        if self.file_accesses:
            reads = sum(1 for f in self.file_accesses if f.op == "read")
            writes = sum(1 for f in self.file_accesses if f.op in ("write", "create"))
            if reads:
                parts.append(f"read {reads} files")
            if writes:
                parts.append(f"wrote {writes} files")
        blocked = len(self.blocked_ops)
        if blocked:
            parts.append(f"blocked {blocked} ops")
        return " | ".join(parts)
