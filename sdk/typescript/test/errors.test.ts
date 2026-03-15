import { describe, it, expect } from "vitest";
import {
  RocheError,
  SandboxNotFound,
  SandboxPaused,
  ProviderUnavailable,
  TimeoutError,
  UnsupportedOperation,
} from "../src/errors";

describe("errors", () => {
  it("SandboxNotFound is instanceof RocheError", () => {
    const err = new SandboxNotFound("abc123");
    expect(err).toBeInstanceOf(RocheError);
    expect(err).toBeInstanceOf(SandboxNotFound);
    expect(err.message).toContain("abc123");
  });

  it("SandboxPaused is instanceof RocheError", () => {
    const err = new SandboxPaused("abc123");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("ProviderUnavailable is instanceof RocheError", () => {
    const err = new ProviderUnavailable("daemon down");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("TimeoutError is instanceof RocheError", () => {
    const err = new TimeoutError("30s");
    expect(err).toBeInstanceOf(RocheError);
  });

  it("UnsupportedOperation is instanceof RocheError", () => {
    const err = new UnsupportedOperation("pause");
    expect(err).toBeInstanceOf(RocheError);
  });
});
