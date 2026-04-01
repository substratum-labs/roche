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

# 1. Inline code — zero config
result = run("print(2 + 2)")
print(result.stdout)  # 4

# 2. Single file
result = run(file="script.py")

# 3. Entire project — auto-detects entry point + installs dependencies
result = run(path="./my-project/")

# 4. Network auto-detected from code intent
result = run("""
    import requests
    r = requests.get('https://api.github.com')
    print(r.status_code)
""")
# Roche analyzed the code → network=True, allowlist=["api.github.com"], provider=Docker

# 5. Get files back from sandbox
result = run(file="generate.py", download=["/app/output.csv"])
open("output.csv", "wb").write(result.files["output.csv"])
```

You pass code, a file, or a project. Roche reads it, infers what it needs (network hosts, writable paths, memory), picks the right provider, and executes in a locked-down sandbox. For projects, it auto-detects `requirements.txt` / `package.json` and installs dependencies.

## 💡 How It Works

| What you write | What Roche does |
|:---|:---|
| `run("print(2+2)")` | WASM, no network, no writes, 30s timeout |
| `run("import requests; ...")` | Docker, network=api.github.com only, readonly FS |
| `run(file="train.py")` | Reads file, infers intent, copies into sandbox, executes |
| `run(path="./project/")` | Finds main.py, detects requirements.txt, installs deps, runs |
| `run(..., download=["/app/out.csv"])` | Executes, then copies files back from sandbox |

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
<summary>Parallel execution: run multiple tasks concurrently</summary>

```python
from roche_sandbox import run_parallel

results = run_parallel([
    {"code": "print('task 1')"},
    {"code": "print('task 2')"},
    {"file": "script.py"},
], max_concurrency=5)

print(f"{results.total_succeeded} ok, {results.total_failed} failed")
```

</details>

<details>
<summary>Snapshot & restore: save sandbox state, resume instantly</summary>

```python
from roche_sandbox import Roche, snapshot, restore

roche = Roche()
sandbox = roche.create(writable=True)
sandbox.exec(["pip", "install", "numpy", "pandas"])

# Save state
snap = snapshot(sandbox.id)
sandbox.destroy()

# Later — restore in <1s (no reinstall)
result = restore(snap, ["python3", "-c", "import numpy; print(numpy.__version__)"])
```

</details>

<details>
<summary>Dependency caching: pip/npm cache persists across sandboxes</summary>

```python
from roche_sandbox import run_cached

# First run: installs deps (~30s)
result = run_cached(path="./ml-project/")

# Second run: cache hit (<1s for deps)
result = run_cached(path="./ml-project/")
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

<details>
<summary>Warm pool: pre-warmed sandboxes for instant acquisition</summary>

Configure `~/.roche/pool.toml`:

```toml
[[pool]]
provider = "docker"
image = "python:3.12-slim"
min_idle = 3
max_idle = 10
max_total = 20
idle_timeout_secs = 600
```

Start the daemon — sandboxes are pre-created and ready:

```bash
roched --port 50051
```

```python
from roche_sandbox import Roche

roche = Roche()

# Instant — picks up a pre-warmed sandbox, no container startup
sandbox = roche.create(image="python:3.12-slim")

# Monitor pool health
for pool in roche.pool_status():
    print(f"{pool.image}: {pool.idle_count} idle, {pool.active_count} active")

roche.pool_warmup()   # trigger refill
roche.pool_drain()    # destroy all idle sandboxes
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
