# roche-sandbox

TypeScript SDK for [Roche](https://github.com/substratum-labs/roche) -- universal sandbox orchestrator for AI agents.

## Requirements

- Node.js >= 18
- Roche CLI on `PATH` (or Roche daemon running)

## Install

```bash
npm install roche-sandbox
```

## Quick Start

```typescript
import { Roche } from "roche-sandbox";

const roche = new Roche();

// Create a sandbox, run a command, clean up
const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
const output = await sandbox.exec(["python3", "-c", "print('Hello!')"]);
console.log(output.stdout); // Hello!
await sandbox.destroy();
```

### Auto-cleanup with `using` (TypeScript >= 5.2)

```typescript
await using sandbox = await roche.createSandbox();
await sandbox.exec(["echo", "hello"]);
// sandbox.destroy() called automatically
```

### Flat Methods

```typescript
const id = await roche.create({ image: "python:3.12-slim" });
const result = await roche.exec(id, ["echo", "hi"]);
await roche.destroy(id);
```

## Configuration

```typescript
const sandbox = await roche.createSandbox({
  image: "python:3.12-slim",
  memory: "512m",
  cpus: 1.0,
  timeout_secs: 600,
  network: false,   // default: AI-safe
  writable: false,  // default: AI-safe
  env: { API_KEY: "secret" },
});
```

## Transport

The SDK auto-detects whether the Roche gRPC daemon is running and connects to it. If the daemon is unavailable, it falls back to invoking the Roche CLI as a subprocess.

## Public Exports

```typescript
import {
  Roche, RocheOptions,
  Sandbox,
  SandboxConfig, ExecOutput, SandboxInfo,
  SandboxStatus, Mount,
  RocheError, SandboxNotFound, SandboxPaused,
  ProviderUnavailable, TimeoutError, UnsupportedOperation,
} from "roche-sandbox";
```

## License

Apache-2.0 -- see [LICENSE](https://github.com/substratum-labs/roche/blob/main/LICENSE).
