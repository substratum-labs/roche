# Execution Trace — Design Spec

**Date:** 2026-03-18
**Version:** 0.2
**Status:** Draft

## Overview

Add structured execution trace to every sandbox exec call, enabling AI agents to "feel" what happened during code execution — not just see the final stdout. This is the first step in transforming Roche from a constraint-only sandbox into an AI agent nervous system.

### Body Model Context

Roche's internal architecture follows a three-layer body model:

- **Bone (Provider):** Docker, Firecracker, WASM, K8s — rigid structure
- **Muscle (Executor):** exec, file ops — produces force
- **Nerve (Sensor):** trace, streaming, control — signal channel

This spec builds the **Nerve layer** via a new `SandboxSensor` trait and `DockerSensor` implementation.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Collection strategy | Per-provider optimal (mixed) | Docker uses seccomp log + stats API; future providers use their native mechanisms |
| Return mode | Opt-out (default on) | "Body defaults to having sensation" — embodiment philosophy |
| Detail levels | 3 levels + off: off / summary / standard (default) / full | Maps to anesthesia / proprioception / touch / deep sensation |
| MVP scope | Docker only | 80%+ users, fastest feedback loop |
| Architecture | Trait separation (SandboxSensor) | Nerve layer independent from Muscle layer; matches body model |

## Data Model

### TraceLevel

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceLevel {
    Off,
    Summary,
    Standard,  // default
    Full,
}
```

### ExecutionTrace

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
    pub duration_secs: f64,
    pub resource_usage: ResourceUsage,
    pub file_accesses: Vec<FileAccess>,       // Standard+
    pub network_attempts: Vec<NetworkAttempt>, // Standard+
    pub blocked_ops: Vec<BlockedOperation>,    // Standard+
    pub syscalls: Vec<SyscallEvent>,           // Full only
    pub resource_timeline: Vec<ResourceSnapshot>, // Full only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub peak_memory_bytes: u64,
    pub cpu_time_secs: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccess {
    pub path: String,
    pub op: FileOp,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileOp { Read, Write, Create, Delete }

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

### ExecOutput Extension

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    #[serde(default)]
    pub trace: Option<ExecutionTrace>,  // NEW — #[serde(default)] for backward-compatible deserialization
}
```

All `Vec` fields use empty Vec (not Option) for "no data" — extensible without breaking changes. New fields added as `Vec<T>` in the future.

## Trait Architecture

### SandboxSensor Trait (Nerve Layer)

The codebase uses native async fn in traits (`#[allow(async_fn_in_trait)]`), not the `async-trait` crate. However, `SandboxSensor::start_trace` returns `Box<dyn TraceCollector>`, which requires object safety. Since native async traits are not object-safe by default, we use a concrete `TraceCollectorHandle` enum instead of a dyn trait for the collector:

```rust
/// Sensor trait — independent from SandboxProvider (Muscle layer).
/// Each provider that supports tracing implements its own Sensor.
#[allow(async_fn_in_trait)]
pub trait SandboxSensor: Send + Sync {
    async fn start_trace(
        &self,
        id: &SandboxId,
        level: TraceLevel,
    ) -> Result<TraceCollectorHandle, ProviderError>;
}

/// Concrete collector enum — avoids dyn trait object-safety issues with async.
/// New providers add variants here.
pub enum TraceCollectorHandle {
    Docker(DockerTraceCollector),
    // Future: Firecracker(FirecrackerTraceCollector),
}

impl TraceCollectorHandle {
    pub async fn finish(self) -> Result<ExecutionTrace, ProviderError> {
        match self {
            Self::Docker(c) => c.finish().await,
        }
    }

    /// Cleanup without collecting trace — called when exec fails.
    pub async fn abort(self) {
        match self {
            Self::Docker(c) => c.abort().await,
        }
    }
}
```

### Daemon Orchestration

The daemon (spinal cord) coordinates Muscle and Nerve within a single exec call. Note: `traced_exec` returns `ExecOutput` (with the `trace` field populated), not a separate type.

```rust
async fn traced_exec(&self, id, request, level) -> Result<ExecOutput> {
    let sensor = self.get_sensor(&provider_name);

    if level != TraceLevel::Off {
        if let Some(collector) = sensor.start_trace(id, level).await? {
            match self.provider.exec(id, request).await {
                Ok(mut output) => {
                    let trace = collector.finish().await?;
                    output.trace = Some(trace);
                    return Ok(output);
                }
                Err(e) => {
                    // Clean up trace collector on exec failure
                    collector.abort().await;
                    return Err(e);
                }
            }
        }
    }
    // TraceLevel::Off or provider has no sensor — exec without trace
    self.provider.exec(id, request).await
}
```

