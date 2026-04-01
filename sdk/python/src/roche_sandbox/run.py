# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""One-line code execution with automatic sandbox management."""

from __future__ import annotations

import asyncio
import os
import shutil
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

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
    install: bool = False
    """Auto-install dependencies (requirements.txt, package.json)."""
    download: list[str] | None = None
    """Paths to copy back from sandbox after execution."""


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


@dataclass
class RunResult(ExecOutput):
    """Result from run() — extends ExecOutput with downloaded files."""

    files: dict[str, bytes] = field(default_factory=dict)
    """Files downloaded from sandbox. Key = filename, value = bytes."""


# Dependency file → install command
_DEP_FILES: dict[str, tuple[str, list[str]]] = {
    "requirements.txt": ("python", ["pip", "install", "-r", "/app/requirements.txt"]),
    "package.json": ("node", ["npm", "install", "--prefix", "/app"]),
}


def _detect_language_from_file(path: str) -> str:
    """Detect language from file extension."""
    ext = Path(path).suffix.lower()
    return {".py": "python", ".js": "node", ".ts": "node", ".sh": "bash"}.get(ext, "python")


def _detect_language_from_dir(path: str) -> str:
    """Detect language from project contents."""
    p = Path(path)
    if (p / "requirements.txt").exists() or list(p.glob("*.py")):
        return "python"
    if (p / "package.json").exists() or list(p.glob("*.js")):
        return "node"
    if list(p.glob("*.sh")):
        return "bash"
    return "python"


def _find_entry_point(path: str, language: str) -> str | None:
    """Find default entry point in a project directory."""
    p = Path(path)
    candidates = {
        "python": ["main.py", "app.py", "run.py", "__main__.py"],
        "node": ["index.js", "main.js", "app.js"],
        "bash": ["run.sh", "main.sh"],
    }
    for name in candidates.get(language, []):
        if (p / name).exists():
            return name
    return None


def _check_provider_available(provider: str) -> bool:
    """Check if a provider's runtime is available."""
    if provider == "docker":
        return shutil.which("docker") is not None
    return False


async def async_run(
    code: str | None = None,
    opts: RunOptions | None = None,
    *,
    file: str | None = None,
    path: str | None = None,
    entry: str | None = None,
    **kwargs,
) -> RunResult:
    """Execute code, a file, or a project in a sandbox.

    Three modes:
        result = await async_run("print(2+2)")                  # inline code
        result = await async_run(file="script.py")              # single file
        result = await async_run(path="./project/", entry="main.py")  # project dir

    Args:
        code: Inline code string.
        file: Path to a single file to execute.
        path: Path to a project directory.
        entry: Entry point file within the project (auto-detected if omitted).
        opts: RunOptions for fine-grained control.
        **kwargs: Shorthand for RunOptions fields.
    """
    if opts is None:
        opts = RunOptions(**kwargs)

    # Determine mode
    if file is not None:
        return await _run_file(file, opts)
    elif path is not None:
        return await _run_project(path, entry, opts)
    elif code is not None:
        return await _run_code(code, opts)
    else:
        raise ValueError("One of code, file, or path must be provided")


async def _run_code(code: str, opts: RunOptions) -> RunResult:
    """Run inline code string."""
    lang = opts.language
    if lang == "auto":
        lang = _detect_language(code)

    intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)

    network = opts.network if opts.network is not None else intent.needs_network
    network_allowlist = opts.network_allowlist if opts.network_allowlist is not None else intent.network_hosts
    writable = opts.writable if opts.writable is not None else intent.needs_writable
    fs_paths = opts.fs_paths if opts.fs_paths is not None else intent.writable_paths
    memory = opts.memory if opts.memory is not None else intent.memory_hint
    provider = opts.provider if opts.provider is not None else intent.provider

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
        result = await sandbox.exec(command, timeout_secs=opts.timeout_secs, trace_level=opts.trace_level)
        files = await _download_files(sandbox, opts.download)
        return RunResult(
            exit_code=result.exit_code, stdout=result.stdout, stderr=result.stderr,
            trace=result.trace, files=files,
        )
    finally:
        await sandbox.destroy()


async def _run_file(file_path: str, opts: RunOptions) -> RunResult:
    """Copy a single file into sandbox and execute it."""
    p = Path(file_path).resolve()
    if not p.is_file():
        raise FileNotFoundError(f"File not found: {file_path}")

    lang = opts.language
    if lang == "auto":
        lang = _detect_language_from_file(str(p))

    # Read file for intent analysis
    code = p.read_text(errors="replace")
    intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)

    network = opts.network if opts.network is not None else intent.needs_network
    network_allowlist = opts.network_allowlist if opts.network_allowlist is not None else intent.network_hosts
    writable = opts.writable if opts.writable is not None else intent.needs_writable
    fs_paths = opts.fs_paths if opts.fs_paths is not None else intent.writable_paths
    memory = opts.memory if opts.memory is not None else intent.memory_hint

    image = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])[0]
    run_cmd = {"python": ["python3"], "node": ["node"], "bash": ["bash"]}.get(lang, ["python3"])

    client = AsyncRoche(provider=opts.provider or "docker")
    sandbox = await client.create(
        image=image,
        timeout_secs=opts.timeout_secs,
        network=network or bool(network_allowlist) or opts.install,
        writable=True,  # need writable to copy file in
        memory=memory,
        network_allowlist=network_allowlist or [],
        fs_paths=fs_paths or ["/app"],
    )
    try:
        await sandbox.copy_to(str(p), f"/app/{p.name}")

        # Install deps if a requirements file is alongside the source file
        if opts.install:
            await _install_deps_from_dir(sandbox, str(p.parent), lang)

        result = await sandbox.exec(
            run_cmd + [f"/app/{p.name}"],
            timeout_secs=opts.timeout_secs,
            trace_level=opts.trace_level,
        )
        files = await _download_files(sandbox, opts.download)
        return RunResult(
            exit_code=result.exit_code, stdout=result.stdout, stderr=result.stderr,
            trace=result.trace, files=files,
        )
    finally:
        await sandbox.destroy()


