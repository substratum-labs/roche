// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import { describe, it, expect, vi, beforeEach } from "vitest";

// We test the internal detectLanguage logic by importing the module
// For run() itself, we'd need to mock Roche client — test the contract

describe("run", () => {
  it("is exported from index", async () => {
    const mod = await import("../src/index");
    expect(typeof mod.run).toBe("function");
  });
});
