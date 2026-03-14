# Phase D: TypeScript & Python SDKs — Design Spec

## Goal

Provide ergonomic, type-safe SDKs for TypeScript (Node.js) and Python that let AI agent frameworks manage Roche sandboxes programmatically. Both SDKs use dual-mode transport: gRPC to the daemon when available, CLI subprocess fallback otherwise.

## Architecture

```
┌──────────────────────┐     ┌──────────────────────┐
│   TypeScript SDK     │     │     Python SDK        │
│   (roche-sandbox)    │     │   (roche-sandbox)     │
│                      │     │                       │
│  Roche ──► Sandbox   │     │  AsyncRoche / Roche   │
│       ▲              │     │  ──► Sandbox          │
│       │              │     │       ▲               │
│  TransportLayer      │     │  TransportLayer       │
│  ┌────┴─────┐        │     │  ┌────┴─────┐         │
│  │gRPC│ CLI │        │     │  │gRPC│ CLI │         │
│  └────┴─────┘        │     │  └────┴─────┘         │
└──────────┬───────────┘     └──────────┬────────────┘
           │ gRPC or subprocess        │
           ▼                           ▼
    ┌─────────────┐            ┌──────────────┐
    │roche-daemon │            │  roche CLI   │
    │  (gRPC)     │            │ (subprocess) │
    └─────────────┘            └──────────────┘
```

Both SDKs share the same dual-mode transport pattern:
1. Check if daemon is running (`~/.roche/daemon.json` + process alive check)
2. If running → use gRPC client (generated from `sandbox.proto`)
3. If not → shell out to `roche` CLI binary and parse output

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Transport | Dual-mode (gRPC + CLI subprocess fallback) | Works with or without daemon; mirrors CLI behavior |
| Proto codegen | `ts-proto` (TS), `grpcio-tools` (Python) | Proto is single source of truth; compile-time type safety |
| TS runtime | Node.js >= 18, TypeScript >= 5.2 | AI agent frameworks overwhelmingly target Node.js; TS 5.2 enables `using` (explicit resource management) with polyfill |
| Python version | Python >= 3.10 | Required for `match` statements, modern type unions |
| Python API style | Async-first (`AsyncRoche`) + sync wrapper (`Roche`) | gRPC is inherently async; AI frameworks moving toward async |
| TS API style | Sandbox-centric (`Sandbox` objects) | Ergonomic; encapsulates sandbox ID; aligns with Python SDK |
| Package names | `roche-sandbox` (both npm and PyPI) | Consistent across ecosystems; no npm org required |
| Default provider | `"docker"` | Matches CLI default; stored on `Roche` client, captured per-`Sandbox` |

## Provider Propagation

The proto requires a `provider` field on every RPC. The SDKs handle this as follows:

1. `Roche` / `AsyncRoche` client stores a default provider (`"docker"` unless overridden at construction)
2. On `create()` / `createSandbox()`, the provider from `SandboxConfig` (or the client default) is used
3. The returned `Sandbox` object captures and stores the provider from the create call
4. All subsequent `Sandbox` methods (`exec`, `pause`, `destroy`, etc.) use the stored provider automatically
5. Flat methods on `Roche` (e.g., `roche.exec(id, cmd)`) use the client's default provider

## TypeScript SDK

### File layout

```
sdk/typescript/
├── package.json
├── tsconfig.json
├── scripts/
│   └── proto-gen.sh          # runs protoc + ts-proto
├── src/
│   ├── index.ts              # public exports
│   ├── roche.ts              # Roche client class
│   ├── sandbox.ts            # Sandbox class (wraps sandbox_id + provider)
│   ├── types.ts              # SandboxConfig, ExecOutput, SandboxInfo, etc.
│   ├── transport/
│   │   ├── index.ts          # Transport interface
│   │   ├── grpc.ts           # gRPC transport (uses generated stubs)
│   │   └── cli.ts            # CLI subprocess transport
│   ├── daemon.ts             # daemon detection (read daemon.json, pid check)
│   └── generated/            # ts-proto generated code (gitignored, built)
│       └── roche/v1/
│           └── sandbox.ts
└── test/
    ├── roche.test.ts
    ├── sandbox.test.ts
    └── transport/
        ├── grpc.test.ts
        └── cli.test.ts
```

### Public API

```typescript
// Construction
const roche = new Roche();                          // auto-detect mode
const roche = new Roche({ mode: "direct" });        // force CLI subprocess
const roche = new Roche({ daemonPort: 50051 });     // explicit port
const roche = new Roche({ provider: "docker" });    // explicit provider

// Sandbox-centric (recommended)
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
const output = await sandbox.exec(["python", "-c", "print('hi')"]);
await sandbox.copyTo("/local/file.py", "/sandbox/file.py");
await sandbox.copyFrom("/sandbox/output.txt", "/local/output.txt");
await sandbox.pause();
await sandbox.unpause();
await sandbox.destroy();

// Auto-cleanup with using (requires TypeScript >= 5.2, Symbol.asyncDispose polyfill)
await using sandbox = await roche.createSandbox();
await sandbox.exec(["echo", "hello"]);
// sandbox.destroy() called automatically

// Flat methods (lower-level)
const id = await roche.create({ image: "python:3.12-slim" });
const output = await roche.exec(id, ["python", "-c", "print('hi')"]);
await roche.destroy(id);
const sandboxes = await roche.list();
await roche.gc();
```

