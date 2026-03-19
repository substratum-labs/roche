# Daemon Auto-Management Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bundle `roched` and `roche` binaries into the Python SDK wheel via maturin, add auto-spawn logic so `pip install roche-sandbox` provides a zero-configuration experience.

**Architecture:** Migrate build from hatchling to maturin. CI cross-compiles binaries for 4 platforms, then maturin packages them into platform-specific wheels. Python SDK auto-detects daemon → tries auto-spawn from bundled binary → falls back to CLI. Daemon supports configurable idle timeout (default: long-lived).

**Tech Stack:** Python (SDK), Rust (daemon/CLI), maturin (wheel build), GitHub Actions (CI), `cross` (cross-compilation)

**Spec:** `docs/superpowers/specs/2026-03-18-daemon-auto-management-design.md`

---

## File Structure

### New Files
- `sdk/python/src/roche_sandbox/bin/.gitkeep` — placeholder for bundled binaries (CI populates)
- `sdk/python/tests/test_auto_spawn.py` — auto-spawn unit tests
- `crates/roche-daemon/src/seccomp-trace.json` — seccomp profile with SCMP_ACT_LOG
- `.github/workflows/python-release.yml` — cross-platform wheel build + publish

### Modified Files
- `sdk/python/pyproject.toml` — hatchling → maturin, remove CLI install hack
- `sdk/python/src/roche_sandbox/daemon.py` — add `_find_bundled_binary`, `_spawn_daemon`, `_wait_for_daemon_ready`
- `sdk/python/src/roche_sandbox/client.py` — auto-spawn in transport selection
- `sdk/python/src/roche_sandbox/transport/cli.py` — bundled binary resolution
- `crates/roche-daemon/src/main.rs` — idle timeout flag + env var, seccomp profile write

### Removed Files
- `sdk/python/src/roche_sandbox/_cli_install.py` — replaced by bundled binary

---

## Chunk 1: Python SDK Auto-Spawn Logic

### Task 1: Auto-spawn helper functions

**Files:**
- Modify: `sdk/python/src/roche_sandbox/daemon.py`
- Create: `sdk/python/tests/test_auto_spawn.py`

- [ ] **Step 1: Write tests for helper functions**

Create `sdk/python/tests/test_auto_spawn.py`:
```python
import os
import socket
import time
from pathlib import Path
from unittest.mock import patch, MagicMock

import pytest

from roche_sandbox.daemon import (
    _find_bundled_binary,
    _spawn_daemon,
    _wait_for_daemon_ready,
)


class TestFindBundledBinary:
    def test_returns_path_when_binary_exists(self, tmp_path):
        bin_dir = tmp_path / "bin"
        bin_dir.mkdir()
        binary = bin_dir / "roched"
        binary.write_text("#!/bin/sh\n")
        binary.chmod(0o755)

        with patch("roche_sandbox.daemon.Path.__file__", tmp_path / "daemon.py"):
            # We need to mock the path resolution
            with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=bin_dir):
                result = _find_bundled_binary("roched")
        assert result == binary

    def test_returns_none_when_binary_missing(self):
        with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=Path("/nonexistent")):
            result = _find_bundled_binary("roched")
        assert result is None

    def test_returns_none_when_not_executable(self, tmp_path):
        bin_dir = tmp_path / "bin"
        bin_dir.mkdir()
        binary = bin_dir / "roched"
        binary.write_text("not executable")
        binary.chmod(0o644)

        with patch("roche_sandbox.daemon._bundled_bin_dir", return_value=bin_dir):
            result = _find_bundled_binary("roched")
        assert result is None


class TestWaitForDaemonReady:
    def test_returns_true_when_daemon_ready(self):
        # Create a temporary server socket to simulate daemon port
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", 0))
            s.listen(1)
            port = s.getsockname()[1]

            with patch("roche_sandbox.daemon.detect_daemon", return_value={"pid": os.getpid(), "port": port}):
                result = _wait_for_daemon_ready(timeout=1.0)
            assert result is True

    def test_returns_false_on_timeout(self):
        with patch("roche_sandbox.daemon.detect_daemon", return_value=None):
            result = _wait_for_daemon_ready(timeout=0.3)
        assert result is False

    def test_returns_false_when_port_not_open(self):
        with patch("roche_sandbox.daemon.detect_daemon", return_value={"pid": os.getpid(), "port": 19999}):
            result = _wait_for_daemon_ready(timeout=0.5)
        assert result is False


class TestSpawnDaemon:
    def test_spawns_detached_process(self, tmp_path):
        fake_binary = tmp_path / "roched"
        fake_binary.write_text("#!/bin/sh\nsleep 100\n")
        fake_binary.chmod(0o755)

        with patch("subprocess.Popen") as mock_popen, \
             patch("roche_sandbox.daemon._roche_dir", return_value=tmp_path):
            _spawn_daemon(fake_binary)
            mock_popen.assert_called_once()
            call_kwargs = mock_popen.call_args
            assert call_kwargs.kwargs.get("start_new_session") is True

    def test_passes_idle_timeout_from_env(self, tmp_path):
        fake_binary = tmp_path / "roched"
        fake_binary.write_text("#!/bin/sh\n")
        fake_binary.chmod(0o755)

        with patch("subprocess.Popen") as mock_popen, \
             patch("roche_sandbox.daemon._roche_dir", return_value=tmp_path), \
             patch.dict(os.environ, {"ROCHE_DAEMON_IDLE_TIMEOUT": "300"}):
            _spawn_daemon(fake_binary)
            args = mock_popen.call_args[0][0]
            assert "--idle-timeout" in args
            assert "300" in args
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/test_auto_spawn.py -v`
Expected: FAIL — functions don't exist yet

