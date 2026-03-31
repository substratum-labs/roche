# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Data models for Roche-Castor integration."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class ExecutionSignals:
    """Structured signals extracted from a Roche ExecutionTrace."""

    duration_secs: float = 0.0
    peak_memory_bytes: int = 0
    network_hosts_contacted: list[str] = field(default_factory=list)
    blocked_operations: list[str] = field(default_factory=list)
    file_writes: list[str] = field(default_factory=list)
    violations: list[str] = field(default_factory=list)


@dataclass
class IntentCheckResult:
    """Result of comparing code intent against Castor capabilities."""

    allowed: bool
    missing_capabilities: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)


@dataclass
class ViolationRecord:
    """A single recorded violation from execution."""

    timestamp: float
    tool_name: str
    violation_type: str  # "blocked_op", "unauthorized_network", "unauthorized_write"
    detail: str
    code_snippet: str = ""


@dataclass
class EscalationPolicy:
    """Configuration for when to escalate to HITL."""

    max_violations_before_hitl: int = 3
    violation_window_secs: float = 300.0
    escalate_on_blocked_ops: bool = True
    escalate_on_network_violations: bool = True
