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


# ---------------------------------------------------------------------------
# Parallel execution
# ---------------------------------------------------------------------------


@dataclass
class ParallelResult:
    """Result from run_parallel(). Contains individual results + summary."""

    results: list[RunResult]
    """Individual results, same order as input."""
    total_succeeded: int = 0
    total_failed: int = 0


async def async_run_parallel(
    tasks: list[dict],
    *,
    max_concurrency: int = 5,
    opts: RunOptions | None = None,
) -> ParallelResult:
    """Execute multiple tasks in parallel, each in its own sandbox.

    Args:
        tasks: List of dicts, each with keys matching run() args
               (code, file, path, entry, etc.)
        max_concurrency: Max simultaneous sandboxes.
        opts: Default RunOptions applied to all tasks (individual task
              kwargs override).

    Usage::

        results = await async_run_parallel([
            {"code": "print(1)"},
            {"code": "print(2)"},
            {"file": "script.py"},
            {"path": "./project/", "entry": "main.py"},
        ])
    """
    sem = asyncio.Semaphore(max_concurrency)

    async def _run_one(task: dict) -> RunResult:
        async with sem:
            merged = {**(opts.__dict__ if opts else {}), **task}
            code = merged.pop("code", None)
            file = merged.pop("file", None)
            path = merged.pop("path", None)
            entry = merged.pop("entry", None)
            try:
                return await async_run(code, file=file, path=path, entry=entry, **merged)
            except Exception as e:
                return RunResult(exit_code=1, stdout="", stderr=str(e))

    results = await asyncio.gather(*[_run_one(t) for t in tasks])
    succeeded = sum(1 for r in results if r.exit_code == 0)
    return ParallelResult(
        results=list(results),
        total_succeeded=succeeded,
        total_failed=len(results) - succeeded,
    )


def run_parallel(
    tasks: list[dict],
    *,
    max_concurrency: int = 5,
    opts: RunOptions | None = None,
) -> ParallelResult:
    """Execute multiple tasks in parallel. Sync API.

    Usage::

        from roche_sandbox import run_parallel

        results = run_parallel([
            {"code": "print(i)"} for i in range(10)
        ], max_concurrency=5)

        for r in results.results:
            print(r.stdout, end="")
    """
    return asyncio.run(async_run_parallel(tasks, max_concurrency=max_concurrency, opts=opts))


# ---------------------------------------------------------------------------
# Dependency caching
# ---------------------------------------------------------------------------

# Cache volume name pattern: roche-deps-{lang}-{hash}
_CACHE_VOLUME_PREFIX = "roche-deps"


def _dep_cache_volume(language: str, dep_file_path: str) -> str | None:
    """Generate a deterministic Docker volume name for dependency caching."""
    import hashlib
    p = Path(dep_file_path)
    if not p.exists():
        return None
    content_hash = hashlib.sha256(p.read_bytes()).hexdigest()[:12]
    return f"{_CACHE_VOLUME_PREFIX}-{language}-{content_hash}"


def _dep_cache_mount(language: str) -> str:
    """Return the container path where deps are cached for a language."""
    return {
        "python": "/root/.cache/pip",
        "node": "/root/.npm",
    }.get(language, "/root/.cache")


