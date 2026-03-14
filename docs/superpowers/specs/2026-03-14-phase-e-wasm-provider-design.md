# Phase E1: WASM Provider — Design Spec

## Goal

Add a `WasmProvider` to roche-core that runs WebAssembly modules as sandboxes using Wasmtime + WASI. Provides sub-millisecond startup, zero external dependencies, and strong isolation — ideal for lightweight AI agent code execution.

## Architecture

```
crates/roche-core/src/provider/
├── mod.rs          # add: pub mod wasm;
├── docker.rs
├── firecracker/
└── wasm/
    ├── mod.rs              # WasmProvider: SandboxProvider + SandboxLifecycle
    ├── engine.rs           # Wasmtime Engine + Module compilation cache
    └── sandbox_state.rs    # In-memory sandbox registry
```

The WASM provider is gated behind a `wasmtime` cargo feature flag to keep dependencies optional.

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Runtime | Wasmtime | Bytecode Alliance, best WASI support, native Rust API |
| Execution model | Per-exec instantiation | Each `exec()` creates a fresh WASI instance from pre-compiled module; natural WASM model |
| `image` field | Path to `.wasm` file | Reuses existing `SandboxConfig.image`; e.g. `image: "/path/to/module.wasm"` |
| Virtual filesystem | wasmtime-wasi preopened dirs | `mounts` map to WASI preopened directories; tmpdir for writable scratch |
| Feature flag | `wasmtime` cargo feature | Keeps WASM deps optional; `cargo build --features wasmtime` |
| Pause/Unpause | Not supported | WASM is per-exec; no persistent process to pause. Returns `ProviderError::Unsupported` |
| File copy | Not supported initially | WASM sandbox has virtual FS; copy_to/copy_from can be added later via preopened dirs |
| Network | Always disabled | WASI does not expose network sockets; aligns with AI-safe defaults |

## Execution Model

### `create(config) → SandboxId`
1. Read `.wasm` file from `config.image` path
2. Pre-compile module using Wasmtime `Engine` (AOT compilation)
3. Store compiled module + config in in-memory registry
4. Generate UUID as sandbox ID
5. Record `expires_at = now + timeout_secs`

### `exec(id, request) → ExecOutput`
1. Look up sandbox in registry by ID
2. Build WASI context:
   - `argv` = `request.command` (first element is the WASM module entrypoint, rest are args)
   - `env` = from stored config
   - Preopened dirs from `config.mounts`
   - stdout/stderr captured to in-memory buffers
3. Instantiate module with WASI context
4. Call `_start` (WASI entrypoint)
5. Collect exit code, stdout, stderr
6. Apply timeout via `tokio::time::timeout`

### `destroy(id)`
1. Remove sandbox from registry
2. Drop compiled module (frees memory)

### `list() → Vec<SandboxInfo>`
1. Iterate registry, return metadata for each sandbox
2. Status is always `Running` (no persistent process state)

### `gc()`
1. Iterate registry, destroy sandboxes where `expires_at <= now`

### `pause(id)` / `unpause(id)`
1. Return `ProviderError::Unsupported("WASM sandboxes cannot be paused")`

## In-Memory Sandbox State

```rust
struct WasmSandbox {
    id: SandboxId,
    module: wasmtime::Module,
    config: SandboxConfig,
    created_at: u64,
    expires_at: Option<u64>,
}
```

Registry is `Arc<Mutex<HashMap<SandboxId, WasmSandbox>>>` for thread safety.

## Dependencies (feature-gated)

```toml
[dependencies]
wasmtime = { version = "29", optional = true }
wasmtime-wasi = { version = "29", optional = true }

[features]
wasmtime = ["dep:wasmtime", "dep:wasmtime-wasi"]
```

## CLI Integration

The CLI's `--provider wasm` routes to `WasmProvider`. The `match` in `main.rs` adds a `"wasm"` arm. Since WASM has no `image` in the Docker sense, the `--image` flag takes a `.wasm` file path.

## Error Mapping

| Scenario | ProviderError |
|----------|--------------|
| `.wasm` file not found | `CreateFailed("WASM module not found: {path}")` |
| Invalid WASM binary | `CreateFailed("invalid WASM module: {details}")` |
| Sandbox ID not in registry | `NotFound(id)` |
| WASI execution trap | `ExecFailed("WASM trap: {details}")` |
| Timeout exceeded | `Timeout(secs)` |
| Pause/unpause called | `Unsupported("WASM sandboxes cannot be paused")` |

## Testing Strategy

### Unit tests (no WASM runtime needed)
- Test sandbox state registry: insert, get, remove, list, gc
- Test memory parsing, config validation
- Test error mapping

### Unit tests (with wasmtime, no external files)
- Compile a trivial WASM module in-memory (WAT text format → binary)
- Test full create → exec → destroy cycle
- Test timeout behavior
- Test environment variables and argv passing

### Integration tests
- Deferred to Item 3 (integration test infrastructure)

## Non-Goals

- WASI networking (not available in WASI preview 1/2)
- Persistent WASM processes
- WASM component model (future — when ecosystem matures)
- Pre-built WASM "images" registry
- `SandboxFileOps` implementation (can be added later)
