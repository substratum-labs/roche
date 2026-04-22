# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Capability Wallet — the agent sandbox specification.

A wallet declares what an agent is allowed to do inside a sandbox.
It is Roche's OCI runtime spec equivalent, but at the capability level.

Docker isolates processes (namespaces + cgroups).
Roche isolates agents (capability wallet = what the agent can *do*).

Usage:
    from roche_sandbox.wallet import SandboxGrant, run_with_wallet

    wallet = SandboxGrant(
        network=NetworkCap(enabled=True, allowed_hosts=["api.openai.com"]),
        compute=ComputeCap(max_exec_count=10, max_duration_secs=60),
    )
    result, usage = await run_with_wallet(wallet, "print(2+2)")
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class NetworkCap:
    """Network access capabilities."""
    enabled: bool = False
    allowed_hosts: list[str] = field(default_factory=list)
    max_egress_bytes: int = 0  # 0 = unlimited


@dataclass
class FilesystemCap:
    """Filesystem write capabilities."""
    writable: bool = False
    writable_paths: list[str] = field(default_factory=list)
    max_write_bytes: int = 0  # 0 = unlimited


@dataclass
class ComputeCap:
    """Compute budget."""
    max_exec_count: int = 0  # 0 = unlimited
    max_duration_secs: int = 0  # 0 = unlimited
    max_memory_bytes: int = 0  # 0 = provider default
    max_cpus: float = 0  # 0 = provider default


@dataclass
class SecretsCap:
    """Secret/environment variable access."""
    allowed_env_keys: list[str] = field(default_factory=list)


@dataclass
class OutputCap:
    """Output limits."""
    max_stdout_bytes: int = 0  # 0 = unlimited
    max_stderr_bytes: int = 0  # 0 = unlimited


@dataclass
class SandboxGrant:
    """Declares what an agent can do inside a sandbox.

    Default = fully locked: no network, no writes, no secrets.
    Explicitly enable what the agent needs.
    """
    network: NetworkCap = field(default_factory=NetworkCap)
    filesystem: FilesystemCap = field(default_factory=FilesystemCap)
    compute: ComputeCap = field(default_factory=ComputeCap)
    secrets: SecretsCap = field(default_factory=SecretsCap)
    output: OutputCap = field(default_factory=OutputCap)
    provider: str = ""
    image: str = ""
    metadata: dict[str, str] = field(default_factory=dict)


@dataclass
class UsageReport:
    """What actually happened during execution."""
    exec_count: int = 0
    duration_secs: float = 0.0
    stdout_bytes: int = 0
    stderr_bytes: int = 0
    network_hosts_contacted: list[str] = field(default_factory=list)
    network_egress_bytes: int = 0
    fs_write_bytes: int = 0
    fs_paths_written: list[str] = field(default_factory=list)
    peak_memory_bytes: int = 0
    violations: list[str] = field(default_factory=list)


# ---------------------------------------------------------------------------
# Wallet-based execution
# ---------------------------------------------------------------------------


async def run_with_wallet(
    wallet: SandboxGrant,
    code: str,
    *,
    language: str = "auto",
) -> tuple[object, UsageReport]:
    """Execute code with a capability wallet. Returns (RunResult, UsageReport).

    The wallet defines what the code is allowed to do. Roche creates a sandbox
    matching the wallet's capabilities, executes the code, and returns a usage
    report showing what was actually consumed.

    The wallet defines what the code is allowed to do. Roche creates a sandbox
    matching the wallet's constraints, executes the code, and returns a usage
    report showing what was actually consumed.
    """
    import os

    from roche_sandbox.run import async_run, RunOptions

    # Wallet → RunOptions
    opts = RunOptions(
        language=language,
        timeout_secs=wallet.compute.max_duration_secs or 30,
        network=wallet.network.enabled or None,
        network_allowlist=wallet.network.allowed_hosts or None,
        writable=wallet.filesystem.writable or None,
        fs_paths=wallet.filesystem.writable_paths or None,
        memory=_bytes_to_memory_str(wallet.compute.max_memory_bytes) if wallet.compute.max_memory_bytes else None,
        provider=wallet.provider or None,
    )

    # Inject allowed secrets into environment (temporarily)
    original_env = {}
    for key in wallet.secrets.allowed_env_keys:
        if key in os.environ:
            original_env[key] = os.environ[key]

    try:
        result = await async_run(code, opts)
    finally:
        pass  # env is read by the sandbox, not modified here

    # Build usage report from result
    trace = result.trace
    usage = UsageReport(
        exec_count=1,
        duration_secs=trace.duration_secs if trace else 0.0,
        stdout_bytes=len(result.stdout.encode()),
        stderr_bytes=len(result.stderr.encode()),
        peak_memory_bytes=trace.resource_usage.peak_memory_bytes if trace and trace.resource_usage else 0,
    )

    if trace:
        if trace.network_attempts:
            usage.network_hosts_contacted = list({n.address for n in trace.network_attempts})
        if trace.file_accesses:
            writes = [f for f in trace.file_accesses if f.op in ("write", "create")]
            usage.fs_paths_written = list({f.path for f in writes})
            usage.fs_write_bytes = sum(f.size_bytes or 0 for f in writes)
        if trace.blocked_ops:
            usage.violations = [f"{b.op_type}: {b.detail}" for b in trace.blocked_ops]

    return result, usage


