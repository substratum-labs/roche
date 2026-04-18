# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Intent-aware pre-check: compare Roche code analysis against Castor capabilities."""

from __future__ import annotations

from typing import Any

from roche_sandbox.castor._types import IntentCheckResult
from roche_sandbox.intent import CodeIntent, analyze


def check_intent_against_capabilities(
    code: str,
    language: str,
    capabilities: dict[str, Any],
) -> IntentCheckResult:
    """Analyze code intent and check if the agent has sufficient Castor budgets.

    Castor convention: missing resource = unlimited (no enforcement).
    So we only flag resources that ARE tracked but have insufficient budget.

    Args:
        code: Source code to analyze.
        language: Language hint ('python', 'node', 'bash', 'auto').
        capabilities: Castor budget dict (resource_type -> Budget object).
    """
    intent = analyze(code, language)
    missing: list[str] = []
    warnings: list[str] = []

    # Check network capability
    if intent.needs_network:
        cap = capabilities.get("network")
        if cap is not None and _remaining(cap) <= 0:
            missing.append("network")

    # Check filesystem capability
    if intent.needs_writable:
        cap = capabilities.get("filesystem")
        if cap is not None and _remaining(cap) <= 0:
            missing.append("filesystem")

    # Check compute budget
    cap = capabilities.get("compute")
    if cap is not None and _remaining(cap) <= 0:
        missing.append("compute")

    # Soft warnings
    if intent.memory_hint:
        warnings.append(f"heavy memory usage detected (hint: {intent.memory_hint})")
    if intent.needs_packages:
        warnings.append(f"package installation detected ({intent.package_manager})")

    return IntentCheckResult(
        allowed=len(missing) == 0,
        missing_capabilities=missing,
        warnings=warnings,
    )


def _remaining(cap: Any) -> float:
    """Extract remaining budget from a Castor Capability object."""
    if hasattr(cap, "max_budget") and hasattr(cap, "current_usage"):
        return cap.max_budget - cap.current_usage
    if isinstance(cap, dict):
        return cap.get("max_budget", float("inf")) - cap.get("current_usage", 0)
    return float("inf")
