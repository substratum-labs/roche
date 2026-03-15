# Roche Examples

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed and running
- `roche` CLI on PATH (`cargo install --path crates/roche-cli`)

## Python

```bash
pip install -e sdk/python
python examples/python/basic.py
python examples/python/async_context_manager.py
```

## TypeScript

```bash
cd sdk/typescript && npm ci && npm run build && cd ../..
npx tsx examples/typescript/basic.ts
```
