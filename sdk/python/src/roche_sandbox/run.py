# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""One-line code execution with automatic sandbox management."""

from __future__ import annotations

import asyncio
import shutil
from dataclasses import dataclass, field

from roche_sandbox.client import AsyncRoche
from roche_sandbox.types import ExecOutput


@dataclass
class RunOptions:
    """Options for roche.run(). All optional — sensible defaults for everything."""

    language: str = "auto"
    """Language hint: 'python', 'node', 'bash', 'auto'. Determines image and command wrapper."""
    timeout_secs: int = 30
    """Maximum execution time in seconds."""
    network: bool = False
    """Allow network access."""
    network_allowlist: list[str] = field(default_factory=list)
    """Restrict network to these hosts (requires network=True)."""
    writable: bool = False
    """Allow filesystem writes."""
    fs_paths: list[str] = field(default_factory=list)
    """Writable filesystem paths (e.g., ['/tmp', '/data'])."""
    memory: str | None = None
    """Memory limit (e.g., '256m')."""
    trace_level: str = "summary"
    """Trace level: 'off', 'summary', 'standard', 'full'."""


# Language → (image, command builder)
_LANGUAGE_CONFIG = {
    "python": ("python:3.12-slim", lambda code: ["python3", "-c", code]),
    "node": ("node:20-slim", lambda code: ["node", "-e", code]),
    "bash": ("ubuntu:22.04", lambda code: ["bash", "-c", code]),
}


def _detect_language(code: str) -> str:
    """Best-effort language detection from code content."""
    indicators = {
        "python": ["import ", "def ", "print(", "from ", "class ", "if __name__"],
        "node": ["console.log", "require(", "const ", "let ", "function ", "=>", "async "],
        "bash": ["#!/bin/bash", "echo ", "grep ", "awk ", "sed ", "curl ", "apt-get", "||", "&&"],
    }
    scores = {lang: 0 for lang in indicators}
    for lang, keywords in indicators.items():
        for kw in keywords:
            if kw in code:
                scores[lang] += 1
    best = max(scores, key=scores.get)
    if scores[best] > 0:
        return best
    return "python"


def _detect_provider() -> str:
    """Pick the best available provider. Docker first, then fallback."""
    if shutil.which("docker"):
        return "docker"
    # Future: check for WASM runtime, Firecracker, etc.
    return "docker"


async def async_run(code: str, opts: RunOptions | None = None, **kwargs) -> ExecOutput:
    """Execute code in a sandbox and return the result. One-liner async API.

    Usage:
        result = await roche.async_run("print(2 + 2)")
        print(result.stdout)   # "4\\n"
        print(result.trace)    # ExecutionTrace(...)
    """
    if opts is None:
        opts = RunOptions(**kwargs)

    # Detect language
    lang = opts.language
    if lang == "auto":
        lang = _detect_language(code)

    image, cmd_builder = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])
    command = cmd_builder(code)
    provider = _detect_provider()

    client = AsyncRoche(provider=provider)
    sandbox = await client.create(
        image=image,
        timeout_secs=opts.timeout_secs,
        network=opts.network or bool(opts.network_allowlist),
        writable=opts.writable or bool(opts.fs_paths),
        memory=opts.memory,
        network_allowlist=opts.network_allowlist,
        fs_paths=opts.fs_paths,
    )
    try:
        result = await sandbox.exec(
            command,
            timeout_secs=opts.timeout_secs,
            trace_level=opts.trace_level,
        )
        return result
    finally:
        await sandbox.destroy()


def run(code: str, opts: RunOptions | None = None, **kwargs) -> ExecOutput:
    """Execute code in a sandbox and return the result. One-liner sync API.

    Usage:
        from roche_sandbox import run

        result = run("print(2 + 2)")
        print(result.stdout)   # "4\\n"
    """
    return asyncio.run(async_run(code, opts, **kwargs))
