import type { SandboxConfig, ExecOutput, SandboxInfo } from "../types";

export interface Transport {
  create(config: SandboxConfig, provider: string): Promise<string>;
  exec(
    sandboxId: string,
    command: string[],
    provider: string,
    timeoutSecs?: number
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
