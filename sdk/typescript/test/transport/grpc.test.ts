import { describe, it, expect, vi, beforeEach } from "vitest";
import { GrpcTransport } from "../../src/transport/grpc";
import {
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
  RocheError,
} from "../../src/errors";

describe("GrpcTransport", () => {
  let transport: GrpcTransport;
  let mockClient: any;

  beforeEach(() => {
    mockClient = {
      create: vi.fn(),
      exec: vi.fn(),
      destroy: vi.fn(),
      list: vi.fn(),
      pause: vi.fn(),
      unpause: vi.fn(),
      gc: vi.fn(),
      copyTo: vi.fn(),
      copyFrom: vi.fn(),
    };
    transport = new GrpcTransport(50051, mockClient);
  });

  it("create sends correct request and returns sandbox ID", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) => cb(null, { sandboxId: "abc123" })
    );
    const id = await transport.create({ image: "node:20" }, "docker");
    expect(id).toBe("abc123");
    expect(mockClient.create).toHaveBeenCalledWith(
      expect.objectContaining({ provider: "docker", image: "node:20" }),
      expect.any(Function),
    );
  });

  it("exec returns ExecOutput", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "hi", stderr: "" })
    );
    const output = await transport.exec("abc", ["echo", "hi"], "docker");
    expect(output).toEqual({ exitCode: 0, stdout: "hi", stderr: "" });
  });

  it("maps NOT_FOUND to SandboxNotFound", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) => cb({ code: 5, details: "not found" }, null)
    );
    await expect(transport.create({}, "docker")).rejects.toBeInstanceOf(SandboxNotFound);
  });

  it("maps FAILED_PRECONDITION to SandboxPaused", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb({ code: 9, details: "paused" }, null)
    );
    await expect(transport.exec("abc", ["echo"], "docker")).rejects.toBeInstanceOf(SandboxPaused);
  });

  it("maps UNAVAILABLE to ProviderUnavailable", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb({ code: 14, details: "unavailable" }, null)
    );
    await expect(transport.exec("abc", ["echo"], "docker")).rejects.toBeInstanceOf(ProviderUnavailable);
  });

  it("maps DEADLINE_EXCEEDED to TimeoutError", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb({ code: 4, details: "deadline" }, null)
    );
    await expect(transport.exec("abc", ["echo"], "docker")).rejects.toBeInstanceOf(TimeoutError);
  });

  it("maps UNIMPLEMENTED to UnsupportedOperation", async () => {
    mockClient.pause.mockImplementation(
      (req: any, cb: any) => cb({ code: 12, details: "unimplemented" }, null)
    );
    await expect(transport.pause("abc", "docker")).rejects.toBeInstanceOf(UnsupportedOperation);
  });

  it("maps other errors to RocheError", async () => {
    mockClient.create.mockImplementation(
      (req: any, cb: any) => cb({ code: 13, details: "internal" }, null)
    );
    await expect(transport.create({}, "docker")).rejects.toBeInstanceOf(RocheError);
  });

  it("list returns SandboxInfo array", async () => {
    mockClient.list.mockImplementation(
      (req: any, cb: any) => cb(null, {
        sandboxes: [{ id: "abc", status: 1, provider: "docker", image: "python:3.12-slim" }],
      })
    );
    const list = await transport.list("docker");
    expect(list).toHaveLength(1);
    expect(list[0].status).toBe("running");
  });

  it("destroy sends IDs and returns destroyed list", async () => {
    mockClient.destroy.mockImplementation(
      (req: any, cb: any) => cb(null, { destroyedIds: ["abc", "def"] })
    );
    const destroyed = await transport.destroy(["abc", "def"], "docker");
    expect(destroyed).toEqual(["abc", "def"]);
  });

  it("gc sends flags and returns destroyed list", async () => {
    mockClient.gc.mockImplementation(
      (req: any, cb: any) => cb(null, { destroyedIds: ["old1"] })
    );
    const destroyed = await transport.gc("docker", true, false);
    expect(destroyed).toEqual(["old1"]);
  });

  it("copyTo sends correct request", async () => {
    mockClient.copyTo.mockImplementation((req: any, cb: any) => cb(null, {}));
    await transport.copyTo("abc", "/local/f.py", "/sandbox/f.py", "docker");
    expect(mockClient.copyTo).toHaveBeenCalledWith(
      expect.objectContaining({ sandboxId: "abc", hostPath: "/local/f.py", sandboxPath: "/sandbox/f.py" }),
      expect.any(Function),
    );
  });

  it("copyFrom sends correct request", async () => {
    mockClient.copyFrom.mockImplementation((req: any, cb: any) => cb(null, {}));
    await transport.copyFrom("abc", "/sandbox/out.txt", "/local/out.txt", "docker");
    expect(mockClient.copyFrom).toHaveBeenCalledWith(
      expect.objectContaining({ sandboxId: "abc", sandboxPath: "/sandbox/out.txt", hostPath: "/local/out.txt" }),
      expect.any(Function),
    );
  });

  it("exec passes traceLevel to proto request", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    await transport.exec("abc", ["echo"], "docker", undefined, "full");
    expect(mockClient.exec).toHaveBeenCalledWith(
      expect.objectContaining({ traceLevel: 3 }),
      expect.any(Function),
    );
  });

  it("exec omits traceLevel from request when undefined", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    await transport.exec("abc", ["echo"], "docker");
    const requestArg = mockClient.exec.mock.calls[0][0];
    expect(requestArg.traceLevel).toBeUndefined();
  });

  it("exec parses trace from proto response", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, {
        exitCode: 0,
        stdout: "ok",
        stderr: "",
        trace: {
          durationSecs: 1.5,
          resourceUsage: {
            peakMemoryBytes: 100_000_000,
            cpuTimeSecs: 0.5,
            networkRxBytes: 0,
            networkTxBytes: 0,
          },
          fileAccesses: [{ path: "/tmp/out.txt", op: 2, sizeBytes: 100 }],
          networkAttempts: [],
          blockedOps: [],
          syscalls: [],
          resourceTimeline: [],
        },
      })
    );
    const output = await transport.exec("abc", ["echo"], "docker", undefined, "standard");
    expect(output.trace).toBeDefined();
    expect(output.trace!.durationSecs).toBe(1.5);
    expect(output.trace!.fileAccesses[0].op).toBe("create");
    expect(output.trace!.fileAccesses[0].path).toBe("/tmp/out.txt");
  });

  it("exec returns undefined trace when response has no trace", async () => {
    mockClient.exec.mockImplementation(
      (req: any, cb: any) => cb(null, { exitCode: 0, stdout: "", stderr: "" })
    );
    const output = await transport.exec("abc", ["echo"], "docker");
    expect(output.trace).toBeUndefined();
  });
});
