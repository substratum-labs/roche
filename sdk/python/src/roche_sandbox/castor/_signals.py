# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Extract security signals from Roche execution traces.

Pure function — no I/O, mirrors Castor's convention of pure decision functions.
"""

from __future__ import annotations

from roche_sandbox.castor._types import ExecutionSignals
from roche_sandbox.intent import CodeIntent
from roche_sandbox.types import ExecOutput


def extract_signals(result: ExecOutput, intent: CodeIntent) -> ExecutionSignals:
    """Extract security-relevant signals from a Roche execution result.

    Compares actual execution behavior (from trace) against predicted intent.
    Blocked operations and unauthorized contacts become violations.
    """
    trace = result.trace
    if trace is None:
        return ExecutionSignals(violations=_base_violations(result))

    # Extract raw signals
    hosts = [n.address for n in trace.network_attempts] if trace.network_attempts else []
    blocked = [f"{b.op_type}: {b.detail}" for b in trace.blocked_ops] if trace.blocked_ops else []
    writes = [
        f.path
        for f in trace.file_accesses
        if f.op in ("write", "create")
    ] if trace.file_accesses else []

    # Build violations
    violations: list[str] = []

    # Every blocked op is a violation
    for b in blocked:
        violations.append(f"blocked: {b}")

    # Network contacts not predicted by intent
    predicted_hosts = set(intent.network_hosts)
    for host in hosts:
        # Check if any predicted host is a suffix match (api.openai.com matches openai.com)
        if not any(host == ph or host.endswith("." + ph) for ph in predicted_hosts):
            violations.append(f"unauthorized_network: {host}")

    # File writes not predicted by intent
    predicted_paths = set(intent.writable_paths)
    for path in writes:
        if not any(path.startswith(pp) for pp in predicted_paths):
            violations.append(f"unauthorized_write: {path}")

    return ExecutionSignals(
        duration_secs=trace.duration_secs,
        peak_memory_bytes=trace.resource_usage.peak_memory_bytes if trace.resource_usage else 0,
        network_hosts_contacted=hosts,
        blocked_operations=blocked,
        file_writes=writes,
        violations=violations,
    )


def _base_violations(result: ExecOutput) -> list[str]:
    """Minimal violations when no trace is available."""
    if result.exit_code != 0 and result.stderr:
        return [f"nonzero_exit: code={result.exit_code}"]
    return []