### Provider ↔ Sensor Dispatch

Since `SandboxSensor` uses native async fn in trait (not `#[async_trait]`), it is not object-safe and cannot be used as `dyn SandboxSensor`. Instead, sensor dispatch uses a concrete enum — same pattern as `TraceCollectorHandle`:

```rust
/// Concrete sensor enum — each provider variant holds its sensor impl.
pub enum SensorDispatch {
    Docker(DockerSensor),
    // Future: Firecracker(FirecrackerSensor),
    None, // provider does not support tracing
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

// In SandboxServiceImpl:
fn get_sensor(&self, provider: &str) -> &SensorDispatch {
    match provider {
        "docker" => &self.docker_sensor_dispatch, // SensorDispatch::Docker(DockerSensor)
        _ => &SensorDispatch::None,
    }
}
```

## Docker Trace Implementation

### DockerSensor

```rust
pub struct DockerSensor;

impl SandboxSensor for DockerSensor {
    async fn start_trace(&self, id: &SandboxId, level: TraceLevel)
        -> Result<TraceCollectorHandle, ProviderError>
    {
        let collector = DockerTraceCollector::start(id, level).await?;
        Ok(TraceCollectorHandle::Docker(collector))
    }
}
```

### DockerTraceCollector

```rust
struct DockerTraceCollector {
    level: TraceLevel,
    container_id: String,
    start_time: Instant,
    fs_snapshot_before: Option<Vec<String>>,  // docker diff baseline
    seccomp_handle: Option<SeccompLogHandle>,
    stats_handle: Option<StatsStreamHandle>,
}
```

### Collection Mechanisms Per Level

| Level | Mechanism | Overhead |
|---|---|---|
| **Summary** | `docker stats --no-stream` after exec; duration from wall clock | ~0% |
| **Standard** | Summary + `docker diff` before/after (file writes) + seccomp log mode (blocked ops, network attempts, file reads as best-effort) | ~1-3% |
| **Full** | Standard + `docker stats` streaming (per-second resource timeline) + optional strace for syscall detail | ~5-15% |

### Container Creation Prerequisite

Trace-enabled containers need a custom seccomp profile with log mode:

```rust
// In docker.rs build_create_args():
if config.trace_enabled {  // default true when daemon manages the container
    args.extend(["--security-opt", "seccomp=roche-seccomp-trace.json"]);
}
```

The `roche-seccomp-trace.json` profile is identical to Docker's default profile but with `SCMP_ACT_LOG` for monitored syscalls (connect, open, openat, unlink, etc.) instead of `SCMP_ACT_ALLOW`.

**Seccomp profile distribution:** The profile JSON is embedded in the `rochd` binary at compile time (via `include_str!`) and written to a temp file at daemon startup (`~/.roche/seccomp-trace.json`). This avoids distribution complexity — if the daemon is running, the profile is available.

### Known Limitations (MVP)

- File **read** tracking is best-effort via seccomp `open()`/`openat()` log — may miss some reads
- File **write** tracking via `docker diff` only shows final state, not intermediate writes
- Network attempt details (destination IP) require parsing seccomp audit log from host syslog
- Full-level `strace` attach not implemented in MVP — syscalls field will be empty initially

## Daemon Auto-Management

### Problem

Trace collection requires the daemon (seccomp log parsing, stats streaming). Currently users must manually start `rochd`. This friction undermines the "default-on sensation" philosophy.

### Solution: Bundle rochd in Python Wheel

Using maturin (same pattern as ruff):

```
roche-sandbox-0.2.0-cp312-cp312-macosx_arm64.whl
├── roche_sandbox/
│   ├── __init__.py
│   ├── bin/
│   │   └── rochd          # precompiled daemon binary
│   └── ...
```

CI builds platform-specific wheels: `manylinux_x86_64`, `manylinux_aarch64`, `macosx_arm64`, `macosx_x86_64`, `windows_x86_64`.

### Auto-spawn Logic

```python
def _auto_transport(self) -> Transport:
    # 1. Existing daemon running? Connect directly
    daemon = detect_daemon()
    if daemon and is_alive(daemon["pid"]):
        return GrpcTransport(port=daemon["port"])

    # 2. No daemon — auto-spawn from bundled binary
    rochd_path = _find_bundled_binary()
    if rochd_path:
        _spawn_daemon(rochd_path)
        return GrpcTransport(port=50051)

    # 3. Fallback to CLI (trace degrades to summary-only)
    return CliTransport(binary=_find_cli())

def _spawn_daemon(rochd_path: str):
    log_path = Path("~/.roche/rochd.log").expanduser()
    log_file = open(log_path, "a")
    subprocess.Popen(
        [rochd_path, "--idle-timeout", "300"],
        stdout=log_file,
        stderr=log_file,
        start_new_session=True,
    )
    _wait_for_daemon_ready(timeout=3.0)
```

