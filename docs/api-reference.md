# Roche API Reference

## CLI

### `roche create`

Create a new sandbox and print its ID.

```bash
roche create [OPTIONS]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--provider` | string | `docker` | Sandbox provider (`docker`, `firecracker`, `wasm`) |
| `--image` | string | `python:3.12-slim` | Container image |
| `--memory` | string | — | Memory limit (e.g. `512m`, `1g`) |
| `--cpus` | float | — | CPU limit (e.g. `0.5`, `2.0`) |
| `--timeout` | int | `300` | Sandbox timeout in seconds |
| `--network` | flag | off | Enable network access |
| `--writable` | flag | off | Enable writable filesystem |
| `--env` | string | — | Environment variable `KEY=VALUE` (repeatable) |
| `--mount` | string | — | Volume mount `host:container[:ro]` (repeatable) |
| `--kernel` | string | — | Linux kernel path (Firecracker only) |
| `--rootfs` | string | — | Root filesystem path (Firecracker only) |

**Output:** Sandbox ID (string).

### `roche exec`

Execute a command inside an existing sandbox.

```bash
roche exec --sandbox <ID> [OPTIONS] -- <COMMAND...>
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--sandbox` | string | (required) | Sandbox ID |
| `--timeout` | int | — | Timeout override in seconds |

**Output:** stdout to stdout, stderr to stderr. Process exit code matches command exit code.

### `roche destroy`

Destroy one or more sandboxes.

```bash
roche destroy <ID...>
```

### `roche list`

List all active Roche-managed sandboxes.

```bash
roche list [OPTIONS]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--json` | flag | off | Output as JSON |

### `roche gc`

Garbage collect expired sandboxes.

```bash
roche gc [OPTIONS]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--dry-run` | flag | off | List without destroying |
| `--all` | flag | off | Destroy all sandboxes |

---

## Python SDK

```bash
pip install roche-sandbox
```

### `Roche` (sync client)

```python
from roche_sandbox import Roche

roche = Roche(
    mode="auto",           # "auto" | "direct" (CLI only)
    daemon_port=None,      # int — override gRPC daemon port
    provider="docker",     # default provider
    binary="roche",        # path to roche CLI binary
)
```

#### `roche.create(**kwargs) -> Sandbox`

Create a sandbox and return a `Sandbox` handle.

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `provider` | `str \| None` | `None` (uses client default) | Override provider |
| `image` | `str` | `"python:3.12-slim"` | Container image |
| `memory` | `str \| None` | `None` | Memory limit |
| `cpus` | `float \| None` | `None` | CPU limit |
| `timeout_secs` | `int` | `300` | Sandbox timeout |
| `network` | `bool` | `False` | Enable network |
| `writable` | `bool` | `False` | Enable writable FS |
| `env` | `dict[str, str] \| None` | `None` | Environment variables |
| `mounts` | `list[Mount] \| None` | `None` | Volume mounts |

#### `roche.exec(sandbox_id, command, timeout_secs=None) -> ExecOutput`

Execute a command in an existing sandbox by ID.

#### `roche.destroy(sandbox_id) -> None`

Destroy a sandbox by ID.

#### `roche.list() -> list[SandboxInfo]`

List all active sandboxes.

#### `roche.gc(dry_run=False, all=False) -> list[str]`

Garbage collect expired sandboxes. Returns list of destroyed sandbox IDs.

---

### `Sandbox` (sync handle)

Returned by `roche.create()`. Supports context manager (`with` statement).

```python
sandbox = roche.create(image="python:3.12-slim")

# Properties
sandbox.id        # str — sandbox ID
sandbox.provider  # str — provider name

# Methods
sandbox.exec(command: list[str], timeout_secs: int | None = None) -> ExecOutput
sandbox.pause() -> None
sandbox.unpause() -> None
sandbox.destroy() -> None
sandbox.copy_to(host_path: str, sandbox_path: str) -> None
sandbox.copy_from(sandbox_path: str, host_path: str) -> None

# Context manager
with roche.create(image="python:3.12-slim") as sandbox:
    output = sandbox.exec(["echo", "hello"])
# sandbox auto-destroyed
```

