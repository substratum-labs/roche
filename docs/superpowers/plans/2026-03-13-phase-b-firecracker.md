# Phase B-B: Firecracker Provider Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Firecracker microVM provider to Roche implementing `SandboxProvider` + `SandboxLifecycle` traits with vsock-based command execution.

**Architecture:** FirecrackerProvider wraps two internal layers: an API client (HTTP over Unix socket to Firecracker's REST API) and a state manager (persistent per-VM state at `~/.roche/firecracker/<vm-id>/`). Command execution uses virtio-vsock to communicate with a guest agent. The provider compiles on all platforms but returns `ProviderError::Unavailable` at runtime on non-Linux.

**Tech Stack:** Rust, hyper 1.x (HTTP over Unix socket), tokio-vsock (Linux-only), uuid, nix, serde_json

**Spec:** `docs/superpowers/specs/2026-03-13-phase-b-firecracker-design.md`

---

## Chunk 1: Foundation (Dependencies, Types, State Manager)

### Task 1: Add Dependencies

**Files:**
- Modify: `crates/roche-core/Cargo.toml`

- [ ] **Step 1: Add new dependencies to roche-core Cargo.toml**

Add the following to `[dependencies]` in `crates/roche-core/Cargo.toml`:

```toml
hyper = { version = "1", features = ["client", "http1"] }
hyper-util = { version = "0.1", features = ["client-legacy", "tokio", "http1"] }
http-body-util = "0.1"
uuid = { version = "1", features = ["v4"] }
nix = { version = "0.29", features = ["signal", "process"] }
dirs = "6"
```

For `tokio-vsock`, add it behind a cfg so it only compiles on Linux:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
tokio-vsock = "0.6"
```

- [ ] **Step 2: Verify the project compiles**

Run: `cargo build`
Expected: Compiles successfully with new dependencies resolved.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/Cargo.toml Cargo.lock
git commit -m "feat(firecracker): add dependencies for Firecracker provider"
```

---

### Task 2: Add kernel/rootfs Fields to SandboxConfig

**Files:**
- Modify: `crates/roche-core/src/types.rs:8-42` (SandboxConfig struct)

- [ ] **Step 1: Write failing test for new config fields**

Add to the bottom of `crates/roche-core/src/types.rs`, inside a new `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_config_default_has_no_kernel_rootfs() {
        let config = SandboxConfig::default();
        assert!(config.kernel.is_none());
        assert!(config.rootfs.is_none());
    }

    #[test]
    fn test_sandbox_config_with_kernel_rootfs() {
        let config = SandboxConfig {
            kernel: Some("/path/to/vmlinux".to_string()),
            rootfs: Some("/path/to/rootfs.ext4".to_string()),
            ..Default::default()
        };
        assert_eq!(config.kernel.as_deref(), Some("/path/to/vmlinux"));
        assert_eq!(config.rootfs.as_deref(), Some("/path/to/rootfs.ext4"));
    }

    #[test]
    fn test_sandbox_config_serde_roundtrip_with_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".to_string(),
            kernel: Some("/boot/vmlinux".to_string()),
            rootfs: Some("/images/rootfs.ext4".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kernel.as_deref(), Some("/boot/vmlinux"));
        assert_eq!(parsed.rootfs.as_deref(), Some("/images/rootfs.ext4"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p roche-core -- types::tests`
Expected: FAIL — `kernel` and `rootfs` fields don't exist yet.

- [ ] **Step 3: Add kernel and rootfs fields to SandboxConfig**

In `crates/roche-core/src/types.rs`, add two new fields to `SandboxConfig` after `mounts`:

```rust
    /// Path to uncompressed Linux kernel (Firecracker only).
    #[serde(default)]
    pub kernel: Option<String>,

    /// Path to ext4 rootfs image (Firecracker only).
    #[serde(default)]
    pub rootfs: Option<String>,
```

Update `Default for SandboxConfig` to include:

```rust
            kernel: None,
            rootfs: None,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p roche-core -- types::tests`
Expected: All 3 tests PASS.

- [ ] **Step 5: Verify full project still compiles**

Run: `cargo build`
Expected: Compiles. The Docker provider and CLI both still work because the new fields have defaults.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-core/src/types.rs
git commit -m "feat(firecracker): add kernel and rootfs fields to SandboxConfig"
```

---

### Task 3: State Manager

**Files:**
- Create: `crates/roche-core/src/provider/firecracker/state.rs`

The state manager handles the per-VM state directory at `~/.roche/firecracker/<vm-id>/`. It creates/reads/removes state directories and manages `metadata.json`.

- [ ] **Step 1: Create firecracker directory and empty state.rs**

Create directory `crates/roche-core/src/provider/firecracker/` and file `state.rs` with:

```rust
use crate::provider::ProviderError;
use crate::types::SandboxId;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Persistent metadata for a Firecracker VM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetadata {
    pub id: SandboxId,
    pub provider: String,
    pub image: String,
    pub pid: u32,
    pub cid: u32,
    pub kernel: String,
    pub expires_at: Option<u64>,
    pub created_at: u64,
}

/// Manages per-VM state directories under `~/.roche/firecracker/`.
pub struct StateManager {
    base_dir: PathBuf,
}

impl StateManager {
    /// Create a new StateManager. Uses `~/.roche/firecracker/` by default.
    pub fn new() -> Result<Self, ProviderError> {
        let home = dirs::home_dir()
            .ok_or_else(|| ProviderError::Unavailable("cannot determine home directory".into()))?;
        let base_dir = home.join(".roche").join("firecracker");
        Ok(Self { base_dir })
    }

    /// Create with a custom base directory (for testing).
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Path to a specific VM's state directory.
    pub fn vm_dir(&self, id: &str) -> PathBuf {
        self.base_dir.join(id)
    }

    /// Path to the API socket for a VM.
    pub fn socket_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("firecracker.sock")
    }

    /// Path to the rootfs copy for a VM.
    pub fn rootfs_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("rootfs.ext4")
    }

    /// Path to the metadata file for a VM.
    pub fn metadata_path(&self, id: &str) -> PathBuf {
        self.vm_dir(id).join("metadata.json")
    }

    /// Create the state directory for a new VM.
    pub fn create_vm_dir(&self, id: &str) -> Result<PathBuf, ProviderError> {
        let dir = self.vm_dir(id);
        std::fs::create_dir_all(&dir)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to create state dir: {e}")))?;
        Ok(dir)
    }

    /// Copy the rootfs image into the VM's state directory.
    pub fn copy_rootfs(&self, id: &str, source: &Path) -> Result<PathBuf, ProviderError> {
        let dest = self.rootfs_path(id);
        std::fs::copy(source, &dest).map_err(|e| {
            ProviderError::CreateFailed(format!("failed to copy rootfs: {e}"))
        })?;
        Ok(dest)
    }

    /// Write VM metadata to disk.
    pub fn write_metadata(&self, metadata: &VmMetadata) -> Result<(), ProviderError> {
        let path = self.metadata_path(&metadata.id);
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to serialize metadata: {e}")))?;
        std::fs::write(&path, json)
            .map_err(|e| ProviderError::CreateFailed(format!("failed to write metadata: {e}")))?;
        Ok(())
    }

    /// Read VM metadata from disk.
    pub fn read_metadata(&self, id: &str) -> Result<VmMetadata, ProviderError> {
        let path = self.metadata_path(id);
        let json = std::fs::read_to_string(&path)
            .map_err(|_| ProviderError::NotFound(id.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|e| ProviderError::ExecFailed(format!("corrupt metadata: {e}")))
    }

    /// Remove the entire state directory for a VM.
    pub fn remove_vm_dir(&self, id: &str) -> Result<(), ProviderError> {
        let dir = self.vm_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| {
                ProviderError::ExecFailed(format!("failed to remove state dir: {e}"))
            })?;
        }
        Ok(())
    }

    /// List all VM IDs by scanning subdirectories.
    pub fn list_vm_ids(&self) -> Result<Vec<String>, ProviderError> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }
        let entries = std::fs::read_dir(&self.base_dir)
            .map_err(|e| ProviderError::Unavailable(format!("cannot read state dir: {e}")))?;

        let mut ids = Vec::new();
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_state_manager() -> (StateManager, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = StateManager::with_base_dir(tmp.path().to_path_buf());
        (mgr, tmp)
    }

    #[test]
    fn test_create_and_remove_vm_dir() {
        let (mgr, _tmp) = temp_state_manager();
        let dir = mgr.create_vm_dir("test-vm-1").unwrap();
        assert!(dir.exists());
        mgr.remove_vm_dir("test-vm-1").unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn test_remove_nonexistent_vm_dir_is_ok() {
        let (mgr, _tmp) = temp_state_manager();
        assert!(mgr.remove_vm_dir("does-not-exist").is_ok());
    }

    #[test]
    fn test_write_and_read_metadata() {
        let (mgr, _tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-meta").unwrap();
        let meta = VmMetadata {
            id: "vm-meta".to_string(),
            provider: "firecracker".to_string(),
            image: "custom".to_string(),
            pid: 12345,
            cid: 3,
            kernel: "/boot/vmlinux".to_string(),
            expires_at: Some(9999999999),
            created_at: 1000000000,
        };
        mgr.write_metadata(&meta).unwrap();
        let read_back = mgr.read_metadata("vm-meta").unwrap();
        assert_eq!(read_back.id, "vm-meta");
        assert_eq!(read_back.pid, 12345);
        assert_eq!(read_back.cid, 3);
    }

    #[test]
    fn test_read_metadata_not_found() {
        let (mgr, _tmp) = temp_state_manager();
        let result = mgr.read_metadata("no-such-vm");
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_rootfs() {
        let (mgr, _tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-rootfs").unwrap();
        // Create a fake rootfs file
        let src_dir = _tmp.path().join("source");
        fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("rootfs.ext4");
        fs::write(&src_file, b"fake rootfs content").unwrap();
        let dest = mgr.copy_rootfs("vm-rootfs", &src_file).unwrap();
        assert!(dest.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "fake rootfs content");
    }

    #[test]
    fn test_list_vm_ids_empty() {
        let (mgr, _tmp) = temp_state_manager();
        let ids = mgr.list_vm_ids().unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn test_list_vm_ids() {
        let (mgr, _tmp) = temp_state_manager();
        mgr.create_vm_dir("vm-a").unwrap();
        mgr.create_vm_dir("vm-b").unwrap();
        let mut ids = mgr.list_vm_ids().unwrap();
        ids.sort();
        assert_eq!(ids, vec!["vm-a", "vm-b"]);
    }

    #[test]
    fn test_path_helpers() {
        let (mgr, _tmp) = temp_state_manager();
        let base = _tmp.path();
        assert_eq!(mgr.socket_path("vm1"), base.join("vm1").join("firecracker.sock"));
        assert_eq!(mgr.rootfs_path("vm1"), base.join("vm1").join("rootfs.ext4"));
        assert_eq!(mgr.metadata_path("vm1"), base.join("vm1").join("metadata.json"));
    }
}
```

- [ ] **Step 2: Add tempfile as a dev-dependency**

In `crates/roche-core/Cargo.toml`, add:

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Create a minimal mod.rs to make it compile**

Create `crates/roche-core/src/provider/firecracker/mod.rs`:

```rust
pub mod state;
```

- [ ] **Step 4: Register the firecracker module**

In `crates/roche-core/src/provider/mod.rs`, add after `pub mod docker;`:

```rust
pub mod firecracker;
```

- [ ] **Step 5: Run the state manager tests**

Run: `cargo test -p roche-core -- provider::firecracker::state::tests`
Expected: All 8 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-core/src/provider/firecracker/ crates/roche-core/src/provider/mod.rs crates/roche-core/Cargo.toml
git commit -m "feat(firecracker): add state manager for VM state directories"
```

---

## Chunk 2: API Client and Vsock Exec

### Task 4: Firecracker API Client

**Files:**
- Create: `crates/roche-core/src/provider/firecracker/api_client.rs`

The API client sends HTTP requests over a Unix socket to the Firecracker process. Each method maps to a Firecracker REST API endpoint.

- [ ] **Step 1: Create api_client.rs**

```rust
use crate::provider::ProviderError;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use std::path::{Path, PathBuf};

/// HTTP client for Firecracker's REST API over a Unix socket.
pub struct FirecrackerApiClient {
    socket_path: PathBuf,
}

impl FirecrackerApiClient {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send a PUT request to the Firecracker API.
    async fn put(&self, path: &str, body: serde_json::Value) -> Result<(), ProviderError> {
        let body_str = serde_json::to_string(&body)
            .map_err(|e| ProviderError::ExecFailed(format!("json serialize: {e}")))?;

        let connector = hyper_util::client::legacy::connect::HttpConnector::new();

        // For Unix socket communication, we use a UDS connector.
        // However, hyper-util doesn't have a built-in Unix connector,
        // so we use tokio's UnixStream directly with hyper.
        let stream = tokio::net::UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| {
                ProviderError::Unavailable(format!(
                    "cannot connect to Firecracker socket {}: {e}",
                    self.socket_path.display()
                ))
            })?;

        let io = hyper_util::rt::TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("HTTP handshake failed: {e}")))?;

        // Spawn the connection driver
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Firecracker API connection error: {e}");
            }
        });

        let req = Request::builder()
            .method("PUT")
            .uri(format!("http://localhost{path}"))
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body_str)))
            .map_err(|e| ProviderError::ExecFailed(format!("request build: {e}")))?;

        let response = sender
            .send_request(req)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("API request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_bytes = http_body_util::BodyExt::collect(response.into_body())
                .await
                .map_err(|e| ProviderError::ExecFailed(format!("read response: {e}")))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes);
            return Err(ProviderError::ExecFailed(format!(
                "Firecracker API error ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    /// Send a PATCH request to the Firecracker API.
    async fn patch(&self, path: &str, body: serde_json::Value) -> Result<(), ProviderError> {
        let body_str = serde_json::to_string(&body)
            .map_err(|e| ProviderError::ExecFailed(format!("json serialize: {e}")))?;

        let stream = tokio::net::UnixStream::connect(&self.socket_path)
            .await
            .map_err(|e| {
                ProviderError::Unavailable(format!(
                    "cannot connect to Firecracker socket {}: {e}",
                    self.socket_path.display()
                ))
            })?;

        let io = hyper_util::rt::TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("HTTP handshake failed: {e}")))?;

        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("Firecracker API connection error: {e}");
            }
        });

        let req = Request::builder()
            .method("PATCH")
            .uri(format!("http://localhost{path}"))
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body_str)))
            .map_err(|e| ProviderError::ExecFailed(format!("request build: {e}")))?;

        let response = sender
            .send_request(req)
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("API request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body_bytes = http_body_util::BodyExt::collect(response.into_body())
                .await
                .map_err(|e| ProviderError::ExecFailed(format!("read response: {e}")))?
                .to_bytes();
            let body_str = String::from_utf8_lossy(&body_bytes);
            return Err(ProviderError::ExecFailed(format!(
                "Firecracker API error ({status}): {body_str}"
            )));
        }

        Ok(())
    }

    /// Configure the boot source (kernel + boot args).
    pub async fn put_boot_source(
        &self,
        kernel_image_path: &str,
        boot_args: &str,
    ) -> Result<(), ProviderError> {
        self.put(
            "/boot-source",
            serde_json::json!({
                "kernel_image_path": kernel_image_path,
                "boot_args": boot_args
            }),
        )
        .await
    }

    /// Configure a drive.
    pub async fn put_drive(
        &self,
        drive_id: &str,
        path_on_host: &str,
        is_root_device: bool,
        is_read_only: bool,
    ) -> Result<(), ProviderError> {
        self.put(
            &format!("/drives/{drive_id}"),
            serde_json::json!({
                "drive_id": drive_id,
                "path_on_host": path_on_host,
                "is_root_device": is_root_device,
                "is_read_only": is_read_only
            }),
        )
        .await
    }

    /// Configure machine resources (vCPUs, memory).
    pub async fn put_machine_config(
        &self,
        vcpu_count: u8,
        mem_size_mib: u64,
    ) -> Result<(), ProviderError> {
        self.put(
            "/machine-config",
            serde_json::json!({
                "vcpu_count": vcpu_count,
                "mem_size_mib": mem_size_mib
            }),
        )
        .await
    }

    /// Configure the vsock device.
    pub async fn put_vsock(&self, guest_cid: u32) -> Result<(), ProviderError> {
        self.put(
            "/vsock",
            serde_json::json!({
                "guest_cid": guest_cid,
                "uds_path": "vsock.sock"
            }),
        )
        .await
    }

    /// Start the microVM.
    pub async fn start(&self) -> Result<(), ProviderError> {
        self.put(
            "/actions",
            serde_json::json!({
                "action_type": "InstanceStart"
            }),
        )
        .await
    }

    /// Pause the microVM.
    pub async fn pause(&self) -> Result<(), ProviderError> {
        self.patch(
            "/vm",
            serde_json::json!({
                "state": "Paused"
            }),
        )
        .await
    }

    /// Resume the microVM.
    pub async fn resume(&self) -> Result<(), ProviderError> {
        self.patch(
            "/vm",
            serde_json::json!({
                "state": "Resumed"
            }),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_new() {
        let client = FirecrackerApiClient::new(PathBuf::from("/tmp/test.sock"));
        assert_eq!(client.socket_path, PathBuf::from("/tmp/test.sock"));
    }

    // Integration tests with actual Firecracker would go here with #[ignore]
    // Unit tests for the HTTP layer require a mock Unix socket server,
    // which is tested at the provider level.
}
```

- [ ] **Step 2: Register the module**

In `crates/roche-core/src/provider/firecracker/mod.rs`, add:

```rust
pub mod api_client;
pub mod state;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p roche-core`
Expected: Compiles successfully.

- [ ] **Step 4: Commit**

```bash
git add crates/roche-core/src/provider/firecracker/api_client.rs crates/roche-core/src/provider/firecracker/mod.rs
git commit -m "feat(firecracker): add API client for Firecracker REST API over Unix socket"
```

---

### Task 5: Vsock Exec Module

**Files:**
- Create: `crates/roche-core/src/provider/firecracker/vsock_exec.rs`

This module handles command execution via vsock. It's platform-gated: real implementation on Linux, stub on other platforms.

- [ ] **Step 1: Create vsock_exec.rs**

```rust
use crate::provider::ProviderError;
use crate::types::{ExecOutput, ExecRequest};

/// Execute a command in the guest via vsock.
///
/// Connects to the roche-agent running inside the guest VM on vsock port 52.
/// Sends a JSON request and reads a JSON response.
#[cfg(target_os = "linux")]
pub async fn exec_via_vsock(
    cid: u32,
    request: &ExecRequest,
    timeout_secs: u64,
) -> Result<ExecOutput, ProviderError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_vsock::VsockStream;

    const AGENT_PORT: u32 = 52;

    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        VsockStream::connect(cid, AGENT_PORT),
    )
    .await
    .map_err(|_| ProviderError::Timeout(5))?
    .map_err(|e| ProviderError::ExecFailed(format!("vsock connect failed: {e}")))?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Send the exec request as JSON
    let req_json = serde_json::json!({
        "command": request.command,
        "timeout_secs": request.timeout_secs.unwrap_or(timeout_secs),
    });
    let req_bytes = serde_json::to_vec(&req_json)
        .map_err(|e| ProviderError::ExecFailed(format!("serialize request: {e}")))?;

    // Write length-prefixed message: 4-byte big-endian length + JSON
    let len = req_bytes.len() as u32;
    writer
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock write len: {e}")))?;
    writer
        .write_all(&req_bytes)
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock write: {e}")))?;
    writer
        .flush()
        .await
        .map_err(|e| ProviderError::ExecFailed(format!("vsock flush: {e}")))?;

    // Read response: 4-byte big-endian length + JSON
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        async {
            let mut len_buf = [0u8; 4];
            reader.read_exact(&mut len_buf).await.map_err(|e| {
                ProviderError::ExecFailed(format!("vsock read response len: {e}"))
            })?;
            let resp_len = u32::from_be_bytes(len_buf) as usize;
            if resp_len > 64 * 1024 * 1024 {
                return Err(ProviderError::ExecFailed(
                    "response too large (>64MB)".into(),
                ));
            }
            let mut resp_buf = vec![0u8; resp_len];
            reader.read_exact(&mut resp_buf).await.map_err(|e| {
                ProviderError::ExecFailed(format!("vsock read response: {e}"))
            })?;
            let output: ExecOutput = serde_json::from_slice(&resp_buf).map_err(|e| {
                ProviderError::ExecFailed(format!("parse response: {e}"))
            })?;
            Ok(output)
        },
    )
    .await
    .map_err(|_| ProviderError::Timeout(timeout_secs))?;

    result
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub async fn exec_via_vsock(
    _cid: u32,
    _request: &ExecRequest,
    _timeout_secs: u64,
) -> Result<ExecOutput, ProviderError> {
    Err(ProviderError::Unsupported(
        "vsock exec requires Linux".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_via_vsock_non_linux_returns_unsupported() {
        // This test only runs on non-Linux (macOS/Windows) to verify the stub.
        #[cfg(not(target_os = "linux"))]
        {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let request = ExecRequest {
                command: vec!["echo".into(), "hello".into()],
                timeout_secs: None,
            };
            let result = rt.block_on(exec_via_vsock(3, &request, 30));
            assert!(result.is_err());
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("Linux"),
                "expected Linux mention, got: {err}"
            );
        }
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/roche-core/src/provider/firecracker/mod.rs`, update to:

```rust
pub mod api_client;
pub mod state;
pub mod vsock_exec;
```

- [ ] **Step 3: Verify it compiles and test passes**

Run: `cargo test -p roche-core -- provider::firecracker::vsock_exec::tests`
Expected: On macOS — 1 test PASSES (stub test). On Linux — test is skipped by the `#[cfg]` gate.

- [ ] **Step 4: Commit**

```bash
git add crates/roche-core/src/provider/firecracker/
git commit -m "feat(firecracker): add vsock exec module with platform gating"
```

---

## Chunk 3: Provider Implementation and CLI Integration

### Task 6: FirecrackerProvider — Trait Implementations

**Files:**
- Modify: `crates/roche-core/src/provider/firecracker/mod.rs`

This is the main provider file. It implements `SandboxProvider` and `SandboxLifecycle`. The `create` flow spawns a Firecracker process, configures it via the API client, and starts the VM. The `exec` flow delegates to vsock_exec. Platform check happens at the top of each method.

- [ ] **Step 1: Implement FirecrackerProvider**

Replace `crates/roche-core/src/provider/firecracker/mod.rs` with:

```rust
pub mod api_client;
pub mod state;
pub mod vsock_exec;

use crate::provider::{ProviderError, SandboxLifecycle, SandboxProvider};
use crate::types::{ExecOutput, ExecRequest, SandboxConfig, SandboxId, SandboxInfo, SandboxStatus};
use state::{StateManager, VmMetadata};

pub struct FirecrackerProvider {
    state: StateManager,
}

impl FirecrackerProvider {
    pub fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            state: StateManager::new()?,
        })
    }

    /// For testing: create with a custom state directory.
    #[cfg(test)]
    pub fn with_state(state: StateManager) -> Self {
        Self { state }
    }

    /// Check that we're on Linux — Firecracker only runs on Linux.
    fn check_platform() -> Result<(), ProviderError> {
        if cfg!(not(target_os = "linux")) {
            return Err(ProviderError::Unavailable(
                "Firecracker requires Linux with KVM".into(),
            ));
        }
        Ok(())
    }

    /// Validate that kernel and rootfs are provided and exist.
    fn validate_config(config: &SandboxConfig) -> Result<(&str, &str), ProviderError> {
        let kernel = config.kernel.as_deref().ok_or_else(|| {
            ProviderError::CreateFailed("--kernel is required for Firecracker provider".into())
        })?;
        let rootfs = config.rootfs.as_deref().ok_or_else(|| {
            ProviderError::CreateFailed("--rootfs is required for Firecracker provider".into())
        })?;

        if !std::path::Path::new(kernel).exists() {
            return Err(ProviderError::CreateFailed(format!(
                "kernel not found: {kernel}"
            )));
        }
        if !std::path::Path::new(rootfs).exists() {
            return Err(ProviderError::CreateFailed(format!(
                "rootfs not found: {rootfs}"
            )));
        }

        Ok((kernel, rootfs))
    }

    /// Parse memory string (e.g. "512m") to MiB. Defaults to 128 MiB.
    fn parse_memory_mib(memory: &Option<String>) -> u64 {
        match memory.as_deref() {
            Some(s) => {
                let s = s.trim().to_lowercase();
                if let Some(num) = s.strip_suffix('m') {
                    num.parse::<u64>().unwrap_or(128)
                } else if let Some(num) = s.strip_suffix('g') {
                    num.parse::<u64>().unwrap_or(1) * 1024
                } else {
                    s.parse::<u64>().unwrap_or(128)
                }
            }
            None => 128,
        }
    }

    /// Allocate a unique CID for vsock (simple: based on timestamp + random).
    fn allocate_cid() -> u32 {
        // CID 0, 1, 2 are reserved. Use 3+.
        // Simple approach: random in [3, 2^31)
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        ((ts % (u32::MAX as u128 - 3)) + 3) as u32
    }

    /// Check if a process with the given PID is still alive.
    #[cfg(target_os = "linux")]
    fn is_process_alive(pid: u32) -> bool {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            None, // signal 0 = check existence
        )
        .is_ok()
    }

    #[cfg(not(target_os = "linux"))]
    fn is_process_alive(_pid: u32) -> bool {
        false
    }

    /// Kill a process by PID.
    #[cfg(target_os = "linux")]
    fn kill_process(pid: u32) -> Result<(), ProviderError> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        let pid = Pid::from_raw(pid as i32);
        kill(pid, Signal::SIGKILL)
            .map_err(|e| ProviderError::ExecFailed(format!("failed to kill process: {e}")))?;

        // Wait briefly for process to exit
        for _ in 0..10 {
            if kill(pid, None).is_err() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn kill_process(_pid: u32) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported(
            "process management requires Linux".into(),
        ))
    }
}

impl Default for FirecrackerProvider {
    fn default() -> Self {
        Self::new().expect("failed to initialize FirecrackerProvider")
    }
}

impl SandboxProvider for FirecrackerProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError> {
        Self::check_platform()?;
        let (kernel, rootfs) = Self::validate_config(config)?;

        let id = uuid::Uuid::new_v4().to_string();
        let cid = Self::allocate_cid();

        // 1. Create state directory
        self.state.create_vm_dir(&id)?;

        // 2. Copy rootfs
        let rootfs_copy = self.state.copy_rootfs(&id, std::path::Path::new(rootfs))?;

        // 3. Spawn firecracker process
        let socket_path = self.state.socket_path(&id);
        let child = tokio::process::Command::new("firecracker")
            .arg("--api-sock")
            .arg(&socket_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                // Cleanup on failure
                let _ = self.state.remove_vm_dir(&id);
                ProviderError::CreateFailed(format!("failed to spawn firecracker: {e}"))
            })?;

        let pid = child.id().unwrap_or(0);

        // 4. Wait for socket to appear
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !socket_path.exists() {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(ProviderError::CreateFailed(
                "firecracker socket did not appear".into(),
            ));
        }

        // 5. Configure VM via API
        let api = api_client::FirecrackerApiClient::new(socket_path);

        let mem_mib = Self::parse_memory_mib(&config.memory);
        let vcpus = config.cpus.map(|c| c.ceil() as u8).unwrap_or(1);

        let boot_args = "console=ttyS0 reboot=k panic=1 pci=off";

        if let Err(e) = api.put_boot_source(kernel, boot_args).await {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(e);
        }

        if let Err(e) = api
            .put_drive("rootfs", &rootfs_copy.to_string_lossy(), true, !config.writable)
            .await
        {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(e);
        }

        if let Err(e) = api.put_machine_config(vcpus, mem_mib).await {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(e);
        }

        if let Err(e) = api.put_vsock(cid).await {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(e);
        }

        if let Err(e) = api.start().await {
            let _ = Self::kill_process(pid);
            let _ = self.state.remove_vm_dir(&id);
            return Err(e);
        }

        // 6. Write metadata
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = VmMetadata {
            id: id.clone(),
            provider: "firecracker".to_string(),
            image: "custom".to_string(),
            pid,
            cid,
            kernel: kernel.to_string(),
            expires_at: Some(now + config.timeout_secs),
            created_at: now,
        };
        self.state.write_metadata(&metadata)?;

        Ok(id)
    }

    async fn exec(
        &self,
        id: &SandboxId,
        request: &ExecRequest,
    ) -> Result<ExecOutput, ProviderError> {
        Self::check_platform()?;

        let metadata = self.state.read_metadata(id)?;

        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let timeout = request.timeout_secs.unwrap_or(300);
        vsock_exec::exec_via_vsock(metadata.cid, request, timeout).await
    }

    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError> {
        let metadata = self.state.read_metadata(id)?;

        if Self::is_process_alive(metadata.pid) {
            Self::kill_process(metadata.pid)?;
        }

        self.state.remove_vm_dir(id)?;

        Ok(())
    }

    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError> {
        let ids = self.state.list_vm_ids()?;
        let mut infos = Vec::new();

        for id in ids {
            if let Ok(metadata) = self.state.read_metadata(&id) {
                let status = if Self::is_process_alive(metadata.pid) {
                    SandboxStatus::Running
                } else {
                    SandboxStatus::Stopped
                };

                infos.push(SandboxInfo {
                    id: metadata.id,
                    status,
                    provider: "firecracker".to_string(),
                    image: metadata.image,
                    expires_at: metadata.expires_at,
                });
            }
        }

        Ok(infos)
    }
}

impl SandboxLifecycle for FirecrackerProvider {
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        Self::check_platform()?;

        let metadata = self.state.read_metadata(id)?;
        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let api = api_client::FirecrackerApiClient::new(self.state.socket_path(id));
        api.pause().await
    }

    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError> {
        Self::check_platform()?;

        let metadata = self.state.read_metadata(id)?;
        if !Self::is_process_alive(metadata.pid) {
            return Err(ProviderError::NotFound(id.clone()));
        }

        let api = api_client::FirecrackerApiClient::new(self.state.socket_path(id));
        api.resume().await
    }

    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError> {
        let infos = self.list().await?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut destroyed = Vec::new();
        for info in infos {
            if let Some(exp) = info.expires_at {
                if exp <= now {
                    if self.destroy(&info.id).await.is_ok() {
                        destroyed.push(info.id);
                    }
                }
            }
        }

        Ok(destroyed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_mib() {
        assert_eq!(FirecrackerProvider::parse_memory_mib(&None), 128);
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("512m".into())),
            512
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("2g".into())),
            2048
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("256".into())),
            256
        );
        assert_eq!(
            FirecrackerProvider::parse_memory_mib(&Some("invalid".into())),
            128
        );
    }

    #[test]
    fn test_allocate_cid() {
        let cid = FirecrackerProvider::allocate_cid();
        assert!(cid >= 3, "CID must be >= 3, got {cid}");
    }

    #[test]
    fn test_check_platform() {
        let result = FirecrackerProvider::check_platform();
        if cfg!(target_os = "linux") {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Linux"));
        }
    }

    #[test]
    fn test_validate_config_missing_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            rootfs: Some("/tmp/rootfs.ext4".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--kernel"));
    }

    #[test]
    fn test_validate_config_missing_rootfs() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            kernel: Some("/tmp/vmlinux".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("--rootfs"));
    }

    #[test]
    fn test_validate_config_nonexistent_kernel() {
        let config = SandboxConfig {
            provider: "firecracker".into(),
            kernel: Some("/nonexistent/vmlinux".into()),
            rootfs: Some("/nonexistent/rootfs.ext4".into()),
            ..Default::default()
        };
        let result = FirecrackerProvider::validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("kernel not found"));
    }

    #[test]
    fn test_list_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let state = StateManager::with_base_dir(tmp.path().to_path_buf());
        let provider = FirecrackerProvider::with_state(state);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let infos = rt.block_on(provider.list()).unwrap();
        assert!(infos.is_empty());
    }

    #[test]
    fn test_destroy_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let state = StateManager::with_base_dir(tmp.path().to_path_buf());
        let provider = FirecrackerProvider::with_state(state);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(provider.destroy(&"nonexistent".to_string()));
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run all Firecracker tests**

Run: `cargo test -p roche-core -- provider::firecracker`
Expected: All tests PASS (state tests + provider unit tests).

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/src/provider/firecracker/
git commit -m "feat(firecracker): implement FirecrackerProvider with SandboxProvider + SandboxLifecycle traits"
```

---

### Task 7: Export FirecrackerProvider from lib.rs

**Files:**
- Modify: `crates/roche-core/src/lib.rs:4`

- [ ] **Step 1: Update lib.rs exports**

In `crates/roche-core/src/lib.rs`, the current exports are:

```rust
pub use provider::{SandboxFileOps, SandboxLifecycle, SandboxProvider};
```

No changes needed to this line — `FirecrackerProvider` is already accessible via `roche_core::provider::firecracker::FirecrackerProvider`. The existing `pub mod firecracker;` in `provider/mod.rs` (added in Task 3) makes it public.

- [ ] **Step 2: Verify the full crate compiles and tests pass**

Run: `cargo test -p roche-core`
Expected: All tests PASS.

- [ ] **Step 3: Commit (if any changes)**

Only commit if changes were made; otherwise skip.

---

### Task 8: CLI Integration — Provider Dispatch + New Flags

**Files:**
- Modify: `crates/roche-cli/src/main.rs:14-54` (Create command — add flags)
- Modify: `crates/roche-cli/src/main.rs:174-179` (run function — provider dispatch)

- [ ] **Step 1: Add --kernel and --rootfs flags to Create command**

In `crates/roche-cli/src/main.rs`, add to the `Create` variant inside the `Commands` enum, after the `writable` field:

```rust
        /// Path to kernel image (required for firecracker provider)
        #[arg(long)]
        kernel: Option<String>,

        /// Path to rootfs image (required for firecracker provider)
        #[arg(long)]
        rootfs: Option<String>,
```

- [ ] **Step 2: Update the Create match arm to pass kernel/rootfs to SandboxConfig**

In the `Commands::Create` match arm destructuring, add `kernel, rootfs,` to the list. Then update the `SandboxConfig` construction:

```rust
            let config = SandboxConfig {
                provider: provider_name.clone(),
                image,
                memory,
                cpus,
                timeout_secs: timeout,
                network,
                writable,
                env: env_map,
                mounts: mount_configs,
                kernel,
                rootfs,
            };
```

- [ ] **Step 3: Add provider dispatch in run()**

Replace the hardcoded `let provider = DockerProvider::new();` at line 179 in the `run` function. The approach depends on the command — we need to know the provider name before creating the provider. For the `Create` command, we extract from the enum. For other commands, we need a different approach.

The simplest approach: use a dynamic dispatch wrapper. But since traits use async fn (no dyn), use an enum dispatch instead.

Add this helper enum and the dispatch logic at the top of the `run` function, replacing the current `let provider = DockerProvider::new();`:

```rust
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::firecracker::FirecrackerProvider;
    use roche_core::provider::{SandboxLifecycle, SandboxProvider};
    use roche_core::types::{ExecRequest, SandboxConfig};

    // Determine which provider to use based on the command
    let provider_name = match &cli.command {
        Commands::Create { provider, .. } => provider.clone(),
        _ => "docker".to_string(), // Default for non-create commands
    };
```

Then wrap each command's body to use the appropriate provider. The cleanest approach: extract a macro or duplicate the match for each provider. Since we only have two providers, use an `if/else`:

Replace the `match cli.command` block structure. Before each provider call, dispatch:

```rust
    if provider_name == "firecracker" {
        let provider = FirecrackerProvider::new()?;
        run_with_provider(cli.command, provider).await
    } else {
        let provider = DockerProvider::new();
        run_with_provider(cli.command, provider).await
    }
```

Extract the match body into a generic function. However, since Rust doesn't support `dyn` for async traits, create a macro instead:

```rust
macro_rules! run_commands {
    ($commands:expr, $provider:expr) => {{
        let provider = $provider;
        match $commands {
            // ... all the existing match arms from the current run() function
        }
    }};
}
```

**Simpler approach:** Since both providers implement the same traits, and we can't use dyn dispatch easily, just duplicate the match into two branches:

```rust
async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::firecracker::FirecrackerProvider;
    use roche_core::provider::{SandboxLifecycle, SandboxProvider};
    use roche_core::types::{ExecRequest, SandboxConfig};

    let provider_name = match &cli.command {
        Commands::Create { provider, .. } => provider.clone(),
        _ => "docker".to_string(),
    };

    match provider_name.as_str() {
        "firecracker" => {
            let provider = FirecrackerProvider::new()?;
            run_with_provider(cli.command, &provider).await
        }
        _ => {
            let provider = DockerProvider::new();
            run_with_provider(cli.command, &provider).await
        }
    }
}
```

Create a `run_with_provider` function that takes `impl SandboxProvider + SandboxLifecycle + SandboxFileOps`. But `FirecrackerProvider` doesn't implement `SandboxFileOps`.

**Best approach:** Use a helper trait or split the dispatch. Since `Cp` only works with Docker, handle it separately:

```rust
async fn run_provider_commands<P>(commands: Commands, provider: &P) -> Result<(), roche_core::provider::ProviderError>
where
    P: SandboxProvider + SandboxLifecycle,
{
    match commands {
        // All commands except Cp
        Commands::Create { .. } => { /* same as before */ }
        Commands::Exec { .. } => { /* same as before */ }
        Commands::Destroy { .. } => { /* same as before */ }
        Commands::Pause { .. } => { /* same as before */ }
        Commands::Unpause { .. } => { /* same as before */ }
        Commands::List { .. } => { /* same as before */ }
        Commands::Gc { .. } => { /* same as before */ }
        Commands::Cp { .. } => {
            return Err(roche_core::provider::ProviderError::Unsupported(
                "file copy not supported for this provider".into()
            ));
        }
    }
    Ok(())
}
```

Then in `run()`:

```rust
async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::docker::DockerProvider;
    use roche_core::provider::firecracker::FirecrackerProvider;
    use roche_core::provider::{SandboxFileOps, SandboxLifecycle, SandboxProvider};
    use roche_core::types::{ExecRequest, SandboxConfig};

    let provider_name = match &cli.command {
        Commands::Create { provider, .. } => provider.clone(),
        _ => "docker".to_string(),
    };

    match provider_name.as_str() {
        "firecracker" => {
            let provider = FirecrackerProvider::new()?;
            run_provider_commands(cli.command, &provider).await
        }
        _ => {
            let provider = DockerProvider::new();
            // Docker supports Cp, so handle it specially
            if let Commands::Cp { src, dest } = cli.command {
                use roche_core::provider::SandboxFileOps;
                // existing cp logic
                match (parse_cp_path(&src), parse_cp_path(&dest)) {
                    (Some((sandbox_id, sandbox_path)), None) => {
                        provider.copy_from(&sandbox_id.to_string(), sandbox_path, std::path::Path::new(&dest)).await?;
                    }
                    (None, Some((sandbox_id, sandbox_path))) => {
                        provider.copy_to(&sandbox_id.to_string(), std::path::Path::new(&src), sandbox_path).await?;
                    }
                    (Some(_), Some(_)) => {
                        eprintln!("Error: both source and destination cannot be sandbox paths");
                        std::process::exit(1);
                    }
                    (None, None) => {
                        eprintln!("Error: one of source or destination must be a sandbox path (sandbox_id:/path)");
                        std::process::exit(1);
                    }
                }
                Ok(())
            } else {
                run_provider_commands(cli.command, &provider).await
            }
        }
    }
}
```

Move all the shared command handlers (Create, Exec, Destroy, Pause, Unpause, List, Gc) into `run_provider_commands`, keeping the Cp handler in the docker-only branch.

- [ ] **Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully.

- [ ] **Step 5: Run all tests**

Run: `cargo test`
Expected: All existing tests pass. New CLI flags are accepted.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat(firecracker): add --kernel/--rootfs flags and provider dispatch to CLI"
```

---

### Task 9: Final Verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: All tests PASS.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: No warnings.

- [ ] **Step 3: Run format check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 4: Verify CLI help shows new options**

Run: `cargo run -- create --help`
Expected: Output shows `--kernel`, `--rootfs`, `--provider` options.

Run: `cargo run -- --help`
Expected: All subcommands listed.
