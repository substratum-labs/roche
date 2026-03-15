// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

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

const Status = {
  NOT_FOUND: 5,
  DEADLINE_EXCEEDED: 4,
  FAILED_PRECONDITION: 9,
  UNIMPLEMENTED: 12,
  UNAVAILABLE: 14,
};

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
      const { SandboxServiceClient } = await import("../generated/roche/v1/sandbox.js");
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

  async exec(sandboxId: string, command: string[], provider: string, timeoutSecs?: number): Promise<ExecOutput> {
    const response = await this.call("exec", { sandboxId, command, provider, timeoutSecs });
    return { exitCode: response.exitCode, stdout: response.stdout, stderr: response.stderr };
  }

  async destroy(sandboxIds: string[], provider: string, all?: boolean): Promise<string[]> {
    const response = await this.call("destroy", { sandboxIds, all: all ?? false, provider });
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

  async gc(provider: string, dryRun?: boolean, all?: boolean): Promise<string[]> {
    const response = await this.call("gc", { dryRun: dryRun ?? false, all: all ?? false, provider });
    return response.destroyedIds ?? [];
  }

  async copyTo(sandboxId: string, hostPath: string, sandboxPath: string, provider: string): Promise<void> {
    await this.call("copyTo", { sandboxId, hostPath, sandboxPath, provider });
  }

  async copyFrom(sandboxId: string, sandboxPath: string, hostPath: string, provider: string): Promise<void> {
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
      case Status.NOT_FOUND: return new SandboxNotFound(details);
      case Status.FAILED_PRECONDITION: return new SandboxPaused(details);
      case Status.UNAVAILABLE: return new ProviderUnavailable(details);
      case Status.DEADLINE_EXCEEDED: return new TimeoutError(details);
      case Status.UNIMPLEMENTED: return new UnsupportedOperation(details);
      default: return new RocheError(details);
    }
  }
}
