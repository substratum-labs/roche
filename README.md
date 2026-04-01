# Roche

[![CI](https://github.com/substratum-labs/roche/actions/workflows/ci.yml/badge.svg)](https://github.com/substratum-labs/roche/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![PyPI](https://img.shields.io/pypi/v/roche-sandbox)](https://pypi.org/project/roche-sandbox/)
[![npm](https://img.shields.io/npm/v/roche-sandbox)](https://www.npmjs.com/package/roche-sandbox)
[![Go Reference](https://pkg.go.dev/badge/github.com/substratum-labs/roche/sdk/go.svg)](https://pkg.go.dev/github.com/substratum-labs/roche/sdk/go)

> The safest way to let code interact with the world.

Roche analyzes what the code needs, picks the right sandbox, and enforces minimal permissions. You write `run("print(2+2)")`. Roche figures out the rest.

It's not a Docker wrapper. It's an execution engine that reads code intent, selects from five providers (Docker, Firecracker, WASM, E2B, Kubernetes), and opens only what's needed — network hosts, filesystem paths, memory. Everything else stays locked.

Named after [Edouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

---

## 🚀 Quick Start

```bash
pip install roche-sandbox
```

```python
from roche_sandbox import run

# 1. Zero config — auto-selects provider, permissions, everything
result = run("print(2 + 2)")
print(result.stdout)  # 4

# 2. Network auto-detected from code intent
result = run("""
    import requests
    r = requests.get('https://api.github.com')
    print(r.status_code)
""")
# Roche analyzed the code → network=True, allowlist=["api.github.com"], provider=Docker

# 3. Pure compute auto-routes to WASM (sub-ms startup)
result = run("print(sum(range(1000)))")
# No network, no writes → provider=WASM
```

You pass code. Roche reads it, infers that `import requests` needs network access to `api.github.com`, enables just that host, picks Docker (because WASM can't do network), executes in a locked-down container, and returns the result with an execution trace. If the code were pure math, it would route to WASM instead — no container overhead.

## 💡 How It Works

| What you write | What Roche does |
|:---|:---|
| `run("print(2+2)")` | WASM, no network, no writes, 30s timeout |
| `run("import requests; ...")` | Docker, network=api.github.com only, readonly FS |
| `run("import pandas; df.to_csv('/tmp/out.csv')")` | Docker, writable=/tmp, memory=512m |
| `run("curl https://example.com")` | Docker, network=example.com, bash |

Five providers, one API. The intent engine handles selection. Override anything explicitly when you need to.

---

## 🔧 Also Works As

<details>
<summary>CLI: create, exec, destroy from the terminal</summary>

```bash
cargo install roche-cli

SANDBOX_ID=$(roche create --provider docker --memory 512m)
roche exec --sandbox $SANDBOX_ID -- python3 -c "print('Hello!')"
roche destroy $SANDBOX_ID
```

</details>

<details>
<summary>Python SDK: full sandbox lifecycle control</summary>

```python
from roche_sandbox import Roche

roche = Roche()
with roche.create(image="python:3.12-slim", network=True) as sandbox:
    sandbox.exec(["pip", "install", "requests"])
    result = sandbox.exec(["python3", "-c", "import requests; print(requests.get('https://httpbin.org/ip').text)"])
    print(result.stdout)
```

</details>

<details>
<summary>TypeScript SDK</summary>

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

</details>

<details>
<summary>Go SDK</summary>

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

</details>

<details>
<summary>Streaming exec with real-time output</summary>

```python
from roche_sandbox import AsyncRoche

roche = AsyncRoche()
sandbox = await roche.create(image="python:3.12-slim")
async for event in sandbox.exec_stream(["python3", "-c", "import time\nfor i in range(5): print(i); time.sleep(1)"]):
    if event.type == "output":
        print(event.data.decode(), end="")
    elif event.type == "heartbeat":
        print(f"  [{event.elapsed_ms}ms, {event.memory_bytes} bytes]")
await sandbox.destroy()
```

</details>

<details>
<summary>Sessions: persistent state across multiple exec calls</summary>

```python
roche = AsyncRoche()
sandbox = await roche.create(writable=True)
session_id = await roche.create_session(sandbox.id, budget=Budget(max_execs=100))

# First exec — write a file
await roche.exec(sandbox.id, ["python3", "-c", "open('/tmp/state.txt','w').write('hello')"])

# Second exec — read it back (same sandbox, state persists)
result = await roche.exec(sandbox.id, ["python3", "-c", "print(open('/tmp/state.txt').read())"])
print(result.stdout)  # hello
```

</details>

Works with any framework. Integration examples for [OpenAI Agents](examples/python/openai-agents/), [LangChain](examples/python/langchain/), [CrewAI](examples/python/crewai/), [Anthropic](examples/python/anthropic/), [AutoGen](examples/python/autogen/), [Camel-AI](examples/python/camel/).

## 🔒 Security Defaults

| Setting | Default | Why |
|:--------|:--------|:----|
| Network | **off** | Prevent exfiltration |
| Filesystem | **readonly** | Prevent tampering |
| Timeout | **300s** | Prevent runaway processes |
| PIDs | **256** | Prevent fork bombs |
| Privileges | **no-new-privileges** | Prevent escalation |

Everything is off by default. Roche's intent engine turns on only what the code needs. Override explicitly with `--network`, `--writable`, or SDK parameters.

## 🛠️ Development

```bash
cargo build && cargo test && cargo clippy && cargo fmt --check    # Rust
pip install -e "sdk/python[dev]" && pytest sdk/python/tests/ -v   # Python
cd sdk/typescript && npm ci && npm test                           # TypeScript
cd sdk/go && go test ./... -v                                     # Go
```

## 📄 License

Apache 2.0
