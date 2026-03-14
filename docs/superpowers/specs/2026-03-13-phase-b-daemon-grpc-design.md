# Phase B-C: Daemon Mode + gRPC — Design Spec

## Goal

Add a long-running daemon process (`roche-daemon`) that exposes a gRPC API over TCP, enabling SDKs to manage sandboxes remotely while handling background lifecycle tasks (garbage collection). The CLI gains dual-mode operation: direct provider access (current behavior, no daemon required) or gRPC client mode (when daemon is running).

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌──────────────┐
│  roche CLI  │     │  TS SDK     │     │  Python SDK  │
│ (direct or  │     │ (gRPC)      │     │ (gRPC)       │
│  gRPC mode) │     └──────┬──────┘     └──────┬───────┘
└──────┬──────┘            │                   │
       │              gRPC (TCP 127.0.0.1:50051)
       │                   │                   │
       ▼                   ▼                   ▼
┌─────────────────────────────────────────────────────┐
│                   roche-daemon                       │
│  ┌─────────────┐  ┌──────────┐  ┌────────────────┐ │
│  │ gRPC Server │  │ Provider │  │ Background GC  │ │
│  │ (tonic)     │──│ Dispatch │  │ (tokio timer)  │ │
│  └─────────────┘  └──────────┘  └────────────────┘ │
│                        │                             │
│              ┌─────────┼─────────┐                   │
│              ▼         ▼         ▼                   │
│         Docker    Firecracker  (future)              │
└─────────────────────────────────────────────────────┘
```

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Purpose | gRPC endpoint + lifecycle mgmt | Single process owns sandbox state, multiple clients connect |
| CLI mode | Dual (direct default, gRPC when daemon running) | No daemon required for basic use; daemon enables SDK access |
| gRPC framework | `tonic` + `prost` | Industry standard, proto-first, TS SDK reuses `.proto` files |
| Daemon management | Explicit `roche daemon start/stop/status` | Predictable, easy to debug, foreground by default |
| Listening | TCP only, `127.0.0.1:50051` | Works everywhere, every SDK language connects trivially |

## Protobuf Service Definition

File: `proto/roche/v1/sandbox.proto`

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

// --- Create ---
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

// --- Exec ---
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

// --- Destroy ---
message DestroyRequest {
  repeated string sandbox_ids = 1;
  bool all = 2;
  string provider = 3;
}

message DestroyResponse {
  repeated string destroyed_ids = 1;
}

// --- List ---
message ListRequest {
  string provider = 1;
}

message ListResponse {
  repeated SandboxInfo sandboxes = 1;
}

// --- Pause / Unpause ---
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

// --- GC ---
message GcRequest {
  bool dry_run = 1;
  bool all = 2;
  string provider = 3;
}

message GcResponse {
  repeated string destroyed_ids = 1;
}

// --- File Copy (Docker-only) ---
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

// --- Shared types ---
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

The service definition maps 1:1 with existing `SandboxProvider`, `SandboxLifecycle`, and `SandboxFileOps` traits. No new semantics are introduced — this is purely a wire format for the existing functionality.

## Daemon (`roche-daemon` crate)

### Startup sequence

1. Parse CLI args (`--port`, `--foreground` are the only options)
2. Instantiate providers: `DockerProvider` always, `FirecrackerProvider` on Linux
3. Spawn background GC task (tokio interval, 60s, calls `gc()` on each provider)
4. Bind tonic gRPC server to `127.0.0.1:{port}` (default 50051)
5. Write `~/.roche/daemon.json` with `{ "pid": <pid>, "port": <port> }`
6. Log startup message to stderr
7. On SIGTERM/SIGINT: graceful shutdown, remove `daemon.json`

### gRPC service implementation

A `SandboxServiceImpl` struct holds references to available providers. Each RPC method:
1. Converts protobuf request → `roche_core` types
2. Dispatches to the appropriate provider based on the `provider` field (every request carries a `provider` field — the daemon is stateless and does not maintain a sandbox-to-provider mapping)
3. Converts `Result<_, ProviderError>` → gRPC response or status error

The CLI defaults the `provider` field to `"docker"` when not explicitly specified by the user (matching current CLI behavior).

### Error mapping

| `ProviderError` | gRPC Status Code |
|-----------------|------------------|
| `NotFound` | `NOT_FOUND` |
| `CreateFailed` | `INTERNAL` |
| `ExecFailed` | `INTERNAL` |
| `Unavailable` | `UNAVAILABLE` |
| `Timeout` | `DEADLINE_EXCEEDED` |
| `Unsupported` | `UNIMPLEMENTED` |
| `FileFailed` | `INTERNAL` |
| `Paused` | `FAILED_PRECONDITION` |

### Background GC

A simple tokio task:
```
loop {
    sleep(60s)
    for provider in providers:
        provider.gc()
}
```

Runs silently. Failed GC calls are logged but do not crash the daemon.

### File layout

```
crates/roche-daemon/
├── Cargo.toml
├── build.rs          # tonic-build compiles .proto
└── src/
    ├── main.rs       # CLI args (clap), startup, signal handling
    ├── server.rs     # SandboxServiceImpl (gRPC handler implementations)
    └── gc.rs         # Background GC task
