# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""@castor_tool-decorated functions wrapping Roche sandbox execution."""

from __future__ import annotations

from typing import Any

from castor.gate.decorator import castor_tool

from roche_sandbox.castor._signals import extract_signals
from roche_sandbox.castor._stream_monitor import StreamMonitor, StreamPolicy
from roche_sandbox.client import AsyncRoche
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


def make_execute_code_stream_tool(
    *,
    default_trace_level: str = "standard",
    cost_per_use: float = 1.0,
    provider: str | None = None,
    stream_policy: StreamPolicy | None = None,
) -> Any:
    """Create a streaming execute_code tool with real-time monitoring."""
    policy = stream_policy or StreamPolicy()

    @castor_tool(consumes="compute", cost_per_use=cost_per_use)
    async def execute_code_stream(
        code: str,
        language: str = "auto",
        timeout_secs: int = 30,
    ) -> dict[str, Any]:
        """Execute code with real-time streaming and policy enforcement.

        Uses Roche ExecStream for real-time output. The monitor watches
        each event and can kill the sandbox mid-execution if a policy
        rule fires (memory limit, output limit, blocked operation).
        """
        from roche_sandbox.run import _detect_language, _LANGUAGE_CONFIG

        lang = language if language != "auto" else _detect_language(code)
        image, cmd_builder = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])
        command = cmd_builder(code)

        client = AsyncRoche(provider=provider or "docker")
        sandbox = await client.create(
            image=image,
            timeout_secs=timeout_secs,
            network=False,
            writable=False,
        )

        monitor = StreamMonitor(policy=policy)
        try:
            async for _event in monitor.watch(
                sandbox, command,
                timeout_secs=timeout_secs,
                trace_level=default_trace_level,
            ):
                pass  # events flow through the monitor
        except Exception:
            try:
                await sandbox.destroy()
            except Exception:
                pass
            raise

        if not monitor.killed:
            try:
                await sandbox.destroy()
            except Exception:
                pass

        result = monitor.result()
        result["_code"] = code[:200]
        return result

    return execute_code_stream


# Default tool instances with standard configuration
execute_code = make_execute_code_tool()
execute_shell = make_execute_shell_tool()
execute_code_stream = make_execute_code_stream_tool()
