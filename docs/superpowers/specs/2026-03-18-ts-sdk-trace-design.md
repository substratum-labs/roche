# TypeScript SDK Execution Trace Integration — Design Spec

**Date:** 2026-03-18
**Version:** 0.1
**Status:** Draft
**Parent Spec:** `docs/superpowers/specs/2026-03-18-execution-trace-design.md`

## Overview

Add execution trace support to the TypeScript SDK, mirroring the Python SDK implementation. The proto schema and daemon already support trace collection — this spec covers the TS SDK surface only.

## Scope

- TypeScript trace types and `summary()` method
- `traceLevel` parameter threaded through `Sandbox.exec()` and both transports
- Proto binding regeneration
- Tests

Out of scope: daemon auto-management, seccomp profile, Rust core changes (all already done).

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Type representation | Interfaces + class for ExecutionTrace | Interfaces for data shapes; class for `summary()` method |
| TraceLevel | String union type `"off" \| "summary" \| "standard" \| "full"` + `TraceLevel` constants object | Union type is idiomatic TS; constants object uses PascalCase (`TraceLevel.Standard`) per TS conventions (Python uses UPPER_SNAKE: `TraceLevel.STANDARD`) |
| `traceLevel` semantics | Three-state: `undefined` (not specified), `"off"` (opt-out), explicit level | Matches Python SDK: `None` = server decides default, `"off"` = no trace, else explicit level |
| CLI fallback | Duration-only trace when explicitly requested, no error | Matches Python SDK silent degradation |
| Proto regeneration | Run existing `scripts/proto-gen.sh` | Proto already has trace messages |
| Positional params | Keep existing positional pattern for now | Matches current codebase; options-object migration is a separate refactor |

## Data Model

### TraceLevel

```typescript
export type TraceLevel = "off" | "summary" | "standard" | "full";

/** Constants for discoverability, matching Python SDK's TraceLevel.STANDARD pattern. */
export const TraceLevel = {
  Off: "off" as const,
  Summary: "summary" as const,
  Standard: "standard" as const,
  Full: "full" as const,
};
```

### Trace Types

```typescript
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
```

### ExecutionTrace

Class (not interface) to support `summary()` method:

```typescript
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
      const reads = this.fileAccesses.filter(f => f.op === "read").length;
      const writes = this.fileAccesses.filter(f => f.op === "write" || f.op === "create").length;
      if (reads) parts.push(`read ${reads} files`);
      if (writes) parts.push(`wrote ${writes} files`);
    }
    const blocked = this.blockedOps.length;
    if (blocked) parts.push(`blocked ${blocked} ops`);
    return parts.join(" | ");
  }
}
```

### ExecOutput Extension

```typescript
export interface ExecOutput {
  exitCode: number;
  stdout: string;
  stderr: string;
  trace?: ExecutionTrace;  // NEW
}
```

## Transport Changes

### Transport Interface

```typescript
export interface Transport {
  exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,  // NEW
  ): Promise<ExecOutput>;
  // ... other methods unchanged
}
```

### GrpcTransport

```typescript
// Mapping
const TRACE_LEVEL_MAP: Record<TraceLevel, number> = {
  off: 0,
  summary: 1,
  standard: 2,
  full: 3,
};

async exec(sandboxId, command, provider, timeoutSecs?, traceLevel?): Promise<ExecOutput> {
  const request: any = {
    sandboxId,
    command,
    provider,
    timeoutSecs,
  };
  // Only set traceLevel when explicitly provided — undefined means "server decides default"
  if (traceLevel !== undefined) {
    request.traceLevel = TRACE_LEVEL_MAP[traceLevel];
  }
  const response = await this.client.exec(request);
  return {
    exitCode: response.exitCode,
    stdout: response.stdout,
    stderr: response.stderr,
    trace: response.trace ? protoToExecutionTrace(response.trace) : undefined,
  };
}
```

The `protoToExecutionTrace` helper maps proto types to TS types:

```typescript
const FILE_OP_MAP: Record<number, FileAccess["op"]> = {
  0: "read",
  1: "write",
  2: "create",
  3: "delete",
};

function protoToExecutionTrace(proto: any): ExecutionTrace {
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

### CliTransport

```typescript
import { performance } from "perf_hooks"; // available in Node.js >= 16; for older versions use Date.now()

