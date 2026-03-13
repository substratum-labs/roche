# Phase B-A: Enhanced Docker Provider — Design Spec

**版本:** 0.2.0
**日期:** 2026-03-13
**状态:** Approved

---

## 1. Overview

Phase B-A enhances the Docker provider with file transfer, automatic timeout cleanup, pause/unpause, and batch operations. It also introduces a layered trait architecture to prepare for future providers with varying capabilities.

**Scope:** 5 feature areas, all additive — existing `SandboxProvider` trait unchanged.

---

## 2. Layered Trait Architecture

### Problem

The current `SandboxProvider` trait bundles all operations. Future providers (Firecracker, WASM) may not support all capabilities (e.g., WASM can't `pause`, Firecracker may not support `docker cp`-style file ops).

### Design

Keep existing `SandboxProvider` trait unchanged. Add two new traits in `provider/mod.rs`:

```rust
/// File operations capability.
#[allow(async_fn_in_trait)]
pub trait SandboxFileOps {
    /// Copy a file from host to sandbox.
    async fn copy_to(
        &self,
        id: &SandboxId,
        src: &std::path::Path,
        dest: &str,
    ) -> Result<(), ProviderError>;

    /// Copy a file from sandbox to host.
    async fn copy_from(
        &self,
        id: &SandboxId,
        src: &str,
        dest: &std::path::Path,
    ) -> Result<(), ProviderError>;
}

/// Lifecycle management capability.
#[allow(async_fn_in_trait)]
pub trait SandboxLifecycle {
    /// Pause a sandbox (freeze all processes).
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Unpause a sandbox.
    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError>;

    /// Garbage collect: destroy all expired sandboxes. Returns IDs of destroyed sandboxes.
    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError>;
}
```

`DockerProvider` implements all three traits. Future providers implement only what they support.

### ProviderError New Variants

```rust
pub enum ProviderError {
    // ... existing ...
    #[error("operation not supported by this provider: {0}")]
    Unsupported(String),

    #[error("file operation failed: {0}")]
    FileFailed(String),

    #[error("sandbox is paused: {0}")]
    Paused(SandboxId),
}
```

**Files touched:**
- `crates/roche-core/src/provider/mod.rs` — add traits and error variants

---

## 3. File Transfer

### 3.1 `roche cp` Command

Bidirectional file copy using `docker cp`:

```bash
# Host → sandbox
roche cp ./script.py sandbox_id:/app/script.py

# Sandbox → host
roche cp sandbox_id:/app/result.json ./result.json
```

**CLI definition:**

```rust
/// Copy files between host and sandbox
Cp {
    /// Source path (local path or sandbox_id:/path)
    src: String,
    /// Destination path (local path or sandbox_id:/path)
    dest: String,
}
```

**Path parsing:** `split_once(':')` determines which side is sandbox. If `src` contains `:`, call `copy_from`; if `dest` contains `:`, call `copy_to`. Both or neither having `:` is an error.

**Implementation:** `DockerProvider::copy_to` runs `docker cp <host_path> <container_id>:<container_path>`. `copy_from` runs `docker cp <container_id>:<container_path> <host_path>`.

### 3.2 Volume Mounts (Create-time)

```bash
roche create --mount /host/data:/sandbox/data:ro --mount /host/out:/sandbox/out:rw
```

Format: `host_path:container_path[:ro|rw]`, default `ro` (AI-safe).

**New types in `types.rs`:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub host_path: String,
    pub container_path: String,
    /// Default: true (readonly, AI-safe)
    pub readonly: bool,
}
```

**`SandboxConfig` change:**

```rust
pub struct SandboxConfig {
    // ... existing fields ...
    /// Volume mounts.
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
}
```

**`build_create_args` change:** For each mount, emit `-v host_path:container_path:ro` or `-v host_path:container_path:rw`.

**Python SDK:** Add `Mount` dataclass to `types.py`, add `mounts: list[Mount]` to `SandboxConfig`, forward as `--mount` flags.

### Testing

- Unit: `build_create_args` with mounts produces correct `-v` flags
- Unit: cp path parsing (host→sandbox, sandbox→host, both-colon error, no-colon error)
- Python: mock tests for `copy_to`, `copy_from`, mount flag forwarding

**Files touched:**
- `crates/roche-core/src/types.rs` — add `MountConfig`, add `mounts` to `SandboxConfig`
- `crates/roche-core/src/provider/docker.rs` — implement `SandboxFileOps`, update `build_create_args`
- `crates/roche-cli/src/main.rs` — add `Cp` command, `--mount` flag on `Create`
- `sdk/python/roche/types.py` — add `Mount` class, `mounts` field
- `sdk/python/roche/client.py` — add `copy_to`, `copy_from` methods
- `sdk/python/tests/test_client.py` — mock tests

---

## 4. Timeout Cleanup (Label + GC)

### Create-time Label

When creating a sandbox, write an expiration timestamp as a Docker label:

```
--label roche.expires=<unix_timestamp>
```

Calculated as `current_time + config.timeout_secs`. Existing `roche.managed=true` label is retained.

**`build_create_args` change:**

```rust
let expires = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() + config.timeout_secs;
args.extend(["--label".into(), format!("roche.expires={expires}")]);
```

### `roche gc` Command

```bash
roche gc              # Destroy all expired sandboxes
roche gc --dry-run    # List expired sandboxes without destroying
roche gc --all        # Destroy ALL roche-managed sandboxes (regardless of expiry)
```

**Implementation:**
1. Query Docker: `docker ps -a --filter label=roche.managed=true --format "{{.ID}}\t{{.Label \"roche.expires\"}}"`
2. Parse each container's `roche.expires` label, compare to current time
3. Destroy expired containers via existing `destroy` method
4. Return list of destroyed sandbox IDs

**CLI definition:**

```rust
/// Garbage collect expired sandboxes
Gc {
    /// Only list expired sandboxes, don't destroy
    #[arg(long)]
    dry_run: bool,

    /// Destroy ALL roche-managed sandboxes (ignore expiry)
    #[arg(long)]
    all: bool,
}
```

`--dry-run` and `--all` are CLI-layer concerns, not in the trait. The trait's `gc()` always destroys expired sandboxes.

### `list` Enhancement

`SandboxInfo` gains `expires_at: Option<u64>`. `roche list` shows remaining time:

```
ID               STATUS     PROVIDER   EXPIRES    IMAGE
abc123def456     running    docker     4m32s      python:3.12-slim
def456ghi789     paused     docker     12m05s     node:20-slim
```

### Testing

- Unit: expiry calculation logic
- Unit: gc filtering (expired vs not-expired containers)
- Python: mock tests for `gc`, `gc_dry_run`

**Files touched:**
- `crates/roche-core/src/types.rs` — add `expires_at` to `SandboxInfo`
- `crates/roche-core/src/provider/docker.rs` — update `build_create_args` (label), update `list` (parse expires), implement `gc`
- `crates/roche-cli/src/main.rs` — add `Gc` command, update `list` display
- `sdk/python/roche/client.py` — add `gc`, `gc_dry_run` methods
- `sdk/python/tests/test_client.py` — mock tests

---

## 5. Pause / Unpause

### CLI

```bash
roche pause <SANDBOX_ID>
roche unpause <SANDBOX_ID>
```

**Implementation:** Maps directly to `docker pause` / `docker unpause`.

### SandboxStatus Change

```rust
pub enum SandboxStatus {
    Running,
    Paused,    // NEW
    Stopped,
    Failed,
}
```

`parse_status` maps Docker's `"paused"` state to `SandboxStatus::Paused`.

Executing `exec` on a paused sandbox returns `ProviderError::Paused(id)`.

### Python SDK

```python
client.pause(sandbox_id)
client.unpause(sandbox_id)

