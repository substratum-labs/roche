# Daemon Auto-Management — Design Spec

**Date:** 2026-03-18
**Version:** 0.2
**Status:** Draft
**Parent Spec:** `docs/superpowers/specs/2026-03-18-execution-trace-design.md`

## Overview

Bundle `roched` (daemon) and `roche` (CLI) binaries into the Python SDK wheel using maturin, and add auto-spawn logic so that `pip install roche-sandbox` gives users a complete, zero-configuration experience. The daemon starts automatically on first SDK use and runs long-lived by default.

**Divergences from parent spec:** This spec supersedes the parent on: (a) daemon binary name is `roched` (not `rochd`), (b) both `roched` and `roche` CLI are bundled (parent only bundles daemon), (c) default idle timeout is `0`/long-lived (parent defaults to 300s), (d) Windows deferred (parent included it). These changes reflect decisions made during brainstorming.

## Scope

- Migrate Python SDK build from hatchling to maturin
- Bundle both `roched` and `roche` binaries in platform-specific wheels
- Auto-spawn daemon from bundled binary when no running daemon detected
- Configurable idle timeout (default: no timeout, long-lived)
- Seccomp trace profile bundling
- CI workflow for cross-platform wheel builds
- Remove `_cli_install.py` download hack

Out of scope: TypeScript SDK binary bundling (separate spec), Windows support (Linux + macOS only for MVP).

**Already implemented (no changes needed):** `roche daemon start/stop/status` CLI subcommands already exist in `crates/roche-cli/src/main.rs` (lines 153-335).

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Build system | Maturin with `python-source` layout | Bundles pre-built binaries into platform-specific wheels via `[tool.maturin] data` config. No Rust-Python FFI. |
| Binary bundling | Both `roched` + `roche` CLI in wheel | One `pip install` gives everything. Replaces `_cli_install.py` hack. |
| Binary name | Keep `roched` (existing Cargo name) | Already in use, no rename needed. Supersedes parent spec's `rochd`. |
| Daemon lifetime | Long-lived by default (no auto-exit) | Daemon is a service, not a temp process. Faster subsequent calls. Supersedes parent spec's 300s default. |
| Idle timeout | Configurable via `--idle-timeout` flag and `ROCHE_DAEMON_IDLE_TIMEOUT` env var. Default `0` (no timeout). | Power users who want auto-cleanup can set a timeout. |
| Platforms | `manylinux_x86_64`, `manylinux_aarch64`, `macosx_arm64`, `macosx_x86_64` | Covers >95% of users. Windows deferred (supersedes parent spec). |
| Auto-spawn | Transparent on first `Roche()` init | "It just works" — matches Docker Desktop UX. |
| Daemon log file | `~/.roche/daemon.log` | Matches existing CLI `daemon start` implementation. |

## Architecture

### Binary Layout in Wheel

```
roche_sandbox-0.2.0-cp312-cp312-macosx_arm64.whl
├── roche_sandbox/
│   ├── __init__.py
│   ├── bin/
│   │   ├── roched          # daemon binary
│   │   └── roche           # CLI binary
│   └── ...
```

Maturin places binaries via `[tool.maturin] data` config pointing to a data directory that CI populates with pre-built binaries.

### Auto-Spawn Flow

```
Roche().__init__
  │
  ├─ detect_daemon()  ──→ daemon.json exists & PID alive?
  │     │                      │
  │     │ yes ─────────────────→ GrpcTransport(port)
  │     │
  │     │ no
  │     ▼
  ├─ _find_bundled_binary("roched")
  │     │
  │     │ found ───→ _spawn_daemon(path) ───→ _wait_for_ready(3s, gRPC check) ───→ GrpcTransport(50051)
  │     │
  │     │ not found
  │     ▼
  └─ CliTransport(binary)  ← degraded mode (trace: duration-only)
```

### Python SDK Changes

#### `_find_bundled_binary(name: str) -> Path | None`

```python
def _find_bundled_binary(name: str) -> Path | None:
    """Locate a bundled binary in the roche_sandbox package."""
    bin_dir = Path(__file__).parent / "bin"
    binary = bin_dir / name
    if binary.exists() and os.access(binary, os.X_OK):
        return binary
    return None
```

#### `_spawn_daemon(roched_path: Path) -> None`

