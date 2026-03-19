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