- [ ] **Step 3: Implement helper functions**

In `sdk/python/src/roche_sandbox/daemon.py`, add after existing code:

```python
import socket
import subprocess
import time


def _bundled_bin_dir() -> Path:
    """Return the bin/ directory inside the roche_sandbox package."""
    return Path(__file__).parent / "bin"


def _roche_dir() -> Path:
    """Return ~/.roche directory, creating it if needed."""
    d = Path.home() / ".roche"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _find_bundled_binary(name: str) -> Path | None:
    """Locate a bundled binary in the roche_sandbox package."""
    binary = _bundled_bin_dir() / name
    if binary.exists() and os.access(binary, os.X_OK):
        return binary
    return None


def _spawn_daemon(roched_path: Path) -> None:
    """Spawn roched as a detached background process."""
    roche_dir = _roche_dir()
    log_path = roche_dir / "daemon.log"

    args = [str(roched_path)]
    idle_timeout = os.environ.get("ROCHE_DAEMON_IDLE_TIMEOUT")
    if idle_timeout:
        args.extend(["--idle-timeout", idle_timeout])

    with open(log_path, "a") as log_file:
        subprocess.Popen(
            args,
            stdout=log_file,
            stderr=log_file,
            start_new_session=True,
        )


def _wait_for_daemon_ready(timeout: float = 3.0) -> bool:
    """Poll until daemon is ready to accept gRPC connections."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        daemon = detect_daemon()
        if daemon is not None:
            try:
                with socket.create_connection(
                    ("127.0.0.1", daemon["port"]), timeout=0.5
                ):
                    return True
            except (ConnectionRefusedError, OSError):
                pass
        time.sleep(0.1)
    return False
```

- [ ] **Step 4: Create bin directory placeholder**

```bash
mkdir -p sdk/python/src/roche_sandbox/bin
touch sdk/python/src/roche_sandbox/bin/.gitkeep
```

- [ ] **Step 5: Run tests**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/test_auto_spawn.py -v`
Expected: All tests PASS

- [ ] **Step 6: Commit**

```bash
git add sdk/python/src/roche_sandbox/daemon.py sdk/python/src/roche_sandbox/bin/.gitkeep sdk/python/tests/test_auto_spawn.py
git commit -m "feat(python-sdk): add daemon auto-spawn helper functions"
```

### Task 2: Integrate auto-spawn into client.py

**Files:**
- Modify: `sdk/python/src/roche_sandbox/client.py:36-41`

- [ ] **Step 1: Write test for auto-spawn transport selection**

Add to `sdk/python/tests/test_auto_spawn.py`:
```python
from roche_sandbox.client import AsyncRoche
from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.transport.grpc import GrpcTransport