async def _ensure_cache_volume(volume_name: str) -> None:
    """Create a Docker volume if it doesn't exist."""
    proc = await asyncio.create_subprocess_exec(
        "docker", "volume", "create", volume_name,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()


async def _run_with_dep_cache(
    code: str | None = None,
    *,
    file: str | None = None,
    path: str | None = None,
    entry: str | None = None,
    opts: RunOptions | None = None,
    **kwargs,
) -> RunResult:
    """Run with dependency caching — pip/npm cache persists across sandboxes.

    Creates a Docker volume keyed by the hash of the dependency file
    (requirements.txt, package.json). Same deps = same volume = cache hit.
    """
    if opts is None:
        opts = RunOptions(**kwargs)
    opts.install = True

    # Find the dependency file to hash
    dep_file = None
    lang = opts.language

    if path:
        p = Path(path).resolve()
        if lang == "auto":
            lang = _detect_language_from_dir(str(p))
        for name in _DEP_FILES:
            if (p / name).exists():
                dep_file = str(p / name)
                break
    elif file:
        p = Path(file).resolve()
        if lang == "auto":
            lang = _detect_language_from_file(str(p))
        for name in _DEP_FILES:
            if (p.parent / name).exists():
                dep_file = str(p.parent / name)
                break

    if dep_file is None:
        # No dep file — just run normally
        return await async_run(code, opts, file=file, path=path, entry=entry)

    volume_name = _dep_cache_volume(lang, dep_file)
    if volume_name is None:
        return await async_run(code, opts, file=file, path=path, entry=entry)

    await _ensure_cache_volume(volume_name)

    cache_path = _dep_cache_mount(lang)
    from roche_sandbox.types import Mount
    mount = Mount(host_path=volume_name, container_path=cache_path, readonly=False)

    # Inject the cache mount into RunOptions via a custom run
    # We need to create the sandbox manually with the mount
    if path:
        return await _run_project_with_mount(path, entry, opts, mount)
    elif file:
        return await _run_file_with_mount(file, opts, mount)
    else:
        return await async_run(code, opts, file=file, path=path, entry=entry)


async def _run_file_with_mount(file_path: str, opts: RunOptions, mount: object) -> RunResult:
    """Run a file with a dependency cache mount."""
    p = Path(file_path).resolve()
    lang = opts.language if opts.language != "auto" else _detect_language_from_file(str(p))

    code = p.read_text(errors="replace")
    intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)

    memory = opts.memory if opts.memory is not None else intent.memory_hint
    image = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])[0]
    run_cmd = {"python": ["python3"], "node": ["node"], "bash": ["bash"]}.get(lang, ["python3"])

    client = AsyncRoche(provider=opts.provider or "docker")
    sandbox = await client.create(
        image=image,
        timeout_secs=opts.timeout_secs,
        network=True,
        writable=True,
        memory=memory,
        mounts=[mount],
        fs_paths=["/app"],
    )
    try:
        await sandbox.copy_to(str(p), f"/app/{p.name}")
        await _install_deps_from_dir(sandbox, str(p.parent), lang)
        result = await sandbox.exec(run_cmd + [f"/app/{p.name}"], timeout_secs=opts.timeout_secs, trace_level=opts.trace_level)
        files = await _download_files(sandbox, opts.download)
        return RunResult(exit_code=result.exit_code, stdout=result.stdout, stderr=result.stderr, trace=result.trace, files=files)
    finally:
        await sandbox.destroy()


async def _run_project_with_mount(dir_path: str, entry: str | None, opts: RunOptions, mount: object) -> RunResult:
    """Run a project with a dependency cache mount."""
    p = Path(dir_path).resolve()
    lang = opts.language if opts.language != "auto" else _detect_language_from_dir(str(p))

    entry_file = entry or _find_entry_point(str(p), lang)
    if entry_file is None:
        raise ValueError(f"No entry point found in {dir_path}. Pass entry= explicitly.")

    entry_path = p / entry_file
    if entry_path.is_file():
        code = entry_path.read_text(errors="replace")
        intent = analyze(code, lang) if opts.auto_infer else CodeIntent(language=lang)
    else:
        intent = CodeIntent(language=lang)

    memory = opts.memory if opts.memory is not None else intent.memory_hint
    image = _LANGUAGE_CONFIG.get(lang, _LANGUAGE_CONFIG["python"])[0]
    run_cmd = {"python": ["python3"], "node": ["node"], "bash": ["bash"]}.get(lang, ["python3"])

    client = AsyncRoche(provider=opts.provider or "docker")
    sandbox = await client.create(
        image=image,
        timeout_secs=max(opts.timeout_secs, 300),
        network=True,
        writable=True,
        memory=memory,
        mounts=[mount],
        fs_paths=["/app"],
    )
    try:
        await sandbox.copy_to(str(p), "/app")
        await _install_deps_from_dir(sandbox, str(p), lang)
        result = await sandbox.exec(run_cmd + [f"/app/{entry_file}"], timeout_secs=opts.timeout_secs, trace_level=opts.trace_level)
        files = await _download_files(sandbox, opts.download)
        return RunResult(exit_code=result.exit_code, stdout=result.stdout, stderr=result.stderr, trace=result.trace, files=files)
    finally:
        await sandbox.destroy()


