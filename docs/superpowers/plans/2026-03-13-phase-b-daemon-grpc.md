# Phase B-C: Daemon Mode + gRPC Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a gRPC daemon (`roche-daemon`) that exposes sandbox management over TCP, with CLI dual-mode (direct or gRPC client).

**Architecture:** New `roche-daemon` binary crate uses `tonic` to serve a gRPC API that delegates to existing providers in `roche-core`. The CLI detects a running daemon and forwards requests via gRPC, falling back to direct provider access. Background GC runs in the daemon via a tokio interval timer.

**Tech Stack:** Rust, tonic 0.12, prost 0.13, tonic-build, protobuf, clap 4, tracing

---

## File Structure

### New files
- `proto/roche/v1/sandbox.proto` — protobuf service + message definitions
- `crates/roche-daemon/Cargo.toml` — daemon crate manifest
- `crates/roche-daemon/build.rs` — tonic-build proto compilation
- `crates/roche-daemon/src/main.rs` — daemon entry point, CLI args, startup, signal handling
- `crates/roche-daemon/src/server.rs` — `SandboxServiceImpl` gRPC handler
- `crates/roche-daemon/src/gc.rs` — background GC task
- `crates/roche-cli/build.rs` — tonic-build proto compilation for client stubs

### Modified files
- `Cargo.toml` — add `roche-daemon` to workspace members
- `crates/roche-cli/Cargo.toml` — add tonic, prost dependencies
- `crates/roche-cli/src/main.rs` — add `Daemon` subcommand, dual-mode dispatch, `--direct` flag

---

## Chunk 1: Proto + Daemon Scaffold

### Task 1: Protobuf definition and workspace setup

**Files:**
- Create: `proto/roche/v1/sandbox.proto`
- Modify: `Cargo.toml`
- Create: `crates/roche-daemon/Cargo.toml`
- Create: `crates/roche-daemon/build.rs`
- Create: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Create the proto file**

Create `proto/roche/v1/sandbox.proto` with the full service definition from the spec:

```protobuf
syntax = "proto3";
package roche.v1;

service SandboxService {
  rpc Create(CreateRequest) returns (CreateResponse);
  rpc Exec(ExecRequest) returns (ExecResponse);
  rpc Destroy(DestroyRequest) returns (DestroyResponse);
  rpc List(ListRequest) returns (ListResponse);
  rpc Pause(PauseRequest) returns (PauseResponse);
  rpc Unpause(UnpauseRequest) returns (UnpauseResponse);
  rpc Gc(GcRequest) returns (GcResponse);
  rpc CopyTo(CopyToRequest) returns (CopyToResponse);
  rpc CopyFrom(CopyFromRequest) returns (CopyFromResponse);
}

// Note: proto3 bool defaults to false, which intentionally aligns with
// AI-safe defaults (network=off, writable=off). timeout_secs=0 means
// "use server default (300s)".
message CreateRequest {
  string provider = 1;
  string image = 2;
  optional string memory = 3;
  optional double cpus = 4;
  uint64 timeout_secs = 5;
  bool network = 6;
  bool writable = 7;
  map<string, string> env = 8;
  repeated MountConfig mounts = 9;
  optional string kernel = 10;
  optional string rootfs = 11;
}

message CreateResponse {
  string sandbox_id = 1;
}

message ExecRequest {
  string sandbox_id = 1;
  repeated string command = 2;
  optional uint64 timeout_secs = 3;
  string provider = 4;
}

message ExecResponse {
  int32 exit_code = 1;
  string stdout = 2;
  string stderr = 3;
}

message DestroyRequest {
  repeated string sandbox_ids = 1;
  bool all = 2;
  string provider = 3;
}

message DestroyResponse {
  repeated string destroyed_ids = 1;
}

message ListRequest {
  string provider = 1;
}

message ListResponse {
  repeated SandboxInfo sandboxes = 1;
}

message PauseRequest {
  string sandbox_id = 1;
  string provider = 2;
}

message PauseResponse {}

message UnpauseRequest {
  string sandbox_id = 1;
  string provider = 2;
}

message UnpauseResponse {}

message GcRequest {
  bool dry_run = 1;
  bool all = 2;
  string provider = 3;
}

message GcResponse {
  repeated string destroyed_ids = 1;
}

message CopyToRequest {
  string sandbox_id = 1;
  string host_path = 2;
  string sandbox_path = 3;
  string provider = 4;
}

message CopyToResponse {}

message CopyFromRequest {
  string sandbox_id = 1;
  string sandbox_path = 2;
  string host_path = 3;
  string provider = 4;
}

message CopyFromResponse {}

message MountConfig {
  string host_path = 1;
  string container_path = 2;
  bool readonly = 3;
}

enum SandboxStatus {
  SANDBOX_STATUS_UNSPECIFIED = 0;
  SANDBOX_STATUS_RUNNING = 1;
  SANDBOX_STATUS_PAUSED = 2;
  SANDBOX_STATUS_STOPPED = 3;
  SANDBOX_STATUS_FAILED = 4;
}

message SandboxInfo {
  string id = 1;
  SandboxStatus status = 2;
  string provider = 3;
  string image = 4;
  optional uint64 expires_at = 5;
}
```

