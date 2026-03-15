# Go SDK Design

## Overview

Add a Go SDK for Roche that mirrors the Python and TypeScript SDK API surface. The SDK provides a `Client` + `Sandbox` abstraction over the same dual-transport layer (gRPC daemon auto-detect → CLI fallback), using Go-idiomatic patterns: `context.Context` for timeouts/cancellation, sentinel errors with `errors.Is()`, functional options, and `defer` for cleanup.

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| API shape | Client + Sandbox structs | Mirrors Python/TS SDKs for cross-language consistency |
| Async model | Synchronous with `context.Context` | Go's goroutine model makes sync-first natural; context handles timeouts/cancellation |
| Error handling | Sentinel errors + `errors.Is()` | Idiomatic Go; no exception hierarchy needed |
| Configuration | Functional options pattern | `New(WithProvider("k8s"), WithBinary("/usr/local/bin/roche"))` |
| Defaults | Zero-value aware with explicit defaults | `SandboxConfig{}` gets safe defaults applied at transport layer |
| Module path | `github.com/roche-dev/roche-go` | Standard Go module naming |
| Package name | `roche` | `import roche "github.com/roche-dev/roche-go"` |
| Proto codegen | `protoc-gen-go` + `protoc-gen-go-grpc` | Standard Go gRPC toolchain |
| Cleanup pattern | `sandbox.Close(ctx)` + `defer` | Idiomatic Go resource management |

## Architecture

```
sdk/go/
├── roche.go            # Client struct, New() constructor, top-level methods
├── sandbox.go          # Sandbox struct with instance methods
├── types.go            # SandboxConfig, ExecOutput, SandboxInfo, Mount, SandboxStatus
├── errors.go           # Sentinel errors
├── daemon.go           # Daemon detection (~/.roche/daemon.json)
├── transport.go        # Transport interface definition
├── transport_grpc.go   # gRPC transport implementation
├── transport_cli.go    # CLI subprocess transport implementation
├── gen/                # Generated protobuf code
│   └── roche/v1/
│       ├── sandbox.pb.go
│       └── sandbox_grpc.pb.go
├── scripts/
│   └── proto-gen.sh    # Proto generation script
├── go.mod
├── go.sum
├── roche_test.go       # Client tests
├── sandbox_test.go     # Sandbox tests
├── transport_grpc_test.go
├── transport_cli_test.go
├── daemon_test.go
└── README.md
```

## Public API

### Client

```go
package roche

// Client is the main entry point for interacting with Roche sandboxes.
type Client struct {
    transport Transport
    provider  string
}

// New creates a new Roche client. By default, it auto-detects the daemon
// (via ~/.roche/daemon.json) and falls back to CLI transport.
func New(opts ...Option) (*Client, error)

// Create creates a new sandbox and returns a Sandbox handle.
func (c *Client) Create(ctx context.Context, cfg SandboxConfig) (*Sandbox, error)

// CreateID creates a new sandbox and returns only the sandbox ID.
func (c *Client) CreateID(ctx context.Context, cfg SandboxConfig) (string, error)

// Exec executes a command in the specified sandbox.
func (c *Client) Exec(ctx context.Context, sandboxID string, command []string) (*ExecOutput, error)

// Destroy destroys the specified sandbox.
func (c *Client) Destroy(ctx context.Context, sandboxID string) error

// DestroyMany destroys multiple sandboxes at once, returning the IDs that were destroyed.
func (c *Client) DestroyMany(ctx context.Context, sandboxIDs []string) ([]string, error)

// List returns all active sandboxes.
func (c *Client) List(ctx context.Context) ([]SandboxInfo, error)

// GC garbage-collects expired sandboxes.
func (c *Client) GC(ctx context.Context, opts GCOptions) ([]string, error)

// PoolStatus returns the current status of sandbox pools.
func (c *Client) PoolStatus(ctx context.Context) ([]PoolInfo, error)

// PoolWarmup warms up sandbox pools with the given configurations.
func (c *Client) PoolWarmup(ctx context.Context, pools []PoolConfig) error

// PoolDrain drains sandbox pools, destroying idle sandboxes.
func (c *Client) PoolDrain(ctx context.Context, provider, image string) error

// Close shuts down the client and releases resources (e.g. gRPC connection).
func (c *Client) Close() error
```

### Options

```go
// Option configures the Client.
type Option func(*clientConfig)

// WithTransport injects a custom transport (for testing or custom implementations).
func WithTransport(t Transport) Option

// WithBinary sets the CLI binary path (default: "roche").
func WithBinary(path string) Option

// WithDaemonPort overrides the daemon port for gRPC transport.
func WithDaemonPort(port int) Option

// WithProvider sets the default provider (default: "docker").
func WithProvider(provider string) Option

// WithDirectMode forces CLI transport, bypassing daemon auto-detection.
func WithDirectMode() Option
```

