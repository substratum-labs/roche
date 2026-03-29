# Execution Trace Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add structured execution trace to every sandbox exec call, transforming Roche from a constraint-only sandbox into an AI agent nervous system.

**Architecture:** New `SandboxSensor` trait (Nerve layer) independent from `SandboxProvider` (Muscle layer). Docker implementation uses `docker stats` + `docker diff` + seccomp log. Daemon auto-spawned from bundled binary in Python wheel. Trace returned as `Option<ExecutionTrace>` in `ExecOutput`, default-on (opt-out).

**Tech Stack:** Rust (roche-core, roche-daemon), protobuf/tonic (gRPC), Python (SDK)

**Spec:** `docs/superpowers/specs/2026-03-18-execution-trace-design.md`

**Scope:** This plan covers Rust core + daemon + Python SDK. TypeScript SDK and daemon auto-management (maturin wheel bundling) are deferred to separate plans.

---

## File Structure

### New Files
- `crates/roche-core/src/sensor/mod.rs` — `SandboxSensor` trait, `TraceCollectorHandle`, `SensorDispatch` enums
- `crates/roche-core/src/sensor/docker.rs` — `DockerSensor`, `DockerTraceCollector` implementation
- `crates/roche-core/src/sensor/types.rs` — `ExecutionTrace`, `TraceLevel`, `ResourceUsage`, `FileAccess`, etc.
- `crates/roche-core/tests/trace_types_test.rs` — unit tests for trace types serialization
- `crates/roche-daemon/src/seccomp-trace.json` — seccomp profile with SCMP_ACT_LOG
- `sdk/python/src/roche_sandbox/trace.py` — Python `ExecutionTrace`, `ResourceUsage`, etc. dataclasses
- `sdk/python/tests/test_trace.py` — Python trace type tests

### Modified Files
- `crates/roche-core/src/lib.rs` — add `pub mod sensor;`
- `crates/roche-core/src/types.rs:118-123` — add `trace: Option<ExecutionTrace>` to `ExecOutput`
- `proto/roche/v1/sandbox.proto:44-56` — add `TraceLevel`, trace messages, extend `ExecRequest`/`ExecResponse`
- `crates/roche-daemon/build.rs` — unchanged (auto-picks up proto changes)
- `crates/roche-daemon/src/server.rs:170-192` — wrap exec with `traced_exec`, add `SensorDispatch`
- `crates/roche-daemon/src/main.rs` — init `DockerSensor` at startup, write seccomp profile
- `crates/roche-core/src/provider/docker.rs` — add `trace_enabled` to `build_create_args`
- `crates/roche-core/src/types.rs` — add `trace_enabled` to `SandboxConfig`
- `sdk/python/src/roche_sandbox/types.py:35-38` — add `trace` field to `ExecOutput`
- `sdk/python/src/roche_sandbox/transport/grpc.py:60-69` — pass `trace_level`, parse trace from response
- `sdk/python/src/roche_sandbox/transport/cli.py:46-54` — basic trace (duration only) for CLI fallback
- `sdk/python/src/roche_sandbox/sandbox.py:29-30` — pass `trace_level` param
- `sdk/python/scripts/proto-gen.sh` — unchanged (regenerate after proto changes)

---

## Chunk 1: Core Data Types and Trait Definitions

### Task 1: Trace Data Types (roche-core)

**Files:**
- Create: `crates/roche-core/src/sensor/types.rs`
- Create: `crates/roche-core/tests/trace_types_test.rs`

- [ ] **Step 1: Write tests for trace type serialization**

Create `crates/roche-core/tests/trace_types_test.rs`:
```rust
use roche_core::sensor::types::*;

#[test]
fn test_trace_level_default() {
    assert_eq!(TraceLevel::default(), TraceLevel::Standard);
}

#[test]
fn test_execution_trace_serialization_roundtrip() {
    let trace = ExecutionTrace {
        duration_secs: 2.3,
        resource_usage: ResourceUsage {
            peak_memory_bytes: 356_000_000,
            cpu_time_secs: 1.2,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
        },
        file_accesses: vec![FileAccess {
            path: "/data/input.csv".to_string(),
            op: FileOp::Read,
            size_bytes: Some(2_300_000),
        }],
        network_attempts: vec![NetworkAttempt {
            address: "169.254.169.254:80".to_string(),
            protocol: "tcp".to_string(),
            allowed: false,
        }],
        blocked_ops: vec![BlockedOperation {
            op_type: "network".to_string(),
            detail: "blocked connect to metadata service".to_string(),
        }],
        syscalls: vec![],
        resource_timeline: vec![],
    };

    let json = serde_json::to_string(&trace).unwrap();
    let deserialized: ExecutionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.duration_secs, 2.3);
    assert_eq!(deserialized.file_accesses.len(), 1);
    assert_eq!(deserialized.file_accesses[0].op, FileOp::Read);
    assert_eq!(deserialized.network_attempts[0].allowed, false);
}

#[test]
fn test_execution_trace_empty_fields_deserialize() {
    // Simulates old data without trace — all Vec fields default to empty
    let json = r#"{"duration_secs":1.0,"resource_usage":{"peak_memory_bytes":0,"cpu_time_secs":0.0,"network_rx_bytes":0,"network_tx_bytes":0}}"#;
    let trace: ExecutionTrace = serde_json::from_str(json).unwrap();
    assert!(trace.file_accesses.is_empty());
    assert!(trace.syscalls.is_empty());
}

#[test]
fn test_trace_level_serialization() {
    assert_eq!(serde_json::to_string(&TraceLevel::Full).unwrap(), "\"Full\"");
    assert_eq!(serde_json::to_string(&TraceLevel::Off).unwrap(), "\"Off\"");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test trace_types_test -p roche-core`
