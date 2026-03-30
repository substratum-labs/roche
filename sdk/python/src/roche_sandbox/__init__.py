# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche — Universal sandbox orchestrator for AI agents (Python SDK)."""

__version__ = "0.1.0"

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.decorator import roche_sandbox
from roche_sandbox.intent import CodeIntent, analyze
from roche_sandbox.run import RunOptions, async_run, run
from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecEvent, ExecOutput, Mount, OutputLimit, RetryPolicy, SandboxConfig, SandboxInfo, SandboxStatus

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
    "CodeIntent",
    "analyze",
]
