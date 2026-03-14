import { describe, it, expect } from "vitest";
import type { SandboxConfig, ExecOutput, SandboxInfo, Mount } from "../src/types";

describe("types", () => {
  it("SandboxConfig has correct defaults", () => {
    const config: SandboxConfig = {};
    expect(config).toBeDefined();
  });

  it("ExecOutput shape", () => {
    const output: ExecOutput = { exitCode: 0, stdout: "hi", stderr: "" };
    expect(output.exitCode).toBe(0);
  });

  it("Mount shape", () => {
    const mount: Mount = { hostPath: "/a", containerPath: "/b" };
    expect(mount.hostPath).toBe("/a");
    expect(mount.readonly).toBeUndefined();
  });

  it("SandboxInfo shape", () => {
    const info: SandboxInfo = {
      id: "abc",
      status: "running",
      provider: "docker",
      image: "python:3.12-slim",
    };
    expect(info.status).toBe("running");
  });
});
