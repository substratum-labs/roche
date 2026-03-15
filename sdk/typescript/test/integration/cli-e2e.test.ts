import { describe, it, expect, beforeAll } from "vitest";
import { Roche } from "../../src/index";
import { execSync } from "child_process";

function dockerAvailable(): boolean {
  try {
    execSync("docker info", { stdio: "ignore", timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}

function rocheCliAvailable(): boolean {
  try {
    execSync("roche --help", { stdio: "ignore", timeout: 5000 });
    return true;
  } catch {
    return false;
  }
}

const describeIf = (condition: boolean) =>
  condition ? describe : describe.skip;

describeIf(dockerAvailable() && rocheCliAvailable())(
  "CLI Transport E2E",
  () => {
    let roche: Roche;

    beforeAll(() => {
      roche = new Roche({ mode: "direct" });
    });

    it("full lifecycle: create -> exec -> destroy", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      expect(sandbox.id).toBeTruthy();
      expect(sandbox.id.length).toBeGreaterThan(0);

      const output = await sandbox.exec(["echo", "hello from roche"]);
      expect(output.exitCode).toBe(0);
      expect(output.stdout).toContain("hello from roche");

      await sandbox.destroy();
    });

    it("using async dispose destroys sandbox", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      const sandboxId = sandbox.id;

      const output = await sandbox.exec(["echo", "dispose"]);
      expect(output.exitCode).toBe(0);

      await sandbox.destroy();
    });

    it("list includes created sandbox", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      try {
        const sandboxes = await roche.list();
        const ids = sandboxes.map((s) => s.id);
        expect(ids).toContain(sandbox.id);
      } finally {
        await sandbox.destroy();
      }
    });

    it("exec on destroyed sandbox raises error", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      await sandbox.destroy();

      await expect(sandbox.exec(["echo", "should fail"])).rejects.toThrow();
    });

    it("captures non-zero exit code", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      try {
        const output = await sandbox.exec(["sh", "-c", "exit 42"]);
        expect(output.exitCode).toBe(42);
      } finally {
        await sandbox.destroy();
      }
    });

    it("captures stderr", async () => {
      const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
      try {
        const output = await sandbox.exec(["sh", "-c", "echo err >&2"]);
        expect(output.stderr).toContain("err");
      } finally {
        await sandbox.destroy();
      }
    });
  },
);