Internal config struct:

```go
type clientConfig struct {
    transport  Transport
    binary     string  // default "roche"
    daemonPort int     // 0 = auto-detect
    provider   string  // default "docker"
    directMode bool    // force CLI transport
}
```

### Constructor Logic

```go
func New(opts ...Option) (*Client, error) {
    cfg := &clientConfig{
        binary:   "roche",
        provider: "docker",
    }
    for _, opt := range opts {
        opt(cfg)
    }

    if cfg.transport != nil {
        return &Client{transport: cfg.transport, provider: cfg.provider}, nil
    }

    if cfg.directMode {
        return &Client{
            transport: newCLITransport(cfg.binary),
            provider:  cfg.provider,
        }, nil
    }

    // Auto-detect daemon
    if info, err := detectDaemon(); err == nil {
        port := info.Port
        if cfg.daemonPort > 0 {
            port = cfg.daemonPort
        }
        return &Client{
            transport: newGRPCTransport(port),
            provider:  cfg.provider,
        }, nil
    }

    // Fallback to CLI
    return &Client{
        transport: newCLITransport(cfg.binary),
        provider:  cfg.provider,
    }, nil
}
```

### Sandbox

```go
// Sandbox represents a running sandbox instance.
type Sandbox struct {
    id        string
    provider  string
    transport Transport
}

// ID returns the sandbox identifier.
func (s *Sandbox) ID() string

// Provider returns the provider name.
func (s *Sandbox) Provider() string

// Exec executes a command in this sandbox.
func (s *Sandbox) Exec(ctx context.Context, command []string) (*ExecOutput, error)

// Pause pauses this sandbox.
func (s *Sandbox) Pause(ctx context.Context) error

// Unpause resumes this sandbox.
func (s *Sandbox) Unpause(ctx context.Context) error

// Destroy destroys this sandbox.
func (s *Sandbox) Destroy(ctx context.Context) error

// Close is an alias for Destroy, enabling defer-based cleanup.
func (s *Sandbox) Close(ctx context.Context) error

// CopyTo copies a file from the host to this sandbox.
func (s *Sandbox) CopyTo(ctx context.Context, hostPath, sandboxPath string) error

// CopyFrom copies a file from this sandbox to the host.
func (s *Sandbox) CopyFrom(ctx context.Context, sandboxPath, hostPath string) error
```

Usage pattern:

```go
client, err := roche.New()
if err != nil {
    log.Fatal(err)
}

sandbox, err := client.Create(ctx, roche.SandboxConfig{
    Image: "python:3.12-slim",
})
if err != nil {
    log.Fatal(err)
}
defer sandbox.Close(ctx)

out, err := sandbox.Exec(ctx, []string{"echo", "hello"})
if err != nil {
    log.Fatal(err)
}
fmt.Println(out.Stdout) // "hello\n"
```

## Types

```go
// SandboxConfig configures a new sandbox.
// Zero values get safe defaults: Provider="docker", Image="python:3.12-slim",
// TimeoutSecs=300, Network=false, Writable=false.
type SandboxConfig struct {
    Provider    string
    Image       string
    Memory      string              // e.g. "512m", "1g"
    CPUs        float64             // e.g. 1.5
    TimeoutSecs uint64              // 0 = use server default (300); cannot express "no timeout"
    Network     bool                // default false (AI-safe)
    Writable    bool                // default false (AI-safe)
    Env         map[string]string
    Mounts      []Mount
    Kernel      string              // Firecracker only
    Rootfs      string              // Firecracker only
}

// Mount configures a host directory mount.
// Use NewMount() to create mounts with AI-safe defaults (Readonly=true).
type Mount struct {
    HostPath      string
    ContainerPath string
    Readonly      bool
}

// NewMount creates a Mount with AI-safe defaults (Readonly=true).
func NewMount(hostPath, containerPath string) Mount {
    return Mount{HostPath: hostPath, ContainerPath: containerPath, Readonly: true}
}

// ExecOutput contains the result of executing a command.
type ExecOutput struct {
    ExitCode int32
    Stdout   string
    Stderr   string
}

// SandboxStatus represents the state of a sandbox.
type SandboxStatus string

const (
    StatusRunning SandboxStatus = "running"
    StatusPaused  SandboxStatus = "paused"
    StatusStopped SandboxStatus = "stopped"
    StatusFailed  SandboxStatus = "failed"
)

// SandboxInfo contains metadata about a sandbox.
type SandboxInfo struct {
    ID        string
    Status    SandboxStatus
    Provider  string
    Image     string
    ExpiresAt *uint64  // nil if no expiry
}

// GCOptions configures garbage collection behavior.
type GCOptions struct {
    DryRun bool
    All    bool
}

// PoolInfo contains metadata about a sandbox pool.
type PoolInfo struct {
    Provider    string
    Image       string
    IdleCount   uint32
    ActiveCount uint32
    MaxIdle     uint32
    MaxTotal    uint32
}

// PoolConfig configures a sandbox pool for warmup.
type PoolConfig struct {
    Provider string
    Image    string
    Count    uint32
}
```