### Dependencies

- `@grpc/grpc-js` — gRPC client (runtime)
- `ts-proto` — proto codegen (dev)
- No other runtime dependencies

## Python SDK

### File layout

```
sdk/python/
├── pyproject.toml
├── scripts/
│   └── proto-gen.sh            # runs grpcio-tools codegen
├── src/
│   └── roche_sandbox/
│       ├── __init__.py         # public exports
│       ├── client.py           # AsyncRoche + Roche (sync wrapper)
│       ├── sandbox.py          # AsyncSandbox + Sandbox (context managers)
│       ├── types.py            # SandboxConfig, ExecOutput, SandboxInfo, Mount, etc.
│       ├── transport/
│       │   ├── __init__.py     # Transport protocol (abstract)
│       │   ├── grpc.py         # gRPC transport (uses generated stubs)
│       │   └── cli.py          # CLI subprocess transport
│       ├── daemon.py           # daemon detection
│       └── generated/          # grpcio-tools output (gitignored, built)
│           └── roche/v1/
│               ├── sandbox_pb2.py
│               └── sandbox_pb2_grpc.py
└── tests/
    ├── test_client.py
    ├── test_sandbox.py
    └── transport/
        ├── test_grpc.py
        └── test_cli.py
```

### Public API

```python
# Async client (primary)
roche = AsyncRoche()                                    # auto-detect mode
roche = AsyncRoche(mode="direct")                       # force CLI
roche = AsyncRoche(daemon_port=50051)                   # explicit port

# create() returns AsyncSandbox
sandbox = await roche.create(image="python:3.12-slim")
output = await sandbox.exec(["python", "-c", "print('hi')"])
print(output.stdout)
await sandbox.pause()
await sandbox.unpause()
await sandbox.copy_to("/local/file.py", "/sandbox/file.py")
await sandbox.copy_from("/sandbox/output.txt", "/local/output.txt")
await sandbox.destroy()

# Async context manager
async with await roche.create(image="python:3.12-slim") as sandbox:
    output = await sandbox.exec(["echo", "hello"])
# auto-destroyed on exit

# Flat async methods (for operating by ID without a Sandbox object)
sandbox_id = await roche.create_id(image="python:3.12-slim")  # returns str, not Sandbox
output = await roche.exec(sandbox_id, ["echo", "hello"])
await roche.destroy(sandbox_id)
sandboxes = await roche.list()
await roche.gc()

# Sync client (wraps async)
roche = Roche()
sandbox = roche.create(image="python:3.12-slim")
output = sandbox.exec(["echo", "hello"])
sandbox.pause()
sandbox.unpause()
sandbox.destroy()

# Sync context manager
with roche.create(image="python:3.12-slim") as sandbox:
    output = sandbox.exec(["echo", "hello"])
```

### Migration from existing SDK

The existing `sdk/python/roche/` directory is deleted and replaced by `sdk/python/src/roche_sandbox/`. The `pyproject.toml` is rewritten for the new `src/` layout with package name `roche-sandbox`. The sync API shape stays the same (`Roche` class, `Sandbox` context manager), so existing users change their import from `from roche import Roche` to `from roche_sandbox import Roche`.

Batch methods (`create_many`, `destroy_many`, `destroy_all`) from the existing SDK are not carried forward. Users can loop over `create()` / `destroy()` calls. Batch convenience methods can be added later if needed.

### Dependencies

- Runtime: `grpcio`, `protobuf`
- Dev: `grpcio-tools` (codegen), `pytest`, `pytest-asyncio`

## Shared Types

Both SDKs expose these types (named idiomatically per language):

### SandboxConfig

```
SandboxConfig {
  provider?: string          // default: client's default ("docker")
  image?: string             // default: "python:3.12-slim"
  memory?: string            // e.g., "512m"
  cpus?: number              // e.g., 1.0
  timeout_secs?: number      // default: 300
  network?: boolean          // default: false (AI-safe)
  writable?: boolean         // default: false (AI-safe)
  env?: Record<string, string>
  mounts?: Mount[]
  kernel?: string            // Firecracker only: path to kernel image
  rootfs?: string            // Firecracker only: path to rootfs image
}
```

### Mount

```
Mount {
  host_path: string
  container_path: string
  readonly?: boolean         // default: true (AI-safe)
}
```

### ExecOutput

```
ExecOutput {
  exit_code: number
  stdout: string
  stderr: string
}
```

### SandboxInfo

```
SandboxInfo {
  id: string
  status: SandboxStatus      // "running" | "paused" | "stopped" | "failed"
  provider: string
  image: string
  expires_at?: number         // Unix timestamp
}
```

### SandboxStatus

```
SandboxStatus = "running" | "paused" | "stopped" | "failed"
```

## Transport Layer

The transport layer is the internal abstraction both SDKs use to dispatch commands.

