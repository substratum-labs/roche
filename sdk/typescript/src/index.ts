// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

export { Roche } from "./roche";
export type { RocheOptions } from "./roche";
export { run } from "./run";
export type { RunOptions } from "./run";
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
