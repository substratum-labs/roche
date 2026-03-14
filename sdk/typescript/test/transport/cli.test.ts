import { describe, it, expect, vi, beforeEach } from "vitest";
import { CliTransport } from "../../src/transport/cli";
import { execFile } from "child_process";
import { RocheError, SandboxNotFound, ProviderUnavailable } from "../../src/errors";

// Mock child_process.execFile
vi.mock("child_process", () => ({
  execFile: vi.fn(),
}));

const mockExecFile = vi.mocked(execFile);

function mockSuccess(stdout: string, stderr = "") {
  mockExecFile.mockImplementation(
    (_file: any, _args: any, _opts: any, cb: any) => {
      cb(null, stdout, stderr);
      return {} as any;
    }
  );
}

function mockError(code: number, stderr: string) {
  mockExecFile.mockImplementation(
    (_file: any, _args: any, _opts: any, cb: any) => {
      const err = Object.assign(new Error("exit " + code), {
        code,
        stdout: "",
        stderr,
      });
      cb(err, "", stderr);
      return {} as any;
    }
  );
}

describe("CliTransport", () => {
  let transport: CliTransport;

  beforeEach(() => {
    transport = new CliTransport("roche");
    vi.clearAllMocks();
  });

  it("create builds correct args and returns sandbox ID", async () => {
    mockSuccess("abc123def456\n");
    const id = await transport.create(
      { image: "python:3.12-slim", network: true },
      "docker"
    );
    expect(id).toBe("abc123def456");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("create");
    expect(args).toContain("--provider");
    expect(args).toContain("docker");
    expect(args).toContain("--image");
    expect(args).toContain("python:3.12-slim");
    expect(args).toContain("--network");
  });

  it("create uses defaults for missing config fields", async () => {
    mockSuccess("id1\n");
    await transport.create({}, "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("python:3.12-slim");
    expect(args).toContain("300");
    expect(args).not.toContain("--network");
    expect(args).not.toContain("--writable");
  });

  it("exec returns ExecOutput for successful commands", async () => {
    mockSuccess("hello\n", "");
    const output = await transport.exec("abc", ["echo", "hello"], "docker");
    expect(output.exitCode).toBe(0);
    expect(output.stdout).toBe("hello\n");
  });

  it("exec returns non-zero exit for command failures", async () => {
    mockError(1, "command failed");
    const output = await transport.exec("abc", ["false"], "docker");
    expect(output.exitCode).toBe(1);
  });

  it("exec throws SandboxNotFound when stderr contains 'not found'", async () => {
    mockError(1, "Error: sandbox not found");
    await expect(
      transport.exec("abc", ["echo"], "docker")
    ).rejects.toBeInstanceOf(SandboxNotFound);
  });

  it("destroy calls with sandbox IDs", async () => {
    mockSuccess("abc\ndef\n");
    const destroyed = await transport.destroy(["abc", "def"], "docker");
    expect(destroyed).toEqual(["abc", "def"]);
  });

  it("list parses JSON output", async () => {
    mockSuccess(
      JSON.stringify([
        { id: "abc", status: "running", provider: "docker", image: "python:3.12-slim" },
      ])
    );
    const sandboxes = await transport.list("docker");
    expect(sandboxes).toHaveLength(1);
    expect(sandboxes[0].id).toBe("abc");
    expect(sandboxes[0].status).toBe("running");
  });

  it("pause sends correct args", async () => {
    mockSuccess("");
    await transport.pause("abc", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("pause");
    expect(args).toContain("abc");
  });

  it("copyTo maps to roche cp syntax", async () => {
    mockSuccess("");
    await transport.copyTo("abc", "/local/f.py", "/sandbox/f.py", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("cp");
    expect(args).toContain("/local/f.py");
    expect(args).toContain("abc:/sandbox/f.py");
  });

  it("copyFrom maps to roche cp syntax", async () => {
    mockSuccess("");
    await transport.copyFrom("abc", "/sandbox/out.txt", "/local/out.txt", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("cp");
    expect(args).toContain("abc:/sandbox/out.txt");
    expect(args).toContain("/local/out.txt");
  });

  it("unpause sends correct args", async () => {
    mockSuccess("");
    await transport.unpause("abc", "docker");
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("unpause");
    expect(args).toContain("abc");
  });

  it("gc sends correct args with flags", async () => {
    mockSuccess("old1\nold2\n");
    const destroyed = await transport.gc("docker", true, true);
    expect(destroyed).toEqual(["old1", "old2"]);
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("gc");
    expect(args).toContain("--dry-run");
    expect(args).toContain("--all");
  });

  it("destroy with --all flag", async () => {
    mockSuccess("abc\ndef\n");
    await transport.destroy([], "docker", true);
    const args = mockExecFile.mock.calls[0][1] as string[];
    expect(args).toContain("destroy");
    expect(args).toContain("--all");
  });

  it("throws ProviderUnavailable when binary not found", async () => {
    mockExecFile.mockImplementation(
      (_file: any, _args: any, _opts: any, cb: any) => {
        const err = Object.assign(new Error("ENOENT"), { code: "ENOENT" });
        cb(err, "", "");
        return {} as any;
      }
    );
    await expect(
      transport.create({}, "docker")
    ).rejects.toBeInstanceOf(ProviderUnavailable);
  });
});
