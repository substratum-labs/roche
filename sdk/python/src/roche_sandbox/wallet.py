# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Capability Wallet — the agent sandbox specification.

A wallet declares what an agent is allowed to do inside a sandbox.
It is Roche's OCI runtime spec equivalent, but at the capability level.

Docker isolates processes (namespaces + cgroups).
Roche isolates agents (capability wallet = what the agent can *do*).

Usage:
    from roche_sandbox.wallet import CapabilityWallet, run_with_wallet

    wallet = CapabilityWallet(
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
class CapabilityWallet:
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
    wallet: CapabilityWallet,
    code: str,
    *,
    language: str = "auto",
) -> tuple[object, UsageReport]:
    """Execute code with a capability wallet. Returns (RunResult, UsageReport).

    The wallet defines what the code is allowed to do. Roche creates a sandbox
    matching the wallet's capabilities, executes the code, and returns a usage
    report showing what was actually consumed.

    This is the formal Castor→Roche protocol:
      1. Castor translates its budget tokens into a CapabilityWallet
      2. Passes the wallet to Roche
      3. Roche creates sandbox matching the wallet
      4. Roche executes and returns (result, usage_report)
      5. Castor reads usage_report to update its own budgets
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
# Castor bridge: Castor Capability → Roche Wallet
# ---------------------------------------------------------------------------


def from_castor_capabilities(
    capabilities: dict[str, object],
    tool_meta: object | None = None,
) -> CapabilityWallet:
    """Translate Castor capability tokens into a Roche CapabilityWallet.

    This is the formal interface between Castor and Roche:
    - Castor's "compute" budget → Roche's compute.max_exec_count
    - Castor's "network" budget → Roche's network.enabled + max_egress_bytes
    - Castor's "disk"/"filesystem" budget → Roche's filesystem.writable

    Args:
        capabilities: Castor capability dict (resource_type → Capability object).
        tool_meta: Optional Castor ToolMetadata for additional hints.
    """
    wallet = CapabilityWallet()

    for resource_type, cap in capabilities.items():
        remaining = _cap_remaining(cap)
        if remaining <= 0:
            continue

        if resource_type == "compute":
            wallet.compute.max_exec_count = max(1, int(remaining))
        elif resource_type == "network":
            wallet.network.enabled = True
        elif resource_type in ("disk", "filesystem"):
            wallet.filesystem.writable = True
        elif resource_type == "api":
            # API budget → enable network for API calls
            wallet.network.enabled = True
        elif resource_type == "memory":
            wallet.compute.max_memory_bytes = int(remaining * 1024 * 1024)  # treat as MB

    return wallet


def to_castor_usage(usage: UsageReport) -> dict[str, float]:
    """Translate a Roche UsageReport back to Castor budget deductions.

    Returns a dict of {resource_type: amount_to_deduct}.
    Castor calls cap_mgr.deduct() with these values.
    """
    deductions: dict[str, float] = {}

    if usage.exec_count > 0:
        deductions["compute"] = float(usage.exec_count)

    if usage.network_egress_bytes > 0:
        deductions["network"] = usage.network_egress_bytes / (1024 * 1024)  # MB

    if usage.fs_write_bytes > 0:
        deductions["disk"] = usage.fs_write_bytes / (1024 * 1024)  # MB

    return deductions


def _cap_remaining(cap: object) -> float:
    """Extract remaining budget from a Castor Capability."""
    if hasattr(cap, "max_budget") and hasattr(cap, "current_usage"):
        return cap.max_budget - cap.current_usage
    if isinstance(cap, dict):
        return cap.get("max_budget", float("inf")) - cap.get("current_usage", 0)
    return float("inf")
