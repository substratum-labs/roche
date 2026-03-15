/**
 * Basic Roche usage — create a sandbox, run code, clean up.
 *
 * Run: npx tsx examples/typescript/basic.ts
 */
import { Roche } from "roche-sandbox";

async function main() {
  const roche = new Roche();

  // Create a sandbox with AI-safe defaults
  const sandbox = await roche.createSandbox({ image: "python:3.12-slim" });
  console.log(`Created sandbox: ${sandbox.id}`);

  // Execute a command
  const output = await sandbox.exec([
    "python3",
    "-c",
    "print('Hello from Roche!')",
  ]);
  console.log(`stdout: ${output.stdout.trim()}`);
  console.log(`exit code: ${output.exitCode}`);

  // List active sandboxes
  const sandboxes = await roche.list();
  console.log(`Active sandboxes: ${sandboxes.length}`);

  // Clean up
  await sandbox.destroy();
  console.log("Sandbox destroyed.");
}

main().catch(console.error);
