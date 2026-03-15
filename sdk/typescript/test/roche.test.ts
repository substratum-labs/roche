import { describe, it, expect, vi, beforeEach } from "vitest";
import { Roche } from "../src/roche";
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

describe("Roche", () => {
  let transport: Transport;
  beforeEach(() => { transport = mockTransport(); });

  it("createSandbox returns a Sandbox object", async () => {
    const roche = new Roche({ transport });
    const sb = await roche.createSandbox({ image: "node:20" });
    expect(sb).toBeInstanceOf(Sandbox);
    expect(sb.id).toBe("sandbox-1");
    expect(sb.provider).toBe("docker");
    expect(transport.create).toHaveBeenCalledWith(expect.objectContaining({ image: "node:20" }), "docker");
  });

  it("createSandbox captures provider from config", async () => {
    const roche = new Roche({ transport });
    const sb = await roche.createSandbox({ provider: "firecracker" });
    expect(sb.provider).toBe("firecracker");
    expect(transport.create).toHaveBeenCalledWith(expect.anything(), "firecracker");
  });

  it("create returns sandbox ID string", async () => {
    const roche = new Roche({ transport });
    const id = await roche.create({ image: "python:3.12-slim" });
    expect(id).toBe("sandbox-1");
  });

  it("exec delegates to transport with default provider", async () => {
    const roche = new Roche({ transport });
    await roche.exec("abc", ["echo", "hello"]);
    expect(transport.exec).toHaveBeenCalledWith("abc", ["echo", "hello"], "docker", undefined);
  });

  it("destroy delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.destroy("abc");
    expect(transport.destroy).toHaveBeenCalledWith(["abc"], "docker");
  });

  it("list delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.list();
    expect(transport.list).toHaveBeenCalledWith("docker");
  });

  it("gc delegates to transport", async () => {
    const roche = new Roche({ transport });
    await roche.gc();
    expect(transport.gc).toHaveBeenCalledWith("docker", undefined, undefined);
  });

  it("uses custom default provider", async () => {
    const roche = new Roche({ transport, provider: "firecracker" });
    await roche.list();
    expect(transport.list).toHaveBeenCalledWith("firecracker");
  });
});
