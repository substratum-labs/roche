// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import { execFile } from "child_process";
import { performance } from "perf_hooks";
import type { Transport } from "./index";
import type {
  SandboxConfig, ExecOutput, SandboxInfo, PoolInfo, Budget, DynamicPermissions,
  SessionInfo, PermissionChange, CodeIntent,
} from "../types";
import type { TraceLevel } from "../trace";
import { ExecutionTrace } from "../trace";
import { DEFAULTS } from "../types";
import {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "../errors";

export class CliTransport implements Transport {
  constructor(private readonly binary: string = "roche") {}

  async create(config: SandboxConfig, provider: string): Promise<string> {
    const args = [
      "create",
      "--provider", provider,
      "--image", config.image ?? DEFAULTS.image,
      "--timeout", String(config.timeoutSecs ?? DEFAULTS.timeoutSecs),
    ];
    if (config.memory) args.push("--memory", config.memory);
    if (config.cpus != null) args.push("--cpus", String(config.cpus));
    if (config.network) args.push("--network");
    if (config.writable) args.push("--writable");
    if (config.env) {
      for (const [k, v] of Object.entries(config.env)) {
        args.push("--env", `${k}=${v}`);
      }
    }
    if (config.mounts) {
      for (const m of config.mounts) {
        const mode = m.readonly !== false ? "ro" : "rw";
        args.push("--mount", `${m.hostPath}:${m.containerPath}:${mode}`);
      }
    }
    if (config.kernel) args.push("--kernel", config.kernel);
    if (config.rootfs) args.push("--rootfs", config.rootfs);
    if (config.networkAllowlist) {
      for (const host of config.networkAllowlist) {
        args.push("--network-allow", host);
      }
    }
    if (config.fsPaths) {
      for (const path of config.fsPaths) {
        args.push("--fs-path", path);
      }
    }

    const { stdout } = await this.run(args);
    return stdout.trim();
  }

  async exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number,
    traceLevel?: TraceLevel,
    idempotencyKey?: string,
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

  async destroy(
    sandboxIds: string[],
    provider: string,
    all?: boolean,
  ): Promise<string[]> {
    const args = ["destroy"];
    if (all) {
      args.push("--all");
    } else {
      args.push(...sandboxIds);
    }
    const { stdout } = await this.run(args);
    return stdout.trim().split("\n").filter(Boolean);
  }

  async list(provider: string): Promise<SandboxInfo[]> {
    const { stdout } = await this.run(["list", "--json"]);
    const raw = JSON.parse(stdout) as Array<{
      id: string;
      status: string;
      provider: string;
      image: string;
      expires_at?: number;
    }>;
    return raw.map((s) => ({
      id: s.id,
      status: s.status as SandboxInfo["status"],
      provider: s.provider,
      image: s.image,
      expiresAt: s.expires_at,
    }));
  }

  async pause(sandboxId: string, provider: string): Promise<void> {
    await this.run(["pause", sandboxId]);
  }

  async unpause(sandboxId: string, provider: string): Promise<void> {
    await this.run(["unpause", sandboxId]);
  }

  async gc(
    provider: string,
    dryRun?: boolean,
    all?: boolean,
  ): Promise<string[]> {
    const args = ["gc"];
    if (dryRun) args.push("--dry-run");
    if (all) args.push("--all");
    const { stdout } = await this.run(args);
    return stdout.trim().split("\n").filter(Boolean);
  }

  async copyTo(
    sandboxId: string,
    hostPath: string,
    sandboxPath: string,
    provider: string,
  ): Promise<void> {
    await this.run(["cp", hostPath, `${sandboxId}:${sandboxPath}`]);
  }

  async copyFrom(
    sandboxId: string,
    sandboxPath: string,
    hostPath: string,
    provider: string,
  ): Promise<void> {
    await this.run(["cp", `${sandboxId}:${sandboxPath}`, hostPath]);
  }

  async poolStatus(): Promise<PoolInfo[]> {
    throw new UnsupportedOperation("Pool management requires the daemon (roched)");
  }

  async poolWarmup(): Promise<void> {
    throw new UnsupportedOperation("Pool management requires the daemon (roched)");
  }

  async poolDrain(): Promise<number> {
    throw new UnsupportedOperation("Pool management requires the daemon (roched)");
  }

  async createSession(): Promise<string> {
    throw new UnsupportedOperation("Session management requires the daemon (roched)");
  }

  async destroySession(): Promise<SessionInfo> {
    throw new UnsupportedOperation("Session management requires the daemon (roched)");
  }

  async listSessions(): Promise<SessionInfo[]> {
    throw new UnsupportedOperation("Session management requires the daemon (roched)");
  }

  async updatePermissions(): Promise<DynamicPermissions> {
    throw new UnsupportedOperation("Session management requires the daemon (roched)");
  }

  async analyzeIntent(): Promise<CodeIntent> {
    throw new UnsupportedOperation("Intent analysis requires the daemon (roched)");
  }

  private run(
    args: string[],
    check = true,
  ): Promise<{ stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      execFile(this.binary, args, {}, (err, stdout, stderr) => {
        if (err) {
          if ((err as any).code === "ENOENT") {
            return reject(
              new ProviderUnavailable(
                `Roche binary not found: ${this.binary}`
              )
            );
          }
          if (check) {
            return reject(
              this.mapCliError((err as any).stderr ?? err.message)
            );
          }
          return reject(err);
        }
        resolve({ stdout: stdout ?? "", stderr: stderr ?? "" });
      });
    });
  }

  private isRocheError(stderr: string): boolean {
    return stderr.trimStart().startsWith("Error: ");
  }

  private mapCliError(stderr: string): RocheError {
    const lower = stderr.toLowerCase();
    if (lower.includes("not found")) return new SandboxNotFound(stderr);
    if (lower.includes("paused")) return new SandboxPaused(stderr);
    if (lower.includes("timeout")) return new TimeoutError(stderr);
    if (lower.includes("unsupported")) return new UnsupportedOperation(stderr);
    if (lower.includes("unavailable") || lower.includes("connection refused"))
      return new ProviderUnavailable(stderr);
    return new RocheError(stderr);
  }
}
