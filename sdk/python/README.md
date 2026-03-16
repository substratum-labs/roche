# roche-sandbox

Python SDK for [Roche](https://github.com/substratum-labs/roche) -- universal sandbox orchestrator for AI agents.

## Requirements

- Python >= 3.10
- Roche CLI on `PATH` (or Roche daemon running)

## Install

```bash
pip install roche-sandbox
```

## Quick Start

```python
from roche_sandbox import Roche

roche = Roche()
sandbox = roche.create(image="python:3.12-slim")
output = sandbox.exec(["python3", "-c", "print('Hello from Roche!')"])
print(output.stdout)  # Hello from Roche!
sandbox.destroy()
```

### Context Manager (auto-cleanup)

```python
with roche.create(image="python:3.12-slim") as sandbox:
    output = sandbox.exec(["echo", "hello"])
```

### Async API

```python
import asyncio
from roche_sandbox import AsyncRoche

async def main():
    roche = AsyncRoche()
    sandbox = await roche.create(image="python:3.12-slim")
    output = await sandbox.exec(["echo", "hello"])
    await sandbox.destroy()

asyncio.run(main())
```

## Configuration

```python
sandbox = roche.create(
    image="python:3.12-slim",
    memory="512m",
    cpus=1.0,
    timeout_secs=600,
    network=False,    # default: AI-safe
    writable=False,   # default: AI-safe
    env={"API_KEY": "secret"},
)
```

## Transport

The SDK auto-detects whether the Roche gRPC daemon is running and connects to it. If the daemon is unavailable, it falls back to invoking the Roche CLI as a subprocess.

You can force CLI mode explicitly:

```python
roche = Roche(mode="direct")
```

## API Styles

The SDK provides two API styles:

- **Async-first**: `AsyncRoche` and `AsyncSandbox` -- native `async`/`await` support.
- **Sync wrapper**: `Roche` and `Sandbox` -- blocking equivalents for scripts and notebooks.

## Public Exports

```python
from roche_sandbox import (
    Roche, AsyncRoche,
    Sandbox, AsyncSandbox,
    roche_sandbox,                 # decorator
    SandboxConfig, ExecOutput, SandboxInfo,
    Mount, SandboxStatus,
    RocheError, SandboxNotFound, SandboxPaused,
    ProviderUnavailable, TimeoutError, UnsupportedOperation,
)
```

## `@roche_sandbox` Decorator

The decorator automatically creates and injects a sandbox into your function — no manual lifecycle management needed. Works with both sync and async functions.

```python
from roche_sandbox import roche_sandbox

@roche_sandbox(image="python:3.12-slim")
def run_code(code: str, sandbox) -> str:
    result = sandbox.exec(["python3", "-c", code])
    return result.stdout

output = run_code("print('hello')")  # sandbox is auto-managed
```

### Async

```python
@roche_sandbox(image="python:3.12-slim")
async def run_code(code: str, sandbox) -> str:
    result = await sandbox.exec(["python3", "-c", code])
    return result.stdout
```

### Agent Framework Integration

The decorator strips the `sandbox` parameter from the function signature, so agent frameworks (OpenAI, LangChain, CrewAI, etc.) only see user-facing parameters:

```python
from agents import function_tool

@function_tool
@roche_sandbox(image="python:3.12-slim")
def run_code(code: str, sandbox) -> str:
    """Execute Python code in a sandbox."""
    return sandbox.exec(["python3", "-c", code]).stdout
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `image` | `str` | `"python:3.12-slim"` | Container image |
| `provider` | `str` | `"docker"` | Sandbox provider |
| `network` | `bool` | `False` | Enable network access |
| `writable` | `bool` | `False` | Enable writable filesystem |
| `timeout_secs` | `int` | `300` | Sandbox timeout |
| `memory` | `str \| None` | `None` | Memory limit (e.g. `"512m"`) |
| `cpus` | `float \| None` | `None` | CPU limit |
| `sandbox_param` | `str` | `"sandbox"` | Name of the injected parameter |

## Agent Framework Examples

See [examples/python/](https://github.com/substratum-labs/roche/tree/main/examples/python) for integration examples with:

- **OpenAI Agents SDK** — `@function_tool` integration
- **LangChain / LangGraph** — custom `BaseTool` + stateful retry workflow
- **CrewAI** — `@tool` decorator + multi-agent crew
- **Anthropic API** — `tool_use` + multi-turn agentic loop
- **AutoGen** — custom `CodeExecutor` + group chat
- **Camel-AI** — `BaseToolkit` + role-playing session

All examples run without API keys (simulated mode) and optionally switch to real LLM calls.

## License

Apache-2.0 -- see [LICENSE](https://github.com/substratum-labs/roche/blob/main/LICENSE).