---

### `AsyncRoche` (async client)

Async equivalent of `Roche`. Same constructor parameters.

```python
from roche_sandbox import AsyncRoche

roche = AsyncRoche()
sandbox = await roche.create(image="python:3.12-slim")
output = await sandbox.exec(["echo", "hello"])
await sandbox.destroy()
```

All methods are `async` versions of `Roche` methods.

### `AsyncSandbox` (async handle)

Async equivalent of `Sandbox`. Supports `async with`.

```python
async with await roche.create(image="python:3.12-slim") as sandbox:
    output = await sandbox.exec(["echo", "hello"])
```

---

### `@roche_sandbox` Decorator

Auto-creates a sandbox and injects it into the decorated function. The sandbox is destroyed after the function returns (or raises).

```python
from roche_sandbox import roche_sandbox

@roche_sandbox(image="python:3.12-slim")
def run_code(code: str, sandbox) -> str:
    return sandbox.exec(["python3", "-c", code]).stdout

output = run_code("print('hello')")  # sandbox is auto-managed
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `image` | `str` | `"python:3.12-slim"` | Container image |
| `provider` | `str` | `"docker"` | Sandbox provider |
| `network` | `bool` | `False` | Enable network access |
| `writable` | `bool` | `False` | Enable writable filesystem |
| `timeout_secs` | `int` | `300` | Sandbox timeout |
| `memory` | `str \| None` | `None` | Memory limit |
| `cpus` | `float \| None` | `None` | CPU limit |
| `sandbox_param` | `str` | `"sandbox"` | Name of the injected parameter |

**Behavior:**

- Detects sync vs async functions automatically — uses `Roche` or `AsyncRoche` accordingly.
- Strips the `sandbox` parameter from `__signature__` so agent framework introspection (e.g. OpenAI `@function_tool`, LangChain `@tool`) does not expose it as a user-facing argument.
- Stacks with framework decorators:

```python
@function_tool
@roche_sandbox(image="python:3.12-slim")
def run_code(code: str, sandbox) -> str:
    """Execute Python code safely."""
    return sandbox.exec(["python3", "-c", code]).stdout
```

---

### Types

#### `SandboxConfig`

```python
@dataclass
class SandboxConfig:
    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout_secs: int = 300
    network: bool = False          # AI-safe default
    writable: bool = False         # AI-safe default
    env: dict[str, str] = field(default_factory=dict)
    mounts: list[Mount] = field(default_factory=list)
    kernel: str | None = None      # Firecracker only
    rootfs: str | None = None      # Firecracker only
```

#### `ExecOutput`

```python
@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str
```

#### `SandboxInfo`

```python
@dataclass
class SandboxInfo:
    id: str
    status: SandboxStatus    # "running" | "paused" | "stopped" | "failed"
    provider: str
    image: str
    expires_at: int | None = None
```

#### `Mount`

```python
@dataclass
class Mount:
    host_path: str
    container_path: str
    readonly: bool = True    # AI-safe default
```

#### `SandboxStatus`

```python
SandboxStatus = Literal["running", "paused", "stopped", "failed"]
```

---

### Errors

All errors inherit from `RocheError`.

| Exception | When |
|-----------|------|
| `RocheError` | Base exception for all Roche errors |
| `SandboxNotFound` | Sandbox ID does not exist |
| `SandboxPaused` | Operation attempted on a paused sandbox |
| `ProviderUnavailable` | Provider not installed or not running (e.g. Docker not available) |
| `TimeoutError` | Operation exceeded timeout |
| `UnsupportedOperation` | Provider does not support requested operation |

```python
from roche_sandbox import RocheError, SandboxNotFound

try:
    sandbox.exec(["echo", "hello"])
except SandboxNotFound:
    print("Sandbox was destroyed")
except RocheError as e:
    print(f"Roche error: {e}")
```

---

## TypeScript SDK

```bash
npm install roche-sandbox
```

### `Roche`

```typescript
import { Roche } from "roche-sandbox";

