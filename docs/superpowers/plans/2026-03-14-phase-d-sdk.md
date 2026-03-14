# Phase D: TypeScript & Python SDKs — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build ergonomic, type-safe TypeScript and Python SDKs (`roche-sandbox`) that let AI agent frameworks manage Roche sandboxes programmatically, with dual-mode transport (gRPC to daemon, CLI subprocess fallback).

**Architecture:** Both SDKs share the same transport abstraction: detect daemon at construction time → use gRPC if available, CLI subprocess otherwise. `Roche` client creates `Sandbox` objects that encapsulate sandbox_id + provider. Proto codegen from `sandbox.proto` provides type-safe gRPC stubs.

**Tech Stack:** TypeScript (Node.js >= 18, TS >= 5.2), `@grpc/grpc-js`, `ts-proto` | Python (>= 3.10), `grpcio`, `protobuf`, `grpcio-tools`

**Spec:** `docs/superpowers/specs/2026-03-14-phase-d-sdk-design.md`

---

## Chunk 1: TypeScript SDK — Project Setup & Proto Codegen

### Task 1: Scaffold TypeScript project

**Files:**
- Create: `sdk/typescript/package.json`
- Create: `sdk/typescript/tsconfig.json`
- Create: `sdk/typescript/.gitignore`

- [ ] **Step 1: Create package.json**

```json
{
  "name": "roche-sandbox",
  "version": "0.1.0",
  "description": "Universal sandbox orchestrator for AI agents — TypeScript SDK",
  "license": "Apache-2.0",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": ["dist"],
  "scripts": {
    "build": "tsc",
    "test": "vitest run",
    "test:watch": "vitest",
    "proto:gen": "bash scripts/proto-gen.sh",
    "prepublishOnly": "npm run build"
  },
  "engines": {
    "node": ">=18"
  },
  "dependencies": {
    "@grpc/grpc-js": "^1.10.0"
  },
  "devDependencies": {
    "ts-proto": "^1.176.0",
    "typescript": "^5.4.0",
    "vitest": "^1.6.0",
    "@types/node": "^20.0.0"
  }
}
```

- [ ] **Step 2: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "Node16",
    "moduleResolution": "Node16",
    "lib": ["ES2022"],
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "skipLibCheck": true
  },
  "include": ["src"],
  "exclude": ["node_modules", "dist", "test"]
}
```

- [ ] **Step 3: Create .gitignore**

```
node_modules/
dist/
src/generated/
```

- [ ] **Step 4: Install dependencies**

Run: `cd sdk/typescript && npm install`
Expected: `node_modules/` created, `package-lock.json` generated

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/package.json sdk/typescript/tsconfig.json sdk/typescript/.gitignore sdk/typescript/package-lock.json
git commit -m "feat(sdk-ts): scaffold TypeScript SDK project"
```

### Task 2: Proto codegen script and generated types

**Files:**
- Create: `sdk/typescript/scripts/proto-gen.sh`

- [ ] **Step 1: Create proto-gen.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

# Requires: protoc installed on the system (e.g., brew install protobuf)
PROTO_DIR="$(cd "$(dirname "$0")/../../.." && pwd)/proto"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/src/generated"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

protoc \
  --plugin=protoc-gen-ts_proto=./node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out="$OUT_DIR" \
  --ts_proto_opt=outputServices=grpc-js \
  --ts_proto_opt=esModuleInterop=true \
  --ts_proto_opt=snakeToCamel=true \
  --ts_proto_opt=forceLong=number \
  -I "$PROTO_DIR" \
  "$PROTO_DIR/roche/v1/sandbox.proto"
```

- [ ] **Step 2: Make executable and run codegen**

Run: `chmod +x sdk/typescript/scripts/proto-gen.sh && cd sdk/typescript && npm run proto:gen`
Expected: `src/generated/roche/v1/sandbox.ts` generated with typed client stubs and message interfaces

- [ ] **Step 3: Verify generated types exist**

Run: `ls sdk/typescript/src/generated/roche/v1/sandbox.ts`
Expected: File exists

- [ ] **Step 4: Commit**

```bash
git add sdk/typescript/scripts/proto-gen.sh
git commit -m "feat(sdk-ts): add proto codegen script"
```

---

## Chunk 2: TypeScript SDK — Types, Errors, Transport Interface

### Task 3: Define SDK types

**Files:**
- Create: `sdk/typescript/src/types.ts`
- Test: `sdk/typescript/test/types.test.ts`

- [ ] **Step 1: Write types test**

```typescript
import { describe, it, expect } from "vitest";
import type { SandboxConfig, ExecOutput, SandboxInfo, Mount } from "../src/types";

