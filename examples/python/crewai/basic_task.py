#!/usr/bin/env python3
"""CrewAI + Roche: basic tool integration."""

import os

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))


def execute_in_sandbox(code: str) -> str:
    """Execute Python code in a Roche sandbox."""
    roche = Roche()
    sandbox = roche.create(image="python:3.12-slim")
    try:
        result = sandbox.exec(["python3", "-c", code])
        if result.exit_code != 0:
            return f"ERROR (exit {result.exit_code}):\n{result.stderr.strip()}"
        return result.stdout.strip()
    finally:
        sandbox.destroy()


def main():
    if USE_REAL_LLM:
        from crewai import Agent, Crew, Task
        from crewai.tools import tool

        @tool("sandbox_execute")
        def sandbox_execute(code: str) -> str:
            """Execute Python code in a secure Roche sandbox. Returns stdout or error."""
            return execute_in_sandbox(code)

        developer = Agent(
            role="Python Developer",
            goal="Write and execute Python code to solve tasks",
            backstory="You are an expert Python developer with access to a sandbox.",
            tools=[sandbox_execute],
            verbose=True,
        )

        task = Task(
            description="Write Python code to find all prime numbers under 50 and print them.",
            expected_output="A list of prime numbers under 50",
            agent=developer,
        )

        crew = Crew(agents=[developer], tasks=[task], verbose=True)
        result = crew.kickoff()
        print(f"\nCrew result: {result}")
    else:
        # Simulated: directly call the sandbox
        print("[simulated] Running code in sandbox via CrewAI tool pattern...")
        code = """\
primes = [n for n in range(2, 50) if all(n % i != 0 for i in range(2, int(n**0.5) + 1))]
print(f"Primes under 50: {primes}")
"""
        output = execute_in_sandbox(code)
        print(f"Output: {output}")


if __name__ == "__main__":
    main()