class TestAutoSpawnTransport:
    def test_uses_grpc_when_daemon_running(self):
        with patch("roche_sandbox.client.detect_daemon", return_value={"pid": 123, "port": 50051}):
            roche = AsyncRoche()
        assert isinstance(roche.transport, GrpcTransport)

    def test_spawns_daemon_from_bundled_binary(self):
        with patch("roche_sandbox.client.detect_daemon", return_value=None), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=Path("/fake/roched")), \
             patch("roche_sandbox.client._spawn_daemon") as mock_spawn, \
             patch("roche_sandbox.client._wait_for_daemon_ready", return_value=True), \
             patch("roche_sandbox.client.detect_daemon", side_effect=[None, {"pid": 123, "port": 50051}]):
            roche = AsyncRoche()
        mock_spawn.assert_called_once()

    def test_falls_back_to_cli_when_no_bundled_binary(self):
        with patch("roche_sandbox.client.detect_daemon", return_value=None), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=None):
            roche = AsyncRoche()
        assert isinstance(roche.transport, CliTransport)

    def test_falls_back_to_cli_when_spawn_times_out(self):
        with patch("roche_sandbox.client.detect_daemon", return_value=None), \
             patch("roche_sandbox.client._find_bundled_binary", return_value=Path("/fake/roched")), \
             patch("roche_sandbox.client._spawn_daemon"), \
             patch("roche_sandbox.client._wait_for_daemon_ready", return_value=False):
            roche = AsyncRoche()
        assert isinstance(roche.transport, CliTransport)
```

- [ ] **Step 2: Update client.py auto-detect logic**

In `sdk/python/src/roche_sandbox/client.py`, add imports and replace auto-detect block:

```python
# Add import:
from roche_sandbox.daemon import detect_daemon, _find_bundled_binary, _spawn_daemon, _wait_for_daemon_ready

# Replace lines 36-41 (the else branch of mode detection):
        else:
            daemon = detect_daemon()
            if daemon is not None:
                self._transport = GrpcTransport(port=daemon["port"])
            else:
                roched_path = _find_bundled_binary("roched")
                if roched_path:
                    _spawn_daemon(roched_path)
                    if _wait_for_daemon_ready(timeout=3.0):
                        daemon = detect_daemon()
                        if daemon is not None:
                            self._transport = GrpcTransport(port=daemon["port"])
                        else:
                            self._transport = CliTransport(binary=binary)
                    else:
                        self._transport = CliTransport(binary=binary)
                else:
                    self._transport = CliTransport(binary=binary)
```

- [ ] **Step 3: Run tests**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/test_auto_spawn.py -v`
Expected: All tests PASS

- [ ] **Step 4: Run full Python test suite**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/ -v`
Expected: All tests PASS (existing tests unaffected because they inject transport explicitly)

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/client.py sdk/python/tests/test_auto_spawn.py
git commit -m "feat(python-sdk): integrate daemon auto-spawn into transport selection"
```

### Task 3: Bundled CLI binary resolution

**Files:**
- Modify: `sdk/python/src/roche_sandbox/transport/cli.py`

- [ ] **Step 1: Update CliTransport to check bundled binary**

In `sdk/python/src/roche_sandbox/transport/cli.py`, modify the constructor:

```python
from roche_sandbox.daemon import _find_bundled_binary

class CliTransport:
    def __init__(self, binary: str = "roche"):
        bundled = _find_bundled_binary("roche")
        self._binary = str(bundled) if bundled else binary
```

- [ ] **Step 2: Run full Python test suite**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/ -v`
Expected: All tests PASS

- [ ] **Step 3: Commit**

```bash
git add sdk/python/src/roche_sandbox/transport/cli.py
git commit -m "feat(python-sdk): resolve CLI binary from bundled package"
```

---

## Chunk 2: Daemon Idle Timeout

### Task 4: Add idle timeout to roched

**Files:**
- Modify: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Add idle_timeout arg to Args struct**

In `crates/roche-daemon/src/main.rs`, add to the `Args` struct:
```rust
    /// Idle timeout in seconds (0 = disabled, run forever)
    #[arg(long, default_value = "0", env = "ROCHE_DAEMON_IDLE_TIMEOUT")]
    idle_timeout: u64,
```

- [ ] **Step 2: Add last_rpc tracking to SandboxServiceImpl**

In `crates/roche-daemon/src/server.rs`, add to `SandboxServiceImpl`:
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Add field:
pub last_rpc_ms: Arc<AtomicU64>,

// In each RPC handler, at the start:
self.last_rpc_ms.store(
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64,
    Ordering::Relaxed,
);
```