### Default Application

Defaults are applied at the transport layer before sending requests. The `SandboxConfig` zero values (empty string, 0, false) are distinguishable from explicit values:

| Field | Zero Value | Default Applied |
|---|---|---|
| Provider | `""` | Use client's default provider |
| Image | `""` | `"python:3.12-slim"` |
| TimeoutSecs | `0` | `300` |
| Network | `false` | `false` (already safe) |
| Writable | `false` | `false` (already safe) |

## Errors

```go
package roche

import "errors"

// Sentinel errors for matching with errors.Is().
var (
    ErrNotFound    = errors.New("roche: sandbox not found")
    ErrPaused      = errors.New("roche: sandbox is paused")
    ErrUnavailable = errors.New("roche: provider unavailable")
    ErrTimeout     = errors.New("roche: operation timed out")
    ErrUnsupported = errors.New("roche: operation not supported")
)
```

Usage:

```go
_, err := sandbox.Exec(ctx, []string{"ls"})
if errors.Is(err, roche.ErrNotFound) {
    // sandbox was destroyed
}
```

## Transport Interface

```go
// Transport defines the low-level communication protocol with the Roche backend.
type Transport interface {
    Create(ctx context.Context, cfg SandboxConfig, provider string) (string, error)
    Exec(ctx context.Context, sandboxID string, command []string, provider string, timeoutSecs *uint64) (*ExecOutput, error)
    Destroy(ctx context.Context, sandboxIDs []string, provider string, all bool) ([]string, error)
    List(ctx context.Context, provider string) ([]SandboxInfo, error)
    Pause(ctx context.Context, sandboxID, provider string) error
    Unpause(ctx context.Context, sandboxID, provider string) error
    GC(ctx context.Context, provider string, dryRun, all bool) ([]string, error)
    CopyTo(ctx context.Context, sandboxID, hostPath, sandboxPath, provider string) error
    CopyFrom(ctx context.Context, sandboxID, sandboxPath, hostPath, provider string) error
    PoolStatus(ctx context.Context) ([]PoolInfo, error)
    PoolWarmup(ctx context.Context, pools []PoolConfig) error
    PoolDrain(ctx context.Context, provider, image string) error
    Close() error
}
```

### gRPC Transport

```go
type grpcTransport struct {
    conn   *grpc.ClientConn
    client pb.SandboxServiceClient  // lazy initialized
    port   int
}

func newGRPCTransport(port int) *grpcTransport
```

Key behaviors:
- Lazy connection: dials on first method call, not at construction
- Maps gRPC status codes to sentinel errors:

| gRPC Code | Roche Error |
|---|---|
| `codes.NotFound` | `ErrNotFound` |
| `codes.FailedPrecondition` | `ErrPaused` |
| `codes.Unavailable` | `ErrUnavailable` |
| `codes.DeadlineExceeded` | `ErrTimeout` |
| `codes.Unimplemented` | `ErrUnsupported` |

- Uses `fmt.Errorf("roche: ...: %w", sentinelErr)` for wrapping so `errors.Is()` works
- Converts proto `SandboxStatus` enum to `SandboxStatus` string constants
- Proto imports from `gen/roche/v1` package

### CLI Transport

```go
type cliTransport struct {
    binary string
}

func newCLITransport(binary string) *cliTransport
```

Key behaviors:
- Executes `roche` binary via `exec.CommandContext(ctx, ...)` (respects context cancellation)
- Constructs CLI args matching the Rust CLI interface:
  - `create --provider docker --image python:3.12-slim --timeout 300`
  - `exec --sandbox {id} --provider docker -- echo hello`
  - `destroy --sandbox {id} --provider docker`
  - `list --provider docker --json`
  - `gc --provider docker --dry-run`
  - `cp --sandbox {id} --provider docker host:src sandbox:dest` (copy_to)
  - `cp --sandbox {id} --provider docker sandbox:src host:dest` (copy_from)
- Parses JSON output for `list` (expects `--json` flag)
- Error detection: stderr starting with "Error: " → map keywords to sentinel errors
  - "not found" → `ErrNotFound`
  - "paused" → `ErrPaused`
  - "unavailable" → `ErrUnavailable`
  - "timeout" / "timed out" → `ErrTimeout`
  - "unsupported" → `ErrUnsupported`
- If binary not found (`exec.ErrNotFound`): return `ErrUnavailable`

