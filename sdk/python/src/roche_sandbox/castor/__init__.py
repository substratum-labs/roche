# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche-Castor integration: secure sandbox execution under Castor's security kernel.

Quick start::

    from roche_sandbox.castor import roche_castor

    kernel = roche_castor()
    cp = await kernel.run(my_agent)

Or bring your own Castor::

    from roche_sandbox.castor import roche_tools

    kernel = Castor(tools=roche_tools() + my_tools, default_budgets={"compute": 10})

Requires ``castor``. Install with: ``pip install roche-sandbox[castor]``
"""

from __future__ import annotations

try:
    import castor as _castor  # noqa: F401
except ImportError:
    raise ImportError(
        "roche_sandbox.castor requires the 'castor' package. "
        "Install with: pip install roche-sandbox[castor]"
    ) from None

# --- Simple API (most users only need these) ---
from roche_sandbox.castor._bridge import roche_castor, roche_tools

# --- Advanced API ---
from roche_sandbox.castor._bridge import RocheCastorBridge
from roche_sandbox.castor._workspace import Workspace, WorkspaceManager
from roche_sandbox.castor._stream_monitor import StreamEvent, StreamMonitor, StreamPolicy
from roche_sandbox.castor._types import EscalationPolicy, ExecutionSignals, IntentCheckResult, ViolationRecord
from roche_sandbox.castor._violations import ViolationTracker
from roche_sandbox.castor._signals import extract_signals
from roche_sandbox.castor._intent_gate import check_intent_against_capabilities
from roche_sandbox.castor._tools import (
    execute_code,
    execute_code_stream,
    execute_shell,
    make_execute_code_stream_tool,
    make_execute_code_tool,
    make_execute_shell_tool,
)

__all__ = [
    # Simple API
    "roche_castor",
    "roche_tools",
    # Advanced API
    "RocheCastorBridge",
    "Workspace",
    "WorkspaceManager",
    "StreamMonitor",
    "StreamPolicy",
    "StreamEvent",
    "EscalationPolicy",
    "ViolationTracker",
    # Types
    "ExecutionSignals",
    "IntentCheckResult",
    "ViolationRecord",
    # Pre-built tools
    "execute_code",
    "execute_shell",
    "execute_code_stream",
    # Factories
    "make_execute_code_tool",
    "make_execute_shell_tool",
    "make_execute_code_stream_tool",
    # Utilities
    "extract_signals",
    "check_intent_against_capabilities",
]