# On Sandbox context manager
sb.pause()
sb.unpause()
```

### Testing

- Unit: `parse_status("paused")` → `SandboxStatus::Paused`
- Python: mock tests for pause/unpause CLI argument forwarding

**Files touched:**
- `crates/roche-core/src/types.rs` — add `Paused` variant
- `crates/roche-core/src/provider/docker.rs` — implement `pause`, `unpause` in `SandboxLifecycle`
- `crates/roche-core/src/provider/mod.rs` — (already covered in Section 2)
- `crates/roche-cli/src/main.rs` — add `Pause`, `Unpause` commands
- `sdk/python/roche/client.py` — add `pause`, `unpause` methods
- `sdk/python/tests/test_client.py` — mock tests

---

## 6. Batch Operations

### Batch Create

```bash
roche create --count 5 --provider docker --memory 512m
```

Output: one ID per line.

**Implementation:** CLI loops `provider.create()` N times. `--count` defaults to 1. On failure, already-created sandboxes are NOT rolled back; error is printed to stderr, successful IDs still printed to stdout.

### Batch Destroy

```bash
roche destroy id1 id2 id3     # Multiple IDs
roche destroy --all            # All roche-managed sandboxes
```

Current `destroy` accepts a single ID. Change to accept `Vec<String>`. `--all` calls `list()` to get all IDs, then destroys each.

**CLI definition changes:**

```rust
/// Destroy sandboxes
Destroy {
    /// Sandbox IDs (one or more)
    #[arg(required_unless_present = "all")]
    ids: Vec<String>,

    /// Destroy ALL roche-managed sandboxes
    #[arg(long)]
    all: bool,
}
```

### Python SDK

```python
ids = client.create_many(config, count=5)   # Returns list[str]
client.destroy_many(["id1", "id2", "id3"])
client.destroy_all()
```

### Testing

- Unit: verify `--count` creates N sandboxes (mock provider)
- Unit: verify `--all` calls list + destroy for each
- Python: mock tests for `create_many`, `destroy_many`, `destroy_all`

**Files touched:**
- `crates/roche-cli/src/main.rs` — modify `Create` (add `--count`), modify `Destroy` (multiple IDs + `--all`)
- `sdk/python/roche/client.py` — add `create_many`, `destroy_many`, `destroy_all`
- `sdk/python/tests/test_client.py` — mock tests

---

## 7. Out of Scope

- Recursive directory copy (`cp` handles single files; use tar for directories)
- Hot-plugging mounts (mounts only at create time)
- Scheduled gc (deferred to Phase C daemon mode)
- New providers (Phase B-B: Firecracker)
- TypeScript SDK (Phase B-D)
