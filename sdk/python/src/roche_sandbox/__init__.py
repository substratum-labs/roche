# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche — Universal sandbox orchestrator for AI agents (Python SDK)."""

__version__ = "0.1.0"

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.decorator import roche_sandbox
from roche_sandbox.intent import CodeIntent, analyze
from roche_sandbox.wallet import (
    CapabilityWallet, NetworkCap, FilesystemCap, ComputeCap,
    SecretsCap, OutputCap, UsageReport, run_with_wallet,
    from_castor_budgets, from_castor_capabilities, to_castor_usage,
)
from roche_sandbox.run import (
    ParallelResult, RunOptions, RunResult, Snapshot,
    async_run, async_run_parallel, async_snapshot, async_restore,
    run, run_cached, run_parallel, snapshot, restore, delete_snapshot,
)
from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import (
    Budget, BudgetUsage, DynamicPermissions, ExecEvent, ExecOutput, ExecRecord, Mount,
    OutputLimit, PoolInfo, RetryPolicy, SandboxConfig, SandboxInfo, SandboxStatus, SessionInfo,
)

__all__ = [
    "AsyncRoche",
    "Roche",
    "AsyncSandbox",
    "Sandbox",
    "RocheError",
    "SandboxNotFound",
    "SandboxPaused",
    "ProviderUnavailable",
    "TimeoutError",
    "UnsupportedOperation",
    "SandboxConfig",
    "ExecOutput",
    "ExecRecord",
    "SandboxInfo",
    "SandboxStatus",
    "Mount",
    "ExecEvent",
    "RetryPolicy",
    "OutputLimit",
    "roche_sandbox",
    "run",
    "async_run",
    "RunOptions",
    "RunResult",
    "ParallelResult",
    "Snapshot",
    "run_parallel",
    "async_run_parallel",
    "run_cached",
    "snapshot",
    "restore",
    "delete_snapshot",
    "async_snapshot",
    "async_restore",
    "CapabilityWallet",
    "NetworkCap",
    "FilesystemCap",
    "ComputeCap",
    "SecretsCap",
    "OutputCap",
    "UsageReport",
    "run_with_wallet",
    "from_castor_budgets",
    "from_castor_capabilities",  # backward compat alias
    "to_castor_usage",
    "CodeIntent",
    "analyze",
    "Budget",
    "BudgetUsage",
    "DynamicPermissions",
    "PoolInfo",
    "SessionInfo",
]
