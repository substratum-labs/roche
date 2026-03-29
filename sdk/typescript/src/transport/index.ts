// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import type { SandboxConfig, ExecOutput, SandboxInfo } from "../types";
import type { TraceLevel } from "../trace";

export interface Transport {
  create(config: SandboxConfig, provider: string): Promise<string>;
  exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,
    idempotencyKey?: string,
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
