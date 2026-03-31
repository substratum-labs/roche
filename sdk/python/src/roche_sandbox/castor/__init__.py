# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche-Castor integration: secure sandbox execution under Castor's security kernel.

Requires the ``castor`` package. Install with::

    pip install roche-sandbox[castor]
"""

from __future__ import annotations

try:
    import castor as _castor  # noqa: F401
except ImportError:
    raise ImportError(
        "roche_sandbox.castor requires the 'castor' package. "
        "Install with: pip install roche-sandbox[castor]"
    ) from None

from roche_sandbox.castor._bridge import RocheCastorBridge
from roche_sandbox.castor._intent_gate import check_intent_against_capabilities
from roche_sandbox.castor._signals import extract_signals
from roche_sandbox.castor._tools import (
    execute_code,
    execute_shell,
    make_execute_code_tool,
    make_execute_shell_tool,
)
from roche_sandbox.castor._types import (
    EscalationPolicy,
    ExecutionSignals,
    IntentCheckResult,
    ViolationRecord,
)
from roche_sandbox.castor._violations import ViolationTracker

__all__ = [
    "RocheCastorBridge",
    "execute_code",
    "execute_shell",
    "make_execute_code_tool",
    "make_execute_shell_tool",
    "ExecutionSignals",
    "IntentCheckResult",
    "ViolationRecord",
    "EscalationPolicy",
    "extract_signals",
    "check_intent_against_capabilities",
    "ViolationTracker",
]