### Lifecycle

| Event | Behavior |
|---|---|
| First `Roche()` init | detect → no daemon → auto-spawn |
| Subsequent `Roche()` | detect → existing daemon → reuse |
| Python process exits | daemon continues (detached) |
| Idle timeout (5 min default) | daemon auto-exits, cleans daemon.json |
| Manual `rochd` start | auto-detect connects, no spawn |

### Trace Degradation

| Mode | Summary | Standard | Full |
|---|---|---|---|
| Daemon (auto/manual) | yes | yes | yes |
| CLI fallback | yes (duration + exit_code only) | no | no |

CLI fallback silently degrades — `trace` fields are empty Vecs, no error raised.

### TypeScript SDK

Same pattern via npm optional platform-specific binary dependencies:

```json
{
  "optionalDependencies": {
    "@anthropic/roche-rochd-darwin-arm64": "0.2.0",
    "@anthropic/roche-rochd-linux-x64": "0.2.0"
  }
}
```

## Proto Changes

```protobuf
enum TraceLevel {
  TRACE_LEVEL_OFF = 0;
  TRACE_LEVEL_SUMMARY = 1;
  TRACE_LEVEL_STANDARD = 2;
  TRACE_LEVEL_FULL = 3;
}

message ExecRequest {
  string sandbox_id = 1;
  repeated string command = 2;
  optional uint64 timeout_secs = 3;
  string provider = 4;
  TraceLevel trace_level = 5;  // NEW — default STANDARD
}

message ExecResponse {
  int32 exit_code = 1;
  string stdout = 2;
  string stderr = 3;
  ExecutionTrace trace = 4;    // NEW
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

enum FileOp {
  FILE_OP_READ = 0;
  FILE_OP_WRITE = 1;
  FILE_OP_CREATE = 2;
  FILE_OP_DELETE = 3;
}

message FileAccess {
  string path = 1;
  FileOp op = 2;
  optional uint64 size_bytes = 3;  // optional: 0 vs unknown distinction
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

## Python SDK Interface

```python
@dataclass
class ExecutionTrace:
    duration_secs: float
    resource_usage: ResourceUsage
    file_accesses: list[FileAccess]
    network_attempts: list[NetworkAttempt]
    blocked_ops: list[BlockedOperation]
    syscalls: list[SyscallEvent]
    resource_timeline: list[ResourceSnapshot]

    def summary(self) -> str:
        """LLM-friendly one-line summary."""
        parts = [f"{self.duration_secs:.1f}s"]
        parts.append(f"mem {self.resource_usage.peak_memory_bytes // 1_000_000}MB")
        if self.file_accesses:
            reads = sum(1 for f in self.file_accesses if f.op == "read")
            writes = sum(1 for f in self.file_accesses if f.op in ("write", "create"))
            if reads: parts.append(f"read {reads} files")
            if writes: parts.append(f"wrote {writes} files")
        blocked = len(self.blocked_ops)
        if blocked:
            parts.append(f"blocked {blocked} ops")
        return " | ".join(parts)

@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str
    trace: ExecutionTrace | None = None

# Usage — trace is automatic
result = await sandbox.exec(["python3", "train.py"])
result.trace.summary()  # "2.3s | mem 340MB | read 2 files | wrote 1 file | blocked 1 ops"

# Opt-out
result = await sandbox.exec(["echo", "hi"], trace_level="off")

# Full detail
result = await sandbox.exec(["python3", "suspicious.py"], trace_level="full")
```

## Backward Compatibility

| Scenario | Behavior |
|---|---|
| Old SDK + new daemon | trace field ignored by old SDK (unknown proto field) |
| New SDK + old daemon | trace field empty, `result.trace` is None |
| New SDK + CLI fallback | trace has duration only, other fields empty |

## Future Evolution (Out of Scope)

- **v0.3:** Streaming exec — `SandboxSensor` extends to emit `ExecEvent` stream during execution
- **v0.3:** Auto-retry policies (spinal cord reflex) based on trace signals
- **v0.4:** Bidirectional control — `TraceCollector` evolves into `ExecSession` with interrupt/adjust
- **v0.4:** Adaptive sandboxing — trace history informs dynamic permission changes

## Testing Strategy

- Unit tests for `DockerTraceCollector` with mocked Docker CLI output
- Integration tests: exec a known script, assert trace contains expected file accesses and resource usage
- Degradation tests: verify CLI fallback returns valid (empty) trace without errors
- Compatibility tests: old proto clients can still call new daemon without errors
