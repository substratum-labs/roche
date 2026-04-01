# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""L3: Real-time streaming monitor — bridges Roche ExecStream with Castor preemption.

Architecture:
    Roche's exec_stream yields ExecEvent objects in real-time:
      - output: stdout/stderr chunks as they arrive
      - heartbeat: periodic resource snapshots (memory, CPU)
      - result: final exit code + trace

    This monitor wraps exec_stream and evaluates each event against a set
    of StreamPolicy rules. When a rule fires, it can:
      1. Record a violation (→ ViolationTracker → escalation)
      2. Preempt the Castor agent (→ CastorTask.cancel())
      3. Destroy the Roche sandbox (→ kill the running process)

    This is the "spinal reflex arc" — responses happen without LLM involvement.
    The LLM only sees the result after the fact (via checkpoint.preemption_reason).
"""

from __future__ import annotations

import asyncio
import time
from collections.abc import AsyncIterator
from dataclasses import dataclass, field
from typing import Any

from roche_sandbox.castor._types import ViolationRecord
from roche_sandbox.castor._violations import ViolationTracker
from roche_sandbox.types import ExecEvent


# ---------------------------------------------------------------------------
# Stream policy: declarative rules for real-time reactions
# ---------------------------------------------------------------------------


@dataclass
class StreamPolicy:
    """Declarative rules for real-time stream monitoring.

    Each threshold, when breached, triggers an automatic response.
    Set to 0 to disable a specific check.
    """

    # Resource thresholds
    memory_limit_bytes: int = 0
    """Kill sandbox if memory exceeds this. 0 = no limit."""

    cpu_percent_limit: float = 0.0
    """Kill sandbox if CPU exceeds this for consecutive heartbeats. 0 = no limit."""

    cpu_consecutive_beats: int = 3
    """How many consecutive heartbeats must exceed cpu_percent_limit."""

    # Output thresholds
    max_output_bytes: int = 0
    """Kill sandbox if cumulative stdout+stderr exceeds this. 0 = no limit."""

    # Behavioral rules
    kill_on_blocked_op: bool = False
    """Immediately kill sandbox if a blocked operation is detected in stderr."""

    blocked_op_patterns: list[str] = field(default_factory=lambda: [
        "Permission denied",
        "Operation not permitted",
        "Connection refused",
    ])
    """Patterns in stderr that indicate a blocked operation."""

    # Escalation
    preempt_agent_on_kill: bool = True
    """Also preempt the Castor agent when the sandbox is killed."""


# ---------------------------------------------------------------------------
# Stream event — enriched event yielded to callers
# ---------------------------------------------------------------------------


@dataclass
class StreamEvent:
    """An enriched stream event with monitor annotations."""

    raw: ExecEvent
    """The original Roche ExecEvent."""

    violation: str | None = None
    """If this event triggered a violation, its description."""

    killed: bool = False
    """True if the monitor killed the sandbox in response to this event."""


# ---------------------------------------------------------------------------
# StreamMonitor — the core L3 component
# ---------------------------------------------------------------------------


class StreamMonitor:
    """Real-time monitor that wraps Roche exec_stream with policy enforcement.

    Usage inside a @castor_tool::

        monitor = StreamMonitor(policy, tracker)
        async for event in monitor.watch(sandbox, command):
            # events flow through with violations annotated
            if event.killed:
                break  # sandbox was terminated

        return monitor.result()

    Or from the bridge, with Castor preemption::

        monitor = StreamMonitor(policy, tracker, castor_task=task)
        # If a rule fires, both the sandbox and the agent are killed.
    """

    def __init__(
        self,
        policy: StreamPolicy | None = None,
        tracker: ViolationTracker | None = None,
        castor_task: Any | None = None,
    ) -> None:
        self._policy = policy or StreamPolicy()
        self._tracker = tracker
        self._castor_task = castor_task

        # Accumulated state during streaming
        self._output_bytes: int = 0
        self._cpu_high_count: int = 0
        self._stdout_chunks: list[str] = []
        self._stderr_chunks: list[str] = []
        self._exit_code: int | None = None
        self._killed: bool = False
        self._kill_reason: str | None = None
        self._violations: list[str] = []

    async def watch(
        self,
        sandbox: Any,
        command: list[str],
        timeout_secs: int | None = None,
        trace_level: str | None = "standard",
    ) -> AsyncIterator[StreamEvent]:
        """Stream exec events with real-time policy enforcement.

        Yields StreamEvent objects. If a policy rule fires, the sandbox
        is destroyed and the stream ends.
        """
        stream = sandbox.exec_stream(
            command,
            timeout_secs=timeout_secs,
            trace_level=trace_level,
        )

        try:
            async for event in stream:
                stream_event = self._evaluate(event)
                yield stream_event

                if stream_event.killed:
                    # Kill the sandbox
                    try:
                        await sandbox.destroy()
                    except Exception:
                        pass  # best-effort cleanup

                    # Preempt the Castor agent if configured
                    if self._policy.preempt_agent_on_kill and self._castor_task is not None:
                        self._preempt_agent(self._kill_reason or "policy violation")

                    break
        except asyncio.CancelledError:
            # Agent was preempted externally — clean up sandbox
            try:
                await sandbox.destroy()
            except Exception:
                pass
            raise

    def _evaluate(self, event: ExecEvent) -> StreamEvent:
        """Evaluate a single event against policy rules. Pure logic."""
        violation = None
        killed = False

        if event.type == "output":
            data = event.data or b""
            self._output_bytes += len(data)

            # Collect output
            text = data.decode("utf-8", errors="replace") if isinstance(data, bytes) else str(data)
            if event.stream == "stdout":
                self._stdout_chunks.append(text)
            elif event.stream == "stderr":
                self._stderr_chunks.append(text)
                # Check for blocked operation patterns in stderr
                if self._policy.kill_on_blocked_op:
                    for pattern in self._policy.blocked_op_patterns:
                        if pattern in text:
                            violation = f"blocked_op_detected: {pattern}"
                            killed = True
                            self._kill_reason = f"blocked operation: {pattern}"
                            break

            # Check output size limit
            if (
                not killed
                and self._policy.max_output_bytes > 0
                and self._output_bytes > self._policy.max_output_bytes
            ):
                violation = f"output_limit_exceeded: {self._output_bytes} bytes"
                killed = True
                self._kill_reason = f"output limit exceeded ({self._output_bytes} bytes)"

        elif event.type == "heartbeat":
            # Check memory
            if (
                self._policy.memory_limit_bytes > 0
                and event.memory_bytes is not None
                and event.memory_bytes > self._policy.memory_limit_bytes
            ):
                violation = f"memory_limit_exceeded: {event.memory_bytes} bytes"
                killed = True
                self._kill_reason = (
                    f"memory limit exceeded "
                    f"({event.memory_bytes} > {self._policy.memory_limit_bytes})"
                )

            # Check CPU (consecutive beats)
            if self._policy.cpu_percent_limit > 0 and event.cpu_percent is not None:
                if event.cpu_percent > self._policy.cpu_percent_limit:
                    self._cpu_high_count += 1
                else:
                    self._cpu_high_count = 0

                if self._cpu_high_count >= self._policy.cpu_consecutive_beats:
                    violation = f"cpu_limit_exceeded: {event.cpu_percent}%"
                    killed = True
                    self._kill_reason = (
                        f"CPU exceeded {self._policy.cpu_percent_limit}% "
                        f"for {self._cpu_high_count} consecutive heartbeats"
                    )

        elif event.type == "result":
            self._exit_code = event.exit_code

        # Record violation
        if violation:
            self._killed = killed
            self._violations.append(violation)
            if self._tracker:
                self._tracker.record(ViolationRecord(
                    timestamp=time.monotonic(),
                    tool_name="execute_code_stream",
                    violation_type=violation.split(":")[0],
                    detail=violation,
                ))

        return StreamEvent(raw=event, violation=violation, killed=killed)

    def _preempt_agent(self, reason: str) -> None:
        """Preempt the Castor agent task."""
        task = self._castor_task
        if task is None:
            return
        # CastorTask has _runner.preempt() — use it if available
        runner = getattr(task, "_runner", None)
        if runner and hasattr(runner, "preempt"):
            runner.preempt(reason, {"violations": self._violations})

    def result(self) -> dict[str, Any]:
        """Return accumulated result after stream ends."""
        return {
            "exit_code": self._exit_code if not self._killed else -1,
            "stdout": "".join(self._stdout_chunks),
            "stderr": "".join(self._stderr_chunks),
            "killed": self._killed,
            "kill_reason": self._kill_reason,
            "signals": {
                "violations": self._violations,
                "output_bytes": self._output_bytes,
            },
            "_tool_name": "execute_code_stream",
        }

    @property
    def killed(self) -> bool:
        return self._killed

    @property
    def kill_reason(self) -> str | None:
        return self._kill_reason
