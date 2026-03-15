#!/usr/bin/env python3
"""OpenAI Agents SDK + Roche: basic tool integration."""

import asyncio
import os

from roche_sandbox import AsyncRoche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))


async def execute_code_in_sandbox(code: str) -> str:
    """Execute Python code in a Roche sandbox and return the output."""
    roche = AsyncRoche()
    sandbox = await roche.create(image="python:3.12-slim")
    try:
        result = await sandbox.exec(["python3", "-c", code])
        output = result.stdout.strip()
        if result.exit_code != 0:
            output = f"ERROR (exit {result.exit_code}):\n{result.stderr.strip()}"
        return output
    finally:
        await sandbox.destroy()


async def main():
    if USE_REAL_LLM:
        from agents import Agent, Runner, function_tool

        @function_tool
        async def execute_code(code: str) -> str:
            """Execute Python code in a secure sandbox. Returns stdout or error."""
            return await execute_code_in_sandbox(code)

        agent = Agent(
            name="Coder",
            instructions="You write and execute Python code to solve tasks.",
            tools=[execute_code],
        )
        result = await Runner.run(agent, "Calculate fibonacci(10) and print the result.")
        print(f"Agent response: {result.final_output}")
    else:
        # Simulated: directly call the sandbox with hardcoded code
        print("[simulated] Running fibonacci in sandbox...")
        code = """\
def fib(n):
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a

print(f"fibonacci(10) = {fib(10)}")
"""
        output = await execute_code_in_sandbox(code)
        print(f"Output: {output}")


if __name__ == "__main__":
    asyncio.run(main())
