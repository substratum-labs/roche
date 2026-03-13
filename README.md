# Roche

> Universal sandbox orchestrator for AI agents.

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Roche provides a single abstraction over multiple sandbox providers (Docker, Firecracker, WASM) with **AI-optimized security defaults** — network disabled, filesystem readonly, timeout enforced.

Named after [Édouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Why Roche?

Every AI agent framework independently integrates sandbox providers, creating an N×M complexity problem:

```
LangChain ──┐         ┌── Docker
CrewAI   ───┤  N × M  ├── E2B
AutoGen  ───┘         └── Modal
```

Roche reduces this to N+M:

```
LangChain ──┐              ┌── Docker
CrewAI   ───┤── Roche() ───├── Firecracker
AutoGen  ───┘              └── WASM
```

## Features

- **AI-safe defaults** — network off, readonly filesystem, 300s timeout
- **Multi-provider** — Docker (MVP), Firecracker, WASM (planned)
- **CLI + SDK** — `roche` binary + Python SDK
- **Resource limits** — memory, CPU, PID limits, timeout enforcement
- **Zero config** — sensible defaults, opt-in for permissions

## Quick Start

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed and running
- [Rust](https://rustup.rs/) toolchain (for building from source)

### Install

```bash
cargo install --path crates/roche-cli
```

### Usage

```bash
# Create a sandbox (network off, readonly FS by default)
SANDBOX_ID=$(roche create --provider docker --memory 512m)

# Execute code in the sandbox
roche exec --sandbox $SANDBOX_ID python3 -c "print('Hello from Roche!')"

# List active sandboxes
roche list

# Clean up
roche destroy $SANDBOX_ID
```

## CLI Reference

### `roche create`

Create a new sandbox and print its ID.

| Flag | Default | Description |
|------|---------|-------------|
| `--provider` | `docker` | Sandbox provider |
| `--image` | `python:3.12-slim` | Container image |
| `--memory` | (none) | Memory limit (e.g. `512m`, `1g`) |
| `--cpus` | (none) | CPU limit (e.g. `0.5`, `2.0`) |
| `--timeout` | `300` | Sandbox timeout in seconds |
| `--network` | off | Enable network access |
| `--writable` | off | Enable writable filesystem |
| `--env` | (none) | Environment variable `KEY=VALUE` (repeatable) |

### `roche exec`

Execute a command inside an existing sandbox.

| Flag | Default | Description |
|------|---------|-------------|
| `--sandbox` | (required) | Sandbox ID |
| `--timeout` | (none) | Timeout override in seconds |

Remaining arguments are the command to execute.

### `roche destroy`

Destroy a sandbox and release its resources.

```bash
roche destroy <SANDBOX_ID>
```

### `roche list`

List all active Roche-managed sandboxes.

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | off | Output as JSON |

## Python SDK

### Install

```bash
pip install -e sdk/python
```

### Usage

```python
from roche import Roche, Sandbox, SandboxConfig

# Direct client usage
client = Roche()
sandbox_id = client.create(SandboxConfig(memory="512m"))
output = client.exec(sandbox_id, ["python3", "-c", "print(2 + 2)"])
print(output.stdout)  # "4\n"
client.destroy(sandbox_id)

# Context manager (auto-cleanup)
with Sandbox(config=SandboxConfig(memory="512m")) as sb:
    result = sb.exec(["python3", "-c", "print('Hello!')"])
    print(result.stdout)
# sandbox is automatically destroyed
```

### Configuration

```python
config = SandboxConfig(
    provider="docker",          # sandbox provider
    image="python:3.12-slim",   # container image
    memory="1g",                # memory limit
    cpus=2.0,                   # CPU limit
    timeout=600,                # timeout in seconds
    network=True,               # enable network (default: False)
    writable=True,              # enable writable FS (default: False)
    env={"API_KEY": "secret"},  # environment variables
)
```

## Security Defaults

Roche is designed for AI agent workloads where untrusted code execution is the norm:

| Setting | Default | Rationale |
|---------|---------|-----------|
| Network | **disabled** | Prevent data exfiltration and C2 communication |
| Filesystem | **readonly** | Prevent persistent compromise and file tampering |
| Timeout | **300s** | Prevent resource exhaustion and infinite loops |
| PID limit | **256** | Prevent fork bombs |
| Privileges | **no-new-privileges** | Prevent privilege escalation |

Override explicitly when needed: `--network`, `--writable`.

## Development

```bash
cargo build              # Build all crates
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting

# Python SDK
pip install -e "sdk/python[dev]"
pytest sdk/python/tests/ -v
```

## License

Apache-2.0
