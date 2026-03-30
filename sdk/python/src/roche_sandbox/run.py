# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""One-line code execution with automatic sandbox management."""

from __future__ import annotations

import asyncio
import shutil
from dataclasses import dataclass, field

from roche_sandbox.client import AsyncRoche
from roche_sandbox.intent import CodeIntent, analyze
from roche_sandbox.types import ExecOutput


@dataclass
class RunOptions:
    """Options for roche.run(). All optional — sensible defaults for everything."""

    language: str = "auto"
    """Language hint: 'python', 'node', 'bash', 'auto'. Determines image and command wrapper."""
    timeout_secs: int = 30
    """Maximum execution time in seconds."""
    network: bool | None = None
    """Allow network access. None = auto-detect from code."""
    network_allowlist: list[str] | None = None
    """Restrict network to these hosts. None = auto-detect from code."""
    writable: bool | None = None
    """Allow filesystem writes. None = auto-detect from code."""
    fs_paths: list[str] | None = None
    """Writable filesystem paths. None = auto-detect from code."""
    memory: str | None = None
    """Memory limit. None = auto-detect from code."""
    trace_level: str = "summary"
    """Trace level: 'off', 'summary', 'standard', 'full'."""
    provider: str | None = None
    """Provider override. None = auto-select based on code analysis."""
    auto_infer: bool = True
    """Enable intent-based analysis. Set False to disable auto-detection."""


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


def _check_provider_available(provider: str) -> bool:
    """Check if a provider's runtime is available."""
    if provider == "docker":
        return shutil.which("docker") is not None
    return False


async def async_run(code: str, opts: RunOptions | None = None, **kwargs) -> ExecOutput:
    """Execute code in a sandbox and return the result. One-liner async API.

    Uses intent-based analysis to auto-detect:
    - Best provider (WASM for pure compute, Docker for packages/network)
    - Network permissions (allowlist from detected URLs)
    - Filesystem permissions (writable paths from detected file ops)
    - Memory hints (from detected data libraries)

    Usage:
        result = await roche.async_run("print(2 + 2)")
        print(result.stdout)   # "4\\n"

        # With network — auto-detected:
        result = await roche.async_run(\"\"\"
            import requests
            r = requests.get('https://api.github.com')
            print(r.status_code)
        \"\"\")
        # Roche auto-infers: network=True, network_allowlist=["api.github.com"]
    """
    if opts is None:
        opts = RunOptions(**kwargs)

    # Detect language
    lang = opts.language
    if lang == "auto":
        lang = _detect_language(code)

    # Run intent analysis
    intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)

    # Resolve settings: explicit opts override auto-inferred
    network = opts.network if opts.network is not None else intent.needs_network
    network_allowlist = opts.network_allowlist if opts.network_allowlist is not None else intent.network_hosts
    writable = opts.writable if opts.writable is not None else intent.needs_writable
    fs_paths = opts.fs_paths if opts.fs_paths is not None else intent.writable_paths
    memory = opts.memory if opts.memory is not None else intent.memory_hint
    provider = opts.provider if opts.provider is not None else intent.provider

    # Fallback if recommended provider not available
    if provider == "wasm" and not _check_provider_available("wasm"):
        provider = "docker"

    image, cmd_builder = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])
    command = cmd_builder(code)

    client = AsyncRoche(provider=provider)
    sandbox = await client.create(
        image=image,
        timeout_secs=opts.timeout_secs,
        network=network or bool(network_allowlist),
        writable=writable or bool(fs_paths),
        memory=memory,
        network_allowlist=network_allowlist or [],
        fs_paths=fs_paths or [],
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

        # Auto-detects network needs:
        result = run("import requests; print(requests.get('https://httpbin.org/ip').text)")
        # Auto-inferred: network=True, allowlist=["httpbin.org"]
    """
    return asyncio.run(async_run(code, opts, **kwargs))
