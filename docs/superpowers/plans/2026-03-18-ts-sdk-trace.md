# TypeScript SDK Execution Trace Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add execution trace support to the TypeScript SDK, mirroring the Python SDK — trace types, `summary()` method, `traceLevel` parameter through all transports.

**Architecture:** New `src/trace.ts` with `ExecutionTrace` class and sub-type interfaces. `TraceLevel` union type + constants object. Transport interface extended with optional `traceLevel` param. GrpcTransport maps to proto enum and parses response trace. CliTransport returns duration-only trace.

**Tech Stack:** TypeScript, vitest, ts-proto (protobuf codegen), @grpc/grpc-js

**Spec:** `docs/superpowers/specs/2026-03-18-ts-sdk-trace-design.md`

---

## File Structure

### New Files
- `sdk/typescript/src/trace.ts` — `ExecutionTrace` class, all trace sub-type interfaces, `TraceLevel`, `protoToExecutionTrace`, `FILE_OP_MAP`
- `sdk/typescript/test/trace.test.ts` — trace type and summary() tests

### Modified Files
- `sdk/typescript/src/types.ts` — add `trace` field to `ExecOutput`
- `sdk/typescript/src/index.ts` — re-export trace types
- `sdk/typescript/src/transport/index.ts` — add `traceLevel` param to `Transport.exec()`
- `sdk/typescript/src/transport/grpc.ts` — trace_level mapping, response trace parsing
- `sdk/typescript/src/transport/cli.ts` — duration measurement, basic trace
- `sdk/typescript/src/sandbox.ts` — `traceLevel` param passthrough
- `sdk/typescript/src/roche.ts` — `traceLevel` param passthrough
- `sdk/typescript/test/sandbox.test.ts` — update exec assertions for traceLevel
- `sdk/typescript/test/roche.test.ts` — update exec assertions for traceLevel
- `sdk/typescript/test/transport/grpc.test.ts` — trace mapping tests
- `sdk/typescript/test/transport/cli.test.ts` — trace fallback tests
- `sdk/typescript/src/generated/roche/v1/sandbox.ts` — regenerated from proto

---

## Task 1: Trace Types and ExecutionTrace Class

**Files:**
- Create: `sdk/typescript/src/trace.ts`
- Create: `sdk/typescript/test/trace.test.ts`

- [ ] **Step 1: Write failing tests for trace types**