- [ ] **Step 3: Add idle monitor task in main.rs**

In `crates/roche-daemon/src/main.rs`, after the server starts:
```rust
if args.idle_timeout > 0 {
    let last_rpc = service.last_rpc_ms.clone();
    let timeout_ms = args.idle_timeout * 1000;
    let daemon_json = roche_dir.join("daemon.json");
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            let last = last_rpc.load(Ordering::Relaxed);
            if last > 0 && now_ms - last > timeout_ms {
                let _ = std::fs::remove_file(&daemon_json);
                std::process::exit(0);
            }
        }
    });
}
```

- [ ] **Step 4: Initialize last_rpc_ms in service construction**

Where `SandboxServiceImpl` is created, add:
```rust
last_rpc_ms: Arc::new(AtomicU64::new(
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
)),
```

- [ ] **Step 5: Verify build**

Run: `export PATH="$HOME/.local/bin:$PATH" && cargo build -p roche-daemon`
Expected: BUILD SUCCESS

- [ ] **Step 6: Run existing daemon tests**

Run: `export PATH="$HOME/.local/bin:$PATH" && cargo test -p roche-daemon`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/roche-daemon/src/main.rs crates/roche-daemon/src/server.rs
git commit -m "feat(daemon): add configurable idle timeout"
```

---

## Chunk 3: Seccomp Profile Bundling

### Task 5: Create and embed seccomp trace profile

**Files:**
- Create: `crates/roche-daemon/src/seccomp-trace.json`
- Modify: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Create minimal seccomp trace profile**

Create `crates/roche-daemon/src/seccomp-trace.json` — Docker's default seccomp profile with `SCMP_ACT_LOG` for monitored syscalls (`connect`, `openat`, `unlink`, `unlinkat`). This is a large JSON file. Use Docker's default profile as base and change the action for these syscalls from `SCMP_ACT_ALLOW` to `SCMP_ACT_LOG`.

For MVP, create a minimal profile that only logs the relevant syscalls:
```json
{
  "defaultAction": "SCMP_ACT_ALLOW",
  "syscalls": [
    {
      "names": ["connect"],
      "action": "SCMP_ACT_LOG"
    },
    {
      "names": ["openat"],
      "action": "SCMP_ACT_LOG"
    },
    {
      "names": ["unlink", "unlinkat"],
      "action": "SCMP_ACT_LOG"
    }
  ]
}
```

- [ ] **Step 2: Embed profile in daemon and write on startup**

In `crates/roche-daemon/src/main.rs`, add:
```rust
const SECCOMP_TRACE_PROFILE: &str = include_str!("seccomp-trace.json");

// In startup, after creating roche_dir:
let seccomp_path = roche_dir.join("seccomp-trace.json");
if !seccomp_path.exists() {
    std::fs::write(&seccomp_path, SECCOMP_TRACE_PROFILE)?;
}
```

- [ ] **Step 3: Verify build**

Run: `export PATH="$HOME/.local/bin:$PATH" && cargo build -p roche-daemon`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add crates/roche-daemon/src/seccomp-trace.json crates/roche-daemon/src/main.rs
git commit -m "feat(daemon): embed and write seccomp trace profile on startup"
```

---

## Chunk 4: Build System Migration

### Task 6: Migrate pyproject.toml from hatchling to maturin

**Files:**
- Modify: `sdk/python/pyproject.toml`

- [ ] **Step 1: Update pyproject.toml**

Replace `sdk/python/pyproject.toml`:
```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[tool.maturin]
python-source = "src"
module-name = "roche_sandbox"
include = ["src/roche_sandbox/bin/*"]

[project]
name = "roche-sandbox"
version = "0.2.0"
description = "Universal sandbox orchestrator for AI agents — Python SDK"
license = {text = "Apache-2.0"}
requires-python = ">=3.10"
readme = "README.md"
keywords = ["sandbox", "docker", "ai", "agent", "wasm", "orchestrator"]
authors = [{name = "Substratum Labs"}]
classifiers = [
    "Development Status :: 3 - Alpha",
    "License :: OSI Approved :: Apache Software License",
    "Programming Language :: Python :: 3",
    "Programming Language :: Python :: 3.10",
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Topic :: Software Development :: Libraries",
    "Topic :: System :: Emulators",
]
dependencies = [
    "grpcio>=1.60.0",
    "protobuf>=4.25.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-asyncio>=0.23.0",
    "grpcio-tools>=1.60.0",
]

[tool.pytest.ini_options]
asyncio_mode = "strict"
testpaths = ["tests"]

[project.urls]
Homepage = "https://github.com/substratum-labs/roche"
Repository = "https://github.com/substratum-labs/roche"
```

