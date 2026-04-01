# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""RocheCastorBridge: main integration class between Roche and Castor."""

from __future__ import annotations

import time
from collections.abc import Callable
from typing import Any

from roche_sandbox.castor._intent_gate import check_intent_against_capabilities
from roche_sandbox.castor._stream_monitor import StreamMonitor, StreamPolicy
from roche_sandbox.castor._tools import (
    make_execute_code_tool,
    make_execute_code_stream_tool,
    make_execute_shell_tool,
    make_workspace_exec_tool,
)
from roche_sandbox.castor._workspace import Workspace, WorkspaceManager
from roche_sandbox.castor._types import (
    EscalationPolicy,
    IntentCheckResult,
    ViolationRecord,
)
from roche_sandbox.castor._violations import ViolationTracker


def _classify_violation(desc: str) -> str:
    """Classify a violation description string into a violation type."""
    d = desc.lower()
    if d.startswith("blocked"):
        return "blocked_op"
    if d.startswith("unauthorized_network"):
        return "unauthorized_network"
    if d.startswith("unauthorized_write"):
        return "unauthorized_write"
    if d.startswith("output_limit"):
        return "output_limit_exceeded"
    if d.startswith("memory_limit"):
        return "memory_limit_exceeded"
    if d.startswith("cpu_limit"):
        return "cpu_limit_exceeded"
    return "unknown"


def roche_tools(**kwargs: Any) -> list[Callable]:
    """Return Roche sandbox tools ready for Castor. Simplest integration.

    Usage::

        from roche_sandbox.castor import roche_tools
        kernel = Castor(tools=roche_tools() + my_tools, default_budgets={"compute": 10})

    All kwargs are forwarded to RocheCastorBridge.
    """
    bridge = RocheCastorBridge(**kwargs)
    return bridge.tools


def roche_castor(
    budgets: dict[str, float] | None = None,
    **kwargs: Any,
) -> Any:
    """Create a Castor kernel with Roche tools pre-registered. One-liner.

    Usage::

        from roche_sandbox.castor import roche_castor
        kernel = roche_castor(budgets={"compute": 10})
        cp = await kernel.run(my_agent)

    Args:
        budgets: Default Castor budgets. If None, defaults to {"compute": 10.0}.
        **kwargs: Forwarded to RocheCastorBridge (e.g. stream_policy, provider).
    """
    from castor import Castor

    bridge = RocheCastorBridge(**kwargs)
    return Castor(
        tools=bridge.tools,
        default_budgets=budgets or {"compute": 10.0},
    )


