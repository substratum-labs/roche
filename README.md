# Roche

> The safest way to let code interact with the world.

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![PyPI](https://img.shields.io/pypi/v/roche-sandbox)](https://pypi.org/project/roche-sandbox/)
[![npm](https://img.shields.io/npm/v/roche-sandbox)](https://www.npmjs.com/package/roche-sandbox)
[![Go Reference](https://pkg.go.dev/badge/github.com/substratum-labs/roche/sdk/go.svg)](https://pkg.go.dev/github.com/substratum-labs/roche/sdk/go)

Roche is a **controlled channel** between AI agents and the world. Not a wall that blocks everything, but a nervous system that controls *how* code interacts with its environment — with the right permissions, the right limits, and real-time feedback.

Named after [Edouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Why Roche?

Every AI agent framework independently integrates sandbox providers, creating an N x M complexity problem. Roche reduces this to N+M:

```
OpenAI Agents ──┐                  ┌── Docker
LangChain    ───┤                  ├── Firecracker
CrewAI       ───┤                  ├── WASM
Anthropic    ───┤── Roche()/SDK ───├── E2B
AutoGen      ───┤                  ├── Kubernetes
Camel-AI     ───┘                  └── ...
```

## Features

- **One-line execution** — `roche.run("print(2+2)")` with zero config
- **Intent-based** — analyzes code to auto-select provider and minimal permissions
- **5 providers** — Docker, Firecracker, WASM, E2B, Kubernetes
- **3 SDKs** — Python, TypeScript, Go
- **Streaming exec** — real-time stdout/stderr with heartbeats
- **Sessions & budgets** — track exec count, time, output across calls
- **Castor integration** — `Castor(roche=True)` for security-gated agent execution
- **Multi-agent workspaces** — shared sandbox across agent hierarchy
- **gRPC daemon** — `roched` for persistent sandbox pooling

## Quick Start

### One-liner (Python)

```bash
pip install roche-sandbox
```

```python
from roche_sandbox import run

# Auto-detects: provider, network, filesystem, memory
result = run("print(2 + 2)")
print(result.stdout)  # 4

# Network auto-detected from code
result = run("""
    import requests
    r = requests.get('https://api.github.com')
    print(r.status_code)
""")
# Auto-inferred: network=True, allowlist=["api.github.com"]
```

### With Castor (security-gated agents)

```bash
pip install roche-sandbox[castor]
```

```python
from castor import Castor

# One line — Castor gates permissions, Roche executes safely
kernel = Castor(roche=True, default_budgets={"compute": 10})

async def my_agent(proxy):
    result = await proxy.syscall("execute_code", code="print('hello')")
    return result["stdout"]

cp = await kernel.run(my_agent)
```

### Multi-agent workspace

```python
from roche_sandbox.castor import RocheCastorBridge

bridge = RocheCastorBridge()

async def coordinator(proxy):
    # Shared sandbox — files persist between agent calls
    async with await bridge.workspace(writable=True) as ws:
        # Researcher writes data
        await proxy.syscall("execute_in_workspace",
            code="open('/tmp/data.json','w').write('{\"key\": 42}')",
            workspace_id=ws.id)
        # Publisher reads same data
        result = await proxy.syscall("execute_in_workspace",
            code="print(open('/tmp/data.json').read())",
            workspace_id=ws.id)
```

### CLI

```bash
cargo install roche-cli

# Create a sandbox (network off, readonly FS by default)
SANDBOX_ID=$(roche create --provider docker --memory 512m)

# Execute code
roche exec --sandbox $SANDBOX_ID -- python3 -c "print('Hello!')"

# Clean up
roche destroy $SANDBOX_ID
```

## SDKs

### Python

```bash
pip install roche-sandbox
```

```python
from roche_sandbox import Roche

roche = Roche()
with roche.create(image="python:3.12-slim") as sandbox:
    result = sandbox.exec(["python3", "-c", "print('Hello!')"])
    print(result.stdout)
```

### TypeScript

```bash
npm install roche-sandbox
```

```typescript
import { Roche } from "roche-sandbox";

const roche = new Roche();
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
const output = await sandbox.exec(["python3", "-c", "print('Hello!')"]);
console.log(output.stdout);
await sandbox.destroy();
```

### Go

```bash
go get github.com/substratum-labs/roche/sdk/go
```

```go
client, _ := roche.New()
sandbox, _ := client.CreateSandbox(ctx, roche.SandboxConfig{Image: "python:3.12-slim"})
defer sandbox.Close(ctx)
out, _ := sandbox.Exec(ctx, []string{"python3", "-c", "print('Hello!')"})
fmt.Println(out.Stdout)
```

## Agent Framework Integrations

| Framework | Example | Env Var |
|-----------|---------|---------|
| [OpenAI Agents SDK](examples/python/openai-agents/) | Tool + code interpreter | `OPENAI_API_KEY` |
| [LangChain / LangGraph](examples/python/langchain/) | Tool + stateful retry workflow | `OPENAI_API_KEY` |
| [CrewAI](examples/python/crewai/) | Task + multi-agent crew | `OPENAI_API_KEY` |
| [Anthropic API](examples/python/anthropic/) | tool_use + agentic loop | `ANTHROPIC_API_KEY` |
| [AutoGen](examples/python/autogen/) | Code executor + group chat | `OPENAI_API_KEY` |
| [Camel-AI](examples/python/camel/) | Toolkit + role-playing | `OPENAI_API_KEY` |

## Architecture

```
                   ┌─────────────────────────────────────────┐
                   │              Castor (optional)           │
                   │     Security kernel: budget, HITL,       │
                   │     intent gating, violation tracking     │
                   └──────────────┬──────────────────────────┘
                                  │
┌─────────────────────────────────▼──────────────────────────────────┐
│                           Roche                                    │
│                                                                    │
│  ┌──────────┐  ┌──────────────┐  ┌──────────┐  ┌───────────────┐ │
│  │  Intent   │  │   Session    │  │ Streaming │  │   Workspace   │ │
│  │  Engine   │  │   Manager    │  │  Monitor  │  │   Manager     │ │
│  │          │  │ budget/perms │  │  L3 kill  │  │ multi-agent   │ │
│  └────┬─────┘  └──────┬───────┘  └─────┬─────┘  └───────┬───────┘ │
│       │               │                │                 │         │
│  ┌────▼───────────────▼────────────────▼─────────────────▼──────┐  │
│  │                     Provider Layer                           │  │
│  │  Docker │ Firecracker │ WASM │ E2B │ Kubernetes              │  │
│  └─────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

## Security Defaults

| Setting | Default | Rationale |
|---------|---------|-----------|
| Network | **disabled** | Prevent data exfiltration |
| Filesystem | **readonly** | Prevent persistent compromise |
| Timeout | **300s** | Prevent resource exhaustion |
| PID limit | **256** | Prevent fork bombs |
| Privileges | **no-new-privileges** | Prevent escalation |

Override explicitly: `--network`, `--writable`, or let intent analysis auto-detect.

## Development

```bash
# Rust
cargo build && cargo test && cargo clippy && cargo fmt --check

# Python SDK
pip install -e "sdk/python[dev]" && pytest sdk/python/tests/ -v

# TypeScript SDK
cd sdk/typescript && npm ci && npm test

# Go SDK
cd sdk/go && go test ./... -v
```

## License

Apache-2.0
