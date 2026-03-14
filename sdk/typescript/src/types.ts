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
}

export interface SandboxInfo {
  id: string;
  status: SandboxStatus;
  provider: string;
  image: string;
  expiresAt?: number;
}

export const DEFAULTS = {
  provider: "docker",
  image: "python:3.12-slim",
  timeoutSecs: 300,
  network: false,
  writable: false,
} as const;
