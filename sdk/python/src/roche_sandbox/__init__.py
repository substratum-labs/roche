# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Roche — Universal sandbox orchestrator for AI agents (Python SDK)."""

__version__ = "0.1.0"

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.decorator import roche_sandbox
from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput, Mount, SandboxConfig, SandboxInfo, SandboxStatus

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
    "roche_sandbox",
]