- [ ] **Step 2: Create daemon crate Cargo.toml**

Create `crates/roche-daemon/Cargo.toml`:

```toml
[package]
name = "roche-daemon"
description = "Universal sandbox orchestrator for AI agents — gRPC daemon"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "roche-daemon"
path = "src/main.rs"

[dependencies]
roche-core = { path = "../roche-core" }
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
clap = { version = "4", features = ["derive"] }
dirs = "6"
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"

[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 3: Create build.rs for proto compilation**

Create `crates/roche-daemon/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["../../proto/roche/v1/sandbox.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
```

- [ ] **Step 4: Create minimal daemon main.rs**

Create `crates/roche-daemon/src/main.rs`:

```rust
use clap::Parser;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

#[derive(Parser)]
#[command(name = "roche-daemon", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,
}

#[tokio::main]
async fn main() {
    let _args = Args::parse();
    println!("roche-daemon placeholder");
}
```

- [ ] **Step 5: Add daemon to workspace**

In the root `Cargo.toml`, change the `members` line:
```toml
members = ["crates/roche-core", "crates/roche-cli", "crates/roche-daemon"]
```

- [ ] **Step 6: Verify proto compilation builds**

Run: `cargo build -p roche-daemon`
Expected: Compiles successfully. The proto file is compiled by `tonic-build` and the generated code is included.

- [ ] **Step 7: Commit**

```bash
git add proto/ crates/roche-daemon/ Cargo.toml
git commit -m "feat(daemon): add roche-daemon crate with proto definition and build scaffold"
```

---

### Task 2: gRPC server implementation — error mapping and provider dispatch

**Files:**
- Create: `crates/roche-daemon/src/server.rs`
- Modify: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Create server.rs with SandboxServiceImpl and error mapping**

Create `crates/roche-daemon/src/server.rs`:

```rust
use crate::proto;
use roche_core::provider::docker::DockerProvider;
#[cfg(target_os = "linux")]
use roche_core::provider::firecracker::FirecrackerProvider;
use roche_core::provider::{ProviderError, SandboxFileOps, SandboxLifecycle, SandboxProvider};
use roche_core::types::{self, SandboxConfig, SandboxStatus};
use tonic::{Request, Response, Status};

pub struct SandboxServiceImpl {
    docker: DockerProvider,
    #[cfg(target_os = "linux")]
    firecracker: Option<FirecrackerProvider>,
}

impl SandboxServiceImpl {
    pub fn new() -> Self {
        Self {
            docker: DockerProvider::new(),
            #[cfg(target_os = "linux")]
            firecracker: FirecrackerProvider::new().ok(),
        }
    }
}

fn provider_error_to_status(err: ProviderError) -> Status {
    match &err {
        ProviderError::NotFound(_) => Status::not_found(err.to_string()),
        ProviderError::CreateFailed(_) => Status::internal(err.to_string()),
        ProviderError::ExecFailed(_) => Status::internal(err.to_string()),
        ProviderError::Unavailable(_) => Status::unavailable(err.to_string()),
        ProviderError::Timeout(_) => Status::deadline_exceeded(err.to_string()),
        ProviderError::Unsupported(_) => Status::unimplemented(err.to_string()),
        ProviderError::FileFailed(_) => Status::internal(err.to_string()),
        ProviderError::Paused(_) => Status::failed_precondition(err.to_string()),
    }
}