Expected: FAIL — `roche_core::sensor::types` module doesn't exist

- [ ] **Step 3: Implement trace types**

Create `crates/roche-core/src/sensor/types.rs`:
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceLevel {
    Off,
    Summary,
    Standard,
    Full,
}

impl Default for TraceLevel {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub duration_secs: f64,
    pub resource_usage: ResourceUsage,
    #[serde(default)]
    pub file_accesses: Vec<FileAccess>,
    #[serde(default)]
    pub network_attempts: Vec<NetworkAttempt>,
    #[serde(default)]
    pub blocked_ops: Vec<BlockedOperation>,
    #[serde(default)]
    pub syscalls: Vec<SyscallEvent>,
    #[serde(default)]
    pub resource_timeline: Vec<ResourceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub peak_memory_bytes: u64,
    pub cpu_time_secs: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileOp {
    Read,
    Write,
    Create,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub path: String,
    pub op: FileOp,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAttempt {
    pub address: String,
    pub protocol: String,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedOperation {
    pub op_type: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallEvent {
    pub name: String,
    pub args: Vec<String>,
    pub result: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub timestamp_ms: u64,
    pub memory_bytes: u64,
    pub cpu_percent: f32,
}
```

- [ ] **Step 4: Create sensor module**

Create `crates/roche-core/src/sensor/mod.rs`:
```rust
pub mod types;

pub use types::*;
```

Add to `crates/roche-core/src/lib.rs`:
```rust
pub mod sensor;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test trace_types_test -p roche-core`
Expected: All 4 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/roche-core/src/sensor/ crates/roche-core/src/lib.rs crates/roche-core/tests/trace_types_test.rs
git commit -m "feat: add execution trace data types (sensor/types.rs)"
```

### Task 2: SandboxSensor Trait and Dispatch Enums

**Files:**
- Modify: `crates/roche-core/src/sensor/mod.rs`

- [ ] **Step 1: Add trait and dispatch enums to sensor/mod.rs**

Update `crates/roche-core/src/sensor/mod.rs`:
```rust
pub mod types;

pub use types::*;

use crate::provider::ProviderError;
use crate::types::SandboxId;

/// Sensor trait — Nerve layer, independent from SandboxProvider (Muscle layer).
#[allow(async_fn_in_trait)]
pub trait SandboxSensor: Send + Sync {
    async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<TraceCollectorHandle, ProviderError>;
}

/// Concrete collector enum — avoids dyn trait object-safety issues with native async traits.
pub enum TraceCollectorHandle {
    Docker(DockerTraceCollector),
}

impl TraceCollectorHandle {
    pub async fn finish(self) -> Result<ExecutionTrace, ProviderError> {
        match self {
            Self::Docker(c) => c.finish().await,
        }
    }

    pub async fn abort(self) {
        match self {
            Self::Docker(c) => c.abort().await,
        }
    }
}

/// Concrete sensor dispatch — routes to provider-specific sensor impl.
pub enum SensorDispatch {
    Docker(DockerSensor),
    None,
}

impl SensorDispatch {
    pub async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<Option<TraceCollectorHandle>, ProviderError> {
        match self {
            Self::Docker(s) => Ok(Some(s.start_trace(id, level).await?)),
            Self::None => Ok(None),
        }
    }
}

// Forward declarations — actual impls in docker.rs
pub use docker::{DockerSensor, DockerTraceCollector};

pub mod docker;
```

- [ ] **Step 2: Create stub docker sensor**

Create `crates/roche-core/src/sensor/docker.rs`:
```rust
use std::time::Instant;

use crate::provider::ProviderError;
use crate::types::SandboxId;
use super::types::*;
use super::TraceCollectorHandle;

pub struct DockerSensor;

impl super::SandboxSensor for DockerSensor {
    async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<TraceCollectorHandle, ProviderError> {
        let collector = DockerTraceCollector::start(id.clone(), level).await?;
        Ok(TraceCollectorHandle::Docker(collector))
    }
}

pub struct DockerTraceCollector {
    level: TraceLevel,
    container_id: String,
    start_time: Instant,
}

impl DockerTraceCollector {
    pub async fn start(id: SandboxId, level: TraceLevel) -> Result<Self, ProviderError> {
        Ok(Self {
            level,
            container_id: id,
            start_time: Instant::now(),
        })
    }

    pub async fn finish(self) -> Result<ExecutionTrace, ProviderError> {
        let duration = self.start_time.elapsed().as_secs_f64();

        // MVP: Summary level — just duration and stats
        let resource_usage = self.collect_stats().await.unwrap_or_default();

        let (file_accesses, network_attempts, blocked_ops) = if self.level >= TraceLevel::Standard {
            self.collect_standard().await.unwrap_or_default()
        } else {
            Default::default()
        };

        Ok(ExecutionTrace {
            duration_secs: duration,
            resource_usage,
            file_accesses,
            network_attempts,
            blocked_ops,
            syscalls: vec![],
            resource_timeline: vec![],
        })
    }

    pub async fn abort(self) {
        // Cleanup: nothing to do for MVP (no background tasks started)
    }

    async fn collect_stats(&self) -> Result<ResourceUsage, ProviderError> {
        // docker stats --no-stream --format '{{json .}}' <container_id>
        let output = tokio::process::Command::new("docker")
            .args([
                "stats", "--no-stream",
                "--format", "{{.MemUsage}}|{{.NetIO}}|{{.CPUPerc}}",
                &self.container_id,
            ])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("docker stats failed: {e}")))?;

        let raw = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = raw.trim().split('|').collect();

        Ok(ResourceUsage {
            peak_memory_bytes: parse_memory_bytes(parts.first().unwrap_or(&"")),
            cpu_time_secs: 0.0, // docker stats doesn't give cumulative CPU time
            network_rx_bytes: parse_net_rx(parts.get(1).unwrap_or(&"")),
            network_tx_bytes: parse_net_tx(parts.get(1).unwrap_or(&"")),
        })
    }

    async fn collect_standard(&self) -> Result<(Vec<FileAccess>, Vec<NetworkAttempt>, Vec<BlockedOperation>), ProviderError> {
        // docker diff <container_id> — shows filesystem changes
        let output = tokio::process::Command::new("docker")
            .args(["diff", &self.container_id])
            .output()
            .await
            .map_err(|e| ProviderError::ExecFailed(format!("docker diff failed: {e}")))?;

        let diff_output = String::from_utf8_lossy(&output.stdout);
        let file_accesses = parse_docker_diff(&diff_output);

        // Network attempts and blocked ops from seccomp log — not implemented in MVP
        let network_attempts = vec![];
        let blocked_ops = vec![];

        Ok((file_accesses, network_attempts, blocked_ops))
    }
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            peak_memory_bytes: 0,
            cpu_time_secs: 0.0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
        }
    }
}

// Implement PartialOrd for TraceLevel to enable >= comparison
impl PartialOrd for TraceLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TraceLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordinal().cmp(&other.ordinal())
    }
}

impl TraceLevel {
    fn ordinal(&self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Summary => 1,
            Self::Standard => 2,
            Self::Full => 3,
        }
    }
}

/// Parse docker stats memory string like "340MiB / 512MiB" → bytes
fn parse_memory_bytes(s: &str) -> u64 {
    let s = s.trim();
    let usage = s.split('/').next().unwrap_or("").trim();
    if usage.ends_with("GiB") {
        usage.trim_end_matches("GiB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1024 * 1024 * 1024
    } else if usage.ends_with("MiB") {
        usage.trim_end_matches("MiB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1024 * 1024
    } else if usage.ends_with("KiB") {
        usage.trim_end_matches("KiB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1024
    } else if usage.ends_with("B") {
        usage.trim_end_matches('B').trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    }
}

/// Parse docker stats net I/O string like "1.2kB / 0B" → (rx_bytes, tx_bytes)
fn parse_net_rx(s: &str) -> u64 {
    let parts: Vec<&str> = s.split('/').collect();
    parse_size_bytes(parts.first().unwrap_or(&""))
}

fn parse_net_tx(s: &str) -> u64 {
    let parts: Vec<&str> = s.split('/').collect();
    parse_size_bytes(parts.get(1).unwrap_or(&""))
}

fn parse_size_bytes(s: &str) -> u64 {
    let s = s.trim();
    if s.ends_with("GB") {
        s.trim_end_matches("GB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1_000_000_000
    } else if s.ends_with("MB") {
        s.trim_end_matches("MB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1_000_000
    } else if s.ends_with("kB") {
        s.trim_end_matches("kB").trim().parse::<f64>().unwrap_or(0.0) as u64 * 1_000
    } else if s.ends_with("B") {
        s.trim_end_matches('B').trim().parse::<u64>().unwrap_or(0)
    } else {
        0
    }
}

/// Parse docker diff output lines like "C /workspace" "A /workspace/output.json"
fn parse_docker_diff(output: &str) -> Vec<FileAccess> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() { return None; }
            let (op_char, path) = line.split_at(1);
            let path = path.trim().to_string();
            let op = match op_char {
                "A" => FileOp::Create,
                "C" => FileOp::Write,
                "D" => FileOp::Delete,
                _ => return None,
            };
            Some(FileAccess { path, op, size_bytes: None })
        })
        .collect()
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p roche-core`
Expected: BUILD SUCCESS

- [ ] **Step 4: Run all existing tests still pass**

Run: `cargo test -p roche-core`
Expected: All tests PASS (including trace_types_test)

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/sensor/
git commit -m "feat: add SandboxSensor trait, DockerSensor, and trace collection"
```

### Task 3: Extend ExecOutput with trace field

**Files:**
- Modify: `crates/roche-core/src/types.rs:118-123`

- [ ] **Step 1: Add trace field to ExecOutput**

In `crates/roche-core/src/types.rs`, modify `ExecOutput`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub trace: Option<crate::sensor::ExecutionTrace>,
}
```

Also add `trace_enabled: bool` to `SandboxConfig` (with `#[serde(default)]`):
```rust
// In SandboxConfig struct, add:
    #[serde(default = "default_true")]
    pub trace_enabled: bool,
```

And in the explicit `Default` impl for `SandboxConfig`, add:
```rust
    trace_enabled: true,  // opt-out model: trace is on by default
```

Add helper:
```rust
fn default_true() -> bool { true }
```

- [ ] **Step 2: Fix all compilation errors**

All existing code that constructs `ExecOutput` needs `trace: None` added. These are in:
- `crates/roche-core/src/provider/docker.rs` (line ~185) — struct literal, add `trace: None`
- `crates/roche-core/src/provider/e2b.rs` — struct literal, add `trace: None`
- `crates/roche-core/src/provider/wasm/engine.rs` (line ~129) — struct literal, add `trace: None`
- `crates/roche-core/src/provider/k8s.rs` — struct literal, add `trace: None`

**Special case — Firecracker vsock:** `crates/roche-core/src/provider/firecracker/vsock_exec.rs` (line ~72) deserializes `ExecOutput` from JSON via `serde_json::from_slice`. Since the `trace` field has `#[serde(default)]`, this will automatically deserialize to `None` — no code change needed.

- [ ] **Step 3: Verify build**

Run: `cargo build`
Expected: BUILD SUCCESS (entire workspace)

- [ ] **Step 4: Run all tests**

Run: `cargo test`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/types.rs crates/roche-core/src/provider/
git commit -m "feat: add trace field to ExecOutput, trace_enabled to SandboxConfig"
```

---

## Chunk 2: Proto and Daemon Integration

### Task 4: Proto changes

**Files:**
- Modify: `proto/roche/v1/sandbox.proto`

- [ ] **Step 1: Add trace messages and fields to proto**

Add to `proto/roche/v1/sandbox.proto`:

After existing enums, add:
```protobuf
enum TraceLevel {
  TRACE_LEVEL_OFF = 0;
  TRACE_LEVEL_SUMMARY = 1;
  TRACE_LEVEL_STANDARD = 2;
  TRACE_LEVEL_FULL = 3;
}

enum FileOp {
  FILE_OP_READ = 0;
  FILE_OP_WRITE = 1;
  FILE_OP_CREATE = 2;
  FILE_OP_DELETE = 3;
}

message ExecutionTrace {
  double duration_secs = 1;
  ResourceUsage resource_usage = 2;
  repeated FileAccess file_accesses = 3;
  repeated NetworkAttempt network_attempts = 4;
  repeated BlockedOperation blocked_ops = 5;
  repeated SyscallEvent syscalls = 6;
  repeated ResourceSnapshot resource_timeline = 7;
}

message ResourceUsage {
  uint64 peak_memory_bytes = 1;
  double cpu_time_secs = 2;
  uint64 network_rx_bytes = 3;
  uint64 network_tx_bytes = 4;
}

message FileAccess {
  string path = 1;
  FileOp op = 2;
  optional uint64 size_bytes = 3;
}

message NetworkAttempt {
  string address = 1;
  string protocol = 2;
  bool allowed = 3;
}

message BlockedOperation {
  string op_type = 1;
  string detail = 2;
}

message SyscallEvent {
  string name = 1;
  repeated string args = 2;
  string result = 3;
  uint64 timestamp_ms = 4;
}

message ResourceSnapshot {
  uint64 timestamp_ms = 1;
  uint64 memory_bytes = 2;
  float cpu_percent = 3;
}
```

Modify `ExecRequest` — add field:
```protobuf
  TraceLevel trace_level = 5;
```

Modify `ExecResponse` — add field:
```protobuf
  ExecutionTrace trace = 4;
```

- [ ] **Step 2: Verify proto compiles**

Run: `cargo build -p roche-daemon`
Expected: BUILD SUCCESS (tonic-build auto-generates from proto)

- [ ] **Step 3: Regenerate Python proto bindings**

Run: `cd sdk/python && bash scripts/proto-gen.sh`
Expected: Generated files updated in `sdk/python/src/roche_sandbox/generated/`

- [ ] **Step 4: Commit**

```bash
git add proto/ sdk/python/src/roche_sandbox/generated/
git commit -m "feat: add execution trace messages to gRPC proto"
```

### Task 5: Daemon server integration

**Files:**
- Modify: `crates/roche-daemon/src/server.rs:170-192`
- Modify: `crates/roche-daemon/src/main.rs`

- [ ] **Step 1: Add SensorDispatch to SandboxServiceImpl**

In `crates/roche-daemon/src/server.rs`, add to the `SandboxServiceImpl` struct:
```rust
use roche_core::sensor::{SensorDispatch, DockerSensor, TraceLevel};
```

Add field to `SandboxServiceImpl`:
```rust
    docker_sensor: SensorDispatch,
```

Add method:
```rust
    fn get_sensor(&self, provider: &str) -> &SensorDispatch {
        match provider {
            "docker" => &self.docker_sensor,
            _ => &SensorDispatch::None,
        }
    }
```

- [ ] **Step 2: Modify exec handler to use traced_exec**

Replace the `exec` RPC handler in `server.rs` with:
```rust
async fn exec(
    &self,
    request: Request<proto::ExecRequest>,
) -> Result<Response<proto::ExecResponse>, Status> {
    let req = request.into_inner();
    let exec_req = types::ExecRequest {
        command: req.command,
        timeout_secs: req.timeout_secs,
    };
    let provider_name = default_provider(&req.provider);
    let trace_level = TraceLevel::from_proto(req.trace_level);

    with_provider!(self, provider_name, |p| {
        let sensor = self.get_sensor(&provider_name);

        let output = if trace_level != TraceLevel::Off {
            if let Some(collector) = sensor
                .start_trace(&req.sandbox_id, trace_level)
                .await
                .map_err(provider_error_to_status)?
            {
                match p.exec(&req.sandbox_id, &exec_req).await {
                    Ok(mut output) => {
                        let trace = collector
                            .finish()
                            .await
                            .map_err(provider_error_to_status)?;
                        output.trace = Some(trace);
                        output
                    }
                    Err(e) => {
                        collector.abort().await;
                        return Err(provider_error_to_status(e));
                    }
                }
            } else {
                p.exec(&req.sandbox_id, &exec_req)
                    .await
                    .map_err(provider_error_to_status)?
            }
        } else {
            p.exec(&req.sandbox_id, &exec_req)
                .await
                .map_err(provider_error_to_status)?
        };

        Ok(Response::new(proto::ExecResponse {
            exit_code: output.exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
            trace: output.trace.map(|t| t.to_proto()),
        }))
    })
}
```

- [ ] **Step 3: Add TraceLevel::from_proto and ExecutionTrace::to_proto conversions**

Add to `crates/roche-core/src/sensor/types.rs`:
```rust
impl TraceLevel {
    pub fn from_proto(value: i32) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::Summary,
            2 => Self::Standard,
            3 => Self::Full,
            _ => Self::Standard, // default
        }
    }
}
```

Add `to_proto` method on `ExecutionTrace` in `crates/roche-daemon/src/server.rs` (or a conversion module) that maps the Rust types to the generated proto types.

- [ ] **Step 4: Initialize DockerSensor in main.rs**

In `crates/roche-daemon/src/main.rs`, where `SandboxServiceImpl` is constructed, add:
```rust
docker_sensor: SensorDispatch::Docker(DockerSensor),
```

- [ ] **Step 5: Verify build**

Run: `cargo build -p roche-daemon`
Expected: BUILD SUCCESS

- [ ] **Step 6: Run existing daemon tests**

Run: `cargo test -p roche-daemon`
Expected: All tests PASS

- [ ] **Step 7: Commit**

```bash
git add crates/roche-daemon/src/ crates/roche-core/src/sensor/
git commit -m "feat: integrate execution trace into daemon exec handler"
```

---

## Chunk 3: Python SDK Integration

### Task 6: Python trace types

**Files:**
- Create: `sdk/python/src/roche_sandbox/trace.py`
- Create: `sdk/python/tests/test_trace.py`

- [ ] **Step 1: Write tests for Python trace types**

Create `sdk/python/tests/test_trace.py`:
```python
from roche_sandbox.trace import ExecutionTrace, ResourceUsage, FileAccess, TraceLevel


def test_trace_summary_basic():
    trace = ExecutionTrace(
        duration_secs=2.3,
        resource_usage=ResourceUsage(
            peak_memory_bytes=356_000_000,
            cpu_time_secs=1.2,
            network_rx_bytes=0,
            network_tx_bytes=0,
        ),
        file_accesses=[
            FileAccess(path="/data/input.csv", op="read", size_bytes=2_300_000),
            FileAccess(path="/workspace/out.json", op="create", size_bytes=4_100),
        ],
        network_attempts=[],
        blocked_ops=[],
        syscalls=[],
        resource_timeline=[],
    )
    summary = trace.summary()
    assert "2.3s" in summary
    assert "356MB" in summary
    assert "read 1 files" in summary
    assert "wrote 1 files" in summary


def test_trace_summary_empty():
    trace = ExecutionTrace(
        duration_secs=0.01,
        resource_usage=ResourceUsage(
            peak_memory_bytes=1_000_000,
            cpu_time_secs=0.0,
            network_rx_bytes=0,
            network_tx_bytes=0,
        ),
        file_accesses=[],
        network_attempts=[],
        blocked_ops=[],
        syscalls=[],
        resource_timeline=[],
    )
    summary = trace.summary()
    assert "0.0s" in summary
    assert "blocked" not in summary


def test_trace_level_values():
    assert TraceLevel.OFF == "off"
    assert TraceLevel.STANDARD == "standard"
    assert TraceLevel.FULL == "full"
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `pytest sdk/python/tests/test_trace.py -v`
Expected: FAIL — `roche_sandbox.trace` module doesn't exist

- [ ] **Step 3: Implement Python trace types**

Create `sdk/python/src/roche_sandbox/trace.py`:
```python
from __future__ import annotations
from dataclasses import dataclass, field


class TraceLevel:
    OFF = "off"
    SUMMARY = "summary"
    STANDARD = "standard"
    FULL = "full"


@dataclass
class ResourceUsage:
    peak_memory_bytes: int
    cpu_time_secs: float
    network_rx_bytes: int
    network_tx_bytes: int


@dataclass
class FileAccess:
    path: str
    op: str  # "read", "write", "create", "delete"
    size_bytes: int | None = None


@dataclass
class NetworkAttempt:
    address: str
    protocol: str
    allowed: bool


@dataclass
class BlockedOperation:
    op_type: str
    detail: str


@dataclass
class SyscallEvent:
    name: str
    args: list[str]
    result: str
    timestamp_ms: int


@dataclass
class ResourceSnapshot:
    timestamp_ms: int
    memory_bytes: int
    cpu_percent: float


@dataclass
class ExecutionTrace:
    duration_secs: float
    resource_usage: ResourceUsage
    file_accesses: list[FileAccess] = field(default_factory=list)
    network_attempts: list[NetworkAttempt] = field(default_factory=list)
    blocked_ops: list[BlockedOperation] = field(default_factory=list)
    syscalls: list[SyscallEvent] = field(default_factory=list)
    resource_timeline: list[ResourceSnapshot] = field(default_factory=list)

    def summary(self) -> str:
        """LLM-friendly one-line summary."""
        parts = [f"{self.duration_secs:.1f}s"]
        parts.append(f"mem {self.resource_usage.peak_memory_bytes // 1_000_000}MB")
        if self.file_accesses:
            reads = sum(1 for f in self.file_accesses if f.op == "read")
            writes = sum(
                1 for f in self.file_accesses if f.op in ("write", "create")
            )
            if reads:
                parts.append(f"read {reads} files")
            if writes:
                parts.append(f"wrote {writes} files")
        blocked = len(self.blocked_ops)
        if blocked:
            parts.append(f"blocked {blocked} ops")
        return " | ".join(parts)
```

- [ ] **Step 4: Run tests**

Run: `pytest sdk/python/tests/test_trace.py -v`
Expected: All 3 tests PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/trace.py sdk/python/tests/test_trace.py
git commit -m "feat: add Python execution trace types with LLM summary"
```

### Task 7: Python SDK ExecOutput and transport integration

**Files:**
- Modify: `sdk/python/src/roche_sandbox/types.py:35-38`
- Modify: `sdk/python/src/roche_sandbox/transport/grpc.py:60-69`
- Modify: `sdk/python/src/roche_sandbox/transport/cli.py:46-54`
- Modify: `sdk/python/src/roche_sandbox/sandbox.py:29-30`

- [ ] **Step 1: Add trace to ExecOutput**

In `sdk/python/src/roche_sandbox/types.py`, modify `ExecOutput`:
```python
from roche_sandbox.trace import ExecutionTrace

@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str
    trace: ExecutionTrace | None = None
```

- [ ] **Step 2: Update GrpcTransport to pass trace_level and parse trace response**

In `sdk/python/src/roche_sandbox/transport/grpc.py`, modify `exec()`:
- Add `trace_level` parameter
- Map trace_level string to proto enum value
- Parse `response.trace` into Python `ExecutionTrace`

- [ ] **Step 3: Update CliTransport to return basic trace**

In `sdk/python/src/roche_sandbox/transport/cli.py`, modify `exec()`:
- Add `trace_level` parameter (ignored for CLI)
- Measure duration with `time.monotonic()`
- Return `ExecOutput` with basic `ExecutionTrace` (duration only)

- [ ] **Step 4: Update AsyncSandbox.exec to accept trace_level**

In `sdk/python/src/roche_sandbox/sandbox.py`, modify `exec()`:
```python
async def exec(
    self,
    command: list[str],
    timeout_secs: int | None = None,
    trace_level: str | None = None,
) -> ExecOutput:
    return await self._transport.exec(
        self._id, command, self._provider, timeout_secs, trace_level
    )
```

- [ ] **Step 5: Update sync Sandbox.exec() to accept trace_level**

In `sdk/python/src/roche_sandbox/sandbox.py`, the sync `Sandbox` class also has an `exec()` method. Update it to pass through `trace_level`:
```python
def exec(
    self,
    command: list[str],
    timeout_secs: int | None = None,
    trace_level: str | None = None,
) -> ExecOutput:
    return asyncio.run(self._async.exec(command, timeout_secs, trace_level))
```

- [ ] **Step 6: Update existing tests to handle new trace field**

Existing tests that assert on `ExecOutput` may need `trace=None` or updated assertions.

- [ ] **Step 7: Run all Python tests**

Run: `pytest sdk/python/tests/ -v`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add sdk/python/src/roche_sandbox/ sdk/python/tests/
git commit -m "feat: integrate execution trace into Python SDK"
```

---

## Chunk 4: Docker trace_enabled and Integration Test

### Task 8: Docker container creation with trace support

**Files:**
- Modify: `crates/roche-core/src/provider/docker.rs`

- [ ] **Step 1: Add trace_enabled to build_create_args**

In `docker.rs`, in `build_create_args()`, add after the existing security opts:
```rust
if config.trace_enabled {
    // Use seccomp profile with SCMP_ACT_LOG for trace-monitored syscalls
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

- [ ] **Step 2: Set trace_enabled default in daemon**

In `crates/roche-daemon/src/server.rs`, in the `create` handler, set `trace_enabled = true` on the `SandboxConfig` before passing to provider.

- [ ] **Step 3: Create seccomp-trace.json**

Create `crates/roche-daemon/src/seccomp-trace.json` — Docker's default seccomp profile with `SCMP_ACT_LOG` for connect, open, openat, unlink syscalls. Embed via `include_str!` in daemon main.rs and write to `~/.roche/seccomp-trace.json` on startup.

- [ ] **Step 4: Verify build**

Run: `cargo build`
Expected: BUILD SUCCESS

- [ ] **Step 5: Commit**

```bash
git add crates/roche-core/src/provider/docker.rs crates/roche-daemon/src/
git commit -m "feat: add trace_enabled support to Docker container creation"
```

### Task 9: DockerTraceCollector unit tests (mocked CLI)

**Files:**
- Create: `crates/roche-core/tests/docker_trace_test.rs`

- [ ] **Step 1: Write unit tests for Docker CLI output parsing**

Create `crates/roche-core/tests/docker_trace_test.rs`:
```rust
use roche_core::sensor::docker::{parse_docker_diff, parse_memory_bytes, parse_net_rx, parse_net_tx};
use roche_core::sensor::types::FileOp;

#[test]
fn test_parse_docker_diff_creates() {
    let output = "A /workspace/output.txt\nA /tmp/cache\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].op, FileOp::Create);
    assert_eq!(accesses[0].path, "/workspace/output.txt");
}

#[test]
fn test_parse_docker_diff_changes() {
    let output = "C /var/log\nC /var/log/syslog\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 2);
    assert_eq!(accesses[0].op, FileOp::Write);
}

#[test]
fn test_parse_docker_diff_deletes() {
    let output = "D /tmp/old_file\n";
    let accesses = parse_docker_diff(output);
    assert_eq!(accesses.len(), 1);
    assert_eq!(accesses[0].op, FileOp::Delete);
}

#[test]
fn test_parse_docker_diff_empty() {
    let accesses = parse_docker_diff("");
    assert!(accesses.is_empty());
}

#[test]
fn test_parse_memory_bytes_mib() {
    assert_eq!(parse_memory_bytes("340MiB / 512MiB"), 340 * 1024 * 1024);
}

#[test]
fn test_parse_memory_bytes_gib() {
    assert_eq!(parse_memory_bytes("1.5GiB / 2GiB"), (1.5 * 1024.0 * 1024.0 * 1024.0) as u64);
}

#[test]
fn test_parse_memory_bytes_empty() {
    assert_eq!(parse_memory_bytes(""), 0);
}

#[test]
fn test_parse_net_io() {
    assert_eq!(parse_net_rx("1.2kB / 0B"), 1200);
    assert_eq!(parse_net_tx("1.2kB / 0B"), 0);
}
```

Note: The `parse_docker_diff`, `parse_memory_bytes`, `parse_net_rx`, `parse_net_tx` functions in `sensor/docker.rs` need to be made `pub` for testing.

- [ ] **Step 2: Make parser functions public**

In `crates/roche-core/src/sensor/docker.rs`, change visibility of `parse_docker_diff`, `parse_memory_bytes`, `parse_net_rx`, `parse_net_tx` from `fn` to `pub fn`.

- [ ] **Step 3: Run tests**

Run: `cargo test --test docker_trace_test -p roche-core`
Expected: All 9 tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/roche-core/tests/docker_trace_test.rs crates/roche-core/src/sensor/docker.rs
git commit -m "test: add DockerTraceCollector unit tests for CLI output parsing"
```

### Task 10: Integration test

**Files:**
- Create: `crates/roche-core/tests/trace_integration.rs`

- [ ] **Step 1: Write integration test**

Create `crates/roche-core/tests/trace_integration.rs`:
```rust
//! Integration test: requires Docker running.
//! Run with: cargo test --test trace_integration -- --ignored

use roche_core::provider::{SandboxProvider, docker::DockerProvider};
use roche_core::sensor::{DockerSensor, SandboxSensor, TraceLevel};
use roche_core::types::{SandboxConfig, ExecRequest};

#[tokio::test]
#[ignore] // requires Docker
async fn test_trace_summary_returns_duration() {
    let provider = DockerProvider::new();
    let sensor = DockerSensor;
    let config = SandboxConfig::default(); // uses python:3.12-slim, network off, readonly

    let id = provider.create(&config).await.unwrap();

    let collector = sensor.start_trace(&id, TraceLevel::Summary).await.unwrap();
    let output = provider.exec(&id, &ExecRequest {
        command: vec!["echo".into(), "hello".into()],
        timeout_secs: Some(10),
    }).await.unwrap();
    let trace = collector.finish().await.unwrap();

    assert!(trace.duration_secs > 0.0);
    assert_eq!(output.stdout.trim(), "hello");

    provider.destroy(&id).await.unwrap();
}

#[tokio::test]
#[ignore] // requires Docker
async fn test_trace_standard_detects_file_writes() {
    let provider = DockerProvider::new();
    let sensor = DockerSensor;
    let mut config = SandboxConfig::default();
    config.writable = true; // need writable to create files

    let id = provider.create(&config).await.unwrap();

    let collector = sensor.start_trace(&id, TraceLevel::Standard).await.unwrap();
    let _output = provider.exec(&id, &ExecRequest {
        command: vec!["sh".into(), "-c".into(), "echo test > /tmp/output.txt".into()],
        timeout_secs: Some(10),
    }).await.unwrap();
    let trace = collector.finish().await.unwrap();

    assert!(trace.duration_secs > 0.0);
    assert!(!trace.file_accesses.is_empty(), "should detect file creation");

    let created_files: Vec<_> = trace.file_accesses.iter()
        .filter(|f| f.path.contains("output.txt"))
        .collect();
    assert!(!created_files.is_empty(), "should detect output.txt creation");

    provider.destroy(&id).await.unwrap();
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test trace_integration -- --ignored`
Expected: Both tests PASS (requires Docker running)

- [ ] **Step 3: Commit**

```bash
git add crates/roche-core/tests/trace_integration.rs
git commit -m "test: add execution trace integration tests"
```

### Task 11: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test && cargo clippy && cargo fmt --check`
Expected: All pass

- [ ] **Step 2: Run full Python test suite**

Run: `pytest sdk/python/tests/ -v`
Expected: All pass

- [ ] **Step 3: Final commit and tag**

```bash
git tag v0.2.0-alpha
```
