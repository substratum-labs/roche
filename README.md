# Roche

> The safest way to let code interact with the world.

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![PyPI](https://img.shields.io/pypi/v/roche-sandbox)](https://pypi.org/project/roche-sandbox/)
[![npm](https://img.shields.io/npm/v/roche-sandbox)](https://www.npmjs.com/package/roche-sandbox)
[![Go Reference](https://pkg.go.dev/badge/github.com/substratum-labs/roche/sdk/go.svg)](https://pkg.go.dev/github.com/substratum-labs/roche/sdk/go)

Run untrusted code safely. Roche analyzes what the code needs, picks the right sandbox, and enforces the minimal permissions — so you don't have to configure anything.

Named after [Edouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Features

- **One line** — `run("print(2+2)")` — zero config, auto-selects everything
- **Intent-based permissions** — analyzes code to infer network, filesystem, and resource needs
- **5 providers** — Docker, Firecracker, WASM, E2B, Kubernetes — one API
- **Streaming + real-time control** — live stdout/stderr, resource monitoring, mid-execution kill
- **Sessions** — persistent sandbox state, budget tracking, dynamic permission changes

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

## Integrations

Works with any framework. Examples included for [OpenAI Agents](examples/python/openai-agents/), [LangChain](examples/python/langchain/), [CrewAI](examples/python/crewai/), [Anthropic](examples/python/anthropic/), [AutoGen](examples/python/autogen/), [Camel-AI](examples/python/camel/).

## Architecture

```
┌───────────────────────────────────────────────────────────────┐
│                          Roche                                │
│                                                               │
│  Intent Engine ── Session Manager ── Streaming Monitor        │
│  (auto-detect     (budget, perms,    (real-time control,      │
│   permissions)     dynamic adjust)    mid-exec kill)          │
│                                                               │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │                   Provider Layer                        │  │
│  │  Docker │ Firecracker │ WASM │ E2B │ Kubernetes         │  │
│  └─────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
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
