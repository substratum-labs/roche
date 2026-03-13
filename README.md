# Roche

Universal sandbox orchestrator for AI agents.

Roche provides a single abstraction (`create` / `exec` / `destroy`) over multiple sandbox providers with **AI-optimized security defaults** — network disabled, filesystem readonly, timeout enforced.

> Named after [Édouard Roche](https://en.wikipedia.org/wiki/%C3%89douard_Roche) — the Roche limit is the inviolable physical boundary for celestial bodies; Roche is the inviolable execution boundary for code.

## Why Roche?

Every agent framework independently integrates sandbox providers, creating N×M complexity:

```
LangChain ──┐         ┌── Docker
CrewAI   ───┤  N × M  ├── E2B
AutoGen  ───┘         └── Modal
```

Roche reduces this to N + M:

```
LangChain ──┐              ┌── Docker
CrewAI   ───┤── Roche() ───├── Firecracker
AutoGen  ───┘              └── WASM
```

## Quick Start

```bash
# Create a sandbox (network off, readonly FS by default)
roche create --provider docker --memory 512m

# Execute code
roche exec --sandbox <id> python3 -c "print('hello')"

# Destroy
roche destroy <id>

# List active sandboxes
roche list
```

## Security Defaults

Roche is designed for AI agent workloads where untrusted code execution is the norm:

| Setting | Default | Why |
|---------|---------|-----|
| Network | **disabled** | Prevent data exfiltration |
| Filesystem | **readonly** | Prevent persistent compromise |
| Timeout | **300s** | Prevent runaway processes |

Override explicitly when needed: `--network`, `--writable`.

## MVP Scope

- **Providers:** Docker (via CLI)
- **CLI:** `roche create / exec / destroy / list`
- **SDKs:** Python (`roche-python`)
- **Defaults:** No network, readonly FS, 300s timeout

## Roadmap

- [ ] Docker provider implementation
- [ ] Python SDK
- [ ] gRPC daemon mode
- [ ] Firecracker provider
- [ ] WASM provider (wasmtime)
- [ ] TypeScript SDK
- [ ] Sandbox pool (warm-start)

## License

Apache-2.0