def _bytes_to_memory_str(b: int) -> str:
    """Convert bytes to Docker memory string."""
    if b >= 1024 * 1024 * 1024:
        return f"{b // (1024 * 1024 * 1024)}g"
    return f"{b // (1024 * 1024)}m"


# ---------------------------------------------------------------------------
# Castor bridge: Castor Budget → Roche Wallet
# ---------------------------------------------------------------------------


def from_castor_budgets(
    budgets: dict[str, object],
    tool_meta: object | None = None,
) -> SandboxGrant:
    """Roche-side helper: derive a partial SandboxGrant from Castor budget signals.

    This is NOT a jointly-owned protocol. Castor's Budget API only provides
    quantitative resource counters (compute / network / disk / api). A real
    capability wallet also needs allowlists (allowed_hosts, writable_paths)
    which Castor cannot provide today. Callers must populate those separately
    (e.g. via intent analysis or explicit configuration).

    The conversion is lossy by design:
    - "compute" budget → compute.max_exec_count (quantitative, direct)
    - "network" budget → network.enabled=True (but allowed_hosts stays empty)
    - "disk"/"filesystem" budget → filesystem.writable=True (but writable_paths stays empty)
    - "api" budget → network.enabled=True (API calls need network)
    - "memory" budget → compute.max_memory_bytes (treated as MB)

    Args:
        budgets: Castor budget dict (resource_type → Budget object with
                 max_budget/current_usage fields).
        tool_meta: Optional Castor ToolMetadata for additional hints.
    """
    wallet = SandboxGrant()

    for resource_type, budget in budgets.items():
        remaining = _budget_remaining(budget)
        if remaining <= 0:
            continue

        if resource_type == "compute":
            wallet.compute.max_exec_count = max(1, int(remaining))
        elif resource_type == "network":
            wallet.network.enabled = True
        elif resource_type in ("disk", "filesystem"):
            wallet.filesystem.writable = True
        elif resource_type == "api":
            wallet.network.enabled = True
        elif resource_type == "memory":
            wallet.compute.max_memory_bytes = int(remaining * 1024 * 1024)

    return wallet


# Keep old name as alias for backward compatibility
from_castor_capabilities = from_castor_budgets


def to_castor_usage(usage: UsageReport) -> dict[str, float]:
    """Translate a Roche UsageReport into Castor budget deductions.

    Returns a dict of {resource_type: amount_to_deduct}.
    The caller (typically the Castor-Roche bridge) uses these values
    to call budget_mgr.deduct().
    """
    deductions: dict[str, float] = {}

    if usage.exec_count > 0:
        deductions["compute"] = float(usage.exec_count)

    if usage.network_egress_bytes > 0:
        deductions["network"] = usage.network_egress_bytes / (1024 * 1024)  # MB

    if usage.fs_write_bytes > 0:
        deductions["disk"] = usage.fs_write_bytes / (1024 * 1024)  # MB

    return deductions


def _budget_remaining(budget: object) -> float:
    """Extract remaining budget from a Castor Budget object."""
    if hasattr(budget, "max_budget") and hasattr(budget, "current_usage"):
        return budget.max_budget - budget.current_usage
    if isinstance(budget, dict):
        return budget.get("max_budget", float("inf")) - budget.get("current_usage", 0)
    return float("inf")