Create `sdk/typescript/test/trace.test.ts`:
```typescript
import { describe, it, expect } from "vitest";
import {
  ExecutionTrace,
  TraceLevel,
  type ResourceUsage,
  type FileAccess,
  type BlockedOperation,
} from "../src/trace";

describe("TraceLevel", () => {
  it("has correct constant values", () => {
    expect(TraceLevel.Off).toBe("off");
    expect(TraceLevel.Summary).toBe("summary");
    expect(TraceLevel.Standard).toBe("standard");
    expect(TraceLevel.Full).toBe("full");
  });
});

describe("ExecutionTrace", () => {
  it("constructs with all fields", () => {
    const trace = new ExecutionTrace({
      durationSecs: 2.3,
      resourceUsage: {
        peakMemoryBytes: 356_000_000,
        cpuTimeSecs: 1.2,
        networkRxBytes: 0,
        networkTxBytes: 0,
      },
      fileAccesses: [
        { path: "/data/input.csv", op: "read", sizeBytes: 2_300_000 },
      ],
    });
    expect(trace.durationSecs).toBe(2.3);
    expect(trace.fileAccesses).toHaveLength(1);
    expect(trace.networkAttempts).toEqual([]);
    expect(trace.syscalls).toEqual([]);
  });

  it("defaults optional arrays to empty", () => {
    const trace = new ExecutionTrace({
      durationSecs: 1.0,
      resourceUsage: { peakMemoryBytes: 0, cpuTimeSecs: 0, networkRxBytes: 0, networkTxBytes: 0 },
    });
    expect(trace.fileAccesses).toEqual([]);
    expect(trace.blockedOps).toEqual([]);
    expect(trace.resourceTimeline).toEqual([]);
  });

  it("summary() formats basic output", () => {
    const trace = new ExecutionTrace({
      durationSecs: 2.3,
      resourceUsage: { peakMemoryBytes: 356_000_000, cpuTimeSecs: 1.2, networkRxBytes: 0, networkTxBytes: 0 },
      fileAccesses: [
        { path: "/data/input.csv", op: "read", sizeBytes: 2_300_000 },
        { path: "/workspace/out.json", op: "create", sizeBytes: 4_100 },
      ],
    });
    const summary = trace.summary();
    expect(summary).toContain("2.3s");
    expect(summary).toContain("356MB");
    expect(summary).toContain("read 1 files");
    expect(summary).toContain("wrote 1 files");
  });

  it("summary() omits empty sections", () => {
    const trace = new ExecutionTrace({
      durationSecs: 0.01,
      resourceUsage: { peakMemoryBytes: 1_000_000, cpuTimeSecs: 0, networkRxBytes: 0, networkTxBytes: 0 },
    });
    const summary = trace.summary();
    expect(summary).toContain("0.0s");
    expect(summary).not.toContain("blocked");
    expect(summary).not.toContain("read");
  });

  it("summary() includes blocked ops count", () => {
    const trace = new ExecutionTrace({
      durationSecs: 1.0,
      resourceUsage: { peakMemoryBytes: 0, cpuTimeSecs: 0, networkRxBytes: 0, networkTxBytes: 0 },
      blockedOps: [{ opType: "network", detail: "blocked connect" }],
    });
    expect(trace.summary()).toContain("blocked 1 ops");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd sdk/typescript && npx vitest run test/trace.test.ts`
Expected: FAIL — module `../src/trace` doesn't exist

- [ ] **Step 3: Implement trace types**