## Daemon Detection

```go
// daemon.go

type daemonInfo struct {
    PID  int `json:"pid"`
    Port int `json:"port"`
}

// detectDaemon reads ~/.roche/daemon.json and verifies the process is alive.
// Returns daemonInfo if daemon is running, error otherwise.
func detectDaemon() (*daemonInfo, error)
```

Logic:
1. Read `~/.roche/daemon.json`
2. Parse JSON into `daemonInfo`
3. Verify process alive via `os.FindProcess(pid)` + signal 0 check
4. Return info or error

## Dependencies

```
module github.com/roche-dev/roche-go

go 1.21

require (
    google.golang.org/grpc v1.62.0
    google.golang.org/protobuf v1.33.0
)
```

Minimum Go 1.21 for `slog` structured logging and modern stdlib features.

## Proto Generation

A `//go:generate` directive in `gen.go` triggers proto generation:

```go
//go:generate bash scripts/proto-gen.sh
package roche
```

Script at `sdk/go/scripts/proto-gen.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

PROTO_DIR="$(cd "$(dirname "$0")/../../.." && pwd)/crates/roche-daemon/proto"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/gen"

mkdir -p "$OUT_DIR"

protoc \
  -I "$PROTO_DIR" \
  --go_out="$OUT_DIR" \
  --go_opt=paths=source_relative \
  --go-grpc_out="$OUT_DIR" \
  --go-grpc_opt=paths=source_relative \
  "$PROTO_DIR/roche/v1/sandbox.proto"
```

## Cross-Language Comparison

| Concept | Python | TypeScript | Go |
|---|---|---|---|
| Async | `async/await` + sync wrapper | `async/await` (Promise) | `context.Context` (sync) |
| Client | `AsyncRoche` / `Roche` | `Roche` | `Client` |
| Sandbox | `AsyncSandbox` / `Sandbox` | `Sandbox` | `Sandbox` |
| Config | dataclass with defaults | interface + DEFAULTS obj | struct with zero-value defaults |
| Errors | Exception subclasses | Error subclasses | Sentinel errors + `errors.Is()` |
| Cleanup | `async with` / `with` | `await using` (Symbol.asyncDispose) | `defer sandbox.Close(ctx)` |
| Transport | Protocol (duck typing) | interface | interface |
| Options | `__init__(**kwargs)` | constructor options object | Functional options pattern |
| Package | `roche-sandbox` (PyPI) | `roche-sandbox` (npm) | `github.com/roche-dev/roche-go` |

## Testing

### Unit Tests

| File | Coverage |
|---|---|
| `roche_test.go` | Client construction, option application, auto-detect logic, Create/Exec/Destroy/List/GC with mock transport |
| `sandbox_test.go` | Sandbox Exec/Pause/Unpause/Destroy/CopyTo/CopyFrom/Close with mock transport |
| `transport_grpc_test.go` | gRPC status code → sentinel error mapping, proto ↔ Go type conversion |
| `transport_cli_test.go` | CLI arg construction, stdout/stderr parsing, error keyword detection, binary-not-found handling |
| `daemon_test.go` | JSON parsing, process-alive check, missing file, malformed JSON |

### Integration Tests

| Test | Coverage |
|---|---|
| `TestE2E_Lifecycle` | Create → Exec → Destroy full cycle |
| `TestE2E_ContextManager` | defer-based cleanup |
| `TestE2E_List` | List after create |
| `TestE2E_CopyToFrom` | File round-trip |

Integration tests gated by build tag: `//go:build integration`

Run: `go test -tags integration ./...`

### Mock Transport

```go
type mockTransport struct {
    createFn  func(ctx context.Context, cfg SandboxConfig, provider string) (string, error)
    execFn    func(ctx context.Context, id string, cmd []string, provider string, timeout *uint64) (*ExecOutput, error)
    // ... other fields for each method
}
```

Each method delegates to the corresponding function field, allowing per-test behavior injection.

## Error Mapping Summary

| Source | Error Condition | Roche Error |
|---|---|---|
| gRPC | `codes.NotFound` | `ErrNotFound` |
| gRPC | `codes.FailedPrecondition` | `ErrPaused` |
| gRPC | `codes.Unavailable` | `ErrUnavailable` |
| gRPC | `codes.DeadlineExceeded` | `ErrTimeout` |
| gRPC | `codes.Unimplemented` | `ErrUnsupported` |
| CLI | stderr contains "not found" | `ErrNotFound` |
| CLI | stderr contains "paused" | `ErrPaused` |
| CLI | stderr contains "unavailable" | `ErrUnavailable` |
| CLI | stderr contains "timeout" | `ErrTimeout` |
| CLI | stderr contains "unsupported" | `ErrUnsupported` |
| CLI | binary not found | `ErrUnavailable` |