describe("types", () => {
  it("SandboxConfig has correct defaults", () => {
    const config: SandboxConfig = {};
    // Type-only check — defaults are applied at usage site, not in the type
    expect(config).toBeDefined();
  });

  it("ExecOutput shape", () => {
    const output: ExecOutput = { exitCode: 0, stdout: "hi", stderr: "" };
    expect(output.exitCode).toBe(0);
  });

  it("Mount shape", () => {
    const mount: Mount = { hostPath: "/a", containerPath: "/b" };
    expect(mount.hostPath).toBe("/a");
    expect(mount.readonly).toBeUndefined();
  });

  it("SandboxInfo shape", () => {
    const info: SandboxInfo = {
      id: "abc",
      status: "running",
      provider: "docker",
      image: "python:3.12-slim",
    };
    expect(info.status).toBe("running");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/types.test.ts`
Expected: FAIL — `../src/types` module not found

- [ ] **Step 3: Write types.ts**

```typescript
export type SandboxStatus = "running" | "paused" | "stopped" | "failed";

export interface SandboxConfig {
  provider?: string;
  image?: string;
  memory?: string;
  cpus?: number;
  timeoutSecs?: number;
  network?: boolean;
  writable?: boolean;
  env?: Record<string, string>;
  mounts?: Mount[];
  kernel?: string;
  rootfs?: string;
}

export interface Mount {
  hostPath: string;
  containerPath: string;
  readonly?: boolean;
}

export interface ExecOutput {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface SandboxInfo {
  id: string;
  status: SandboxStatus;
  provider: string;
  image: string;
  expiresAt?: number;
}

export const DEFAULTS = {
  provider: "docker",
  image: "python:3.12-slim",
  timeoutSecs: 300,
  network: false,
  writable: false,
} as const;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/types.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/types.ts sdk/typescript/test/types.test.ts
git commit -m "feat(sdk-ts): add SDK type definitions"
```

### Task 4: Define error hierarchy

**Files:**
- Create: `sdk/typescript/src/errors.ts`
- Test: `sdk/typescript/test/errors.test.ts`

- [ ] **Step 1: Write errors test**

```typescript
import { describe, it, expect } from "vitest";
import {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "../src/errors";

describe("errors", () => {
  it("SandboxNotFound is instanceof RocheError", () => {
    const err = new SandboxNotFound("abc123");
    expect(err).toBeInstanceOf(RocheError);
    expect(err).toBeInstanceOf(SandboxNotFound);
    expect(err.message).toContain("abc123");
  });

  it("SandboxPaused is instanceof RocheError", () => {
    const err = new SandboxPaused("abc123");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("ProviderUnavailable is instanceof RocheError", () => {
    const err = new ProviderUnavailable("daemon down");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("TimeoutError is instanceof RocheError", () => {
    const err = new TimeoutError("30s");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("UnsupportedOperation is instanceof RocheError", () => {
    const err = new UnsupportedOperation("pause");
    expect(err).toBeInstanceOf(RocheError);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/errors.test.ts`
Expected: FAIL — module not found

- [ ] **Step 3: Write errors.ts**

```typescript
export class RocheError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RocheError";
  }
}

export class SandboxNotFound extends RocheError {
  constructor(sandboxId: string) {
    super(`Sandbox not found: ${sandboxId}`);
    this.name = "SandboxNotFound";
  }
}

export class SandboxPaused extends RocheError {
  constructor(sandboxId: string) {
    super(`Sandbox is paused: ${sandboxId}`);
    this.name = "SandboxPaused";
  }
}

export class ProviderUnavailable extends RocheError {
  constructor(detail: string) {
    super(`Provider unavailable: ${detail}`);
    this.name = "ProviderUnavailable";
  }
}

export class TimeoutError extends RocheError {
  constructor(detail: string) {
    super(`Operation timed out: ${detail}`);
    this.name = "TimeoutError";
  }
}

export class UnsupportedOperation extends RocheError {
  constructor(operation: string) {
    super(`Unsupported operation: ${operation}`);
    this.name = "UnsupportedOperation";
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/errors.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/errors.ts sdk/typescript/test/errors.test.ts
git commit -m "feat(sdk-ts): add error hierarchy"
```

### Task 5: Define Transport interface

**Files:**
- Create: `sdk/typescript/src/transport/index.ts`

- [ ] **Step 1: Write Transport interface**

```typescript
import type { SandboxConfig, ExecOutput, SandboxInfo } from "../types";

export interface Transport {
  create(config: SandboxConfig, provider: string): Promise<string>;
  exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number
  ): Promise<ExecOutput>;
  destroy(
    sandboxIds: string[],
    provider: string,
    all?: boolean
  ): Promise<string[]>;
  list(provider: string): Promise<SandboxInfo[]>;
  pause(sandboxId: string, provider: string): Promise<void>;
  unpause(sandboxId: string, provider: string): Promise<void>;
  gc(
    provider: string,
    dryRun?: boolean,
    all?: boolean
  ): Promise<string[]>;
  copyTo(
    sandboxId: string,
    hostPath: string,
    sandboxPath: string,
    provider: string
  ): Promise<void>;
  copyFrom(
    sandboxId: string,
    sandboxPath: string,
    hostPath: string,
    provider: string
  ): Promise<void>;
}
```

- [ ] **Step 2: Commit**

```bash
git add sdk/typescript/src/transport/index.ts
git commit -m "feat(sdk-ts): add Transport interface"
```

---

## Chunk 3: TypeScript SDK — CLI Transport

### Task 6: Implement CLI transport

**Files:**
- Create: `sdk/typescript/src/transport/cli.ts`
- Test: `sdk/typescript/test/transport/cli.test.ts`

- [ ] **Step 1: Write CLI transport test — create**

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { CliTransport } from "../../src/transport/cli";
import { execFile } from "child_process";
import { RocheError, SandboxNotFound, ProviderUnavailable } from "../../src/errors";

// Mock child_process.execFile
vi.mock("child_process", () => ({
  execFile: vi.fn(),
}));

const mockExecFile = vi.mocked(execFile);

function mockSuccess(stdout: string, stderr = "") {
  mockExecFile.mockImplementation(
    (_file: any, _args: any, _opts: any, cb: any) => {
      cb(null, stdout, stderr);
      return {} as any;
    }
  );
}

function mockError(code: number, stderr: string) {
  mockExecFile.mockImplementation(
    (_file: any, _args: any, _opts: any, cb: any) => {
      const err = Object.assign(new Error("exit " + code), {
        code,
        stdout: "",
        stderr,
      });
      cb(err, "", stderr);
      return {} as any;
    }
  );
}

describe("CliTransport", () => {
  let transport: CliTransport;

  beforeEach(() => {
    transport = new CliTransport("roche");
    vi.clearAllMocks();
  });

  it("create builds correct args and returns sandbox ID", async () => {
    mockSuccess("abc123def456\n");
    const id = await transport.create(
      { image: "python:3.12-slim", network: true },
      "docker"
    );
    expect(id).toBe("abc123def456");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("create");
    expect(args).toContain("--provider");
    expect(args).toContain("docker");
    expect(args).toContain("--image");
    expect(args).toContain("python:3.12-slim");
    expect(args).toContain("--network");
  });

  it("create uses defaults for missing config fields", async () => {
    mockSuccess("id1\n");
    await transport.create({}, "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("python:3.12-slim");
    expect(args).toContain("300");
    expect(args).not.toContain("--network");
    expect(args).not.toContain("--writable");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/transport/cli.test.ts`
Expected: FAIL — module not found

- [ ] **Step 3: Write cli.ts — create method**

```typescript
import { execFile as execFileCb } from "child_process";
import { promisify } from "util";
import type { Transport } from "./index";
import type { SandboxConfig, ExecOutput, SandboxInfo } from "../types";
import { DEFAULTS } from "../types";
import {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "../errors";

const execFile = promisify(execFileCb);

export class CliTransport implements Transport {
  constructor(private readonly binary: string = "roche") {}

  async create(config: SandboxConfig, provider: string): Promise<string> {
    const args = [
      "create",
      "--provider", provider,
      "--image", config.image ?? DEFAULTS.image,
      "--timeout", String(config.timeoutSecs ?? DEFAULTS.timeoutSecs),
    ];
    if (config.memory) args.push("--memory", config.memory);
    if (config.cpus != null) args.push("--cpus", String(config.cpus));
    if (config.network) args.push("--network");
    if (config.writable) args.push("--writable");
    if (config.env) {
      for (const [k, v] of Object.entries(config.env)) {
        args.push("--env", `${k}=${v}`);
      }
    }
    if (config.mounts) {
      for (const m of config.mounts) {
        const mode = m.readonly !== false ? "ro" : "rw";
        args.push("--mount", `${m.hostPath}:${m.containerPath}:${mode}`);
      }
    }
    if (config.kernel) args.push("--kernel", config.kernel);
    if (config.rootfs) args.push("--rootfs", config.rootfs);

    const { stdout } = await this.run(args);
    return stdout.trim();
  }

  async exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
  ): Promise<ExecOutput> {
    const args = ["exec", "--sandbox", sandboxId];
    if (timeoutSecs != null) args.push("--timeout", String(timeoutSecs));
    args.push("--", ...command);

    try {
      const { stdout, stderr } = await this.run(args, false);
      return { exitCode: 0, stdout, stderr };
    } catch (err: any) {
      if (err.stderr && this.isRocheError(err.stderr)) {
        throw this.mapCliError(err.stderr);
      }
      return {
        exitCode: err.code ?? 1,
        stdout: err.stdout ?? "",
        stderr: err.stderr ?? "",
      };
    }
  }

  async destroy(
    sandboxIds: string[],
    provider: string,
    all?: boolean,
  ): Promise<string[]> {
    const args = ["destroy"];
    if (all) {
      args.push("--all");
    } else {
      args.push(...sandboxIds);
    }
    const { stdout } = await this.run(args);
    return stdout.trim().split("\n").filter(Boolean);
  }

  async list(provider: string): Promise<SandboxInfo[]> {
    const { stdout } = await this.run(["list", "--json"]);
    const raw = JSON.parse(stdout) as Array<{
      id: string;
      status: string;
      provider: string;
      image: string;
      expires_at?: number;
    }>;
    return raw.map((s) => ({
      id: s.id,
      status: s.status as SandboxInfo["status"],
      provider: s.provider,
      image: s.image,
      expiresAt: s.expires_at,
    }));
  }

  async pause(sandboxId: string, provider: string): Promise<void> {
    await this.run(["pause", sandboxId]);
  }

  async unpause(sandboxId: string, provider: string): Promise<void> {
    await this.run(["unpause", sandboxId]);
  }

  async gc(
    provider: string,
    dryRun?: boolean,
    all?: boolean,
  ): Promise<string[]> {
    const args = ["gc"];
    if (dryRun) args.push("--dry-run");
    if (all) args.push("--all");
    const { stdout } = await this.run(args);
    return stdout.trim().split("\n").filter(Boolean);
  }

  async copyTo(
    sandboxId: string,
    hostPath: string,
    sandboxPath: string,
    provider: string,
  ): Promise<void> {
    await this.run(["cp", hostPath, `${sandboxId}:${sandboxPath}`]);
  }

  async copyFrom(
    sandboxId: string,
    sandboxPath: string,
    hostPath: string,
    provider: string,
  ): Promise<void> {
    await this.run(["cp", `${sandboxId}:${sandboxPath}`, hostPath]);
  }

  private async run(
    args: string[],
    check = true,
  ): Promise<{ stdout: string; stderr: string }> {
    try {
      const result = await execFile(this.binary, args);
      return { stdout: result.stdout ?? "", stderr: result.stderr ?? "" };
    } catch (err: any) {
      if (err.code === "ENOENT") {
        throw new ProviderUnavailable(
          `Roche binary not found: ${this.binary}`
        );
      }
      if (check) {
        throw this.mapCliError(err.stderr ?? err.message);
      }
      throw err;
    }
  }

  private isRocheError(stderr: string): boolean {
    return stderr.trimStart().startsWith("Error: ");
  }

  private mapCliError(stderr: string): RocheError {
    const lower = stderr.toLowerCase();
    if (lower.includes("not found")) return new SandboxNotFound(stderr);
    if (lower.includes("paused")) return new SandboxPaused(stderr);
    if (lower.includes("timeout")) return new TimeoutError(stderr);
    if (lower.includes("unsupported")) return new UnsupportedOperation(stderr);
    if (lower.includes("unavailable") || lower.includes("connection refused"))
      return new ProviderUnavailable(stderr);
    return new RocheError(stderr);
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/transport/cli.test.ts`
Expected: PASS

- [ ] **Step 5: Add remaining CLI transport tests**

Add more tests to `test/transport/cli.test.ts`:

```typescript
  it("exec returns ExecOutput for successful commands", async () => {
    mockSuccess("hello\n", "");
    const output = await transport.exec("abc", ["echo", "hello"], "docker");
    expect(output.exitCode).toBe(0);
    expect(output.stdout).toBe("hello\n");
  });

  it("exec returns non-zero exit for command failures", async () => {
    mockError(1, "command failed");
    const output = await transport.exec("abc", ["false"], "docker");
    expect(output.exitCode).toBe(1);
  });

  it("exec throws SandboxNotFound when stderr contains 'not found'", async () => {
    mockError(1, "Error: sandbox not found");
    await expect(
      transport.exec("abc", ["echo"], "docker")
    ).rejects.toBeInstanceOf(SandboxNotFound);
  });

  it("destroy calls with sandbox IDs", async () => {
    mockSuccess("abc\ndef\n");
    const destroyed = await transport.destroy(["abc", "def"], "docker");
    expect(destroyed).toEqual(["abc", "def"]);
  });

  it("list parses JSON output", async () => {
    mockSuccess(
      JSON.stringify([
        { id: "abc", status: "running", provider: "docker", image: "python:3.12-slim" },
      ])
    );
    const sandboxes = await transport.list("docker");
    expect(sandboxes).toHaveLength(1);
    expect(sandboxes[0].id).toBe("abc");
    expect(sandboxes[0].status).toBe("running");
  });

  it("pause sends correct args", async () => {
    mockSuccess("");
    await transport.pause("abc", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("pause");
    expect(args).toContain("abc");
  });

  it("copyTo maps to roche cp syntax", async () => {
    mockSuccess("");
    await transport.copyTo("abc", "/local/f.py", "/sandbox/f.py", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("cp");
    expect(args).toContain("/local/f.py");
    expect(args).toContain("abc:/sandbox/f.py");
  });

  it("copyFrom maps to roche cp syntax", async () => {
    mockSuccess("");
    await transport.copyFrom("abc", "/sandbox/out.txt", "/local/out.txt", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("cp");
    expect(args).toContain("abc:/sandbox/out.txt");
    expect(args).toContain("/local/out.txt");
  });

  it("unpause sends correct args", async () => {
    mockSuccess("");
    await transport.unpause("abc", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("unpause");
    expect(args).toContain("abc");
  });

  it("gc sends correct args with flags", async () => {
    mockSuccess("old1\nold2\n");
    const destroyed = await transport.gc("docker", true, true);
    expect(destroyed).toEqual(["old1", "old2"]);
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("gc");
    expect(args).toContain("--dry-run");
    expect(args).toContain("--all");
  });

  it("destroy with --all flag", async () => {
    mockSuccess("abc\ndef\n");
    await transport.destroy([], "docker", true);
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("destroy");
    expect(args).toContain("--all");
  });

  it("throws ProviderUnavailable when binary not found", async () => {
    mockExecFile.mockImplementation(
      (_file: any, _args: any, _opts: any, cb: any) => {
        const err = Object.assign(new Error("ENOENT"), { code: "ENOENT" });
        cb(err, "", "");
        return {} as any;
      }
    );
    await expect(
      transport.create({}, "docker")
    ).rejects.toBeInstanceOf(ProviderUnavailable);
  });
```

- [ ] **Step 6: Run all CLI transport tests**

Run: `cd sdk/typescript && npx vitest run test/transport/cli.test.ts`
Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add sdk/typescript/src/transport/cli.ts sdk/typescript/test/transport/cli.test.ts
git commit -m "feat(sdk-ts): implement CLI transport with tests"
```

---

## Chunk 4: TypeScript SDK — gRPC Transport & Daemon Detection

### Task 7: Implement daemon detection

**Files:**
- Create: `sdk/typescript/src/daemon.ts`
- Test: `sdk/typescript/test/daemon.test.ts`

- [ ] **Step 1: Write daemon detection test**

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { detectDaemon, type DaemonInfo } from "../src/daemon";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

vi.mock("fs");

describe("detectDaemon", () => {
  const daemonJsonPath = path.join(os.homedir(), ".roche", "daemon.json");

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns null when daemon.json does not exist", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(false);
    const result = await detectDaemon();
    expect(result).toBeNull();
  });

  it("returns null when daemon.json is malformed", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue("not json");
    const result = await detectDaemon();
    expect(result).toBeNull();
  });

  it("returns DaemonInfo when daemon.json is valid and pid is alive", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ pid: process.pid, port: 50051 })
    );
    const result = await detectDaemon();
    expect(result).toEqual({ pid: process.pid, port: 50051 });
  });

  it("returns null when pid is not alive", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ pid: 999999999, port: 50051 })
    );
    const result = await detectDaemon();
    expect(result).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/daemon.test.ts`
Expected: FAIL

- [ ] **Step 3: Write daemon.ts**

```typescript
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

export interface DaemonInfo {
  pid: number;
  port: number;
}

export async function detectDaemon(): Promise<DaemonInfo | null> {
  const daemonPath = path.join(os.homedir(), ".roche", "daemon.json");

  if (!fs.existsSync(daemonPath)) return null;

  let data: { pid?: number; port?: number };
  try {
    data = JSON.parse(fs.readFileSync(daemonPath, "utf-8"));
  } catch {
    return null;
  }

  if (typeof data.pid !== "number" || typeof data.port !== "number") {
    return null;
  }

  if (!isProcessAlive(data.pid)) return null;

  return { pid: data.pid, port: data.port };
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/daemon.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/daemon.ts sdk/typescript/test/daemon.test.ts
git commit -m "feat(sdk-ts): add daemon detection"
```

### Task 8: Implement gRPC transport

**Files:**
- Create: `sdk/typescript/src/transport/grpc.ts`
- Test: `sdk/typescript/test/transport/grpc.test.ts`

- [ ] **Step 1: Write gRPC transport test**

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { GrpcTransport } from "../../src/transport/grpc";
import {
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
  RocheError,
} from "../../src/errors";

// We test error mapping and argument conversion.
// The actual gRPC client is mocked.
describe("GrpcTransport", () => {
  let transport: GrpcTransport;
  let mockClient: any;

  beforeEach(() => {
    mockClient = {
      create: vi.fn(),
      exec: vi.fn(),
      destroy: vi.fn(),
      list: vi.fn(),
      pause: vi.fn(),
      unpause: vi.fn(),
      gc: vi.fn(),
      copyTo: vi.fn(),
      copyFrom: vi.fn(),
    };
    transport = new GrpcTransport(50051, mockClient);
  });

  it("create sends correct request and returns sandbox ID", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) => cb(null, { sandboxId: "abc123" })
    );
    const id = await transport.create({ image: "node:20" }, "docker");
    expect(id).toBe("abc123");
    expect(mockClient.create).toHaveBeenCalledWith(
      expect.objectContaining({
        provider: "docker",
        image: "node:20",
      }),
      expect.any(Function),
    );
  });

  it("exec returns ExecOutput", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) =>
        cb(null, { exitCode: 0, stdout: "hi", stderr: "" })
    );
    const output = await transport.exec("abc", ["echo", "hi"], "docker");
    expect(output).toEqual({ exitCode: 0, stdout: "hi", stderr: "" });
  });

  it("maps NOT_FOUND to SandboxNotFound", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 5, details: "not found" }, null)
    );
    await expect(
      transport.create({}, "docker")
    ).rejects.toBeInstanceOf(SandboxNotFound);
  });

  it("maps FAILED_PRECONDITION to SandboxPaused", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 9, details: "paused" }, null)
    );
    await expect(
      transport.exec("abc", ["echo"], "docker")
    ).rejects.toBeInstanceOf(SandboxPaused);
  });

  it("maps UNAVAILABLE to ProviderUnavailable", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 14, details: "unavailable" }, null)
    );
    await expect(
      transport.exec("abc", ["echo"], "docker")
    ).rejects.toBeInstanceOf(ProviderUnavailable);
  });

  it("maps DEADLINE_EXCEEDED to TimeoutError", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 4, details: "deadline" }, null)
    );
    await expect(
      transport.exec("abc", ["echo"], "docker")
    ).rejects.toBeInstanceOf(TimeoutError);
  });

  it("maps UNIMPLEMENTED to UnsupportedOperation", async () => {
    mockClient.pause.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 12, details: "unimplemented" }, null)
    );
    await expect(
      transport.pause("abc", "docker")
    ).rejects.toBeInstanceOf(UnsupportedOperation);
  });

  it("maps other errors to RocheError", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) =>
        cb({ code: 13, details: "internal" }, null)
    );
    await expect(
      transport.create({}, "docker")
    ).rejects.toBeInstanceOf(RocheError);
  });

  it("list returns SandboxInfo array", async () => {
    mockClient.list.mockImplementation(
      (req: any, cb: any) =>
        cb(null, {
          sandboxes: [
            { id: "abc", status: 1, provider: "docker", image: "python:3.12-slim" },
          ],
        })
    );
    const list = await transport.list("docker");
    expect(list).toHaveLength(1);
    expect(list[0].status).toBe("running");
  });

  it("destroy sends IDs and returns destroyed list", async () => {
    mockClient.destroy.mockImplementation(
      (req: any, cb: any) =>
        cb(null, { destroyedIds: ["abc", "def"] })
    );
    const destroyed = await transport.destroy(["abc", "def"], "docker");
    expect(destroyed).toEqual(["abc", "def"]);
  });

  it("gc sends flags and returns destroyed list", async () => {
    mockClient.gc.mockImplementation(
      (req: any, cb: any) =>
        cb(null, { destroyedIds: ["old1"] })
    );
    const destroyed = await transport.gc("docker", true, false);
    expect(destroyed).toEqual(["old1"]);
  });

  it("copyTo sends correct request", async () => {
    mockClient.copyTo.mockImplementation(
      (req: any, cb: any) => cb(null, {})
    );
    await transport.copyTo("abc", "/local/f.py", "/sandbox/f.py", "docker");
    expect(mockClient.copyTo).toHaveBeenCalledWith(
      expect.objectContaining({
        sandboxId: "abc",
        hostPath: "/local/f.py",
        sandboxPath: "/sandbox/f.py",
      }),
      expect.any(Function),
    );
  });

  it("copyFrom sends correct request", async () => {
    mockClient.copyFrom.mockImplementation(
      (req: any, cb: any) => cb(null, {})
    );
    await transport.copyFrom("abc", "/sandbox/out.txt", "/local/out.txt", "docker");
    expect(mockClient.copyFrom).toHaveBeenCalledWith(
      expect.objectContaining({
        sandboxId: "abc",
        sandboxPath: "/sandbox/out.txt",
        hostPath: "/local/out.txt",
      }),
      expect.any(Function),
    );
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/transport/grpc.test.ts`
Expected: FAIL

- [ ] **Step 3: Write grpc.ts**

```typescript
import * as grpc from "@grpc/grpc-js";
import type { Transport } from "./index";
import type { SandboxConfig, ExecOutput, SandboxInfo, SandboxStatus } from "../types";
import { DEFAULTS } from "../types";
import {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "../errors";

// gRPC status codes
const Status = {
  NOT_FOUND: 5,
  DEADLINE_EXCEEDED: 4,
  FAILED_PRECONDITION: 9,
  UNIMPLEMENTED: 12,
  UNAVAILABLE: 14,
};

// Proto SandboxStatus enum values
const PROTO_STATUS_MAP: Record<number, SandboxStatus> = {
  1: "running",
  2: "paused",
  3: "stopped",
  4: "failed",
};

export class GrpcTransport implements Transport {
  private client: any;

  constructor(
    private readonly port: number,
    injectedClient?: any,
  ) {
    this.client = injectedClient ?? null;
  }

  private async getClient(): Promise<any> {
    if (!this.client) {
      // Lazy initialization — load generated stubs via dynamic import
      const { SandboxServiceClient } = await import("../generated/roche/v1/sandbox");
      this.client = new SandboxServiceClient(
        `127.0.0.1:${this.port}`,
        grpc.credentials.createInsecure(),
      );
    }
    return this.client;
  }

  async create(config: SandboxConfig, provider: string): Promise<string> {
    const request = {
      provider,
      image: config.image ?? DEFAULTS.image,
      memory: config.memory,
      cpus: config.cpus,
      timeoutSecs: config.timeoutSecs ?? DEFAULTS.timeoutSecs,
      network: config.network ?? DEFAULTS.network,
      writable: config.writable ?? DEFAULTS.writable,
      env: config.env ?? {},
      mounts: (config.mounts ?? []).map((m) => ({
        hostPath: m.hostPath,
        containerPath: m.containerPath,
        readonly: m.readonly !== false,
      })),
      kernel: config.kernel,
      rootfs: config.rootfs,
    };
    const response = await this.call("create", request);
    return response.sandboxId;
  }

  async exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
  ): Promise<ExecOutput> {
    const response = await this.call("exec", {
      sandboxId,
      command,
      provider,
      timeoutSecs,
    });
    return {
      exitCode: response.exitCode,
      stdout: response.stdout,
      stderr: response.stderr,
    };
  }

  async destroy(
    sandboxIds: string[],
    provider: string,
    all?: boolean,
  ): Promise<string[]> {
    const response = await this.call("destroy", {
      sandboxIds,
      all: all ?? false,
      provider,
    });
    return response.destroyedIds ?? [];
  }

  async list(provider: string): Promise<SandboxInfo[]> {
    const response = await this.call("list", { provider });
    return (response.sandboxes ?? []).map((s: any) => ({
      id: s.id,
      status: PROTO_STATUS_MAP[s.status] ?? "failed",
      provider: s.provider,
      image: s.image,
      expiresAt: s.expiresAt,
    }));
  }

  async pause(sandboxId: string, provider: string): Promise<void> {
    await this.call("pause", { sandboxId, provider });
  }

  async unpause(sandboxId: string, provider: string): Promise<void> {
    await this.call("unpause", { sandboxId, provider });
  }

  async gc(
    provider: string,
    dryRun?: boolean,
    all?: boolean,
  ): Promise<string[]> {
    const response = await this.call("gc", {
      dryRun: dryRun ?? false,
      all: all ?? false,
      provider,
    });
    return response.destroyedIds ?? [];
  }

  async copyTo(
    sandboxId: string,
    hostPath: string,
    sandboxPath: string,
    provider: string,
  ): Promise<void> {
    await this.call("copyTo", { sandboxId, hostPath, sandboxPath, provider });
  }

  async copyFrom(
    sandboxId: string,
    sandboxPath: string,
    hostPath: string,
    provider: string,
  ): Promise<void> {
    await this.call("copyFrom", { sandboxId, sandboxPath, hostPath, provider });
  }

  private async call(method: string, request: any): Promise<any> {
    const client = await this.getClient();
    return new Promise((resolve, reject) => {
      client[method](request, (err: any, response: any) => {
        if (err) {
          reject(this.mapGrpcError(err));
        } else {
          resolve(response);
        }
      });
    });
  }

  private mapGrpcError(err: any): RocheError {
    const details = err.details ?? err.message ?? "unknown error";
    switch (err.code) {
      case Status.NOT_FOUND:
        return new SandboxNotFound(details);
      case Status.FAILED_PRECONDITION:
        return new SandboxPaused(details);
      case Status.UNAVAILABLE:
        return new ProviderUnavailable(details);
      case Status.DEADLINE_EXCEEDED:
        return new TimeoutError(details);
      case Status.UNIMPLEMENTED:
        return new UnsupportedOperation(details);
      default:
        return new RocheError(details);
    }
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/transport/grpc.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/transport/grpc.ts sdk/typescript/test/transport/grpc.test.ts
git commit -m "feat(sdk-ts): implement gRPC transport with error mapping"
```

---

## Chunk 5: TypeScript SDK — Roche Client & Sandbox Class

### Task 9: Implement Sandbox class

**Files:**
- Create: `sdk/typescript/src/sandbox.ts`
- Test: `sdk/typescript/test/sandbox.test.ts`

- [ ] **Step 1: Write Sandbox test**

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { Sandbox } from "../src/sandbox";
import type { Transport } from "../src/transport";

function mockTransport(): Transport {
  return {
    create: vi.fn().mockResolvedValue("sandbox-1"),
    exec: vi.fn().mockResolvedValue({ exitCode: 0, stdout: "ok", stderr: "" }),
    destroy: vi.fn().mockResolvedValue(["sandbox-1"]),
    list: vi.fn().mockResolvedValue([]),
    pause: vi.fn().mockResolvedValue(undefined),
    unpause: vi.fn().mockResolvedValue(undefined),
    gc: vi.fn().mockResolvedValue([]),
    copyTo: vi.fn().mockResolvedValue(undefined),
    copyFrom: vi.fn().mockResolvedValue(undefined),
  };
}

describe("Sandbox", () => {
  let transport: Transport;

  beforeEach(() => {
    transport = mockTransport();
  });

  it("stores sandboxId and provider", () => {
    const sb = new Sandbox("abc", "docker", transport);
    expect(sb.id).toBe("abc");
    expect(sb.provider).toBe("docker");
  });

  it("exec delegates to transport with stored provider", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    const output = await sb.exec(["echo", "hi"]);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined);
    expect(output.exitCode).toBe(0);
  });

  it("exec passes timeout", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["sleep", "10"], 5);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["sleep", "10"], "docker", 5);
  });

  it("pause delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.pause();
    expect(transport.pause).toHaveBeenCalledWith("abc", "docker");
  });

  it("unpause delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.unpause();
    expect(transport.unpause).toHaveBeenCalledWith("abc", "docker");
  });

  it("destroy delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.destroy();
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });

  it("copyTo delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.copyTo("/local/f.py", "/sandbox/f.py");
    expect(transport.copyTo).toHaveBeenCalledWith(
      "abc", "/local/f.py", "/sandbox/f.py", "docker"
    );
  });

  it("copyFrom delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.copyFrom("/sandbox/out.txt", "/local/out.txt");
    expect(transport.copyFrom).toHaveBeenCalledWith(
      "abc", "/sandbox/out.txt", "/local/out.txt", "docker"
    );
  });

  it("asyncDispose calls destroy", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb[Symbol.asyncDispose]();
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/sandbox.test.ts`
Expected: FAIL

- [ ] **Step 3: Write sandbox.ts**

```typescript
import type { Transport } from "./transport";
import type { ExecOutput } from "./types";

export class Sandbox {
  constructor(
    public readonly id: string,
    public readonly provider: string,
    private readonly transport: Transport,
  ) {}

  async exec(command: string[], timeoutSecs?: number): Promise<ExecOutput> {
    return this.transport.exec(this.id, command, this.provider, timeoutSecs);
  }

  async pause(): Promise<void> {
    await this.transport.pause(this.id, this.provider);
  }

  async unpause(): Promise<void> {
    await this.transport.unpause(this.id, this.provider);
  }

  async destroy(): Promise<void> {
    await this.transport.destroy([this.id], this.provider);
  }

  async copyTo(hostPath: string, sandboxPath: string): Promise<void> {
    await this.transport.copyTo(this.id, hostPath, sandboxPath, this.provider);
  }

  async copyFrom(sandboxPath: string, hostPath: string): Promise<void> {
    await this.transport.copyFrom(
      this.id,
      sandboxPath,
      hostPath,
      this.provider,
    );
  }

  async [Symbol.asyncDispose](): Promise<void> {
    await this.destroy();
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/sandbox.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/sandbox.ts sdk/typescript/test/sandbox.test.ts
git commit -m "feat(sdk-ts): implement Sandbox class"
```

### Task 10: Implement Roche client class

**Files:**
- Create: `sdk/typescript/src/roche.ts`
- Test: `sdk/typescript/test/roche.test.ts`

- [ ] **Step 1: Write Roche client test**

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";
import { Roche } from "../src/roche";
import { Sandbox } from "../src/sandbox";
import type { Transport } from "../src/transport";

function mockTransport(): Transport {
  return {
    create: vi.fn().mockResolvedValue("sandbox-1"),
    exec: vi.fn().mockResolvedValue({ exitCode: 0, stdout: "ok", stderr: "" }),
    destroy: vi.fn().mockResolvedValue(["sandbox-1"]),
    list: vi.fn().mockResolvedValue([]),
    pause: vi.fn().mockResolvedValue(undefined),
    unpause: vi.fn().mockResolvedValue(undefined),
    gc: vi.fn().mockResolvedValue([]),
    copyTo: vi.fn().mockResolvedValue(undefined),
    copyFrom: vi.fn().mockResolvedValue(undefined),
  };
}

// We test with injected transport to avoid daemon detection
describe("Roche", () => {
  let transport: Transport;

  beforeEach(() => {
    transport = mockTransport();
  });

  it("createSandbox returns a Sandbox object", async () => {
    const roche = new Roche({ transport });
    const sb = await roche.createSandbox({ image: "node:20" });
    expect(sb).toBeInstanceOf(Sandbox);
    expect(sb.id).toBe("sandbox-1");
    expect(sb.provider).toBe("docker");
    expect(transport.create).toHaveBeenCalledWith(
      expect.objectContaining({ image: "node:20" }),
      "docker",
    );
  });

  it("createSandbox captures provider from config", async () => {
    const roche = new Roche({ transport });
    const sb = await roche.createSandbox({ provider: "firecracker" });
    expect(sb.provider).toBe("firecracker");
    expect(transport.create).toHaveBeenCalledWith(
      expect.anything(),
      "firecracker",
    );
  });

  it("create returns sandbox ID string", async () => {
    const roche = new Roche({ transport });
    const id = await roche.create({ image: "python:3.12-slim" });
    expect(id).toBe("sandbox-1");
  });

  it("exec delegates to transport with default provider", async () => {
    const roche = new Roche({ transport });
    await roche.exec("abc", ["echo", "hello"]);
    expect(transport.exec).toHaveBeenCalledWith(
      "abc", ["echo", "hello"], "docker", undefined,
    );
  });

  it("destroy delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.destroy("abc");
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });

  it("list delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.list();
    expect(transport.list).toHaveBeenCalledWith("docker");
  });

  it("gc delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.gc();
    expect(transport.gc).toHaveBeenCalledWith("docker", undefined, undefined);
  });

  it("uses custom default provider", async () => {
    const roche = new Roche({ transport, provider: "firecracker" });
    await roche.list();
    expect(transport.list).toHaveBeenCalledWith("firecracker");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/typescript && npx vitest run test/roche.test.ts`
Expected: FAIL

- [ ] **Step 3: Write roche.ts**

```typescript
import type { Transport } from "./transport";
import { CliTransport } from "./transport/cli";
import { GrpcTransport } from "./transport/grpc";
import { detectDaemon } from "./daemon";
import { Sandbox } from "./sandbox";
import type { SandboxConfig, ExecOutput, SandboxInfo } from "./types";
import { DEFAULTS } from "./types";

export interface RocheOptions {
  mode?: "auto" | "direct";
  daemonPort?: number;
  provider?: string;
  binary?: string;
  transport?: Transport; // For testing
}

export class Roche {
  private readonly provider: string;
  private transportPromise: Promise<Transport>;

  constructor(options: RocheOptions = {}) {
    this.provider = options.provider ?? DEFAULTS.provider;

    if (options.transport) {
      this.transportPromise = Promise.resolve(options.transport);
    } else if (options.mode === "direct") {
      this.transportPromise = Promise.resolve(
        new CliTransport(options.binary ?? "roche"),
      );
    } else {
      this.transportPromise = this.autoDetect(options);
    }
  }

  private async autoDetect(options: RocheOptions): Promise<Transport> {
    if (options.daemonPort) {
      return new GrpcTransport(options.daemonPort);
    }
    const daemon = await detectDaemon();
    if (daemon) {
      return new GrpcTransport(daemon.port);
    }
    return new CliTransport(options.binary ?? "roche");
  }

  private async getTransport(): Promise<Transport> {
    return this.transportPromise;
  }

  async createSandbox(config: SandboxConfig = {}): Promise<Sandbox> {
    const transport = await this.getTransport();
    const provider = config.provider ?? this.provider;
    const id = await transport.create(config, provider);
    return new Sandbox(id, provider, transport);
  }

  async create(config: SandboxConfig = {}): Promise<string> {
    const transport = await this.getTransport();
    const provider = config.provider ?? this.provider;
    return transport.create(config, provider);
  }

  async exec(
    sandboxId: string,
    command: string[],
    timeoutSecs?: number,
  ): Promise<ExecOutput> {
    const transport = await this.getTransport();
    return transport.exec(sandboxId, command, this.provider, timeoutSecs);
  }

  async destroy(sandboxId: string): Promise<void> {
    const transport = await this.getTransport();
    await transport.destroy([sandboxId], this.provider);
  }

  async list(): Promise<SandboxInfo[]> {
    const transport = await this.getTransport();
    return transport.list(this.provider);
  }

  async gc(dryRun?: boolean, all?: boolean): Promise<string[]> {
    const transport = await this.getTransport();
    return transport.gc(this.provider, dryRun, all);
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/typescript && npx vitest run test/roche.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/roche.ts sdk/typescript/test/roche.test.ts
git commit -m "feat(sdk-ts): implement Roche client class"
```

### Task 11: Create public exports

**Files:**
- Create: `sdk/typescript/src/index.ts`

- [ ] **Step 1: Write index.ts**

```typescript
export { Roche } from "./roche";
export type { RocheOptions } from "./roche";
export { Sandbox } from "./sandbox";
export type {
  SandboxConfig,
  ExecOutput,
  SandboxInfo,
  SandboxStatus,
  Mount,
} from "./types";
export {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "./errors";
```

- [ ] **Step 2: Verify build compiles**

Run: `cd sdk/typescript && npx tsc --noEmit`
Expected: No errors (or only generated file import errors which are OK since codegen isn't checked in)

- [ ] **Step 3: Run all tests**

Run: `cd sdk/typescript && npx vitest run`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add sdk/typescript/src/index.ts
git commit -m "feat(sdk-ts): add public exports"
```

---

## Chunk 6: Python SDK — Project Setup & Restructure

### Task 12: Replace existing Python SDK with new structure

**Files:**
- Delete: `sdk/python/roche/` (entire directory)
- Delete: `sdk/python/tests/` (entire directory)
- Modify: `sdk/python/pyproject.toml`
- Create: `sdk/python/src/roche_sandbox/__init__.py`
- Create: `sdk/python/src/roche_sandbox/types.py`
- Create: `sdk/python/.gitignore`

- [ ] **Step 1: Remove old SDK directory and create new structure**

```bash
rm -rf sdk/python/roche sdk/python/tests
mkdir -p sdk/python/src/roche_sandbox/transport
mkdir -p sdk/python/tests/transport
mkdir -p sdk/python/scripts
touch sdk/python/src/roche_sandbox/__init__.py
touch sdk/python/src/roche_sandbox/transport/__init__.py
touch sdk/python/tests/__init__.py
touch sdk/python/tests/transport/__init__.py
```

- [ ] **Step 2: Rewrite pyproject.toml**

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "roche-sandbox"
version = "0.1.0"
description = "Universal sandbox orchestrator for AI agents — Python SDK"
license = "Apache-2.0"
requires-python = ">=3.10"
readme = "README.md"
dependencies = [
    "grpcio>=1.60.0",
    "protobuf>=4.25.0",
]

[tool.hatch.build.targets.wheel]
packages = ["src/roche_sandbox"]

[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-asyncio>=0.23.0",
    "grpcio-tools>=1.60.0",
]

[tool.pytest.ini_options]
asyncio_mode = "strict"
testpaths = ["tests"]

[project.urls]
Homepage = "https://github.com/substratum-labs/roche"
Repository = "https://github.com/substratum-labs/roche"
```

- [ ] **Step 3: Create .gitignore**

```
src/roche_sandbox/generated/
__pycache__/
*.pyc
*.egg-info/
dist/
.venv/
```

- [ ] **Step 4: Commit**

```bash
git add sdk/python/pyproject.toml sdk/python/.gitignore sdk/python/src/ sdk/python/tests/
git rm -r sdk/python/roche/ sdk/python/tests/test_client.py 2>/dev/null || true
git commit -m "refactor(sdk-py): restructure for roche-sandbox package with src layout"
```

### Task 13: Define Python types and errors

**Files:**
- Create: `sdk/python/src/roche_sandbox/types.py`
- Create: `sdk/python/src/roche_sandbox/errors.py`
- Test: `sdk/python/tests/test_types.py`

- [ ] **Step 1: Write types test**

```python
from roche_sandbox.types import SandboxConfig, ExecOutput, Mount, SandboxInfo

def test_sandbox_config_defaults():
    config = SandboxConfig()
    assert config.provider == "docker"
    assert config.image == "python:3.12-slim"
    assert config.timeout_secs == 300
    assert config.network is False
    assert config.writable is False
    assert config.env == {}
    assert config.mounts == []

def test_exec_output():
    output = ExecOutput(exit_code=0, stdout="hi", stderr="")
    assert output.exit_code == 0

def test_mount_defaults():
    mount = Mount(host_path="/a", container_path="/b")
    assert mount.readonly is True

def test_sandbox_info():
    info = SandboxInfo(id="abc", status="running", provider="docker", image="python:3.12-slim")
    assert info.status == "running"
    assert info.expires_at is None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/test_types.py -v`
Expected: FAIL — `roche_sandbox.types` not found

- [ ] **Step 3: Write types.py**

```python
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal

SandboxStatus = Literal["running", "paused", "stopped", "failed"]


@dataclass
class Mount:
    host_path: str
    container_path: str
    readonly: bool = True


@dataclass
class SandboxConfig:
    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout_secs: int = 300
    network: bool = False
    writable: bool = False
    env: dict[str, str] = field(default_factory=dict)
    mounts: list[Mount] = field(default_factory=list)
    kernel: str | None = None
    rootfs: str | None = None


@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str


@dataclass
class SandboxInfo:
    id: str
    status: SandboxStatus
    provider: str
    image: str
    expires_at: int | None = None
```

- [ ] **Step 4: Write errors test**

Add `sdk/python/tests/test_errors.py`:

```python
from roche_sandbox.errors import (
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    ProviderUnavailable,
    TimeoutError,
    UnsupportedOperation,
)

def test_sandbox_not_found_is_roche_error():
    err = SandboxNotFound("abc123")
    assert isinstance(err, RocheError)
    assert "abc123" in str(err)

def test_sandbox_paused_is_roche_error():
    assert isinstance(SandboxPaused("abc"), RocheError)

def test_provider_unavailable_is_roche_error():
    assert isinstance(ProviderUnavailable("down"), RocheError)

def test_timeout_error_is_roche_error():
    assert isinstance(TimeoutError("30s"), RocheError)

def test_unsupported_operation_is_roche_error():
    assert isinstance(UnsupportedOperation("pause"), RocheError)
```

- [ ] **Step 5: Write errors.py**

```python
class RocheError(Exception):
    def __init__(self, message: str):
        super().__init__(message)


class SandboxNotFound(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Sandbox not found: {detail}")


class SandboxPaused(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Sandbox is paused: {detail}")


class ProviderUnavailable(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Provider unavailable: {detail}")


class TimeoutError(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Operation timed out: {detail}")


class UnsupportedOperation(RocheError):
    def __init__(self, detail: str):
        super().__init__(f"Unsupported operation: {detail}")
```

- [ ] **Step 6: Install package in dev mode and run tests**

Run: `cd sdk/python && pip install -e ".[dev]" && python -m pytest tests/test_types.py tests/test_errors.py -v`
Expected: All PASS

- [ ] **Step 7: Commit**

```bash
git add sdk/python/src/roche_sandbox/types.py sdk/python/src/roche_sandbox/errors.py sdk/python/tests/test_types.py sdk/python/tests/test_errors.py
git commit -m "feat(sdk-py): add types and error hierarchy"
```

---

## Chunk 7: Python SDK — Transport Layer

### Task 14: Define Transport protocol

**Files:**
- Create: `sdk/python/src/roche_sandbox/transport/__init__.py`

- [ ] **Step 1: Write Transport protocol**

```python
from __future__ import annotations

from typing import Protocol

from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo


class Transport(Protocol):
    async def create(self, config: SandboxConfig, provider: str) -> str: ...
    async def exec(
        self,
        sandbox_id: str,
        command: list[str],
        provider: str,
        timeout_secs: int | None = None,
    ) -> ExecOutput: ...
    async def destroy(
        self,
        sandbox_ids: list[str],
        provider: str,
        all: bool = False,
    ) -> list[str]: ...
    async def list(self, provider: str) -> list[SandboxInfo]: ...
    async def pause(self, sandbox_id: str, provider: str) -> None: ...
    async def unpause(self, sandbox_id: str, provider: str) -> None: ...
    async def gc(
        self,
        provider: str,
        dry_run: bool = False,
        all: bool = False,
    ) -> list[str]: ...
    async def copy_to(
        self,
        sandbox_id: str,
        host_path: str,
        sandbox_path: str,
        provider: str,
    ) -> None: ...
    async def copy_from(
        self,
        sandbox_id: str,
        sandbox_path: str,
        host_path: str,
        provider: str,
    ) -> None: ...
```

- [ ] **Step 2: Commit**

```bash
git add sdk/python/src/roche_sandbox/transport/__init__.py
git commit -m "feat(sdk-py): add Transport protocol"
```

### Task 15: Implement CLI transport

**Files:**
- Create: `sdk/python/src/roche_sandbox/transport/cli.py`
- Test: `sdk/python/tests/transport/test_cli.py`

- [ ] **Step 1: Write CLI transport test**

```python
import asyncio
import json
from unittest.mock import AsyncMock, patch, MagicMock

import pytest

from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.types import SandboxConfig, Mount
from roche_sandbox.errors import (
    ProviderUnavailable,
    SandboxNotFound,
)


@pytest.fixture
def transport():
    return CliTransport(binary="roche")


def make_process_mock(stdout="", stderr="", returncode=0):
    proc = AsyncMock()
    proc.communicate = AsyncMock(return_value=(stdout.encode(), stderr.encode()))
    proc.returncode = returncode
    return proc


@pytest.mark.asyncio
class TestCliTransportCreate:
    async def test_create_default_config(self, transport):
        proc = make_process_mock(stdout="abc123\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            sandbox_id = await transport.create(SandboxConfig(), "docker")

        assert sandbox_id == "abc123"
        args = mock_exec.call_args[0]
        assert "create" in args
        assert "--provider" in args
        assert "docker" in args
        assert "--image" in args
        assert "python:3.12-slim" in args

    async def test_create_with_network_and_writable(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(network=True, writable=True, memory="1g", cpus=2.0)
            await transport.create(config, "docker")

        args = mock_exec.call_args[0]
        assert "--network" in args
        assert "--writable" in args
        assert "--memory" in args
        assert "1g" in args
        assert "--cpus" in args
        assert "2.0" in args

    async def test_create_with_mounts(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(mounts=[
                Mount("/host/a", "/container/a"),
                Mount("/host/b", "/container/b", readonly=False),
            ])
            await transport.create(config, "docker")

        args = mock_exec.call_args[0]
        assert "/host/a:/container/a:ro" in args
        assert "/host/b:/container/b:rw" in args

    async def test_create_with_env(self, transport):
        proc = make_process_mock(stdout="id1\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            config = SandboxConfig(env={"FOO": "bar"})
            await transport.create(config, "docker")

        args = mock_exec.call_args[0]
        assert "--env" in args
        assert "FOO=bar" in args


@pytest.mark.asyncio
class TestCliTransportExec:
    async def test_exec_success(self, transport):
        proc = make_process_mock(stdout="hello\n", returncode=0)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            output = await transport.exec("abc", ["echo", "hello"], "docker")

        assert output.exit_code == 0
        assert output.stdout == "hello\n"

    async def test_exec_nonzero_exit(self, transport):
        proc = make_process_mock(stderr="command failed", returncode=1)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            output = await transport.exec("abc", ["false"], "docker")

        assert output.exit_code == 1

    async def test_exec_roche_error_raises(self, transport):
        proc = make_process_mock(stderr="Error: sandbox not found", returncode=1)
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            with pytest.raises(SandboxNotFound):
                await transport.exec("abc", ["echo"], "docker")


@pytest.mark.asyncio
class TestCliTransportOther:
    async def test_list_parses_json(self, transport):
        data = [{"id": "abc", "status": "running", "provider": "docker", "image": "python:3.12-slim"}]
        proc = make_process_mock(stdout=json.dumps(data))
        with patch("asyncio.create_subprocess_exec", return_value=proc):
            sandboxes = await transport.list("docker")

        assert len(sandboxes) == 1
        assert sandboxes[0].id == "abc"

    async def test_pause(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.pause("abc", "docker")

        args = mock_exec.call_args[0]
        assert "pause" in args
        assert "abc" in args

    async def test_copy_to(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.copy_to("abc", "/local/f.py", "/sandbox/f.py", "docker")

        args = mock_exec.call_args[0]
        assert "cp" in args
        assert "/local/f.py" in args
        assert "abc:/sandbox/f.py" in args

    async def test_copy_from(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.copy_from("abc", "/sandbox/out.txt", "/local/out.txt", "docker")

        args = mock_exec.call_args[0]
        assert "cp" in args
        assert "abc:/sandbox/out.txt" in args
        assert "/local/out.txt" in args

    async def test_unpause(self, transport):
        proc = make_process_mock()
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.unpause("abc", "docker")

        args = mock_exec.call_args[0]
        assert "unpause" in args
        assert "abc" in args

    async def test_destroy_with_ids(self, transport):
        proc = make_process_mock(stdout="abc\ndef\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            destroyed = await transport.destroy(["abc", "def"], "docker")

        assert destroyed == ["abc", "def"]
        args = mock_exec.call_args[0]
        assert "destroy" in args
        assert "abc" in args
        assert "def" in args

    async def test_destroy_all(self, transport):
        proc = make_process_mock(stdout="abc\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            await transport.destroy([], "docker", all=True)

        args = mock_exec.call_args[0]
        assert "destroy" in args
        assert "--all" in args

    async def test_gc_with_flags(self, transport):
        proc = make_process_mock(stdout="old1\nold2\n")
        with patch("asyncio.create_subprocess_exec", return_value=proc) as mock_exec:
            destroyed = await transport.gc("docker", dry_run=True, all=True)

        assert destroyed == ["old1", "old2"]
        args = mock_exec.call_args[0]
        assert "gc" in args
        assert "--dry-run" in args
        assert "--all" in args

    async def test_binary_not_found(self, transport):
        with patch("asyncio.create_subprocess_exec", side_effect=FileNotFoundError):
            with pytest.raises(ProviderUnavailable):
                await transport.create(SandboxConfig(), "docker")
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/transport/test_cli.py -v`
Expected: FAIL

- [ ] **Step 3: Write cli.py**

```python
from __future__ import annotations

import asyncio
import json

from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo


class CliTransport:
    def __init__(self, binary: str = "roche"):
        self._binary = binary

    async def create(self, config: SandboxConfig, provider: str) -> str:
        args = [
            "create",
            "--provider", provider,
            "--image", config.image,
            "--timeout", str(config.timeout_secs),
        ]
        if config.memory:
            args.extend(["--memory", config.memory])
        if config.cpus is not None:
            args.extend(["--cpus", str(config.cpus)])
        if config.network:
            args.append("--network")
        if config.writable:
            args.append("--writable")
        for k, v in config.env.items():
            args.extend(["--env", f"{k}={v}"])
        for m in config.mounts:
            mode = "ro" if m.readonly else "rw"
            args.extend(["--mount", f"{m.host_path}:{m.container_path}:{mode}"])
        if config.kernel:
            args.extend(["--kernel", config.kernel])
        if config.rootfs:
            args.extend(["--rootfs", config.rootfs])

        stdout, _ = await self._run(args)
        return stdout.strip()

    async def exec(
        self,
        sandbox_id: str,
        command: list[str],
        provider: str,
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        args = ["exec", "--sandbox", sandbox_id]
        if timeout_secs is not None:
            args.extend(["--timeout", str(timeout_secs)])
        args.extend(["--", *command])

        stdout, stderr, returncode = await self._run_unchecked(args)
        if returncode != 0 and self._is_roche_error(stderr):
            raise self._map_cli_error(stderr)
        return ExecOutput(exit_code=returncode, stdout=stdout, stderr=stderr)

    async def destroy(
        self,
        sandbox_ids: list[str],
        provider: str,
        all: bool = False,
    ) -> list[str]:
        args = ["destroy"]
        if all:
            args.append("--all")
        else:
            args.extend(sandbox_ids)
        stdout, _ = await self._run(args)
        return [line for line in stdout.strip().split("\n") if line]

    async def list(self, provider: str) -> list[SandboxInfo]:
        stdout, _ = await self._run(["list", "--json"])
        raw = json.loads(stdout)
        return [
            SandboxInfo(
                id=s["id"],
                status=s["status"],
                provider=s["provider"],
                image=s["image"],
                expires_at=s.get("expires_at"),
            )
            for s in raw
        ]

    async def pause(self, sandbox_id: str, provider: str) -> None:
        await self._run(["pause", sandbox_id])

    async def unpause(self, sandbox_id: str, provider: str) -> None:
        await self._run(["unpause", sandbox_id])

    async def gc(
        self,
        provider: str,
        dry_run: bool = False,
        all: bool = False,
    ) -> list[str]:
        args = ["gc"]
        if dry_run:
            args.append("--dry-run")
        if all:
            args.append("--all")
        stdout, _ = await self._run(args)
        return [line for line in stdout.strip().split("\n") if line]

    async def copy_to(
        self,
        sandbox_id: str,
        host_path: str,
        sandbox_path: str,
        provider: str,
    ) -> None:
        await self._run(["cp", host_path, f"{sandbox_id}:{sandbox_path}"])

    async def copy_from(
        self,
        sandbox_id: str,
        sandbox_path: str,
        host_path: str,
        provider: str,
    ) -> None:
        await self._run(["cp", f"{sandbox_id}:{sandbox_path}", host_path])

    async def _run(self, args: list[str]) -> tuple[str, str]:
        stdout, stderr, returncode = await self._run_unchecked(args)
        if returncode != 0:
            raise self._map_cli_error(stderr)
        return stdout, stderr

    async def _run_unchecked(self, args: list[str]) -> tuple[str, str, int]:
        try:
            proc = await asyncio.create_subprocess_exec(
                self._binary,
                *args,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
        except FileNotFoundError:
            raise ProviderUnavailable(f"Roche binary not found: {self._binary}")

        stdout_bytes, stderr_bytes = await proc.communicate()
        return (
            stdout_bytes.decode(),
            stderr_bytes.decode(),
            proc.returncode or 0,
        )

    def _is_roche_error(self, stderr: str) -> bool:
        return stderr.lstrip().startswith("Error: ")

    def _map_cli_error(self, stderr: str) -> RocheError:
        lower = stderr.lower()
        if "not found" in lower:
            return SandboxNotFound(stderr.strip())
        if "paused" in lower:
            return SandboxPaused(stderr.strip())
        if "timeout" in lower:
            return TimeoutError(stderr.strip())
        if "unsupported" in lower:
            return UnsupportedOperation(stderr.strip())
        if "unavailable" in lower or "connection refused" in lower:
            return ProviderUnavailable(stderr.strip())
        return RocheError(stderr.strip())
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/transport/test_cli.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/transport/cli.py sdk/python/tests/transport/test_cli.py
git commit -m "feat(sdk-py): implement CLI transport with tests"
```

### Task 16: Implement daemon detection

**Files:**
- Create: `sdk/python/src/roche_sandbox/daemon.py`
- Test: `sdk/python/tests/test_daemon.py`

- [ ] **Step 1: Write daemon detection test**

```python
import json
import os
from pathlib import Path
from unittest.mock import patch

import pytest

from roche_sandbox.daemon import detect_daemon


class TestDetectDaemon:
    def test_returns_none_when_file_missing(self, tmp_path):
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=tmp_path / "daemon.json"):
            result = detect_daemon()
        assert result is None

    def test_returns_none_when_file_malformed(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text("not json")
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is None

    def test_returns_info_when_valid_and_alive(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text(json.dumps({"pid": os.getpid(), "port": 50051}))
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is not None
        assert result["pid"] == os.getpid()
        assert result["port"] == 50051

    def test_returns_none_when_pid_dead(self, tmp_path):
        p = tmp_path / "daemon.json"
        p.write_text(json.dumps({"pid": 999999999, "port": 50051}))
        with patch("roche_sandbox.daemon.daemon_json_path", return_value=p):
            result = detect_daemon()
        assert result is None
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/test_daemon.py -v`
Expected: FAIL

- [ ] **Step 3: Write daemon.py**

```python
from __future__ import annotations

import json
import os
import signal
from pathlib import Path
from typing import TypedDict


class DaemonInfo(TypedDict):
    pid: int
    port: int


def daemon_json_path() -> Path:
    return Path.home() / ".roche" / "daemon.json"


def detect_daemon() -> DaemonInfo | None:
    path = daemon_json_path()
    if not path.exists():
        return None

    try:
        data = json.loads(path.read_text())
    except (json.JSONDecodeError, OSError):
        return None

    pid = data.get("pid")
    port = data.get("port")
    if not isinstance(pid, int) or not isinstance(port, int):
        return None

    if not _is_process_alive(pid):
        return None

    return DaemonInfo(pid=pid, port=port)


def _is_process_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ProcessLookupError):
        return False
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/test_daemon.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/daemon.py sdk/python/tests/test_daemon.py
git commit -m "feat(sdk-py): add daemon detection"
```

### Task 17: Implement gRPC transport

**Files:**
- Create: `sdk/python/src/roche_sandbox/transport/grpc.py`
- Create: `sdk/python/scripts/proto-gen.sh`
- Test: `sdk/python/tests/transport/test_grpc.py`

- [ ] **Step 1: Create proto-gen.sh**

```bash
#!/usr/bin/env bash
set -euo pipefail

PROTO_DIR="$(cd "$(dirname "$0")/../../.." && pwd)/proto"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/src/roche_sandbox/generated"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/roche/v1"
touch "$OUT_DIR/__init__.py"
touch "$OUT_DIR/roche/__init__.py"
touch "$OUT_DIR/roche/v1/__init__.py"

python -m grpc_tools.protoc \
  -I "$PROTO_DIR" \
  --python_out="$OUT_DIR" \
  --grpc_python_out="$OUT_DIR" \
  --pyi_out="$OUT_DIR" \
  "$PROTO_DIR/roche/v1/sandbox.proto"
```

- [ ] **Step 2: Write gRPC transport test (error mapping)**

```python
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from roche_sandbox.transport.grpc import GrpcTransport
from roche_sandbox.errors import (
    SandboxNotFound,
    SandboxPaused,
    ProviderUnavailable,
    TimeoutError,
    UnsupportedOperation,
    RocheError,
)
from roche_sandbox.types import SandboxConfig


class FakeRpcError(Exception):
    def __init__(self, code, details="error"):
        self._code = code
        self._details = details

    def code(self):
        return self._code

    def details(self):
        return self._details


@pytest.mark.asyncio
class TestGrpcTransportErrorMapping:
    async def test_not_found_maps_to_sandbox_not_found(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("NOT_FOUND", "sandbox not found")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, SandboxNotFound)

    async def test_failed_precondition_maps_to_sandbox_paused(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("FAILED_PRECONDITION", "paused")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, SandboxPaused)

    async def test_unavailable_maps_to_provider_unavailable(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("UNAVAILABLE", "conn refused")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, ProviderUnavailable)

    async def test_deadline_exceeded_maps_to_timeout(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("DEADLINE_EXCEEDED", "timeout")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, TimeoutError)

    async def test_unimplemented_maps_to_unsupported(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("UNIMPLEMENTED", "not impl")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, UnsupportedOperation)

    async def test_other_maps_to_roche_error(self):
        transport = GrpcTransport.__new__(GrpcTransport)
        err = FakeRpcError("INTERNAL", "boom")
        mapped = transport._map_grpc_error(err)
        assert isinstance(mapped, RocheError)
        assert not isinstance(mapped, SandboxNotFound)
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/transport/test_grpc.py -v`
Expected: FAIL

- [ ] **Step 4: Write grpc.py**

```python
from __future__ import annotations

from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo, SandboxStatus

_PROTO_STATUS_MAP: dict[int, SandboxStatus] = {
    1: "running",
    2: "paused",
    3: "stopped",
    4: "failed",
}

_GRPC_CODE_MAP = {
    "NOT_FOUND": SandboxNotFound,
    "FAILED_PRECONDITION": SandboxPaused,
    "UNAVAILABLE": ProviderUnavailable,
    "DEADLINE_EXCEEDED": TimeoutError,
    "UNIMPLEMENTED": UnsupportedOperation,
}


class GrpcTransport:
    def __init__(self, port: int):
        self._port = port
        self._channel = None
        self._stub = None

    def _get_stub(self):
        if self._stub is None:
            import grpc.aio
            from roche_sandbox.generated.roche.v1 import sandbox_pb2_grpc

            self._channel = grpc.aio.insecure_channel(f"127.0.0.1:{self._port}")
            self._stub = sandbox_pb2_grpc.SandboxServiceStub(self._channel)
        return self._stub

    async def create(self, config: SandboxConfig, provider: str) -> str:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        request = sandbox_pb2.CreateRequest(
            provider=provider,
            image=config.image,
            timeout_secs=config.timeout_secs,
            network=config.network,
            writable=config.writable,
            env=config.env,
            mounts=[
                sandbox_pb2.MountConfig(
                    host_path=m.host_path,
                    container_path=m.container_path,
                    readonly=m.readonly,
                )
                for m in config.mounts
            ],
        )
        if config.memory:
            request.memory = config.memory
        if config.cpus is not None:
            request.cpus = config.cpus
        if config.kernel:
            request.kernel = config.kernel
        if config.rootfs:
            request.rootfs = config.rootfs

        try:
            response = await self._get_stub().Create(request)
        except Exception as e:
            raise self._map_grpc_error(e)
        return response.sandbox_id

    async def exec(
        self,
        sandbox_id: str,
        command: list[str],
        provider: str,
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        request = sandbox_pb2.ExecRequest(
            sandbox_id=sandbox_id,
            command=command,
            provider=provider,
        )
        if timeout_secs is not None:
            request.timeout_secs = timeout_secs

        try:
            response = await self._get_stub().Exec(request)
        except Exception as e:
            raise self._map_grpc_error(e)
        return ExecOutput(
            exit_code=response.exit_code,
            stdout=response.stdout,
            stderr=response.stderr,
        )

    async def destroy(
        self,
        sandbox_ids: list[str],
        provider: str,
        all: bool = False,
    ) -> list[str]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        request = sandbox_pb2.DestroyRequest(
            sandbox_ids=sandbox_ids,
            all=all,
            provider=provider,
        )
        try:
            response = await self._get_stub().Destroy(request)
        except Exception as e:
            raise self._map_grpc_error(e)
        return list(response.destroyed_ids)

    async def list(self, provider: str) -> list[SandboxInfo]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            response = await self._get_stub().List(sandbox_pb2.ListRequest(provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)
        return [
            SandboxInfo(
                id=s.id,
                status=_PROTO_STATUS_MAP.get(s.status, "failed"),
                provider=s.provider,
                image=s.image,
                expires_at=s.expires_at if s.HasField("expires_at") else None,
            )
            for s in response.sandboxes
        ]

    async def pause(self, sandbox_id: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            await self._get_stub().Pause(sandbox_pb2.PauseRequest(sandbox_id=sandbox_id, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    async def unpause(self, sandbox_id: str, provider: str) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            await self._get_stub().Unpause(sandbox_pb2.UnpauseRequest(sandbox_id=sandbox_id, provider=provider))
        except Exception as e:
            raise self._map_grpc_error(e)

    async def gc(
        self,
        provider: str,
        dry_run: bool = False,
        all: bool = False,
    ) -> list[str]:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            response = await self._get_stub().Gc(
                sandbox_pb2.GcRequest(dry_run=dry_run, all=all, provider=provider)
            )
        except Exception as e:
            raise self._map_grpc_error(e)
        return list(response.destroyed_ids)

    async def copy_to(
        self,
        sandbox_id: str,
        host_path: str,
        sandbox_path: str,
        provider: str,
    ) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            await self._get_stub().CopyTo(
                sandbox_pb2.CopyToRequest(
                    sandbox_id=sandbox_id,
                    host_path=host_path,
                    sandbox_path=sandbox_path,
                    provider=provider,
                )
            )
        except Exception as e:
            raise self._map_grpc_error(e)

    async def copy_from(
        self,
        sandbox_id: str,
        sandbox_path: str,
        host_path: str,
        provider: str,
    ) -> None:
        from roche_sandbox.generated.roche.v1 import sandbox_pb2

        try:
            await self._get_stub().CopyFrom(
                sandbox_pb2.CopyFromRequest(
                    sandbox_id=sandbox_id,
                    sandbox_path=sandbox_path,
                    host_path=host_path,
                    provider=provider,
                )
            )
        except Exception as e:
            raise self._map_grpc_error(e)

    def _map_grpc_error(self, err: Exception) -> RocheError:
        code_str = ""
        details = str(err)
        if hasattr(err, "code") and callable(err.code):
            code_val = err.code()
            code_str = code_val if isinstance(code_val, str) else code_val.name if hasattr(code_val, "name") else str(code_val)
        if hasattr(err, "details") and callable(err.details):
            details = err.details()

        cls = _GRPC_CODE_MAP.get(code_str, RocheError)
        return cls(details)
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/transport/test_grpc.py -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
chmod +x sdk/python/scripts/proto-gen.sh
git add sdk/python/src/roche_sandbox/transport/grpc.py sdk/python/tests/transport/test_grpc.py sdk/python/scripts/proto-gen.sh
git commit -m "feat(sdk-py): implement gRPC transport and proto codegen script"
```

---

## Chunk 8: Python SDK — Client & Sandbox Classes

### Task 18: Implement AsyncSandbox and Sandbox classes

**Files:**
- Create: `sdk/python/src/roche_sandbox/sandbox.py`
- Test: `sdk/python/tests/test_sandbox.py`

- [ ] **Step 1: Write sandbox test**

```python
from unittest.mock import AsyncMock

import pytest

from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput


def mock_transport():
    t = AsyncMock()
    t.create.return_value = "sb-1"
    t.exec.return_value = ExecOutput(exit_code=0, stdout="ok", stderr="")
    t.destroy.return_value = ["sb-1"]
    return t


@pytest.mark.asyncio
class TestAsyncSandbox:
    async def test_stores_id_and_provider(self):
        sb = AsyncSandbox("abc", "docker", mock_transport())
        assert sb.id == "abc"
        assert sb.provider == "docker"

    async def test_exec_delegates(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        output = await sb.exec(["echo", "hi"])
        t.exec.assert_called_once_with("abc", ["echo", "hi"], "docker", None)
        assert output.exit_code == 0

    async def test_exec_with_timeout(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.exec(["sleep", "10"], timeout_secs=5)
        t.exec.assert_called_once_with("abc", ["sleep", "10"], "docker", 5)

    async def test_pause(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.pause()
        t.pause.assert_called_once_with("abc", "docker")

    async def test_unpause(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.unpause()
        t.unpause.assert_called_once_with("abc", "docker")

    async def test_destroy(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.destroy()
        t.destroy.assert_called_once_with(["abc"], "docker")

    async def test_copy_to(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.copy_to("/local/f.py", "/sandbox/f.py")
        t.copy_to.assert_called_once_with("abc", "/local/f.py", "/sandbox/f.py", "docker")

    async def test_copy_from(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        await sb.copy_from("/sandbox/out.txt", "/local/out.txt")
        t.copy_from.assert_called_once_with("abc", "/sandbox/out.txt", "/local/out.txt", "docker")

    async def test_async_context_manager(self):
        t = mock_transport()
        sb = AsyncSandbox("abc", "docker", t)
        async with sb:
            pass
        t.destroy.assert_called_once_with(["abc"], "docker")


class TestSyncSandbox:
    def test_exec(self):
        t = mock_transport()
        sb = Sandbox("abc", "docker", t)
        output = sb.exec(["echo", "hi"])
        assert output.exit_code == 0

    def test_context_manager(self):
        t = mock_transport()
        sb = Sandbox("abc", "docker", t)
        with sb:
            pass
        t.destroy.assert_called_once()
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/test_sandbox.py -v`
Expected: FAIL

- [ ] **Step 3: Write sandbox.py**

```python
from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from roche_sandbox.types import ExecOutput

if TYPE_CHECKING:
    from roche_sandbox.transport import Transport


class AsyncSandbox:
    def __init__(self, id: str, provider: str, transport: Transport):
        self._id = id
        self._provider = provider
        self._transport = transport

    @property
    def id(self) -> str:
        return self._id

    @property
    def provider(self) -> str:
        return self._provider

    async def exec(
        self,
        command: list[str],
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        return await self._transport.exec(
            self._id, command, self._provider, timeout_secs
        )

    async def pause(self) -> None:
        await self._transport.pause(self._id, self._provider)

    async def unpause(self) -> None:
        await self._transport.unpause(self._id, self._provider)

    async def destroy(self) -> None:
        await self._transport.destroy([self._id], self._provider)

    async def copy_to(self, host_path: str, sandbox_path: str) -> None:
        await self._transport.copy_to(
            self._id, host_path, sandbox_path, self._provider
        )

    async def copy_from(self, sandbox_path: str, host_path: str) -> None:
        await self._transport.copy_from(
            self._id, sandbox_path, host_path, self._provider
        )

    async def __aenter__(self) -> AsyncSandbox:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.destroy()


class Sandbox:
    def __init__(self, id: str, provider: str, transport: Transport):
        self._inner = AsyncSandbox(id, provider, transport)

    @property
    def id(self) -> str:
        return self._inner.id

    @property
    def provider(self) -> str:
        return self._inner.provider

    def exec(
        self,
        command: list[str],
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        return asyncio.run(self._inner.exec(command, timeout_secs))

    def pause(self) -> None:
        asyncio.run(self._inner.pause())

    def unpause(self) -> None:
        asyncio.run(self._inner.unpause())

    def destroy(self) -> None:
        asyncio.run(self._inner.destroy())

    def copy_to(self, host_path: str, sandbox_path: str) -> None:
        asyncio.run(self._inner.copy_to(host_path, sandbox_path))

    def copy_from(self, sandbox_path: str, host_path: str) -> None:
        asyncio.run(self._inner.copy_from(sandbox_path, host_path))

    def __enter__(self) -> Sandbox:
        return self

    def __exit__(self, *exc: object) -> None:
        self.destroy()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/test_sandbox.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/sandbox.py sdk/python/tests/test_sandbox.py
git commit -m "feat(sdk-py): implement AsyncSandbox and Sandbox classes"
```

### Task 19: Implement AsyncRoche and Roche client classes

**Files:**
- Create: `sdk/python/src/roche_sandbox/client.py`
- Test: `sdk/python/tests/test_client.py`

- [ ] **Step 1: Write client test**

```python
from unittest.mock import AsyncMock

import pytest

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo


def mock_transport():
    t = AsyncMock()
    t.create.return_value = "sb-1"
    t.exec.return_value = ExecOutput(exit_code=0, stdout="ok", stderr="")
    t.destroy.return_value = ["sb-1"]
    t.list.return_value = [
        SandboxInfo(id="sb-1", status="running", provider="docker", image="python:3.12-slim")
    ]
    t.gc.return_value = ["sb-old"]
    return t


@pytest.mark.asyncio
class TestAsyncRoche:
    async def test_create_returns_async_sandbox(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sb = await roche.create(image="node:20")
        assert isinstance(sb, AsyncSandbox)
        assert sb.id == "sb-1"
        assert sb.provider == "docker"

    async def test_create_captures_provider(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sb = await roche.create(provider="firecracker")
        assert sb.provider == "firecracker"
        t.create.assert_called_once()
        config_arg, provider_arg = t.create.call_args[0]
        assert provider_arg == "firecracker"

    async def test_create_id_returns_string(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sandbox_id = await roche.create_id(image="python:3.12-slim")
        assert sandbox_id == "sb-1"

    async def test_exec(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        output = await roche.exec("sb-1", ["echo", "hi"])
        assert output.exit_code == 0
        t.exec.assert_called_once_with("sb-1", ["echo", "hi"], "docker", None)

    async def test_destroy(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        await roche.destroy("sb-1")
        t.destroy.assert_called_once_with(["sb-1"], "docker")

    async def test_list(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        sandboxes = await roche.list()
        assert len(sandboxes) == 1

    async def test_gc(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t)
        destroyed = await roche.gc()
        assert destroyed == ["sb-old"]

    async def test_custom_provider(self):
        t = mock_transport()
        roche = AsyncRoche(transport=t, provider="firecracker")
        await roche.list()
        t.list.assert_called_once_with("firecracker")


class TestSyncRoche:
    def test_create_returns_sync_sandbox(self):
        t = mock_transport()
        roche = Roche(transport=t)
        sb = roche.create(image="node:20")
        assert isinstance(sb, Sandbox)
        assert sb.id == "sb-1"

    def test_exec(self):
        t = mock_transport()
        roche = Roche(transport=t)
        output = roche.exec("sb-1", ["echo", "hi"])
        assert output.exit_code == 0
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd sdk/python && python -m pytest tests/test_client.py -v`
Expected: FAIL

- [ ] **Step 3: Write client.py**

```python
from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

from roche_sandbox.daemon import detect_daemon
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.transport.cli import CliTransport
from roche_sandbox.transport.grpc import GrpcTransport
from roche_sandbox.types import ExecOutput, SandboxConfig, SandboxInfo

if TYPE_CHECKING:
    from roche_sandbox.transport import Transport


class AsyncRoche:
    def __init__(
        self,
        *,
        mode: str = "auto",
        daemon_port: int | None = None,
        provider: str = "docker",
        binary: str = "roche",
        transport: Transport | None = None,
    ):
        self._provider = provider
        if transport is not None:
            self._transport = transport
        elif mode == "direct":
            self._transport = CliTransport(binary=binary)
        elif daemon_port is not None:
            self._transport = GrpcTransport(port=daemon_port)
        else:
            daemon = detect_daemon()
            if daemon is not None:
                self._transport = GrpcTransport(port=daemon["port"])
            else:
                self._transport = CliTransport(binary=binary)

    @property
    def transport(self) -> Transport:
        return self._transport

    async def create(
        self,
        *,
        provider: str | None = None,
        image: str = "python:3.12-slim",
        memory: str | None = None,
        cpus: float | None = None,
        timeout_secs: int = 300,
        network: bool = False,
        writable: bool = False,
        env: dict[str, str] | None = None,
        mounts: list | None = None,
        kernel: str | None = None,
        rootfs: str | None = None,
    ) -> AsyncSandbox:
        effective_provider = provider or self._provider
        config = SandboxConfig(
            provider=effective_provider,
            image=image,
            memory=memory,
            cpus=cpus,
            timeout_secs=timeout_secs,
            network=network,
            writable=writable,
            env=env or {},
            mounts=mounts or [],
            kernel=kernel,
            rootfs=rootfs,
        )
        sandbox_id = await self._transport.create(config, effective_provider)
        return AsyncSandbox(sandbox_id, effective_provider, self._transport)

    async def create_id(self, **kwargs) -> str:
        sb = await self.create(**kwargs)
        return sb.id

    async def exec(
        self,
        sandbox_id: str,
        command: list[str],
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        return await self._transport.exec(
            sandbox_id, command, self._provider, timeout_secs
        )

    async def destroy(self, sandbox_id: str) -> None:
        await self._transport.destroy([sandbox_id], self._provider)

    async def list(self) -> list[SandboxInfo]:
        return await self._transport.list(self._provider)

    async def gc(
        self,
        dry_run: bool = False,
        all: bool = False,
    ) -> list[str]:
        return await self._transport.gc(self._provider, dry_run, all)


class Roche:
    def __init__(self, **kwargs):
        self._async = AsyncRoche(**kwargs)

    def create(self, **kwargs) -> Sandbox:
        sb = asyncio.run(self._async.create(**kwargs))
        return Sandbox(sb.id, sb.provider, self._async.transport)

    def create_id(self, **kwargs) -> str:
        return asyncio.run(self._async.create_id(**kwargs))

    def exec(
        self,
        sandbox_id: str,
        command: list[str],
        timeout_secs: int | None = None,
    ) -> ExecOutput:
        return asyncio.run(
            self._async.exec(sandbox_id, command, timeout_secs)
        )

    def destroy(self, sandbox_id: str) -> None:
        asyncio.run(self._async.destroy(sandbox_id))

    def list(self) -> list[SandboxInfo]:
        return asyncio.run(self._async.list())

    def gc(self, dry_run: bool = False, all: bool = False) -> list[str]:
        return asyncio.run(self._async.gc(dry_run, all))
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd sdk/python && python -m pytest tests/test_client.py -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/python/src/roche_sandbox/client.py sdk/python/tests/test_client.py
git commit -m "feat(sdk-py): implement AsyncRoche and Roche client classes"
```

### Task 20: Create public exports

**Files:**
- Modify: `sdk/python/src/roche_sandbox/__init__.py`

- [ ] **Step 1: Write __init__.py**

```python
"""Roche — Universal sandbox orchestrator for AI agents (Python SDK)."""

__version__ = "0.1.0"

from roche_sandbox.client import AsyncRoche, Roche
from roche_sandbox.errors import (
    ProviderUnavailable,
    RocheError,
    SandboxNotFound,
    SandboxPaused,
    TimeoutError,
    UnsupportedOperation,
)
from roche_sandbox.sandbox import AsyncSandbox, Sandbox
from roche_sandbox.types import ExecOutput, Mount, SandboxConfig, SandboxInfo, SandboxStatus

__all__ = [
    "AsyncRoche",
    "Roche",
    "AsyncSandbox",
    "Sandbox",
    "RocheError",
    "SandboxNotFound",
    "SandboxPaused",
    "ProviderUnavailable",
    "TimeoutError",
    "UnsupportedOperation",
    "SandboxConfig",
    "ExecOutput",
    "SandboxInfo",
    "SandboxStatus",
    "Mount",
]
```

- [ ] **Step 2: Run all Python tests**

Run: `cd sdk/python && python -m pytest tests/ -v`
Expected: All PASS

- [ ] **Step 3: Commit**

```bash
git add sdk/python/src/roche_sandbox/__init__.py
git commit -m "feat(sdk-py): add public exports"
```

---

## Chunk 9: Final Verification

### Task 21: Run all tests and verify builds

- [ ] **Step 1: Run all TypeScript tests**

Run: `cd sdk/typescript && npx vitest run`
Expected: All tests pass

- [ ] **Step 2: Verify TypeScript compiles (excluding generated)**

Run: `cd sdk/typescript && npx tsc --noEmit`
Expected: Clean compile (may have warnings about generated imports — acceptable)

- [ ] **Step 3: Run all Python tests**

Run: `cd sdk/python && python -m pytest tests/ -v`
Expected: All tests pass

- [ ] **Step 4: Verify Python package installs**

Run: `cd sdk/python && pip install -e ".[dev]"`
Expected: Clean install

- [ ] **Step 5: Final commit with any fixes**

If any fixes were needed, commit them with an appropriate message.