Create `sdk/typescript/src/trace.ts`:
```typescript
// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

export type TraceLevel = "off" | "summary" | "standard" | "full";

/** Constants for discoverability, matching Python SDK's TraceLevel.STANDARD pattern. */
export const TraceLevel = {
  Off: "off" as const,
  Summary: "summary" as const,
  Standard: "standard" as const,
  Full: "full" as const,
};

export interface ResourceUsage {
  peakMemoryBytes: number;
  cpuTimeSecs: number;
  networkRxBytes: number;
  networkTxBytes: number;
}

export interface FileAccess {
  path: string;
  op: "read" | "write" | "create" | "delete";
  sizeBytes?: number;
}

export interface NetworkAttempt {
  address: string;
  protocol: string;
  allowed: boolean;
}

export interface BlockedOperation {
  opType: string;
  detail: string;
}

export interface SyscallEvent {
  name: string;
  args: string[];
  result: string;
  timestampMs: number;
}

export interface ResourceSnapshot {
  timestampMs: number;
  memoryBytes: number;
  cpuPercent: number;
}

export class ExecutionTrace {
  durationSecs: number;
  resourceUsage: ResourceUsage;
  fileAccesses: FileAccess[];
  networkAttempts: NetworkAttempt[];
  blockedOps: BlockedOperation[];
  syscalls: SyscallEvent[];
  resourceTimeline: ResourceSnapshot[];

  constructor(data: {
    durationSecs: number;
    resourceUsage: ResourceUsage;
    fileAccesses?: FileAccess[];
    networkAttempts?: NetworkAttempt[];
    blockedOps?: BlockedOperation[];
    syscalls?: SyscallEvent[];
    resourceTimeline?: ResourceSnapshot[];
  }) {
    this.durationSecs = data.durationSecs;
    this.resourceUsage = data.resourceUsage;
    this.fileAccesses = data.fileAccesses ?? [];
    this.networkAttempts = data.networkAttempts ?? [];
    this.blockedOps = data.blockedOps ?? [];
    this.syscalls = data.syscalls ?? [];
    this.resourceTimeline = data.resourceTimeline ?? [];
  }

  /** LLM-friendly one-line summary. */
  summary(): string {
    const parts: string[] = [`${this.durationSecs.toFixed(1)}s`];
    parts.push(`mem ${Math.floor(this.resourceUsage.peakMemoryBytes / 1_000_000)}MB`);
    if (this.fileAccesses.length > 0) {
      const reads = this.fileAccesses.filter((f) => f.op === "read").length;
      const writes = this.fileAccesses.filter(
        (f) => f.op === "write" || f.op === "create",
      ).length;
      if (reads) parts.push(`read ${reads} files`);
      if (writes) parts.push(`wrote ${writes} files`);
    }
    const blocked = this.blockedOps.length;
    if (blocked) parts.push(`blocked ${blocked} ops`);
    return parts.join(" | ");
  }
}

const FILE_OP_MAP: Record<number, FileAccess["op"]> = {
  0: "read",
  1: "write",
  2: "create",
  3: "delete",
};

export const TRACE_LEVEL_MAP: Record<TraceLevel, number> = {
  off: 0,
  summary: 1,
  standard: 2,
  full: 3,
};

/** Convert proto ExecutionTrace to TypeScript ExecutionTrace. */
export function protoToExecutionTrace(proto: any): ExecutionTrace {
  return new ExecutionTrace({
    durationSecs: proto.durationSecs ?? 0,
    resourceUsage: {
      peakMemoryBytes: proto.resourceUsage?.peakMemoryBytes ?? 0,
      cpuTimeSecs: proto.resourceUsage?.cpuTimeSecs ?? 0,
      networkRxBytes: proto.resourceUsage?.networkRxBytes ?? 0,
      networkTxBytes: proto.resourceUsage?.networkTxBytes ?? 0,
    },
    fileAccesses: (proto.fileAccesses ?? []).map((f: any) => ({
      path: f.path,
      op: FILE_OP_MAP[f.op] ?? "read",
      sizeBytes: f.sizeBytes,
    })),
    networkAttempts: proto.networkAttempts ?? [],
    blockedOps: (proto.blockedOps ?? []).map((b: any) => ({
      opType: b.opType,
      detail: b.detail,
    })),
    syscalls: proto.syscalls ?? [],
    resourceTimeline: proto.resourceTimeline ?? [],
  });
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd sdk/typescript && npx vitest run test/trace.test.ts`
Expected: All 7 tests PASS

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/trace.ts sdk/typescript/test/trace.test.ts
git commit -m "feat(ts-sdk): add execution trace types and summary()"
```

---

## Task 2: Extend ExecOutput and Transport Interface

**Files:**
- Modify: `sdk/typescript/src/types.ts:26-30`
- Modify: `sdk/typescript/src/transport/index.ts:8-13`
- Modify: `sdk/typescript/src/index.ts`

- [ ] **Step 1: Add trace to ExecOutput**

In `sdk/typescript/src/types.ts`, add import and trace field:
```typescript
// Add at top:
import type { ExecutionTrace } from "./trace";

// Replace ExecOutput interface:
export interface ExecOutput {
  exitCode: number;
  stdout: string;
  stderr: string;
  trace?: ExecutionTrace;
}
```

- [ ] **Step 2: Add traceLevel to Transport interface**

In `sdk/typescript/src/transport/index.ts`, add import and param:
```typescript
// Add at top:
import type { TraceLevel } from "../trace";

// Replace exec signature:
  exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,
  ): Promise<ExecOutput>;