class RocheCastorBridge:
    """Bridges Roche sandbox execution with Castor's security kernel.

    For most users, use ``roche_castor()`` or ``roche_tools()`` instead.

    Advanced usage::

        bridge = RocheCastorBridge(stream_policy=StreamPolicy(...))
        kernel = Castor(tools=bridge.tools + other_tools)
    """

    def __init__(
        self,
        *,
        escalation_policy: EscalationPolicy | None = None,
        default_trace_level: str = "standard",
        compute_cost: float = 1.0,
        provider: str | None = None,
        intent_pre_check: bool = True,
        stream_policy: StreamPolicy | None = None,
    ) -> None:
        self._tracker = ViolationTracker(escalation_policy or EscalationPolicy())
        self._intent_pre_check = intent_pre_check
        self._stream_policy = stream_policy
        self._workspace_mgr = WorkspaceManager(provider=provider)

        self._execute_code = make_execute_code_tool(
            default_trace_level=default_trace_level,
            cost_per_use=compute_cost,
            provider=provider,
        )
        self._execute_shell = make_execute_shell_tool(
            default_trace_level=default_trace_level,
            cost_per_use=compute_cost,
            provider=provider,
        )
        self._execute_code_stream = make_execute_code_stream_tool(
            default_trace_level=default_trace_level,
            cost_per_use=compute_cost,
            provider=provider,
            stream_policy=stream_policy,
        )

        self._execute_in_workspace = make_workspace_exec_tool(
            workspace_manager=self._workspace_mgr,
            default_trace_level=default_trace_level,
            cost_per_use=compute_cost,
        )

        # Wrap tools to intercept results for violation tracking
        self._execute_code = self._wrap_with_tracking(self._execute_code)
        self._execute_shell = self._wrap_with_tracking(self._execute_shell)
        self._execute_code_stream = self._wrap_with_tracking(self._execute_code_stream)
        self._execute_in_workspace = self._wrap_with_tracking(self._execute_in_workspace)

    def _wrap_with_tracking(self, tool_fn: Callable) -> Callable:
        """Wrap a tool function to intercept results for violation tracking."""
        bridge = self
        original_meta = getattr(tool_fn, "_castor_metadata", None)

        async def wrapped(**kwargs: Any) -> dict[str, Any]:
            result = await tool_fn(**kwargs)
            return bridge.process_result(result)

        # Preserve castor metadata so the tool is still recognized
        if original_meta is not None:
            original_meta.func = wrapped
            wrapped._castor_metadata = original_meta  # type: ignore[attr-defined]
        wrapped.__name__ = tool_fn.__name__  # type: ignore[attr-defined]
        wrapped.__doc__ = tool_fn.__doc__
        return wrapped

    @property
    def tools(self) -> list[Callable]:
        """Return tool functions for passing to Castor(tools=...).

        Usage::

            bridge = RocheCastorBridge()
            kernel = Castor(tools=bridge.tools + other_tools)
        """
        tools: list[Callable] = [self._execute_code, self._execute_shell, self._execute_code_stream, self._execute_in_workspace]
        if self._intent_pre_check:
            tools.append(self._check_intent_tool)
        return tools

    @property
    def tracker(self) -> ViolationTracker:
        """Access the violation tracker for external monitoring."""
        return self._tracker

    def create_monitor(
        self,
        castor_task: Any | None = None,
        policy: StreamPolicy | None = None,
    ) -> StreamMonitor:
        """Create a StreamMonitor wired to this bridge's tracker and policy.

        For advanced use — when you want to manually control streaming execution
        and have the monitor preempt a specific CastorTask on violations::

            task = await kernel.run_async(my_agent)
            monitor = bridge.create_monitor(castor_task=task)
            async for event in monitor.watch(sandbox, command):
                ...  # events flow, monitor may kill sandbox + preempt agent
        """
        return StreamMonitor(
            policy=policy or self._stream_policy or StreamPolicy(),
            tracker=self._tracker,
            castor_task=castor_task,
        )

    async def workspace(self, **kwargs: Any) -> Workspace:
        """Create a shared workspace for multi-agent collaboration.

        The workspace is a long-lived sandbox that multiple agents can exec into.
        Files and state persist between calls. Use as async context manager::

            async with await bridge.workspace(writable=True) as ws:
                await proxy.syscall("execute_in_workspace",
                    code="open('/tmp/x','w').write('hello')",
                    workspace_id=ws.id)
        """
        return await self._workspace_mgr.create(**kwargs)

    @property
    def workspaces(self) -> WorkspaceManager:
        """Access the workspace manager directly."""
        return self._workspace_mgr

    def check_intent(
        self,
        code: str,
        language: str = "auto",
        capabilities: dict[str, Any] | None = None,
    ) -> IntentCheckResult:
        """Pre-check code intent against capabilities (convenience method)."""
        return check_intent_against_capabilities(code, language, capabilities or {})

    def process_result(self, result: dict[str, Any]) -> dict[str, Any]:
        """Post-process execution result: record violations, check escalation.

        Called internally by wrapped tools. Records violations and annotates
        the result with an ``escalation_needed`` flag.
        """
        signals = result.get("signals", {})
        violations = signals.get("violations", [])

        for v_desc in violations:
            record = ViolationRecord(
                timestamp=time.monotonic(),
                tool_name=result.get("_tool_name", "unknown"),
                violation_type=_classify_violation(v_desc),
                detail=v_desc,
                code_snippet=result.get("_code", "")[:200],
            )
            self._tracker.record(record)

        result["escalation_needed"] = self._tracker.should_escalate()
        return result

    @staticmethod
    async def _check_intent_tool_impl(code: str, language: str = "auto") -> dict[str, Any]:
        """Pre-check code intent without executing. Returns capability requirements."""
        from roche_sandbox.intent import analyze

        intent = analyze(code, language)
        return {
            "provider": intent.provider,
            "needs_network": intent.needs_network,
            "network_hosts": intent.network_hosts,
            "needs_writable": intent.needs_writable,
            "writable_paths": intent.writable_paths,
            "needs_packages": intent.needs_packages,
            "package_manager": intent.package_manager,
            "memory_hint": intent.memory_hint,
            "confidence": intent.confidence,
            "reasoning": intent.reasoning,
        }

    @property
    def _check_intent_tool(self) -> Callable:
        """Lazy-create the check_code_intent tool with @castor_tool decorator."""
        if not hasattr(self, "_cached_check_intent"):
            from castor.gate.decorator import castor_tool

            @castor_tool(consumes="_default", cost_per_use=0.0)
            async def check_code_intent(code: str, language: str = "auto") -> dict[str, Any]:
                """Pre-check code intent without executing. Free (no budget cost)."""
                return await RocheCastorBridge._check_intent_tool_impl(code, language)

            self._cached_check_intent = check_code_intent
        return self._cached_check_intent
