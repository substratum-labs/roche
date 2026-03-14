# Phase B-B: Firecracker Provider Design

## Overview

Add a Firecracker-based sandbox provider to Roche, giving users hardware-level VM isolation via lightweight microVMs. Firecracker is AWS's microVM manager — sub-second boot times with strong security boundaries.

**Goal:** Implement `SandboxProvider` + `SandboxLifecycle` traits for Firecracker, with vsock-based command execution.

## Architecture

### Provider Structure

`FirecrackerProvider` has two internal layers:

1. **API Client** (`FirecrackerApiClient`) — raw HTTP over Unix socket to Firecracker's REST API
2. **State Directory** (`StateManager`) — persistent per-VM state at `~/.roche/firecracker/<vm-id>/`

### Cross-Platform Strategy

- Compiles on all platforms (macOS, Linux, Windows)
- Runtime check: returns `ProviderError::Unavailable("Firecracker requires Linux with KVM")` on non-Linux
- Platform-gated code behind `#[cfg(target_os = "linux")]` for vsock and nix-specific calls
- Stub implementations on non-Linux return `ProviderError::Unsupported`

### Kernel & Rootfs

User-provided paths — no download magic:
- `--kernel /path/to/vmlinux` — uncompressed Linux kernel
- `--rootfs /path/to/rootfs.ext4` — ext4 root filesystem image
- Both required when `--provider firecracker`; validated at create time

## VM Lifecycle

### Create Flow

1. Generate UUID for VM ID
2. Create state directory: `~/.roche/firecracker/<vm-id>/`
3. Copy rootfs to state dir (so each VM has its own mutable copy)
4. Spawn `firecracker --api-sock <state-dir>/firecracker.sock`
5. Configure VM via API:
   - `PUT /boot-source` — kernel path + boot args
   - `PUT /drives/rootfs` — rootfs drive
   - `PUT /machine-config` — vCPU count, memory (from config)
   - `PUT /vsock` — vsock device with guest CID
6. `PUT /actions` — start the VM
7. Write `metadata.json` to state dir (ID, config, PID, expires_at, CID)

### Destroy Flow

1. Read PID from state dir
2. `SIGKILL` the firecracker process
3. Wait for process exit
4. Remove entire state directory

### Pause / Unpause

- `PATCH /vm` with `state: "Paused"` or `state: "Resumed"` via Firecracker API

### List

- Scan `~/.roche/firecracker/*/metadata.json`
- Check if PID is still alive
- Return `SandboxInfo` for each live VM

### GC (Garbage Collect)

- List all VMs, destroy those past `expires_at`

## Firecracker API Client

`FirecrackerApiClient` wraps hyper HTTP client over Unix socket:

```rust
struct FirecrackerApiClient {
    socket_path: PathBuf,
}

impl FirecrackerApiClient {
    async fn put_boot_source(&self, kernel: &str, boot_args: &str) -> Result<(), ProviderError>;
    async fn put_drive(&self, id: &str, path: &str, is_root: bool, readonly: bool) -> Result<(), ProviderError>;
    async fn put_machine_config(&self, vcpu: u8, mem_mib: u64) -> Result<(), ProviderError>;
    async fn put_vsock(&self, guest_cid: u32) -> Result<(), ProviderError>;
    async fn start(&self) -> Result<(), ProviderError>;
    async fn pause(&self) -> Result<(), ProviderError>;
    async fn resume(&self) -> Result<(), ProviderError>;
}
```

Uses `hyper` + `hyper-util` + `http-body-util` for HTTP, connecting to the Unix socket.

## Command Execution via Vsock

### Architecture

- **Host side** (roche): connects to guest via vsock (CID + port 52)
- **Guest side** (roche-agent): lightweight agent pre-installed in rootfs, listens on vsock port 52

### Protocol

Request (JSON over vsock):
```json
{"command": ["python3", "-c", "print('hello')"], "timeout_secs": 30}
```

Response (JSON over vsock):
```json
{"exit_code": 0, "stdout": "hello\n", "stderr": ""}
```

### Platform Gating

- `tokio-vsock` only available on Linux
- `#[cfg(target_os = "linux")]` for vsock connection code
- Non-Linux: `ProviderError::Unsupported("vsock exec requires Linux")`

## State Directory

```
~/.roche/firecracker/<vm-id>/
├── firecracker.sock     # API socket
├── firecracker.pid      # Process ID
├── metadata.json        # VM metadata (config, expires_at, cid)
└── rootfs.ext4          # Copy of rootfs (mutable per-VM)
```

`metadata.json` schema:
```json
{
  "id": "uuid-string",
  "provider": "firecracker",
  "image": "custom",
  "pid": 12345,
  "cid": 3,
  "kernel": "/path/to/vmlinux",
  "expires_at": 1710345600,
  "created_at": 1710345300,
  "config": { "memory": "512m", "cpus": 1.0, "network": false }
}
```

## Config Changes

`SandboxConfig` gets two new optional fields:

```rust
pub kernel: Option<String>,   // Path to vmlinux (Firecracker only)
pub rootfs: Option<String>,   // Path to rootfs.ext4 (Firecracker only)
```

Both default to `None`. Validated at Firecracker provider create time.

## CLI Changes

New flags on `create` subcommand:
- `--kernel <PATH>` — path to uncompressed Linux kernel
- `--rootfs <PATH>` — path to ext4 rootfs image

Provider dispatch in `main.rs`: match on `--provider` value to instantiate `DockerProvider` or `FirecrackerProvider`.

## File Structure

```
crates/roche-core/src/provider/
├── mod.rs                    # Existing — add `pub mod firecracker;`
├── docker.rs                 # Existing — unchanged
└── firecracker/
    ├── mod.rs                # FirecrackerProvider + trait impls
    ├── api_client.rs         # FirecrackerApiClient (HTTP over Unix socket)
    ├── state.rs              # StateManager (state directory operations)
    └── vsock_exec.rs         # Vsock exec logic (platform-gated)
```

## Dependencies (new)

- `hyper` (1.x) — HTTP client for Unix socket API
- `hyper-util` — hyper utilities (client, TokioExecutor)
- `http-body-util` — body utilities for hyper
- `tokio-vsock` — vsock support (Linux only, behind cfg)
- `uuid` — VM ID generation
- `nix` — Unix process management (signal, waitpid)

## Trait Implementation

| Trait | Implemented | Notes |
|-------|------------|-------|
| `SandboxProvider` | Yes | create, exec, destroy, list |
| `SandboxLifecycle` | Yes | pause, unpause, gc |
| `SandboxFileOps` | No (MVP) | Requires SSH or virtio-fs — deferred |

## Testing Strategy

- **Unit tests**: mock API client responses, test state directory operations, test config validation
- **Integration tests**: marked `#[ignore]`, require Linux + KVM + Firecracker binary
- **Platform tests**: verify graceful `Unavailable` error on non-Linux

## Non-Goals (MVP)

- Jailer support (security hardening — deferred)
- `SandboxFileOps` (requires SSH or virtio-fs complexity)
- Network interface configuration (sandboxes are network-isolated by default)
- Automatic kernel/rootfs download
- roche-agent binary (user pre-installs in rootfs for MVP)
