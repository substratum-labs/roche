import { describe, it, expect, vi, beforeEach } from "vitest";
import { Sandbox } from "../src/sandbox";
import type { Transport } from "../src/transport";

function mockTransport(): Transport {
  return {
    create: vi.fn().mockResolvedValue("sandbox-1"),
    exec: vi.fn().mockResolvedValue({ exitCode: 0, stdout: "ok", stderr: "" }),
    destroy: vi.fn().mockResolvedValue(["sandbox-1"]),
    list: vi.fn().mockResolvedValue([]),
    pause: vi.fn().mockResolvedValue(undefined),
    unpause: vi.fn().mockResolvedValue(undefined),
    gc: vi.fn().mockResolvedValue([]),
    copyTo: vi.fn().mockResolvedValue(undefined),
    copyFrom: vi.fn().mockResolvedValue(undefined),
  };
}

describe("Sandbox", () => {
  let transport: Transport;
  beforeEach(() => { transport = mockTransport(); });

  it("stores sandboxId and provider", () => {
    const sb = new Sandbox("abc", "docker", transport);
    expect(sb.id).toBe("abc");
    expect(sb.provider).toBe("docker");
  });

  it("exec delegates to transport with stored provider", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    const output = await sb.exec(["echo", "hi"]);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, undefined);
    expect(output.exitCode).toBe(0);
  });

  it("exec passes timeout", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["sleep", "10"], 5);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["sleep", "10"], "docker", 5, undefined);
  });

  it("exec passes traceLevel to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["echo", "hi"], undefined, "full");
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, "full");
  });

  it("exec passes undefined traceLevel by default", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.exec(["echo", "hi"]);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hi"], "docker", undefined, undefined);
  });

  it("pause delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.pause();
    expect(transport.pause).toHaveBeenCalledWith("abc", "docker");
  });

  it("unpause delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.unpause();
    expect(transport.unpause).toHaveBeenCalledWith("abc", "docker");
  });

  it("destroy delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.destroy();
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });

  it("copyTo delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.copyTo("/local/f.py", "/sandbox/f.py");
    expect(transport.copyTo).toHaveBeenCalledWith("abc", "/local/f.py", "/sandbox/f.py", "docker");
  });

  it("copyFrom delegates to transport", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb.copyFrom("/sandbox/out.txt", "/local/out.txt");
    expect(transport.copyFrom).toHaveBeenCalledWith("abc", "/sandbox/out.txt", "/local/out.txt", "docker");
  });

  it("asyncDispose calls destroy", async () => {
    const sb = new Sandbox("abc", "docker", transport);
    await sb[Symbol.asyncDispose]();
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });
});
