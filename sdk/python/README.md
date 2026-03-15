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
    SandboxConfig, ExecOutput, SandboxInfo,
    Mount, SandboxStatus,
    RocheError, SandboxNotFound, SandboxPaused,
    ProviderUnavailable, TimeoutError, UnsupportedOperation,
)
```

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