const roche = new Roche({
  mode?: "auto" | "direct",
  daemonPort?: number,
  provider?: string,      // default: "docker"
  binary?: string,        // default: "roche"
});
```

#### `roche.createSandbox(config?) -> Promise<Sandbox>`

#### `roche.exec(sandboxId, command, timeoutSecs?) -> Promise<ExecOutput>`

#### `roche.destroy(sandboxId) -> Promise<void>`

#### `roche.list() -> Promise<SandboxInfo[]>`

### `Sandbox`

```typescript
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });

sandbox.id: string;
sandbox.provider: string;

await sandbox.exec(command: string[], timeoutSecs?: number): Promise<ExecOutput>;
await sandbox.pause(): Promise<void>;
await sandbox.unpause(): Promise<void>;
await sandbox.destroy(): Promise<void>;
await sandbox.copyTo(hostPath: string, sandboxPath: string): Promise<void>;
await sandbox.copyFrom(sandboxPath: string, hostPath: string): Promise<void>;

// Auto-cleanup with `using` (TC39 Explicit Resource Management)
await using sandbox = await roche.createSandbox();
```

### Types

```typescript
interface SandboxConfig {
  provider?: string;          // default: "docker"
  image?: string;             // default: "python:3.12-slim"
  memory?: string;
  cpus?: number;
  timeoutSecs?: number;       // default: 300
  network?: boolean;          // default: false
  writable?: boolean;         // default: false
  env?: Record<string, string>;
  mounts?: Mount[];
}

interface ExecOutput {
  exitCode: number;
  stdout: string;
  stderr: string;
}

interface SandboxInfo {
  id: string;
  status: SandboxStatus;      // "running" | "paused" | "stopped" | "failed"
  provider: string;
  image: string;
  expiresAt?: number;
}

interface Mount {
  hostPath: string;
  containerPath: string;
  readonly?: boolean;         // default: true
}

type SandboxStatus = "running" | "paused" | "stopped" | "failed";
```

### Errors

All errors extend `RocheError`.

```typescript
import { RocheError, SandboxNotFound } from "roche-sandbox";
```

| Error | When |
|-------|------|
| `RocheError` | Base error |
| `SandboxNotFound` | Sandbox ID not found |
| `SandboxPaused` | Operation on paused sandbox |
| `ProviderUnavailable` | Provider not available |
| `TimeoutError` | Operation timed out |
| `UnsupportedOperation` | Provider does not support operation |

---

## Rust Crate (`roche-core`)

```toml
[dependencies]
roche-core = "0.1"
```

### `SandboxProvider` trait

```rust
pub trait SandboxProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError>;
    async fn exec(&self, id: &SandboxId, request: &ExecRequest) -> Result<ExecOutput, ProviderError>;
    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError>;
    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError>;
}
```

### `SandboxFileOps` trait

```rust
pub trait SandboxFileOps {
    async fn copy_to(&self, id: &SandboxId, src: &Path, dest: &str) -> Result<(), ProviderError>;
    async fn copy_from(&self, id: &SandboxId, src: &str, dest: &Path) -> Result<(), ProviderError>;
}
```

### `SandboxLifecycle` trait

```rust
pub trait SandboxLifecycle {
    async fn pause(&self, id: &SandboxId) -> Result<(), ProviderError>;
    async fn unpause(&self, id: &SandboxId) -> Result<(), ProviderError>;
    async fn gc(&self) -> Result<Vec<SandboxId>, ProviderError>;
}
```

### `ProviderError`

```rust
pub enum ProviderError {
    NotFound(SandboxId),
    CreateFailed(String),
    ExecFailed(String),
    Unavailable(String),
    Timeout(u64),
    Unsupported(String),
    FileFailed(String),
    Paused(SandboxId),
}
```

### Available Providers

| Provider | Struct | Feature Flag |
|----------|--------|-------------|
| Docker | `docker::DockerProvider` | (default) |
| Firecracker | `firecracker::FirecrackerProvider` | (default) |
| WASM | `wasm::WasmProvider` | `wasmtime` |
