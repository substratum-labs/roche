#!/usr/bin/env python3
"""OpenAI Agents SDK + Roche: multi-step code interpreter with file I/O."""

import asyncio
import os
import tempfile

from roche_sandbox import AsyncRoche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))

SAMPLE_CSV = """\
name,score
Alice,95
Bob,87
Charlie,92
Diana,88
Eve,91
"""

# Simulated multi-step responses
SIMULATED_STEPS = [
    # Step 1: read and analyze the CSV
    """\
import csv

with open("/data/scores.csv") as f:
    reader = csv.DictReader(f)
    rows = list(reader)

scores = [int(r["score"]) for r in rows]
avg = sum(scores) / len(scores)
top = max(rows, key=lambda r: int(r["score"]))

print(f"Average score: {avg:.1f}")
print(f"Top student: {top['name']} ({top['score']})")

with open("/output/summary.txt", "w") as f:
    f.write(f"Average: {avg:.1f}\\nTop: {top['name']}\\n")
print("Summary written to /output/summary.txt")
""",
]


async def main():
    roche = AsyncRoche()
    sandbox = await roche.create(image="python:3.12-slim", writable=True)

    try:
        print(f"Created sandbox: {sandbox.id}")

        # Upload sample data
        with tempfile.NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
            f.write(SAMPLE_CSV)
            tmp_csv = f.name

        await sandbox.exec(["mkdir", "-p", "/data", "/output"])
        await sandbox.copy_to(tmp_csv, "/data/scores.csv")
        print("Uploaded scores.csv to sandbox")

        if USE_REAL_LLM:
            from agents import Agent, Runner, function_tool

            @function_tool
            async def execute_code(code: str) -> str:
                """Execute Python code in the sandbox. Files at /data/ and /output/."""
                result = await sandbox.exec(["python3", "-c", code])
                output = result.stdout.strip()
                if result.exit_code != 0:
                    output = f"ERROR (exit {result.exit_code}):\n{result.stderr.strip()}"
                return output

            agent = Agent(
                name="DataAnalyst",
                instructions=(
                    "You analyze data files. CSV data is at /data/scores.csv. "
                    "Write results to /output/. Use execute_code to run Python."
                ),
                tools=[execute_code],
            )
            result = await Runner.run(
                agent, "Analyze scores.csv: find the average and top student."
            )
            print(f"\nAgent: {result.final_output}")
        else:
            # Simulated execution
            for i, code in enumerate(SIMULATED_STEPS, 1):
                print(f"\n--- Step {i} ---")
                print(f"Code:\n{code.strip()[:80]}...")
                result = await sandbox.exec(["python3", "-c", code])
                print(f"Output: {result.stdout.strip()}")
                if result.exit_code != 0:
                    print(f"Stderr: {result.stderr.strip()}")

        # Retrieve output
        with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as f:
            tmp_out = f.name
        await sandbox.copy_from("/output/summary.txt", tmp_out)
        with open(tmp_out) as f:
            print(f"\nRetrieved summary:\n{f.read()}")

    finally:
        await sandbox.destroy()
        print("Sandbox destroyed.")


if __name__ == "__main__":
    asyncio.run(main())
