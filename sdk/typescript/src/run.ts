// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import { Roche } from "./roche";
import type { ExecOutput } from "./types";
import type { TraceLevel } from "./trace";

export interface RunOptions {
  /** Language hint: 'python', 'node', 'bash', 'auto'. Default: 'auto'. */
  language?: string;
  /** Maximum execution time in seconds. Default: 30. */
  timeoutSecs?: number;
  /** Allow network access. Default: false. */
  network?: boolean;
  /** Restrict network to these hosts (requires network=true). */
  networkAllowlist?: string[];
  /** Allow filesystem writes. Default: false. */
  writable?: boolean;
  /** Writable filesystem paths. */
  fsPaths?: string[];
  /** Memory limit (e.g., '256m'). */
  memory?: string;
  /** Trace level. Default: 'summary'. */
  traceLevel?: TraceLevel;
}

interface LanguageConfig {
  image: string;
  command: (code: string) => string[];
}

const LANGUAGE_CONFIG: Record<string, LanguageConfig> = {
  python: {
    image: "python:3.12-slim",
    command: (code) => ["python3", "-c", code],
  },
  node: {
    image: "node:20-slim",
    command: (code) => ["node", "-e", code],
  },
  bash: {
    image: "ubuntu:22.04",
    command: (code) => ["bash", "-c", code],
  },
};

function detectLanguage(code: string): string {
  const indicators: Record<string, string[]> = {
    python: ["import ", "def ", "print(", "from ", "class "],
    node: ["console.log", "require(", "const ", "let ", "function ", "=>"],
    bash: ["#!/bin/bash", "echo ", "grep ", "awk ", "curl "],
  };
  const scores: Record<string, number> = { python: 0, node: 0, bash: 0 };
  for (const [lang, keywords] of Object.entries(indicators)) {
    for (const kw of keywords) {
      if (code.includes(kw)) scores[lang]++;
    }
  }
  const best = Object.entries(scores).sort((a, b) => b[1] - a[1])[0];
  return best[1] > 0 ? best[0] : "python";
}

/**
 * Execute code in a sandbox and return the result. One-liner API.
 *
 * @example
 * ```ts
 * import { run } from 'roche-sandbox';
 *
 * const result = await run("print(2 + 2)");
 * console.log(result.stdout); // "4\n"
 * ```
 */
export async function run(
  code: string,
  opts: RunOptions = {},
): Promise<ExecOutput> {
  const lang = opts.language === "auto" || !opts.language
    ? detectLanguage(code)
    : opts.language;

  const config = LANGUAGE_CONFIG[lang] ?? LANGUAGE_CONFIG.python;
  const command = config.command(code);
  const timeoutSecs = opts.timeoutSecs ?? 30;

  const client = new Roche();
  const sandbox = await client.createSandbox({
    image: config.image,
    timeoutSecs,
    network: opts.network ?? !!opts.networkAllowlist?.length,
    writable: opts.writable ?? !!opts.fsPaths?.length,
    memory: opts.memory,
    networkAllowlist: opts.networkAllowlist,
    fsPaths: opts.fsPaths,
  });

  try {
    return await sandbox.exec(command, timeoutSecs, opts.traceLevel ?? "summary");
  } finally {
    await sandbox.destroy();
  }
}