fn sandbox_status_to_proto(status: SandboxStatus) -> i32 {
    match status {
        SandboxStatus::Running => proto::SandboxStatus::Running as i32,
        SandboxStatus::Paused => proto::SandboxStatus::Paused as i32,
        SandboxStatus::Stopped => proto::SandboxStatus::Stopped as i32,
        SandboxStatus::Failed => proto::SandboxStatus::Failed as i32,
    }
}

fn default_timeout(t: u64) -> u64 {
    if t == 0 { 300 } else { t }
}

/// Macro to dispatch to the correct provider based on the provider name string.
/// Since Rust async traits don't support dyn dispatch, we match on the string
/// and call the concrete provider type.
macro_rules! with_provider {
    ($self:expr, $provider_name:expr, |$p:ident| $body:expr) => {{
        match $provider_name.as_str() {
            #[cfg(target_os = "linux")]
            "firecracker" => {
                if let Some(ref $p) = $self.firecracker {
                    $body
                } else {
                    Err(Status::unavailable("Firecracker provider not available"))
                }
            }
            _ => {
                let $p = &$self.docker;
                $body
            }
        }
    }};
}

#[tonic::async_trait]
impl proto::sandbox_service_server::SandboxService for SandboxServiceImpl {
    async fn create(
        &self,
        request: Request<proto::CreateRequest>,
    ) -> Result<Response<proto::CreateResponse>, Status> {
        let req = request.into_inner();
        let config = SandboxConfig {
            provider: req.provider.clone(),
            image: if req.image.is_empty() {
                "python:3.12-slim".to_string()
            } else {
                req.image
            },
            memory: req.memory,
            cpus: req.cpus,
            timeout_secs: default_timeout(req.timeout_secs),
            network: req.network,
            writable: req.writable,
            env: req.env,
            mounts: req
                .mounts
                .into_iter()
                .map(|m| types::MountConfig {
                    host_path: m.host_path,
                    container_path: m.container_path,
                    readonly: m.readonly,
                })
                .collect(),
            kernel: req.kernel,
            rootfs: req.rootfs,
        };

        with_provider!(self, config.provider, |p| {
            let id = p
                .create(&config)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::CreateResponse { sandbox_id: id }))
        })
    }

    async fn exec(
        &self,
        request: Request<proto::ExecRequest>,
    ) -> Result<Response<proto::ExecResponse>, Status> {
        let req = request.into_inner();
        let exec_req = types::ExecRequest {
            command: req.command,
            timeout_secs: req.timeout_secs,
        };
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            let output = p
                .exec(&req.sandbox_id, &exec_req)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::ExecResponse {
                exit_code: output.exit_code,
                stdout: output.stdout,
                stderr: output.stderr,
            }))
        })
    }

    async fn destroy(
        &self,
        request: Request<proto::DestroyRequest>,
    ) -> Result<Response<proto::DestroyResponse>, Status> {
        let req = request.into_inner();
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            let targets = if req.all {
                p.list()
                    .await
                    .map_err(provider_error_to_status)?
                    .into_iter()
                    .map(|sb| sb.id)
                    .collect()
            } else {
                req.sandbox_ids
            };
            let mut destroyed = Vec::new();
            for id in &targets {
                if p.destroy(id).await.is_ok() {
                    destroyed.push(id.clone());
                }
            }
            Ok(Response::new(proto::DestroyResponse {
                destroyed_ids: destroyed,
            }))
        })
    }

    async fn list(
        &self,
        request: Request<proto::ListRequest>,
    ) -> Result<Response<proto::ListResponse>, Status> {
        let req = request.into_inner();
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            let sandboxes = p.list().await.map_err(provider_error_to_status)?;
            let infos = sandboxes
                .into_iter()
                .map(|sb| proto::SandboxInfo {
                    id: sb.id,
                    status: sandbox_status_to_proto(sb.status),
                    provider: sb.provider,
                    image: sb.image,
                    expires_at: sb.expires_at,
                })
                .collect();
            Ok(Response::new(proto::ListResponse { sandboxes: infos }))
        })
    }

    async fn pause(
        &self,
        request: Request<proto::PauseRequest>,
    ) -> Result<Response<proto::PauseResponse>, Status> {
        let req = request.into_inner();
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            p.pause(&req.sandbox_id)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::PauseResponse {}))
        })
    }

    async fn unpause(
        &self,
        request: Request<proto::UnpauseRequest>,
    ) -> Result<Response<proto::UnpauseResponse>, Status> {
        let req = request.into_inner();
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            p.unpause(&req.sandbox_id)
                .await
                .map_err(provider_error_to_status)?;
            Ok(Response::new(proto::UnpauseResponse {}))
        })
    }

    async fn gc(
        &self,
        request: Request<proto::GcRequest>,
    ) -> Result<Response<proto::GcResponse>, Status> {
        let req = request.into_inner();
        let provider_name = if req.provider.is_empty() {
            "docker".to_string()
        } else {
            req.provider
        };

        with_provider!(self, provider_name, |p| {
            if req.all {
                let sandboxes = p.list().await.map_err(provider_error_to_status)?;
                let mut destroyed = Vec::new();
                for sb in &sandboxes {
                    if req.dry_run {
                        destroyed.push(sb.id.clone());
                    } else if p.destroy(&sb.id).await.is_ok() {
                        destroyed.push(sb.id.clone());
                    }
                }
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: destroyed,
                }))
            } else if req.dry_run {
                let sandboxes = p.list().await.map_err(provider_error_to_status)?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let expired: Vec<String> = sandboxes
                    .into_iter()
                    .filter(|sb| sb.expires_at.map_or(false, |exp| exp <= now))
                    .map(|sb| sb.id)
                    .collect();
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: expired,
                }))
            } else {
                let destroyed = p.gc().await.map_err(provider_error_to_status)?;
                Ok(Response::new(proto::GcResponse {
                    destroyed_ids: destroyed,
                }))
            }
        })
    }

    async fn copy_to(
        &self,
        request: Request<proto::CopyToRequest>,
    ) -> Result<Response<proto::CopyToResponse>, Status> {
        let req = request.into_inner();
        // File copy is Docker-only
        self.docker
            .copy_to(
                &req.sandbox_id,
                std::path::Path::new(&req.host_path),
                &req.sandbox_path,
            )
            .await
            .map_err(provider_error_to_status)?;
        Ok(Response::new(proto::CopyToResponse {}))
    }

    async fn copy_from(
        &self,
        request: Request<proto::CopyFromRequest>,
    ) -> Result<Response<proto::CopyFromResponse>, Status> {
        let req = request.into_inner();
        // File copy is Docker-only
        self.docker
            .copy_from(
                &req.sandbox_id,
                &req.sandbox_path,
                std::path::Path::new(&req.host_path),
            )
            .await
            .map_err(provider_error_to_status)?;
        Ok(Response::new(proto::CopyFromResponse {}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_error_to_status_not_found() {
        let err = ProviderError::NotFound("abc".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_provider_error_to_status_timeout() {
        let err = ProviderError::Timeout(30);
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::DeadlineExceeded);
    }

    #[test]
    fn test_provider_error_to_status_unavailable() {
        let err = ProviderError::Unavailable("no docker".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::Unavailable);
    }

    #[test]
    fn test_provider_error_to_status_unsupported() {
        let err = ProviderError::Unsupported("not impl".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[test]
    fn test_provider_error_to_status_paused() {
        let err = ProviderError::Paused("abc".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn test_provider_error_to_status_create_failed() {
        let err = ProviderError::CreateFailed("oom".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_provider_error_to_status_exec_failed() {
        let err = ProviderError::ExecFailed("crash".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_provider_error_to_status_file_failed() {
        let err = ProviderError::FileFailed("no such file".into());
        let status = provider_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn test_sandbox_status_to_proto() {
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Running),
            proto::SandboxStatus::Running as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Paused),
            proto::SandboxStatus::Paused as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Stopped),
            proto::SandboxStatus::Stopped as i32
        );
        assert_eq!(
            sandbox_status_to_proto(SandboxStatus::Failed),
            proto::SandboxStatus::Failed as i32
        );
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(default_timeout(0), 300);
        assert_eq!(default_timeout(60), 60);
        assert_eq!(default_timeout(1), 1);
    }
}
```

- [ ] **Step 2: Update main.rs to wire up the server**

Replace `crates/roche-daemon/src/main.rs` with:

```rust
use clap::Parser;
use tonic::transport::Server;
use tracing_subscriber;

pub mod proto {
    tonic::include_proto!("roche.v1");
}

mod gc;
mod server;

#[derive(Parser)]
#[command(name = "roche-daemon", about = "Roche sandbox orchestrator daemon")]
struct Args {
    /// Port to listen on
    #[arg(long, default_value = "50051")]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let addr = format!("127.0.0.1:{}", args.port).parse()?;

    let service = server::SandboxServiceImpl::new();
    let svc = proto::sandbox_service_server::SandboxServiceServer::new(service);

    // Write daemon.json
    let roche_dir = dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche");
    std::fs::create_dir_all(&roche_dir)?;
    let daemon_json = roche_dir.join("daemon.json");
    let info = serde_json::json!({
        "pid": std::process::id(),
        "port": args.port
    });
    std::fs::write(&daemon_json, serde_json::to_string_pretty(&info)?)?;

    tracing::info!("roche-daemon listening on {}", addr);

    // Spawn background GC
    let gc_handle = tokio::spawn(gc::run_gc_loop());

    let shutdown = async {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("shutting down");
    };

    Server::builder()
        .add_service(svc)
        .serve_with_shutdown(addr, shutdown)
        .await?;

    gc_handle.abort();

    // Clean up daemon.json
    let _ = std::fs::remove_file(&daemon_json);

    Ok(())
}
```

- [ ] **Step 3: Build and run tests**

Run: `cargo build -p roche-daemon`
Expected: Compiles successfully.

Run: `cargo test -p roche-daemon`
Expected: All error mapping and status conversion tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/roche-daemon/src/
git commit -m "feat(daemon): implement gRPC server with provider dispatch and error mapping"
```

---

### Task 3: Background GC task

**Files:**
- Create: `crates/roche-daemon/src/gc.rs`

- [ ] **Step 1: Implement the GC loop**

Create `crates/roche-daemon/src/gc.rs`:

```rust
use roche_core::provider::docker::DockerProvider;
use roche_core::provider::SandboxLifecycle;

/// Runs garbage collection every 60 seconds on all providers.
pub async fn run_gc_loop() {
    let docker = DockerProvider::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

    loop {
        interval.tick().await;
        match docker.gc().await {
            Ok(ids) => {
                if !ids.is_empty() {
                    tracing::info!("GC destroyed {} sandbox(es)", ids.len());
                }
            }
            Err(e) => {
                tracing::warn!("GC error (docker): {e}");
            }
        }

        #[cfg(target_os = "linux")]
        {
            use roche_core::provider::firecracker::FirecrackerProvider;
            if let Ok(fc) = FirecrackerProvider::new() {
                match fc.gc().await {
                    Ok(ids) => {
                        if !ids.is_empty() {
                            tracing::info!("GC (firecracker) destroyed {} sandbox(es)", ids.len());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("GC error (firecracker): {e}");
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p roche-daemon`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add crates/roche-daemon/src/gc.rs
git commit -m "feat(daemon): add background GC task with 60s interval"
```

---

## Chunk 2: CLI Integration

### Task 4: Add tonic client dependencies and build.rs to CLI

**Files:**
- Modify: `crates/roche-cli/Cargo.toml`
- Create: `crates/roche-cli/build.rs`

- [ ] **Step 1: Update CLI Cargo.toml**

Add to `crates/roche-cli/Cargo.toml` dependencies:

```toml
tonic = "0.12"
prost = "0.13"
dirs = "6"
```

Add build-dependencies:

```toml
[build-dependencies]
tonic-build = "0.12"
```

- [ ] **Step 2: Create CLI build.rs**

Create `crates/roche-cli/build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &["../../proto/roche/v1/sandbox.proto"],
            &["../../proto"],
        )?;
    Ok(())
}
```

- [ ] **Step 3: Build**

Run: `cargo build -p roche-cli`
Expected: Compiles successfully with generated gRPC client stubs.

- [ ] **Step 4: Commit**

```bash
git add crates/roche-cli/Cargo.toml crates/roche-cli/build.rs
git commit -m "feat(cli): add tonic/prost deps and build.rs for gRPC client stubs"
```

---

### Task 5: Add `roche daemon` subcommand

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add Daemon subcommand to the Commands enum**

Add after the existing `Cp` variant in the `Commands` enum:

```rust
/// Manage the roche daemon
Daemon {
    #[command(subcommand)]
    action: DaemonAction,
},
```

Add the `DaemonAction` enum after `Commands`:

```rust
#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon
    Start {
        /// Port to listen on
        #[arg(long, default_value = "50051")]
        port: u16,

        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,
    },
    /// Stop the running daemon
    Stop,
    /// Show daemon status
    Status,
}
```

- [ ] **Step 2: Implement daemon command handlers**

Add a helper function to read daemon.json and the daemon command handler:

```rust
fn daemon_json_path() -> std::path::PathBuf {
    dirs::home_dir()
        .expect("cannot find home directory")
        .join(".roche")
        .join("daemon.json")
}

#[derive(serde::Deserialize)]
struct DaemonInfo {
    pid: u32,
    port: u16,
}

fn read_daemon_info() -> Option<DaemonInfo> {
    let path = daemon_json_path();
    let json = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&json).ok()
}

fn is_process_alive(pid: u32) -> bool {
    // signal 0 checks if process exists
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

async fn handle_daemon(action: DaemonAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        DaemonAction::Start { port, foreground } => {
            if let Some(info) = read_daemon_info() {
                if is_process_alive(info.pid) {
                    eprintln!("Daemon already running (pid={}, port={})", info.pid, info.port);
                    std::process::exit(1);
                }
            }

            if foreground {
                // Exec roche-daemon in foreground (replace this process)
                let status = tokio::process::Command::new("roche-daemon")
                    .arg("--port")
                    .arg(port.to_string())
                    .status()
                    .await?;
                std::process::exit(status.code().unwrap_or(1));
            } else {
                let roche_dir = dirs::home_dir()
                    .expect("cannot find home directory")
                    .join(".roche");
                std::fs::create_dir_all(&roche_dir)?;
                let log_file = std::fs::File::create(roche_dir.join("daemon.log"))?;
                let err_file = log_file.try_clone()?;

                let child = std::process::Command::new("roche-daemon")
                    .arg("--port")
                    .arg(port.to_string())
                    .stdout(log_file)
                    .stderr(err_file)
                    .spawn()?;

                println!("Daemon started (pid={}, port={})", child.id(), port);
            }
        }
        DaemonAction::Stop => {
            let info = read_daemon_info().ok_or("No daemon running")?;
            if !is_process_alive(info.pid) {
                // Clean up stale daemon.json
                let _ = std::fs::remove_file(daemon_json_path());
                eprintln!("No daemon running (stale pid file cleaned up)");
                std::process::exit(1);
            }
            unsafe {
                libc::kill(info.pid as i32, libc::SIGTERM);
            }
            println!("Daemon stopped (pid={})", info.pid);
        }
        DaemonAction::Status => {
            match read_daemon_info() {
                Some(info) if is_process_alive(info.pid) => {
                    println!("Daemon running (pid={}, port={})", info.pid, info.port);
                }
                Some(info) => {
                    let _ = std::fs::remove_file(daemon_json_path());
                    println!("Daemon not running (stale pid={}, cleaned up)", info.pid);
                }
                None => {
                    println!("Daemon not running");
                }
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Wire daemon handler into the run function**

In the `run()` function, add a match arm for `Commands::Daemon` before the provider dispatch:

```rust
// At the top of run(), before provider dispatch:
if let Commands::Daemon { action } = cli.command {
    return handle_daemon(action).await.map_err(|e| {
        ProviderError::ExecFailed(e.to_string())
    });
}
```

- [ ] **Step 4: Add the `libc` dependency to CLI Cargo.toml**

Add to `crates/roche-cli/Cargo.toml`:

```toml
libc = "0.2"
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p roche-cli`
Expected: Compiles successfully.

Run: `cargo run -- daemon status`
Expected: Prints "Daemon not running"

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/
git commit -m "feat(cli): add roche daemon start/stop/status subcommands"
```

---

### Task 6: CLI dual-mode dispatch (gRPC client fallback)

**Files:**
- Modify: `crates/roche-cli/src/main.rs`

- [ ] **Step 1: Add proto module and `--direct` flag**

Add the proto module near the top of main.rs:

```rust
pub mod proto {
    tonic::include_proto!("roche.v1");
}
```

Add `--direct` as a global arg on the `Cli` struct:

```rust
#[derive(Parser)]
#[command(name = "roche", about = "Universal sandbox orchestrator for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Force direct provider access (skip daemon even if running)
    #[arg(long, global = true)]
    direct: bool,
}
```

- [ ] **Step 2: Add daemon detection and gRPC client dispatch**

Add a function that tries to connect to the daemon and run the command via gRPC:

```rust
async fn try_daemon_dispatch(cli: &Cli) -> Option<Result<(), roche_core::provider::ProviderError>> {
    if cli.direct {
        return None;
    }

    let info = read_daemon_info()?;
    if !is_process_alive(info.pid) {
        return None;
    }

    // Try to connect
    let addr = format!("http://127.0.0.1:{}", info.port);
    let mut client = proto::sandbox_service_client::SandboxServiceClient::connect(addr)
        .await
        .ok()?;

    let result = run_via_grpc(&mut client, &cli.command).await;
    Some(result)
}
```

Add the gRPC dispatch function that maps CLI commands to gRPC calls:

```rust
async fn run_via_grpc(
    client: &mut proto::sandbox_service_client::SandboxServiceClient<tonic::transport::Channel>,
    command: &Commands,
) -> Result<(), roche_core::provider::ProviderError> {
    use roche_core::provider::ProviderError;

    match command {
        Commands::Create {
            provider,
            image,
            memory,
            cpus,
            timeout,
            network,
            writable,
            env,
            mounts,
            count,
            kernel,
            rootfs,
        } => {
            let env_map = parse_env_vars(env).map_err(|e| ProviderError::ExecFailed(e))?;
            let mount_configs: Vec<proto::MountConfig> = mounts
                .iter()
                .map(|s| {
                    let m = parse_mount(s).unwrap_or_else(|e| {
                        eprintln!("Error: {e}");
                        std::process::exit(1);
                    });
                    proto::MountConfig {
                        host_path: m.host_path,
                        container_path: m.container_path,
                        readonly: m.readonly,
                    }
                })
                .collect();

            for _ in 0..*count {
                let resp = client
                    .create(proto::CreateRequest {
                        provider: provider.clone(),
                        image: image.clone(),
                        memory: memory.clone(),
                        cpus: *cpus,
                        timeout_secs: *timeout,
                        network: *network,
                        writable: *writable,
                        env: env_map.clone(),
                        mounts: mount_configs.clone(),
                        kernel: kernel.clone(),
                        rootfs: rootfs.clone(),
                    })
                    .await
                    .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                println!("{}", resp.into_inner().sandbox_id);
            }
        }
        Commands::Exec {
            sandbox,
            timeout,
            command,
        } => {
            let resp = client
                .exec(proto::ExecRequest {
                    sandbox_id: sandbox.clone(),
                    command: command.clone(),
                    timeout_secs: *timeout,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let output = resp.into_inner();
            print!("{}", output.stdout);
            eprint!("{}", output.stderr);
            if output.exit_code != 0 {
                std::process::exit(output.exit_code);
            }
        }
        Commands::Destroy { ids, all } => {
            let resp = client
                .destroy(proto::DestroyRequest {
                    sandbox_ids: ids.clone(),
                    all: *all,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let _ = resp.into_inner();
        }
        Commands::List { json } => {
            let resp = client
                .list(proto::ListRequest {
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let sandboxes = resp.into_inner().sandboxes;
            if *json {
                println!("{}", serde_json::to_string_pretty(&sandboxes).unwrap());
            } else if sandboxes.is_empty() {
                println!("No active sandboxes.");
            } else {
                println!(
                    "{:<16} {:<10} {:<10} {:<10} IMAGE",
                    "ID", "STATUS", "PROVIDER", "EXPIRES"
                );
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                for sb in &sandboxes {
                    let status_str = match sb.status {
                        1 => "running",
                        2 => "paused",
                        3 => "stopped",
                        4 => "failed",
                        _ => "unknown",
                    };
                    let expires_str = match sb.expires_at {
                        Some(exp) if exp > now => {
                            let remaining = exp - now;
                            let mins = remaining / 60;
                            let secs = remaining % 60;
                            format!("{mins}m{secs:02}s")
                        }
                        Some(_) => "expired".to_string(),
                        None => "-".to_string(),
                    };
                    println!(
                        "{:<16} {:<10} {:<10} {:<10} {}",
                        sb.id, status_str, sb.provider, expires_str, sb.image,
                    );
                }
            }
        }
        Commands::Pause { id } => {
            client
                .pause(proto::PauseRequest {
                    sandbox_id: id.clone(),
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
        }
        Commands::Unpause { id } => {
            client
                .unpause(proto::UnpauseRequest {
                    sandbox_id: id.clone(),
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
        }
        Commands::Gc { dry_run, all } => {
            let resp = client
                .gc(proto::GcRequest {
                    dry_run: *dry_run,
                    all: *all,
                    provider: "docker".to_string(),
                })
                .await
                .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
            let destroyed = resp.into_inner().destroyed_ids;
            for id in &destroyed {
                if *dry_run {
                    println!("{id}");
                } else {
                    println!("destroyed: {id}");
                }
            }
            if destroyed.is_empty() && !*dry_run {
                println!("No expired sandboxes found.");
            }
        }
        Commands::Cp { src, dest } => {
            match (parse_cp_path(src), parse_cp_path(dest)) {
                (Some((sandbox_id, sandbox_path)), None) => {
                    client
                        .copy_from(proto::CopyFromRequest {
                            sandbox_id: sandbox_id.to_string(),
                            sandbox_path: sandbox_path.to_string(),
                            host_path: dest.clone(),
                            provider: "docker".to_string(),
                        })
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
                }
                (None, Some((sandbox_id, sandbox_path))) => {
                    client
                        .copy_to(proto::CopyToRequest {
                            sandbox_id: sandbox_id.to_string(),
                            host_path: src.clone(),
                            sandbox_path: sandbox_path.to_string(),
                            provider: "docker".to_string(),
                        })
                        .await
                        .map_err(|s| ProviderError::ExecFailed(s.message().to_string()))?;
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
        }
        Commands::Daemon { .. } => unreachable!("daemon handled earlier"),
    }
    Ok(())
}
```

- [ ] **Step 3: Wire dual-mode dispatch into run()**

Modify the `run()` function to try daemon dispatch first:

```rust
async fn run(cli: Cli) -> Result<(), roche_core::provider::ProviderError> {
    // Handle daemon subcommand first
    if let Commands::Daemon { ref action } = cli.command {
        return handle_daemon(action.clone()).await.map_err(|e| {
            roche_core::provider::ProviderError::ExecFailed(e.to_string())
        });
    }

    // Try daemon gRPC dispatch
    if let Some(result) = try_daemon_dispatch(&cli).await {
        return result;
    }

    // Fall through to direct provider access (existing code)
    // ... existing run() body ...
}
```

- [ ] **Step 4: Make DaemonAction Clone**

Add `#[derive(Clone)]` to `DaemonAction` so it can be used after moving:

```rust
#[derive(Subcommand, Clone)]
enum DaemonAction { ... }
```

- [ ] **Step 5: Build and verify**

Run: `cargo build`
Expected: All crates compile.

Run: `cargo test`
Expected: All existing tests pass, plus new error mapping tests.

Run: `cargo clippy`
Expected: No warnings.

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 6: Commit**

```bash
git add crates/roche-cli/src/main.rs
git commit -m "feat(cli): add dual-mode dispatch with gRPC client fallback"
```

---

## Chunk 3: Final Verification

### Task 7: Full build and test verification

- [ ] **Step 1: Run full build**

Run: `cargo build`
Expected: All 3 crates compile successfully.

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass (existing + new error mapping tests).

- [ ] **Step 3: Run clippy**

Run: `cargo clippy`
Expected: No warnings.

- [ ] **Step 4: Run fmt check**

Run: `cargo fmt --check`
Expected: No formatting issues.

- [ ] **Step 5: Verify daemon runs**

Run: `cargo run -p roche-daemon -- --port 50052`
Expected: Starts, prints "roche-daemon listening on 127.0.0.1:50052", CTRL-C stops it cleanly.

- [ ] **Step 6: Verify CLI daemon commands**

Run: `cargo run -- daemon status`
Expected: "Daemon not running"

- [ ] **Step 7: Commit any fixes**

If any issues were found, fix and commit.