```python
def _spawn_daemon(roched_path: Path) -> None:
    """Spawn roched as a detached background process."""
    roche_dir = Path.home() / ".roche"
    roche_dir.mkdir(parents=True, exist_ok=True)
    log_path = roche_dir / "daemon.log"  # matches existing CLI daemon start

    # Respect ROCHE_DAEMON_IDLE_TIMEOUT if set
    args = [str(roched_path)]
    idle_timeout = os.environ.get("ROCHE_DAEMON_IDLE_TIMEOUT")
    if idle_timeout:
        args.extend(["--idle-timeout", idle_timeout])

    with open(log_path, "a") as log_file:
        subprocess.Popen(
            args,
            stdout=log_file,
            stderr=log_file,
            start_new_session=True,  # detach from parent process
        )
```

#### `_wait_for_daemon_ready(timeout: float = 3.0, port: int = 50051) -> bool`

Polls both `daemon.json` AND gRPC connectivity to avoid the race where daemon.json is written before the gRPC server is listening:

```python
import socket

def _wait_for_daemon_ready(timeout: float = 3.0, port: int = 50051) -> bool:
    """Poll until daemon is ready to accept gRPC connections."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        daemon = detect_daemon()
        if daemon is not None:
            # Verify gRPC port is actually accepting connections
            try:
                with socket.create_connection(("127.0.0.1", daemon["port"]), timeout=0.5):
                    return True
            except (ConnectionRefusedError, OSError):
                pass
        time.sleep(0.1)
    return False
```

#### Updated `AsyncRoche.__init__` transport selection

```python
# In client.py, replace current auto-detect logic:
if mode == "auto":
    daemon = detect_daemon()
    if daemon is not None:
        self._transport = GrpcTransport(port=daemon["port"])
    else:
        # Try auto-spawn from bundled binary
        roched_path = _find_bundled_binary("roched")
        if roched_path:
            _spawn_daemon(roched_path)
            if _wait_for_daemon_ready(timeout=3.0):
                daemon = detect_daemon()
                self._transport = GrpcTransport(port=daemon["port"])
            else:
                # Spawn failed or timed out — fall back to CLI
                self._transport = CliTransport(binary=binary)
        else:
            self._transport = CliTransport(binary=binary)
```

#### Updated `CliTransport` binary resolution

With the CLI bundled, `CliTransport` should also check the bundled binary:

```python
def _resolve_binary(self, binary: str) -> str:
    # Check bundled first
    bundled = _find_bundled_binary("roche")
    if bundled:
        return str(bundled)
    # Fall back to PATH
    return binary
```

### Daemon Idle Timeout

#### `roched` CLI changes

```
roched --idle-timeout <seconds>    # 0 = no timeout (default), >0 = auto-exit after N seconds idle
```

Also respects `ROCHE_DAEMON_IDLE_TIMEOUT` env var (CLI flag takes precedence).

#### Implementation in `crates/roche-daemon/src/main.rs`

```rust
// Add to Args struct:
#[arg(long, default_value = "0", env = "ROCHE_DAEMON_IDLE_TIMEOUT")]
idle_timeout: u64,  // 0 = disabled

// After server starts, spawn idle monitor task:
if args.idle_timeout > 0 {
    let last_rpc = server.last_rpc_timestamp(); // Arc<AtomicU64>
    let timeout = Duration::from_secs(args.idle_timeout);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let elapsed = Instant::now() - last_rpc.load();
            if elapsed > timeout {
                // Graceful shutdown
                cleanup_daemon_json();
                std::process::exit(0);
            }
        }
    });
}
```

Each RPC handler updates `last_rpc_timestamp` via `Arc<AtomicU64>` (epoch millis).

### Seccomp Profile Bundling

The `seccomp-trace.json` profile is embedded in `roched` at compile time:

```rust
// In crates/roche-daemon/src/main.rs:
const SECCOMP_TRACE_PROFILE: &str = include_str!("seccomp-trace.json");

// On startup:
let seccomp_path = roche_dir.join("seccomp-trace.json");
if !seccomp_path.exists() {
    std::fs::write(&seccomp_path, SECCOMP_TRACE_PROFILE)?;
}
```

The profile is Docker's default seccomp profile with `SCMP_ACT_LOG` for monitored syscalls: `connect`, `open`, `openat`, `unlink`, `unlinkat`.

The Docker provider uses the **absolute path** to this file:
```rust
// In docker.rs build_create_args():
if config.trace_enabled {
    let seccomp_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".roche")
        .join("seccomp-trace.json");
    if seccomp_path.exists() {
        args.extend([
            "--security-opt".into(),
            format!("seccomp={}", seccomp_path.display()),
        ]);
    }
}
```

### Maturin Configuration

