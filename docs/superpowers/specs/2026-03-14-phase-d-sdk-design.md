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
| TS runtime | Node.js only | AI agent frameworks overwhelmingly target Node.js |
| Python API style | Async-first (`AsyncRoche`) + sync wrapper (`Roche`) | gRPC is inherently async; AI frameworks moving toward async |
| TS API style | Sandbox-centric (`Sandbox` objects) | Ergonomic; encapsulates sandbox ID; aligns with Python SDK |
| Package names | `roche-sandbox` (both npm and PyPI) | Consistent across ecosystems; no npm org required |

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
│   ├── sandbox.ts            # Sandbox class (wraps sandbox_id)
│   ├── types.ts              # SandboxConfig, ExecOutput, etc.
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

// Flat methods
const id = await roche.create({ image: "python:3.12-slim" });
const output = await roche.exec(id, ["python", "-c", "print('hi')"]);
await roche.destroy(id);
const sandboxes = await roche.list();

// Sandbox-centric (recommended)
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
const output = await sandbox.exec(["python", "-c", "print('hi')"]);
await sandbox.copyTo("/local/file.py", "/sandbox/file.py");
await sandbox.pause();
await sandbox.unpause();
await sandbox.destroy();

// Auto-cleanup with using (explicit resource management)
await using sandbox = await roche.createSandbox();
await sandbox.exec(["echo", "hello"]);
// sandbox.destroy() called automatically
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
│       ├── sandbox.py          # Sandbox + AsyncSandbox (context managers)
│       ├── types.py            # SandboxConfig, ExecOutput, Mount, etc.
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

sandbox = await roche.create(image="python:3.12-slim")
output = await sandbox.exec(["python", "-c", "print('hi')"])
print(output.stdout)
await sandbox.destroy()

# Async context manager
async with await roche.create(image="python:3.12-slim") as sandbox:
    output = await sandbox.exec(["echo", "hello"])
# auto-destroyed on exit

# Sync client (wraps async)
roche = Roche()
sandbox = roche.create(image="python:3.12-slim")
output = sandbox.exec(["echo", "hello"])
sandbox.destroy()

# Sync context manager
with roche.create(image="python:3.12-slim") as sandbox:
    output = sandbox.exec(["echo", "hello"])
```

### Migration from existing SDK

The existing `sdk/python/roche/` is replaced by `sdk/python/src/roche_sandbox/`. The sync API shape stays the same (`Roche` class, `Sandbox` context manager), so existing users change their import from `from roche import Roche` to `from roche_sandbox import Roche`.

### Dependencies

- Runtime: `grpcio`, `protobuf`
- Dev: `grpcio-tools` (codegen), `pytest`, `pytest-asyncio`

## Transport Layer

The transport layer is the internal abstraction both SDKs use to dispatch commands.

### Interface

```
Transport {
  create(config) → sandbox_id
  exec(sandbox_id, command, timeout?) → ExecOutput
  destroy(sandbox_ids, all?) → destroyed_ids
  list() → SandboxInfo[]
  pause(sandbox_id) → void
  unpause(sandbox_id) → void
  gc(dry_run?, all?) → destroyed_ids
  copy_to(sandbox_id, host_path, sandbox_path) → void
  copy_from(sandbox_id, sandbox_path, host_path) → void
}
```

### gRPC transport

- Uses generated client stubs from `sandbox.proto`
- Connects to `127.0.0.1:{port}` from `daemon.json`
- Maps gRPC status errors back to SDK-level exceptions/errors

### CLI transport

- Spawns `roche` binary as subprocess
- Passes arguments matching CLI command structure
- Parses stdout for results (sandbox IDs, JSON output for `list --json`, exec output)
- Maps exit codes and stderr to SDK-level exceptions/errors

### Auto-detection logic

```
1. If mode="direct" → use CLI transport
2. Read ~/.roche/daemon.json
3. If file exists and process alive → try gRPC connect
4. If gRPC connects → use gRPC transport
5. Otherwise → use CLI transport
```

Detection happens once at client construction time. No per-call overhead.

## Error Handling

Both SDKs expose a consistent error hierarchy:

| SDK Error | Maps from (gRPC) | Maps from (CLI) |
|---|---|---|
| `SandboxNotFound` | `NOT_FOUND` | exit code + "not found" in stderr |
| `SandboxPaused` | `FAILED_PRECONDITION` | exit code + "paused" in stderr |
| `ProviderUnavailable` | `UNAVAILABLE` | `roche` binary not found / connection refused |
| `TimeoutError` | `DEADLINE_EXCEEDED` | exit code + "timeout" in stderr |
| `UnsupportedOperation` | `UNIMPLEMENTED` | exit code + "unsupported" in stderr |
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
- Verify dual-mode fallback: stop daemon mid-test, confirm CLI fallback works

### CI approach

- Unit tests run on every PR
- Integration tests run when Docker is available

## Non-Goals

- Browser support (Node.js only for TypeScript)
- Deno/Bun support (can be added later)
- Streaming exec output (unary RPC for now, matches daemon)
- SDK-level sandbox pooling or connection pooling
- Auto-starting the daemon from SDK