async def _run_project(dir_path: str, entry: str | None, opts: RunOptions) -> RunResult:
    """Copy a project directory into sandbox, install deps, and execute."""
    p = Path(dir_path).resolve()
    if not p.is_dir():
        raise NotADirectoryError(f"Directory not found: {dir_path}")

    lang = opts.language
    if lang == "auto":
        lang = _detect_language_from_dir(str(p))

    # Find entry point
    entry_file = entry or _find_entry_point(str(p), lang)
    if entry_file is None:
        raise ValueError(
            f"No entry point found in {dir_path}. "
            f"Pass entry= explicitly (e.g. entry='main.py')"
        )

    # Read entry file for intent analysis
    entry_path = p / entry_file
    if entry_path.is_file():
        code = entry_path.read_text(errors="replace")
        intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)
    else:
        intent = CodeIntent(language=lang)

    network = opts.network if opts.network is not None else intent.needs_network
    network_allowlist = opts.network_allowlist if opts.network_allowlist is not None else intent.network_hosts
    memory = opts.memory if opts.memory is not None else intent.memory_hint

    image = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])[0]
    run_cmd = {"python": ["python3"], "node": ["node"], "bash": ["bash"]}.get(lang, ["python3"])

    # For install, we need network to package registries
    needs_install = opts.install or _has_dep_file(str(p))
    if needs_install:
        network = True
        registries = {"python": "pypi.org", "node": "registry.npmjs.org"}.get(lang)
        if registries and (not network_allowlist or registries not in network_allowlist):
            network_allowlist = (network_allowlist or []) + [registries]

    client = AsyncRoche(provider=opts.provider or "docker")
    sandbox = await client.create(
        image=image,
        timeout_secs=max(opts.timeout_secs, 300),  # projects need more time
        network=network or bool(network_allowlist),
        writable=True,
        memory=memory,
        network_allowlist=network_allowlist or [],
        fs_paths=["/app"],
    )
    try:
        # Copy entire project directory
        await sandbox.copy_to(str(p), "/app")

        # Install dependencies
        if needs_install:
            await _install_deps_from_dir(sandbox, str(p), lang)

        result = await sandbox.exec(
            run_cmd + [f"/app/{entry_file}"],
            timeout_secs=opts.timeout_secs,
            trace_level=opts.trace_level,
        )
        files = await _download_files(sandbox, opts.download)
        return RunResult(
            exit_code=result.exit_code, stdout=result.stdout, stderr=result.stderr,
            trace=result.trace, files=files,
        )
    finally:
        await sandbox.destroy()


def _has_dep_file(dir_path: str) -> bool:
    """Check if a directory has a dependency file."""
    p = Path(dir_path)
    return any((p / name).exists() for name in _DEP_FILES)


async def _install_deps_from_dir(sandbox: object, dir_path: str, language: str) -> None:
    """Install dependencies if a known dep file exists."""
    p = Path(dir_path)
    for dep_file, (lang, install_cmd) in _DEP_FILES.items():
        if lang == language and (p / dep_file).exists():
            # Dep file was already copied with the project
            await sandbox.exec(install_cmd, timeout_secs=120)  # type: ignore[attr-defined]
            break


async def _download_files(sandbox: object, paths: list[str] | None) -> dict[str, bytes]:
    """Download files from sandbox to local temp dir, return as bytes dict."""
    if not paths:
        return {}

    files: dict[str, bytes] = {}
    with tempfile.TemporaryDirectory() as tmp:
        for sandbox_path in paths:
            filename = Path(sandbox_path).name
            local_path = os.path.join(tmp, filename)
            try:
                await sandbox.copy_from(sandbox_path, local_path)  # type: ignore[attr-defined]
                files[filename] = Path(local_path).read_bytes()
            except Exception:
                pass  # skip files that don't exist
    return files


def run(
    code: str | None = None,
    opts: RunOptions | None = None,
    *,
    file: str | None = None,
    path: str | None = None,
    entry: str | None = None,
    **kwargs,
) -> RunResult:
    """Execute code, a file, or a project in a sandbox. Sync API.

    Usage::

        from roche_sandbox import run

        # Inline code
        result = run("print(2 + 2)")

        # Single file
        result = run(file="script.py")

        # Project directory
        result = run(path="./my-project/", entry="main.py")

        # With dependency install + file download
        result = run(path="./ml-pipeline/", install=True, download=["/app/output.csv"])
        print(result.files["output.csv"])
    """
    return asyncio.run(async_run(code, opts, file=file, path=path, entry=entry, **kwargs))
