# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""@castor_tool-decorated functions wrapping Roche sandbox execution."""

from __future__ import annotations

from typing import Any

from castor.gate.decorator import castor_tool

from roche_sandbox.castor._signals import extract_signals
from roche_sandbox.intent import analyze
from roche_sandbox.run import RunOptions, async_run


def make_execute_code_tool(
    *,
    default_trace_level: str = "standard",
    cost_per_use: float = 1.0,
    provider: str | None = None,
) -> Any:
    """Create a configured execute_code tool function."""

    @castor_tool(consumes="compute", cost_per_use=cost_per_use)
    async def execute_code(
        code: str,
        language: str = "auto",
        timeout_secs: int = 30,
    ) -> dict[str, Any]:
        """Execute code in a Roche sandbox with full tracing.

        Returns stdout, stderr, exit_code, and execution signals including
        violations, resource usage, and actual network hosts contacted.
        """
        intent = analyze(code, language)
        opts = RunOptions(
            language=language,
            timeout_secs=timeout_secs,
            trace_level=default_trace_level,
            provider=provider,
        )
        result = await async_run(code, opts)
        signals = extract_signals(result, intent)

        return {
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "signals": {
                "duration_secs": signals.duration_secs,
                "peak_memory_bytes": signals.peak_memory_bytes,
                "network_hosts": signals.network_hosts_contacted,
                "blocked_ops": signals.blocked_operations,
                "file_writes": signals.file_writes,
                "violations": signals.violations,
            },
            "intent": {
                "provider": intent.provider,
                "needs_network": intent.needs_network,
                "network_hosts": intent.network_hosts,
                "needs_writable": intent.needs_writable,
            },
            "_code": code[:200],
            "_tool_name": "execute_code",
        }

    return execute_code


def make_execute_shell_tool(
    *,
    default_trace_level: str = "standard",
    cost_per_use: float = 1.0,
    provider: str | None = None,
) -> Any:
    """Create a configured execute_shell tool function."""

    @castor_tool(consumes="compute", cost_per_use=cost_per_use, destructive=True)
    async def execute_shell(
        command: str,
        timeout_secs: int = 30,
    ) -> dict[str, Any]:
        """Execute a shell command in a Roche sandbox.

        Marked destructive because shell commands have broad capabilities.
        Returns stdout, stderr, exit_code, and execution signals.
        """
        intent = analyze(command, "bash")
        opts = RunOptions(
            language="bash",
            timeout_secs=timeout_secs,
            trace_level=default_trace_level,
            provider=provider,
        )
        result = await async_run(command, opts)
        signals = extract_signals(result, intent)

        return {
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "signals": {
                "duration_secs": signals.duration_secs,
                "peak_memory_bytes": signals.peak_memory_bytes,
                "network_hosts": signals.network_hosts_contacted,
                "blocked_ops": signals.blocked_operations,
                "file_writes": signals.file_writes,
                "violations": signals.violations,
            },
            "intent": {
                "provider": intent.provider,
                "needs_network": intent.needs_network,
                "network_hosts": intent.network_hosts,
                "needs_writable": intent.needs_writable,
            },
            "_code": command[:200],
            "_tool_name": "execute_shell",
        }

    return execute_shell


# Default tool instances with standard configuration
execute_code = make_execute_code_tool()
execute_shell = make_execute_shell_tool()
