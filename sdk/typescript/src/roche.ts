// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import type { Transport } from "./transport";
import { CliTransport } from "./transport/cli";
import { GrpcTransport } from "./transport/grpc";
import { detectDaemon } from "./daemon";
import { Sandbox } from "./sandbox";
import type {
  SandboxConfig, ExecOutput, SandboxInfo, PoolInfo, Budget, DynamicPermissions,
  SessionInfo, PermissionChange, CodeIntent,
} from "./types";
import type { TraceLevel } from "./trace";
import { DEFAULTS } from "./types";

export interface RocheOptions {
  mode?: "auto" | "direct";
  daemonPort?: number;
  provider?: string;
  binary?: string;
  transport?: Transport;
}

export class Roche {
  private readonly provider: string;
  private transportPromise: Promise<Transport>;

  constructor(options: RocheOptions = {}) {
    this.provider = options.provider ?? DEFAULTS.provider;
    if (options.transport) {
      this.transportPromise = Promise.resolve(options.transport);
    } else if (options.mode === "direct") {
      this.transportPromise = Promise.resolve(new CliTransport(options.binary ?? "roche"));
    } else {
      this.transportPromise = this.autoDetect(options);
    }
  }

  private async autoDetect(options: RocheOptions): Promise<Transport> {
    if (options.daemonPort) return new GrpcTransport(options.daemonPort);
    const daemon = await detectDaemon();
    if (daemon) return new GrpcTransport(daemon.port);
    return new CliTransport(options.binary ?? "roche");
  }

  private async getTransport(): Promise<Transport> { return this.transportPromise; }

  async createSandbox(config: SandboxConfig = {}): Promise<Sandbox> {
    const transport = await this.getTransport();
    const provider = config.provider ?? this.provider;
    const id = await transport.create(config, provider);
    return new Sandbox(id, provider, transport);
  }

  async create(config: SandboxConfig = {}): Promise<string> {
    const transport = await this.getTransport();
    return transport.create(config, config.provider ?? this.provider);
  }

  async exec(sandboxId: string, command: string[], timeoutSecs?: number, traceLevel?: TraceLevel): Promise<ExecOutput> {
    const transport = await this.getTransport();
    return transport.exec(sandboxId, command, this.provider, timeoutSecs, traceLevel);
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

  async poolStatus(): Promise<PoolInfo[]> {
    const transport = await this.getTransport();
    return transport.poolStatus();
  }

  async poolWarmup(): Promise<void> {
    const transport = await this.getTransport();
    await transport.poolWarmup();
  }

  async poolDrain(): Promise<number> {
    const transport = await this.getTransport();
    return transport.poolDrain();
  }

  async createSession(
    sandboxId: string,
    options?: { provider?: string; permissions?: DynamicPermissions; budget?: Budget },
  ): Promise<string> {
    const transport = await this.getTransport();
    return transport.createSession(
      sandboxId,
      options?.provider ?? this.provider,
      options?.permissions,
      options?.budget,
    );
  }

  async destroySession(sessionId: string): Promise<SessionInfo> {
    const transport = await this.getTransport();
    return transport.destroySession(sessionId);
  }

  async listSessions(): Promise<SessionInfo[]> {
    const transport = await this.getTransport();
    return transport.listSessions();
  }

  async updatePermissions(sessionId: string, change: PermissionChange): Promise<DynamicPermissions> {
    const transport = await this.getTransport();
    return transport.updatePermissions(sessionId, change);
  }

  async analyzeIntent(code: string, language: string = "python"): Promise<CodeIntent> {
    const transport = await this.getTransport();
    return transport.analyzeIntent(code, language);
  }
}
