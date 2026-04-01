// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { Roche } from "./roche";
import type { ExecOutput } from "./types";
import type { TraceLevel } from "./trace";

const execFileAsync = promisify(execFile);

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

// ---------------------------------------------------------------------------
// Parallel execution
// ---------------------------------------------------------------------------

export interface ParallelTask extends RunOptions {
  /** The code to execute. */
  code: string;
}

export interface ParallelResult {
  /** Ordered results corresponding to each input task. */
  results: Array<{ output?: ExecOutput; error?: Error }>;
  /** Number of tasks that exited with code 0. */
  totalSucceeded: number;
  /** Number of tasks that failed (non-zero exit or threw). */
  totalFailed: number;
}

export interface ParallelOptions {
  /** Maximum number of tasks to run concurrently. Default: 5. */
  maxConcurrency?: number;
}

/**
 * Execute multiple code snippets in parallel sandboxes.
 *
 * @example
 * ```ts
 * const result = await runParallel([
 *   { code: "print(1)" },
 *   { code: "print(2)", language: "python" },
 * ]);
 * console.log(result.totalSucceeded);
 * ```
 */
export async function runParallel(
  tasks: ParallelTask[],
  opts: ParallelOptions = {},
): Promise<ParallelResult> {
  const maxConcurrency = opts.maxConcurrency ?? 5;
  const results: Array<{ output?: ExecOutput; error?: Error }> = new Array(tasks.length);
  let totalSucceeded = 0;
  let totalFailed = 0;

  // Semaphore pattern: limit concurrent promises
  let running = 0;
  let idx = 0;
  const queue: Array<() => void> = [];

  function release(): void {
    running--;
    if (queue.length > 0) {
      const next = queue.shift()!;
      next();
    }
  }

  function acquire(): Promise<void> {
    if (running < maxConcurrency) {
      running++;
      return Promise.resolve();
    }
    return new Promise<void>((resolve) => {
      queue.push(() => {
        running++;
        resolve();
      });
    });
  }

  const promises = tasks.map(async (task, i) => {
    await acquire();
    try {
      const output = await run(task.code, task);
      results[i] = { output };
      if (output.exitCode === 0) totalSucceeded++;
      else totalFailed++;
    } catch (err) {
      results[i] = { error: err instanceof Error ? err : new Error(String(err)) };
      totalFailed++;
    } finally {
      release();
    }
  });

  await Promise.all(promises);
  return { results, totalSucceeded, totalFailed };
}

// ---------------------------------------------------------------------------
// Snapshot / Restore
// ---------------------------------------------------------------------------

export interface Snapshot {
  /** The committed image ID. */
  snapshotId: string;
  /** The source sandbox (container) ID. */
  sandboxId: string;
  /** Provider that created the sandbox. */
  provider: string;
  /** Docker image name for the snapshot. */
  image: string;
}

/**
 * Commit a running sandbox to a Docker image (snapshot).
 *
 * @param sandboxId - Container ID of the sandbox to snapshot.
 * @param provider  - Provider name (default: "docker").
 * @returns A {@link Snapshot} with the committed image details.
 */
export async function snapshot(
  sandboxId: string,
  provider: string = "docker",
): Promise<Snapshot> {
  const image = `roche-snapshot-${sandboxId}`;
  const { stdout } = await execFileAsync("docker", ["commit", sandboxId, image]);
  const snapshotId = stdout.trim().replace(/^sha256:/, "").slice(0, 12);
  return { snapshotId, sandboxId, provider, image };
}

/**
 * Restore a snapshot: create a sandbox from the snapshot image, execute a
 * command, destroy the sandbox, and return the output.
 *
 * @param snap    - The {@link Snapshot} to restore from.
 * @param command - Shell command to execute inside the restored sandbox.
 * @param opts    - Optional {@link RunOptions} overrides.
 * @returns The {@link ExecOutput} from executing the command.
 */
export async function restore(
  snap: Snapshot,
  command: string[],
  opts: RunOptions = {},
): Promise<ExecOutput> {
  const timeoutSecs = opts.timeoutSecs ?? 30;
  const client = new Roche();
  const sandbox = await client.createSandbox({
    image: snap.image,
    timeoutSecs,
    network: opts.network ?? false,
    writable: opts.writable ?? false,
    memory: opts.memory,
  });

  try {
    return await sandbox.exec(command, timeoutSecs, opts.traceLevel ?? "summary");
  } finally {
    await sandbox.destroy();
  }
}

/**
 * Delete a snapshot image.
 *
 * @param snap - The {@link Snapshot} to delete.
 */
export async function deleteSnapshot(snap: Snapshot): Promise<void> {
  await execFileAsync("docker", ["rmi", snap.image]);
}
