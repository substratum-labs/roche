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