### Interface

```
Transport {
  create(config, provider) → sandbox_id
  exec(sandbox_id, command, provider, timeout?) → ExecOutput
  destroy(sandbox_ids, provider, all?) → destroyed_ids
  list(provider) → SandboxInfo[]
  pause(sandbox_id, provider) → void
  unpause(sandbox_id, provider) → void
  gc(provider, dry_run?, all?) → destroyed_ids
  copy_to(sandbox_id, host_path, sandbox_path, provider) → void
  copy_from(sandbox_id, sandbox_path, host_path, provider) → void
}
```

Every transport method takes `provider` explicitly. The `Roche` client and `Sandbox` objects are responsible for supplying the correct provider string. For single-sandbox operations (`exec`, `pause`, `destroy` by one ID, etc.), the caller wraps the single ID in a one-element list when calling `destroy()`.

### gRPC transport

- Uses generated client stubs from `sandbox.proto`
- Connects to `127.0.0.1:{port}` from `daemon.json`
- gRPC channel is created lazily on first call
- If the daemon crashes after construction, gRPC calls fail with `ProviderUnavailable` — no automatic fallback to CLI (transport is chosen once at construction)
- Channel is shared across concurrent calls (gRPC channels are multiplexed)
- Maps gRPC status errors back to SDK-level exceptions/errors

### CLI transport

- Spawns `roche` binary as subprocess
- Passes arguments matching CLI command structure
- For `exec`: captures the subprocess stdout/stderr and exit code directly — these correspond to the sandboxed command's output since the CLI pipes them through. If the `roche` binary itself fails (e.g., sandbox not found), it writes an error message to stderr and exits with code 1. The CLI transport detects roche-level errors by checking if stderr contains known error prefixes (`"Error: "`) before the process output.
- For `list`: uses `--json` flag for structured output
- For `create`: parses sandbox ID from stdout (one ID per line)
- For `copy_to`/`copy_from`: maps to `roche cp` with the `sandbox_id:/path` syntax. `copy_to(id, host, sandbox_path)` → `roche cp {host} {id}:{sandbox_path}`. `copy_from(id, sandbox_path, host)` → `roche cp {id}:{sandbox_path} {host}`.
- For other commands: checks exit code for success/failure
- **Provider on CLI transport**: The CLI currently only accepts `--provider` on the `create` subcommand. For non-create commands, the CLI transport omits the provider flag and relies on the CLI's own default (`"docker"`). This is acceptable because in CLI-fallback mode (no daemon), the CLI uses direct provider access which defaults to Docker. If multi-provider CLI support is needed later, `--provider` flags can be added to other subcommands.

### Auto-detection logic

```
1. If mode="direct" → use CLI transport
2. Read ~/.roche/daemon.json
3. If file exists and process alive → try gRPC connect
4. If gRPC connects → use gRPC transport
5. Otherwise → use CLI transport
```

Detection happens once at client construction time. No per-call overhead. No automatic fallback after construction — if the chosen transport fails, it raises an error.

### Daemon info file

`~/.roche/daemon.json` schema:

```json
{
  "pid": 12345,
  "port": 50051
}
```

Both SDKs parse this file at construction time to detect a running daemon. The `pid` is checked for liveness (kill signal 0 on Unix). If the process is not alive, the file is treated as stale and CLI transport is used.

## Error Handling

Both SDKs expose a consistent error hierarchy:

| SDK Error | Maps from (gRPC) | Maps from (CLI) |
|---|---|---|
| `SandboxNotFound` | `NOT_FOUND` | stderr contains "not found" |
| `SandboxPaused` | `FAILED_PRECONDITION` | stderr contains "paused" |
| `ProviderUnavailable` | `UNAVAILABLE` | `roche` binary not found / connection refused |
| `TimeoutError` | `DEADLINE_EXCEEDED` | stderr contains "timeout" |
| `UnsupportedOperation` | `UNIMPLEMENTED` | stderr contains "unsupported" |
| `RocheError` | all other gRPC errors | all other non-zero exits |

Both transports map to the same error types, so user code doesn't care which mode is active.

## Testing Strategy

### Unit tests (no daemon/Docker required)

- Mock transport layer to test `Roche` client and `Sandbox` class logic
- Test daemon detection with fixture `daemon.json` files
- Test CLI transport argument building and output parsing
- Test gRPC transport error mapping

### Integration tests (require daemon running)

- Start daemon in-process or via subprocess
- Run full create → exec → destroy cycle via gRPC transport
- Verify mode selection: construct one client with daemon running (gRPC mode), construct another with daemon stopped (CLI mode), confirm both work independently

### CI approach

- Unit tests run on every PR
- Integration tests run when Docker is available

## Non-Goals

- Browser support (Node.js only for TypeScript)
- Deno/Bun support (can be added later)
- Streaming exec output (unary RPC for now, matches daemon)
- SDK-level sandbox pooling or connection pooling
- Auto-starting the daemon from SDK
- Batch methods (`create_many`, `destroy_many`) — users loop instead
- Proto schema versioning strategy (additive-only changes for now; breaking changes would require a `v2` package)
