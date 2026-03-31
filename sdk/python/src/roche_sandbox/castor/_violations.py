# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Violation tracker with sliding-window escalation."""

from __future__ import annotations

import time

from roche_sandbox.castor._types import EscalationPolicy, ViolationRecord


class ViolationTracker:
    """Tracks execution violations and determines HITL escalation."""

    def __init__(self, policy: EscalationPolicy | None = None):
        self._policy = policy or EscalationPolicy()
        self._records: list[ViolationRecord] = []

    def record(self, violation: ViolationRecord) -> None:
        """Record a violation."""
        self._records.append(violation)

    def should_escalate(self) -> bool:
        """Check if violations in the current window exceed the threshold."""
        recent = self.recent_violations()
        if len(recent) >= self._policy.max_violations_before_hitl:
            return True

        if not recent:
            return False

        last = recent[-1]
        if self._policy.escalate_on_blocked_ops and last.violation_type == "blocked_op":
            return True
        if self._policy.escalate_on_network_violations and last.violation_type == "unauthorized_network":
            return True

        return False

    def recent_violations(self, window_secs: float | None = None) -> list[ViolationRecord]:
        """Return violations within the sliding window."""
        window = window_secs or self._policy.violation_window_secs
        cutoff = time.time() - window
        return [r for r in self._records if r.timestamp >= cutoff]

    @property
    def total_count(self) -> int:
        return len(self._records)

    def reset(self) -> None:
        """Clear all recorded violations."""
        self._records.clear()
