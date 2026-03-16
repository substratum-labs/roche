# Roche

> Universal sandbox orchestrator for AI agents.

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![PyPI](https://img.shields.io/pypi/v/roche-sandbox)](https://pypi.org/project/roche-sandbox/)
[![npm](https://img.shields.io/npm/v/roche-sandbox)](https://www.npmjs.com/package/roche-sandbox)
[![Go Reference](https://pkg.go.dev/badge/github.com/substratum-labs/roche/sdk/go.svg)](https://pkg.go.dev/github.com/substratum-labs/roche/sdk/go)

Roche provides a single abstraction over multiple sandbox providers (Docker, Firecracker, WASM) with **AI-optimized security defaults** — network disabled, filesystem readonly, timeout enforced.

Named after [Édouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Why Roche?

Every AI agent framework independently integrates sandbox providers, creating an N×M complexity problem:

```
OpenAI Agents ──┐              ┌── Docker
LangChain    ───┤              ├── E2B
CrewAI       ───┤    N × M     ├── Modal
Anthropic    ───┤              ├── Firecracker
AutoGen      ───┤              ├── Fly.io
Camel-AI     ───┘              └── ...
```

Roche reduces this to N+M:

```
OpenAI Agents ──┐                  ┌── Docker
LangChain    ───┤                  ├── Firecracker
CrewAI       ───┤                  ├── WASM
Anthropic    ───┤── Roche()/SDK ───├── E2B
AutoGen      ───┤                  ├── Kubernetes
Camel-AI     ───┘                  └── ...
```

## Features

- **AI-safe defaults** — network off, readonly filesystem, 300s timeout
- **Multi-provider** — Docker, Firecracker, WASM, E2B, Kubernetes
- **CLI + SDK** — `roche` binary + Python, TypeScript & Go SDKs
- **gRPC daemon** — `roched` for persistent sandbox pooling and fast acquisition
- **Resource limits** — memory, CPU, PID limits, timeout enforcement
- **Zero config** — sensible defaults, opt-in for permissions

## Quick Start

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed and running
- [Rust](https://rustup.rs/) toolchain (for building from source)

### Install

```bash
# From crates.io
cargo install roche-cli

# Or from source
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

```bash
pip install roche-sandbox
```

```python
from roche_sandbox import Roche

roche = Roche()
sandbox = roche.create(image="python:3.12-slim")
output = sandbox.exec(["python3", "-c", "print('Hello from Roche!')"])
print(output.stdout)  # Hello from Roche!
sandbox.destroy()

# Context manager (auto-cleanup)
with roche.create(image="python:3.12-slim") as sandbox:
    result = sandbox.exec(["echo", "hello"])
```

### `@roche_sandbox` Decorator

Auto-inject a sandbox into any function — works with agent framework decorators:

```python
from roche_sandbox import roche_sandbox

@roche_sandbox(image="python:3.12-slim")
def run_code(code: str, sandbox) -> str:
    return sandbox.exec(["python3", "-c", code]).stdout

output = run_code("print('hello')")  # sandbox auto-created and destroyed
```

See [Python SDK README](sdk/python/README.md) for full documentation.

### Agent Framework Integrations

Roche integrates with all major AI agent frameworks. Examples run in simulated mode by default — set the appropriate API key env var to enable real LLM calls.

| Framework | Example | Env Var |
|-----------|---------|---------|
| [OpenAI Agents SDK](examples/python/openai-agents/) | Tool + code interpreter | `OPENAI_API_KEY` |
| [LangChain / LangGraph](examples/python/langchain/) | Tool + stateful retry workflow | `OPENAI_API_KEY` |
| [CrewAI](examples/python/crewai/) | Task + multi-agent crew | `OPENAI_API_KEY` |
| [Anthropic API](examples/python/anthropic/) | tool_use + agentic loop | `ANTHROPIC_API_KEY` |
| [AutoGen](examples/python/autogen/) | Code executor + group chat | `OPENAI_API_KEY` |
| [Camel-AI](examples/python/camel/) | Toolkit + role-playing | `OPENAI_API_KEY` |

See [examples/README.md](examples/README.md) for setup instructions.

## TypeScript SDK

```bash
npm install roche-sandbox
```

```typescript
import { Roche } from "roche-sandbox";

const roche = new Roche();
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
const output = await sandbox.exec(["python3", "-c", "print('Hello!')"]);
console.log(output.stdout); // Hello!
await sandbox.destroy();
```

See [TypeScript SDK README](sdk/typescript/README.md) for full documentation.

## Go SDK

```bash
go get github.com/substratum-labs/roche/sdk/go
```

```go
package main

import (
	"context"
	"fmt"
	"log"

	roche "github.com/substratum-labs/roche/sdk/go"
)

func main() {
	client, err := roche.New()
	if err != nil {
		log.Fatal(err)
	}
	ctx := context.Background()
	sandbox, err := client.CreateSandbox(ctx, roche.SandboxConfig{
		Image: "python:3.12-slim",
	})
	if err != nil {
		log.Fatal(err)
	}
	defer sandbox.Close(ctx)

	out, err := sandbox.Exec(ctx, []string{"python3", "-c", "print('Hello from Roche!')"})
	if err != nil {
		log.Fatal(err)
	}
	fmt.Println(out.Stdout) // Hello from Roche!
}
```

See [Go SDK README](sdk/go/README.md) for full documentation.

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
# Rust
cargo build              # Build all crates
cargo test               # Run tests
cargo clippy             # Lint
cargo fmt --check        # Check formatting

# Python SDK
pip install -e "sdk/python[dev]"
pytest sdk/python/tests/ -v

# TypeScript SDK
cd sdk/typescript && npm ci && npm test

# Go SDK
cd sdk/go && go test ./... -v
```

## License

Apache-2.0