```

- [ ] **Step 3: Re-export trace types from index.ts**

In `sdk/typescript/src/index.ts`, add:
```typescript
export {
  ExecutionTrace,
  TraceLevel,
  type ResourceUsage,
  type FileAccess,
  type NetworkAttempt,
  type BlockedOperation,
  type SyscallEvent,
  type ResourceSnapshot,
} from "./trace";
```

- [ ] **Step 4: Verify build compiles (expect transport impl errors)**

Run: `cd sdk/typescript && npx tsc --noEmit 2>&1 | head -20`
Expected: Errors in `grpc.ts` and `cli.ts` about missing `traceLevel` param — this is correct, we fix them in Task 3 and 4.

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/types.ts sdk/typescript/src/transport/index.ts sdk/typescript/src/index.ts
git commit -m "feat(ts-sdk): extend ExecOutput and Transport with trace support"
```

---

## Task 3: GrpcTransport Trace Integration

**Files:**
- Modify: `sdk/typescript/src/transport/grpc.ts:75-78`
- Modify: `sdk/typescript/test/transport/grpc.test.ts`

- [ ] **Step 1: Write tests for GrpcTransport trace handling**

Add to `sdk/typescript/test/transport/grpc.test.ts`, inside the existing `describe("GrpcTransport")`:
```typescript
  it("exec passes traceLevel to proto request", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    await transport.exec("abc", ["echo"], "docker", undefined, "full");
    expect(mockClient.exec).toHaveBeenCalledWith(
      expect.objectContaining({ traceLevel: 3 }),
      expect.any(Function),
    );
  });

  it("exec omits traceLevel from request when undefined", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    await transport.exec("abc", ["echo"], "docker");
    const requestArg = mockClient.exec.mock.calls[0][0];
    expect(requestArg.traceLevel).toBeUndefined();
  });

  it("exec parses trace from proto response", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, {
        exitCode: 0,
        stdout: "ok",
        stderr: "",
        trace: {
          durationSecs: 1.5,
          resourceUsage: {
            peakMemoryBytes: 100_000_000,
            cpuTimeSecs: 0.5,
            networkRxBytes: 0,
            networkTxBytes: 0,
          },
          fileAccesses: [{ path: "/tmp/out.txt", op: 2, sizeBytes: 100 }],
          networkAttempts: [],
          blockedOps: [],
          syscalls: [],
          resourceTimeline: [],
        },
      })
    );
    const output = await transport.exec("abc", ["echo"], "docker", undefined, "standard");
    expect(output.trace).toBeDefined();
    expect(output.trace!.durationSecs).toBe(1.5);
    expect(output.trace!.fileAccesses[0].op).toBe("create");
    expect(output.trace!.fileAccesses[0].path).toBe("/tmp/out.txt");
  });

  it("exec returns undefined trace when response has no trace", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    const output = await transport.exec("abc", ["echo"], "docker");
    expect(output.trace).toBeUndefined();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd sdk/typescript && npx vitest run test/transport/grpc.test.ts`
Expected: FAIL — `traceLevel` not accepted, `output.trace` undefined

- [ ] **Step 3: Update GrpcTransport.exec()**

In `sdk/typescript/src/transport/grpc.ts`, replace the `exec` method:

```typescript
// Add import at top:
import type { TraceLevel } from "../trace";
import { TRACE_LEVEL_MAP, protoToExecutionTrace } from "../trace";

// Replace exec method:
  async exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,
  ): Promise<ExecOutput> {
    const request: any = { sandboxId, command, provider, timeoutSecs };
    if (traceLevel !== undefined) {
      request.traceLevel = TRACE_LEVEL_MAP[traceLevel];
    }
    const response = await this.call("exec", request);
    return {
      exitCode: response.exitCode,
      stdout: response.stdout,
      stderr: response.stderr,
      trace: response.trace ? protoToExecutionTrace(response.trace) : undefined,
    };
  }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd sdk/typescript && npx vitest run test/transport/grpc.test.ts`