async exec(sandboxId, command, provider, timeoutSecs?, traceLevel?): Promise<ExecOutput> {
  const start = performance.now();
  // ... existing exec logic (both success and error/non-zero exit paths) ...
  const durationSecs = (performance.now() - start) / 1000;

  // Only return trace when traceLevel is explicitly provided and not "off"
  // (matches Python SDK: None/undefined = no trace from CLI, explicit level = duration-only trace)
  // NOTE: duration measurement must wrap BOTH the success path and the catch/error path
  // since the existing CliTransport uses try/catch for non-zero exit codes.
  const includeTrace = traceLevel !== undefined && traceLevel !== "off";

  return {
    exitCode,
    stdout,
    stderr,
    trace: includeTrace ? new ExecutionTrace({
      durationSecs,
      resourceUsage: { peakMemoryBytes: 0, cpuTimeSecs: 0, networkRxBytes: 0, networkTxBytes: 0 },
    }) : undefined,
  };
}
```

### Sandbox.exec()

```typescript
async exec(command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
  return this.transport.exec(this.id, command, this.provider, timeoutSecs, traceLevel);
}
```

### Roche.exec()

The `Roche` class also has an `exec()` method that delegates to the transport. It must also accept and pass through `traceLevel`:

```typescript
async exec(sandboxId: string, command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
  const transport = await this.getTransport();
  return transport.exec(sandboxId, command, this.provider, timeoutSecs, traceLevel);
}
```

## Proto Regeneration

Run `cd sdk/typescript && bash scripts/proto-gen.sh` to regenerate `src/generated/roche/v1/sandbox.ts` with the trace messages already in `proto/roche/v1/sandbox.proto`.

## Backward Compatibility

| Scenario | Behavior |
|---|---|
| New TS SDK + old daemon | `trace` field absent in response, `result.trace` is `undefined` |
| New TS SDK + new daemon | Full trace returned |
| New TS SDK + CLI fallback | Duration-only trace when `traceLevel` explicitly set and not `"off"`; `undefined` otherwise |

## Testing Strategy

### Unit Tests
- `ExecutionTrace.summary()` formatting (basic, empty, with blocked ops)
- `TraceLevel` type validation
- `protoToExecutionTrace` mapping

### Transport Tests
- `GrpcTransport.exec()` passes `traceLevel` to proto request
- `GrpcTransport.exec()` parses trace from proto response
- `CliTransport.exec()` returns duration-only trace
- `CliTransport.exec()` returns no trace when `traceLevel: "off"`
- `CliTransport.exec()` returns no trace when `traceLevel` is `undefined`

### Roche.exec() Tests
- `Roche.exec()` passes `traceLevel` through to transport

### Existing Test Updates
- All existing tests that construct `ExecOutput` or mock transport `exec()` may need `trace` field added

## Files Changed

### New Files
- `sdk/typescript/src/trace.ts` — `ExecutionTrace` class, trace sub-types, `protoToExecutionTrace`
- `sdk/typescript/test/trace.test.ts` — trace type and summary tests

### Modified Files
- `sdk/typescript/src/types.ts` — `ExecOutput.trace` field (imports `ExecutionTrace` from `./trace`; no circular dependency since `trace.ts` does not import from `types.ts`)
- `sdk/typescript/src/index.ts` — re-export `ExecutionTrace`, `TraceLevel`, `ResourceUsage`, `FileAccess`, `NetworkAttempt`, `BlockedOperation`, `SyscallEvent`, `ResourceSnapshot`
- `sdk/typescript/src/transport/index.ts` — `traceLevel` param in `Transport.exec()`
- `sdk/typescript/src/transport/grpc.ts` — trace_level mapping, response parsing
- `sdk/typescript/src/transport/cli.ts` — duration measurement, basic trace return
- `sdk/typescript/src/sandbox.ts` — `traceLevel` param passthrough
- `sdk/typescript/src/roche.ts` — `traceLevel` param passthrough in `Roche.exec()`
- `sdk/typescript/src/generated/roche/v1/sandbox.ts` — regenerated from proto
- `sdk/typescript/test/sandbox.test.ts` — updated for trace param
- `sdk/typescript/test/transport/grpc.test.ts` — trace mapping tests
- `sdk/typescript/test/transport/cli.test.ts` — trace fallback tests
- `sdk/typescript/test/roche.test.ts` — trace passthrough tests