def run_cached(
    code: str | None = None,
    *,
    file: str | None = None,
    path: str | None = None,
    entry: str | None = None,
    **kwargs,
) -> RunResult:
    """Run with dependency caching. Sync API.

    Usage::

        # First run: installs deps (~30s)
        result = run_cached(path="./ml-project/")

        # Second run: cache hit (<1s for deps)
        result = run_cached(path="./ml-project/")
    """
    return asyncio.run(_run_with_dep_cache(code, file=file, path=path, entry=entry, **kwargs))


# ---------------------------------------------------------------------------
# Snapshot & Restore
# ---------------------------------------------------------------------------


@dataclass
class Snapshot:
    """A saved sandbox state that can be restored later."""

    snapshot_id: str
    sandbox_id: str
    provider: str
    image: str


async def async_snapshot(sandbox_id: str, provider: str = "docker") -> Snapshot:
    """Save a sandbox's filesystem state as a Docker image.

    The sandbox is committed to a local image. Restore creates a new
    sandbox from that image — all files and state are preserved.

    Usage::

        # Set up environment
        sandbox = await roche.create(writable=True)
        await sandbox.exec(["pip", "install", "numpy", "pandas"])
        await sandbox.exec(["python3", "-c", "open('/app/config.json','w').write('{}')"])

        # Snapshot
        snap = await async_snapshot(sandbox.id)

        # Later — restore in <1s (no reinstall needed)
        result = await async_restore(snap, ["python3", "-c", "import numpy; print(numpy.__version__)"])
    """
    snapshot_id = f"roche-snap-{sandbox_id[:12]}"
    proc = await asyncio.create_subprocess_exec(
        "docker", "commit", sandbox_id, snapshot_id,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await proc.communicate()
    if proc.returncode != 0:
        raise RuntimeError(f"Snapshot failed: {stderr.decode()}")

    return Snapshot(
        snapshot_id=snapshot_id,
        sandbox_id=sandbox_id,
        provider=provider,
        image=snapshot_id,
    )


async def async_restore(
    snapshot: Snapshot,
    command: list[str] | None = None,
    *,
    timeout_secs: int = 30,
    trace_level: str = "summary",
) -> RunResult:
    """Restore a sandbox from snapshot and optionally execute a command.

    Creates a new sandbox from the snapshot image. All files and installed
    packages from the original sandbox are preserved.
    """
    client = AsyncRoche(provider=snapshot.provider)
    sandbox = await client.create(
        image=snapshot.image,
        timeout_secs=timeout_secs,
        writable=True,
    )
    try:
        if command:
            result = await sandbox.exec(command, timeout_secs=timeout_secs, trace_level=trace_level)
            return RunResult(
                exit_code=result.exit_code, stdout=result.stdout,
                stderr=result.stderr, trace=result.trace,
            )
        return RunResult(exit_code=0, stdout=sandbox.id, stderr="")
    finally:
        if command:
            await sandbox.destroy()
        # If no command, caller owns the sandbox lifecycle


async def async_delete_snapshot(snapshot: Snapshot) -> None:
    """Delete a snapshot image."""
    proc = await asyncio.create_subprocess_exec(
        "docker", "rmi", snapshot.snapshot_id,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    await proc.communicate()


def snapshot(sandbox_id: str, provider: str = "docker") -> Snapshot:
    """Save sandbox state. Sync API."""
    return asyncio.run(async_snapshot(sandbox_id, provider))


def restore(snapshot: Snapshot, command: list[str] | None = None, **kwargs) -> RunResult:
    """Restore from snapshot and run. Sync API."""
    return asyncio.run(async_restore(snapshot, command, **kwargs))


def delete_snapshot(snap: Snapshot) -> None:
    """Delete a snapshot. Sync API."""
    asyncio.run(async_delete_snapshot(snap))