Expected: All tests PASS (existing + 4 new)

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/transport/grpc.ts sdk/typescript/test/transport/grpc.test.ts
git commit -m "feat(ts-sdk): add trace support to GrpcTransport"
```

---

## Task 4: CliTransport Trace Integration

**Files:**
- Modify: `sdk/typescript/src/transport/cli.ts:49-72`
- Modify: `sdk/typescript/test/transport/cli.test.ts`

- [ ] **Step 1: Write tests for CliTransport trace handling**

Add to `sdk/typescript/test/transport/cli.test.ts`, inside the existing `describe("CliTransport")`:
```typescript
  it("exec returns trace with duration when traceLevel is set", async () => {
    mockSuccess("hello\n", "");
    const output = await transport.exec("abc", ["echo", "hello"], "docker", undefined, "standard");
    expect(output.trace).toBeDefined();
    expect(output.trace!.durationSecs).toBeGreaterThan(0);
    expect(output.trace!.resourceUsage.peakMemoryBytes).toBe(0);
    expect(output.trace!.fileAccesses).toEqual([]);
  });

  it("exec returns no trace when traceLevel is undefined", async () => {
    mockSuccess("hello\n", "");
    const output = await transport.exec("abc", ["echo", "hello"], "docker");
    expect(output.trace).toBeUndefined();
  });

  it("exec returns no trace when traceLevel is off", async () => {
    mockSuccess("hello\n", "");
    const output = await transport.exec("abc", ["echo", "hello"], "docker", undefined, "off");
    expect(output.trace).toBeUndefined();
  });

  it("exec returns trace with duration on non-zero exit", async () => {
    mockError(1, "command failed");
    const output = await transport.exec("abc", ["false"], "docker", undefined, "summary");
    expect(output.exitCode).toBe(1);
    expect(output.trace).toBeDefined();
    expect(output.trace!.durationSecs).toBeGreaterThan(0);
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd sdk/typescript && npx vitest run test/transport/cli.test.ts`
Expected: FAIL — `traceLevel` not accepted

- [ ] **Step 3: Update CliTransport.exec()**

In `sdk/typescript/src/transport/cli.ts`, replace the `exec` method:

```typescript
// Add import at top:
import { performance } from "perf_hooks";
import type { TraceLevel } from "../trace";
import { ExecutionTrace } from "../trace";

// Replace exec method:
  async exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,
  ): Promise<ExecOutput> {
    const args = ["exec", "--sandbox", sandboxId];
    if (timeoutSecs != null) args.push("--timeout", String(timeoutSecs));
    args.push("--", ...command);

    const includeTrace = traceLevel !== undefined && traceLevel !== "off";
    const start = performance.now();

    try {
      const { stdout, stderr } = await this.run(args, false);
      const durationSecs = (performance.now() - start) / 1000;
      return {
        exitCode: 0,
        stdout,
        stderr,
        trace: includeTrace ? this.basicTrace(durationSecs) : undefined,
      };
    } catch (err: any) {
      const durationSecs = (performance.now() - start) / 1000;
      if (err.stderr && this.isRocheError(err.stderr)) {
        throw this.mapCliError(err.stderr);
      }
      return {
        exitCode: err.code ?? 1,
        stdout: err.stdout ?? "",
        stderr: err.stderr ?? "",
        trace: includeTrace ? this.basicTrace(durationSecs) : undefined,
      };
    }
  }

  private basicTrace(durationSecs: number): ExecutionTrace {
    return new ExecutionTrace({
      durationSecs,
      resourceUsage: { peakMemoryBytes: 0, cpuTimeSecs: 0, networkRxBytes: 0, networkTxBytes: 0 },
    });
  }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd sdk/typescript && npx vitest run test/transport/cli.test.ts`
Expected: All tests PASS (existing + 4 new)

- [ ] **Step 5: Commit**

```bash
git add sdk/typescript/src/transport/cli.ts sdk/typescript/test/transport/cli.test.ts
git commit -m "feat(ts-sdk): add trace support to CliTransport"
```

---

## Task 5: Sandbox and Roche Passthrough

**Files:**
- Modify: `sdk/typescript/src/sandbox.ts:14-16`
- Modify: `sdk/typescript/src/roche.ts:56-59`
- Modify: `sdk/typescript/test/sandbox.test.ts`
- Modify: `sdk/typescript/test/roche.test.ts`

- [ ] **Step 1: Write tests for Sandbox.exec traceLevel passthrough**

Add to `sdk/typescript/test/sandbox.test.ts`, inside `describe("Sandbox")`:
```typescript
  it("exec passes traceLevel to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["echo", "hi"], undefined, "full");
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, "full");
  });

  it("exec passes undefined traceLevel by default", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["echo", "hi"]);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, undefined);
  });
