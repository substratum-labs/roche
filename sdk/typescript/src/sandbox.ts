// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import type { Transport } from "./transport";
import type { ExecOutput } from "./types";
import type { TraceLevel } from "./trace";

export class Sandbox {
  constructor(
    public readonly id: string,
    public readonly provider: string,
    private readonly transport: Transport,
  ) {}

  async exec(command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
    return this.transport.exec(this.id, command, this.provider, timeoutSecs, traceLevel);
  }

  async pause(): Promise<void> { await this.transport.pause(this.id, this.provider); }
  async unpause(): Promise<void> { await this.transport.unpause(this.id, this.provider); }
  async destroy(): Promise<void> { await this.transport.destroy([this.id], this.provider); }

  async copyTo(hostPath: string, sandboxPath: string): Promise<void> {
    await this.transport.copyTo(this.id, hostPath, sandboxPath, this.provider);
  }

  async copyFrom(sandboxPath: string, hostPath: string): Promise<void> {
    await this.transport.copyFrom(this.id, sandboxPath, hostPath, this.provider);
  }

  async [Symbol.asyncDispose](): Promise<void> { await this.destroy(); }
}