proto/
└── roche/v1/
    └── sandbox.proto
```

## CLI Changes

### New subcommand: `roche daemon`

```
roche daemon start [--port PORT] [--foreground]
roche daemon stop
roche daemon status
```

- `start`: Uses `std::process::Command` to spawn `roche-daemon` with stdout/stderr redirected to `~/.roche/daemon.log`. The process is detached. With `--foreground`, runs in the current terminal instead. Fails if daemon is already running.
- `stop`: Reads `~/.roche/daemon.json`, sends SIGTERM to the PID. Fails if no daemon running.
- `status`: Checks if daemon is alive, prints port and PID.

### Dual-mode dispatch for sandbox commands

For `create`, `exec`, `destroy`, `list`, `pause`, `unpause`, `gc`, `cp`:

1. Check if `~/.roche/daemon.json` exists and daemon is alive (quick TCP connect probe)
2. If daemon is running → forward request via gRPC client
3. If daemon is not running → use direct provider access (current behavior)
4. `--direct` flag forces direct mode regardless of daemon status

The CLI needs `tonic` (client) and the generated protobuf types to act as a gRPC client.

## Dependencies

### `roche-daemon` (new crate)

```toml
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
prost-build = "0.13"
```

### `roche-cli` (additions)

```toml
tonic = "0.12"
prost = "0.13"
```

Plus a `build.rs` to compile the same `.proto` file for client stubs.

### System requirement

`protoc` (protobuf compiler) must be installed for `tonic-build` to work at compile time.

## Testing Strategy

- **Proto compilation**: Build test ensures `tonic-build` compiles the `.proto` successfully
- **gRPC server unit tests**: Start in-process tonic server with DockerProvider, make gRPC calls, verify responses. Bind to `127.0.0.1:0` for random port allocation.
- **CLI daemon detection**: Unit test the `~/.roche/daemon.json` reading and gRPC fallback logic
- **Daemon lifecycle**: Integration test that starts daemon in foreground, verifies gRPC connectivity, sends stop, verifies clean shutdown
- **Background GC**: Unit test that the GC task timer fires and calls provider `gc()`
- **Error mapping**: Test that each `ProviderError` variant maps to the correct gRPC status code

## Non-Goals

- Authentication/authorization (localhost-only, single-user)
- TLS encryption (localhost TCP, not needed)
- Exec output streaming (unary RPC for MVP, streaming later)
- Hot-reload of providers
- Metrics/observability endpoints
- Systemd/launchd service files (can be added later as thin wrappers)