Key changes:
- `hatchling` → `maturin` build backend
- Added `[tool.maturin]` config with `python-source` and `include`
- Version bumped to `0.2.0`
- Removed `[project.scripts] roche-install-cli` entry point
- Removed `[project.optional-dependencies] cli` extra
- Removed `[tool.hatch.build.targets.wheel]`

- [ ] **Step 2: Delete _cli_install.py**

```bash
rm sdk/python/src/roche_sandbox/_cli_install.py
```

- [ ] **Step 3: Verify dev install still works**

Run: `source /tmp/roche-test-venv/bin/activate && pip install -e "sdk/python[dev]"`
Expected: Install succeeds (maturin can build in-place for dev)

Note: If maturin is not installed in the venv, run `pip install maturin` first.

- [ ] **Step 4: Run full test suite**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/ -v`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/pyproject.toml
git rm sdk/python/src/roche_sandbox/_cli_install.py
git commit -m "feat(python-sdk): migrate build from hatchling to maturin, bundle binaries"
```

---

## Chunk 5: CI Workflow

### Task 7: Create cross-platform wheel build workflow

**Files:**
- Create: `.github/workflows/python-release.yml`

- [ ] **Step 1: Create workflow file**

Create `.github/workflows/python-release.yml`:
```yaml
name: Python SDK Release

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:

jobs:
  build-binaries:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
            use_cross: true
          - target: x86_64-apple-darwin
            os: macos-13
          - target: aarch64-apple-darwin
            os: macos-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Install cross
        if: matrix.use_cross
        run: cargo install cross --git https://github.com/cross-rs/cross
      - name: Build (native)
        if: ${{ !matrix.use_cross }}
        run: cargo build --release --target ${{ matrix.target }} -p roche-cli -p roche-daemon
      - name: Build (cross)
        if: matrix.use_cross
        run: cross build --release --target ${{ matrix.target }} -p roche-cli -p roche-daemon
      - uses: actions/upload-artifact@v4
        with:
          name: binaries-${{ matrix.target }}
          path: |
            target/${{ matrix.target }}/release/roche
            target/${{ matrix.target }}/release/roched

  build-wheels:
    needs: build-binaries
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-13
          - target: aarch64-apple-darwin
            os: macos-latest
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: binaries-${{ matrix.target }}
          path: binaries
      - name: Place binaries in package
        run: |
          mkdir -p sdk/python/src/roche_sandbox/bin
          cp binaries/roche sdk/python/src/roche_sandbox/bin/ || true
          cp binaries/roched sdk/python/src/roche_sandbox/bin/ || true
          chmod +x sdk/python/src/roche_sandbox/bin/* 2>/dev/null || true
      - uses: PyO3/maturin-action@v1
        with:
          command: build
          args: --release -o dist
          working-directory: sdk/python
      - uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.target }}
          path: sdk/python/dist/*.whl

  publish:
    needs: build-wheels
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/')
    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist
      - uses: PyO3/maturin-action@v1
        with:
          command: upload
          args: --skip-existing dist/*
        env:
          MATURIN_PYPI_TOKEN: ${{ secrets.PYPI_TOKEN }}
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/python-release.yml
git commit -m "ci: add cross-platform Python wheel build and publish workflow"
```

---

## Chunk 6: Final Verification

### Task 8: End-to-end verification

- [ ] **Step 1: Run full Rust test suite**

Run: `export PATH="$HOME/.local/bin:$PATH" && cargo test && cargo clippy && cargo fmt --check`
Expected: All pass

- [ ] **Step 2: Run full Python test suite**

Run: `source /tmp/roche-test-venv/bin/activate && pytest sdk/python/tests/ -v`
Expected: All pass

- [ ] **Step 3: Run full TypeScript test suite**

Run: `cd sdk/typescript && npx vitest run`
Expected: All pass

- [ ] **Step 4: Commit and push**

```bash
git push origin feat/execution-trace
```
