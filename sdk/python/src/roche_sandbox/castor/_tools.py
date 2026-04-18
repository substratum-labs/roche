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
from roche_sandbox.wallet import (
    CapabilityWallet, NetworkCap, FilesystemCap, ComputeCap, OutputCap,
    run_with_wallet, from_castor_budgets, to_castor_usage, UsageReport,
)


def _get_castor_budgets() -> dict[str, Any] | None:
    """Try to get Castor budgets from the current proxy context."""
    try:
        from castor.lib._context import get_proxy
        proxy = get_proxy()
        return proxy.checkpoint.capabilities  # Castor calls these "capabilities" but they're budget counters
    except (RuntimeError, ImportError):
        return None


def _build_wallet(
    code: str,
    language: str,
    timeout_secs: int,
    provider: str | None,
) -> CapabilityWallet:
    """Build a wallet from intent analysis + Castor budgets (if available)."""
    intent = analyze(code, language)
    budgets = _get_castor_budgets()

    if budgets:
        # Inside Castor — derive partial wallet from budget counters
        wallet = from_castor_budgets(budgets)
    else:
        # Standalone — build wallet from intent
        wallet = CapabilityWallet()

    # Intent analysis enriches the wallet
    if intent.needs_network:
        wallet.network.enabled = True
    if intent.network_hosts:
        for host in intent.network_hosts:
            if host not in wallet.network.allowed_hosts:
                wallet.network.allowed_hosts.append(host)
    if intent.needs_writable:
        wallet.filesystem.writable = True
    if intent.writable_paths:
        for p in intent.writable_paths:
            if p not in wallet.filesystem.writable_paths:
                wallet.filesystem.writable_paths.append(p)
    if intent.memory_hint:
        mb = int(intent.memory_hint.rstrip("m"))
        wallet.compute.max_memory_bytes = max(wallet.compute.max_memory_bytes, mb * 1024 * 1024)

    wallet.compute.max_duration_secs = max(wallet.compute.max_duration_secs, timeout_secs)
    if provider:
        wallet.provider = provider

    return wallet


def _wallet_result(result: Any, usage: UsageReport, intent: Any, code: str, tool_name: str) -> dict[str, Any]:
    """Format a standard tool response from wallet execution."""
    return {
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "signals": {
            "duration_secs": usage.duration_secs,
            "peak_memory_bytes": usage.peak_memory_bytes,
            "network_hosts": usage.network_hosts_contacted,
            "blocked_ops": [],
            "file_writes": usage.fs_paths_written,
            "violations": usage.violations,
        },
        "intent": {
            "provider": intent.provider,
            "needs_network": intent.needs_network,
            "network_hosts": intent.network_hosts,
            "needs_writable": intent.needs_writable,
        },
        "usage": {
            "exec_count": usage.exec_count,
            "duration_secs": usage.duration_secs,
            "stdout_bytes": usage.stdout_bytes,
            "stderr_bytes": usage.stderr_bytes,
        },
        "_code": code[:200],
        "_tool_name": tool_name,
    }


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
        """Execute code in a Roche sandbox via Capability Wallet.

        Inside Castor: wallet built from Castor capabilities + intent analysis.
        Standalone: wallet built from intent analysis only.
        """
        intent = analyze(code, language)
        wallet = _build_wallet(code, language, timeout_secs, provider)
        result, usage = await run_with_wallet(wallet, code, language=language)
        return _wallet_result(result, usage, intent, code, "execute_code")

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
        """Execute a shell command in a Roche sandbox via Capability Wallet.

        Marked destructive because shell commands have broad capabilities.
        """
        intent = analyze(command, "bash")
        wallet = _build_wallet(command, "bash", timeout_secs, provider)
        result, usage = await run_with_wallet(wallet, command, language="bash")
        return _wallet_result(result, usage, intent, command, "execute_shell")

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
        finally:
            # Always destroy sandbox — monitor.watch may or may not have killed it
            try:
                await sandbox.destroy()
            except Exception:
                pass

        result = monitor.result()
        result["_code"] = code[:200]
        return result

    return execute_code_stream


def make_workspace_exec_tool(
    *,
    workspace_manager: Any = None,
    default_trace_level: str = "standard",
    cost_per_use: float = 1.0,
) -> Any:
    """Create a tool that executes code in a shared workspace sandbox."""
    from roche_sandbox.castor._workspace import WorkspaceManager

    mgr = workspace_manager

    @castor_tool(consumes="compute", cost_per_use=cost_per_use)
    async def execute_in_workspace(
        code: str,
        workspace_id: str,
        language: str = "auto",
        timeout_secs: int = 30,
    ) -> dict[str, Any]:
        """Execute code in a shared workspace sandbox.

        The workspace must be created first via the bridge. Multiple agents
        can exec into the same workspace — files and state persist between calls.
        """
        from roche_sandbox.run import _detect_language, _LANGUAGE_CONFIG

        _empty_signals = {
            "duration_secs": 0.0, "peak_memory_bytes": 0,
            "network_hosts": [], "blocked_ops": [], "file_writes": [],
            "violations": [],
        }
        _empty_intent = {"provider": "docker", "needs_network": False, "network_hosts": [], "needs_writable": False}

        if mgr is None:
            s = {**_empty_signals, "violations": ["workspace_manager_unavailable"]}
            return {"exit_code": 1, "stdout": "", "stderr": "No workspace manager configured",
                    "workspace_id": workspace_id, "signals": s, "intent": _empty_intent,
                    "_tool_name": "execute_in_workspace", "_code": code[:200]}

        ws = mgr.get(workspace_id)
        if ws is None:
            s = {**_empty_signals, "violations": [f"workspace_not_found:{workspace_id}"]}
            return {"exit_code": 1, "stdout": "", "stderr": f"Workspace not found: {workspace_id}",
                    "workspace_id": workspace_id, "signals": s, "intent": _empty_intent,
                    "_tool_name": "execute_in_workspace", "_code": code[:200]}

        lang = language if language != "auto" else _detect_language(code)
        _, cmd_builder = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])
        command = cmd_builder(code)

        intent = analyze(code, lang)
        result = await ws.exec(command, timeout_secs=timeout_secs, trace_level=default_trace_level)
        signals = extract_signals(result, intent)

        return {
            "exit_code": result.exit_code,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "workspace_id": workspace_id,
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
            "_tool_name": "execute_in_workspace",
        }

    return execute_in_workspace


# Default tool instances with standard configuration
execute_code = make_execute_code_tool()
execute_shell = make_execute_shell_tool()
execute_code_stream = make_execute_code_stream_tool()