```

- [ ] **Step 2: Write tests for Roche.exec traceLevel passthrough**

Add to `sdk/typescript/test/roche.test.ts`, inside `describe("Roche")`:
```typescript
  it("exec passes traceLevel to transport", async () => {
    const roche = new Roche({ transport });
    await roche.exec("abc", ["echo", "hello"], undefined, "standard");
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hello"], "docker", undefined, "standard");
  });
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd sdk/typescript && npx vitest run test/sandbox.test.ts test/roche.test.ts`
Expected: FAIL — `traceLevel` param not accepted

- [ ] **Step 4: Update Sandbox.exec()**

In `sdk/typescript/src/sandbox.ts`, replace the `exec` method:
```typescript
// Add import at top:
import type { TraceLevel } from "./trace";

// Replace exec method:
  async exec(command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
    return this.transport.exec(this.id, command, this.provider, timeoutSecs, traceLevel);
  }
```

- [ ] **Step 5: Update Roche.exec()**

In `sdk/typescript/src/roche.ts`, replace the `exec` method:
```typescript
// Add import at top:
import type { TraceLevel } from "./trace";

// Replace exec method:
  async exec(sandboxId: string, command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
    const transport = await this.getTransport();
    return transport.exec(sandboxId, command, this.provider, timeoutSecs, traceLevel);
  }
```

- [ ] **Step 6: Update existing test assertions**

In `sdk/typescript/test/sandbox.test.ts`, update the existing `exec` assertions to include the 5th param:
- Line 32: `expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, undefined);`
- Line 39: `expect(transport.exec).toHaveBeenCalledWith("abc", ["sleep", "10"], "docker", 5, undefined);`

In `sdk/typescript/test/roche.test.ts`, update the existing `exec` assertion:
- Line 49: `expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hello"], "docker", undefined, undefined);`

- [ ] **Step 7: Run all tests**

Run: `cd sdk/typescript && npx vitest run`
Expected: All tests PASS

- [ ] **Step 8: Commit**

```bash
git add sdk/typescript/src/sandbox.ts sdk/typescript/src/roche.ts sdk/typescript/test/sandbox.test.ts sdk/typescript/test/roche.test.ts
git commit -m "feat(ts-sdk): add traceLevel passthrough to Sandbox and Roche"
```

---

## Task 6: Proto Regeneration and Final Verification

**Files:**
- Regenerate: `sdk/typescript/src/generated/roche/v1/sandbox.ts`

- [ ] **Step 1: Regenerate proto bindings**

Run: `cd sdk/typescript && npm install && bash scripts/proto-gen.sh`
Expected: `src/generated/roche/v1/sandbox.ts` regenerated with `TraceLevel` enum, `ExecutionTrace` message, etc.

- [ ] **Step 2: Run full test suite**

Run: `cd sdk/typescript && npx vitest run`
Expected: All tests PASS

- [ ] **Step 3: Type-check entire project**

Run: `cd sdk/typescript && npx tsc --noEmit`
Expected: No errors

- [ ] **Step 4: Commit regenerated proto**

```bash
git add sdk/typescript/src/generated/
git commit -m "chore(ts-sdk): regenerate proto bindings with trace messages"
```

- [ ] **Step 5: Final commit verification**

Run: `cd sdk/typescript && npx vitest run && npx tsc --noEmit`
Expected: All tests PASS, no type errors
