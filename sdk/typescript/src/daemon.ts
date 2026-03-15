// SPDX-License-Identifier: Apache-2.0
// Copyright 2025 Substratum Labs

import * as fs from "fs";
import * as os from "os";
import * as path from "path";

export interface DaemonInfo {
  pid: number;
  port: number;
}

export async function detectDaemon(): Promise<DaemonInfo | null> {
  const daemonPath = path.join(os.homedir(), ".roche", "daemon.json");

  if (!fs.existsSync(daemonPath)) return null;

  let data: { pid?: number; port?: number };
  try {
    data = JSON.parse(fs.readFileSync(daemonPath, "utf-8"));
  } catch {
    return null;
  }

  if (typeof data.pid !== "number" || typeof data.port !== "number") {
    return null;
  }

  if (!isProcessAlive(data.pid)) return null;

  return { pid: data.pid, port: data.port };
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