```toml
# sdk/python/pyproject.toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[tool.maturin]
# Pure Python package with bundled data (pre-built binaries)
# Binaries are placed in roche_sandbox/bin/ by CI before maturin build
python-source = "src"
module-name = "roche_sandbox"
include = ["roche_sandbox/bin/*"]

[project]
name = "roche-sandbox"
version = "0.2.0"
requires-python = ">=3.10"
# ... rest unchanged, but REMOVE:
# - [project.scripts] roche-install-cli entry point
# - [project.optional-dependencies] cli extra
```

**Note:** maturin `bindings = "bin"` is NOT correct for this use case (that builds a Rust binary crate as a Python entry point). Instead, we use maturin's `python-source` layout with pre-built binaries included as package data.

**CI workflow builds binaries first, then runs maturin to package them.**

### CI Workflow

```yaml
# .github/workflows/python-release.yml
jobs:
  build-binaries:
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest  # uses cross for cross-compilation
          - target: x86_64-apple-darwin
            os: macos-13       # Intel macOS
          - target: aarch64-apple-darwin
            os: macos-latest   # ARM macOS
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      # Install cross-compilation tools for aarch64-linux
      - name: Install cross
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: cargo install cross --git https://github.com/cross-rs/cross
      # Build binaries
      - name: Build (native)
        if: matrix.target != 'aarch64-unknown-linux-gnu'
        run: cargo build --release --target ${{ matrix.target }} -p roche-cli -p roche-daemon
      - name: Build (cross)
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: cross build --release --target ${{ matrix.target }} -p roche-cli -p roche-daemon
      - uses: actions/upload-artifact@v4
        with:
          name: binaries-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/{roche,roched}

  build-wheels:
    needs: build-binaries
    strategy:
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            manylinux: auto
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            manylinux: auto
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
          cp binaries/roche sdk/python/src/roche_sandbox/bin/
          cp binaries/roched sdk/python/src/roche_sandbox/bin/
          chmod +x sdk/python/src/roche_sandbox/bin/*
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

## Backward Compatibility

| Scenario | Behavior |
|---|---|
| `pip install roche-sandbox` (new maturin wheel) | Gets both binaries, daemon auto-spawns on first use |
| Existing manual `roched` running | SDK detects and reuses, no spawn |
| Old SDK (hatchling wheel) | No bundled binaries, CLI fallback as before |
| `ROCHE_DAEMON_IDLE_TIMEOUT=300` | Daemon auto-exits after 5 min idle |
| `roche daemon stop` | Graceful shutdown, cleans daemon.json (already implemented) |
| No Docker on system | Daemon starts but provider operations fail with clear error |

## Testing Strategy

### Unit Tests
- `_find_bundled_binary()` returns correct path when binary exists, None when missing
- `_wait_for_daemon_ready()` returns True when daemon.json appears and port is open, False on timeout
- Auto-detect logic: daemon running → GrpcTransport, no daemon + bundled → spawn + GrpcTransport, no binary → CliTransport
- Idle timeout: daemon exits after configured timeout (mock time)

### Integration Tests
- Full lifecycle: spawn daemon → exec command → verify trace → stop daemon
- Idle timeout: spawn with `--idle-timeout 2` → wait → verify daemon exited

### CI Tests
- Wheel builds for all 4 platforms
- Wheel installs and `roche --version` / `roched --version` work

## Files Changed

### New Files
- `sdk/python/src/roche_sandbox/bin/` — directory for bundled binaries (populated by CI, .gitignored)
- `crates/roche-daemon/src/seccomp-trace.json` — seccomp profile with SCMP_ACT_LOG
- `.github/workflows/python-release.yml` — cross-platform wheel build + publish
- `sdk/python/tests/test_auto_spawn.py` — auto-spawn unit tests

### Modified Files
- `sdk/python/pyproject.toml` — hatchling → maturin build system, remove `[project.scripts] roche-install-cli` and `[project.optional-dependencies] cli`
- `sdk/python/src/roche_sandbox/client.py` — auto-spawn logic in transport selection
- `sdk/python/src/roche_sandbox/transport/cli.py` — bundled binary resolution
- `sdk/python/src/roche_sandbox/daemon.py` — add `_find_bundled_binary`, `_spawn_daemon`, `_wait_for_daemon_ready`
- `crates/roche-daemon/src/main.rs` — idle timeout (`--idle-timeout` flag + env var), seccomp profile write on startup

### Removed Files
- `sdk/python/src/roche_sandbox/_cli_install.py` — replaced by bundled binary
