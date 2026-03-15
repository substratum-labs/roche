import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { detectDaemon, type DaemonInfo } from "../src/daemon";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

vi.mock("fs");

describe("detectDaemon", () => {
  const daemonJsonPath = path.join(os.homedir(), ".roche", "daemon.json");

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("returns null when daemon.json does not exist", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(false);
    const result = await detectDaemon();
    expect(result).toBeNull();
  });

  it("returns null when daemon.json is malformed", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue("not json");
    const result = await detectDaemon();
    expect(result).toBeNull();
  });

  it("returns DaemonInfo when daemon.json is valid and pid is alive", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ pid: process.pid, port: 50051 })
    );
    const result = await detectDaemon();
    expect(result).toEqual({ pid: process.pid, port: 50051 });
  });

  it("returns null when pid is not alive", async () => {
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ pid: 999999999, port: 50051 })
    );
    const result = await detectDaemon();
    expect(result).toBeNull();
  });
});
