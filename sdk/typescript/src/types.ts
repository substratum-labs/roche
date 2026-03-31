// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import type { ExecutionTrace } from "./trace";

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
  networkAllowlist?: string[];
  fsPaths?: string[];
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
  trace?: ExecutionTrace;
}

export interface SandboxInfo {
  id: string;
  status: SandboxStatus;
  provider: string;
  image: string;
  expiresAt?: number;
}

export interface Budget {
  maxExecs?: number;
  maxTotalSecs?: number;
  maxOutputBytes?: number;
}

export interface BudgetUsage {
  execCount: number;
  totalSecs: number;
  outputBytes: number;
}

export interface DynamicPermissions {
  network: boolean;
  networkAllowlist: string[];
  writable: boolean;
  fsPaths: string[];
}

export interface SessionInfo {
  sessionId: string;
  sandboxId: string;
  provider: string;
  permissions: DynamicPermissions;
  budget: Budget;
  usage: BudgetUsage;
  createdAtMs: number;
}

export type PermissionChange =
  | { allowHost: string }
  | { denyHost: string }
  | { allowPath: string }
  | { denyPath: string }
  | { enableNetwork: true }
  | { disableNetwork: true };

export interface CodeIntent {
  provider: string;
  needsNetwork: boolean;
  networkHosts: string[];
  needsWritable: boolean;
  writablePaths: string[];
  needsPackages: boolean;
  packageManager?: string;
  memoryHint?: string;
  language: string;
  confidence: number;
  reasoning: string[];
}

export const DEFAULTS = {
  provider: "docker",
  image: "python:3.12-slim",
  timeoutSecs: 300,
  network: false,
  writable: false,
} as const;
