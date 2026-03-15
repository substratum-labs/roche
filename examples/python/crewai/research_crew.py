#!/usr/bin/env python3
"""CrewAI + Roche: multi-agent crew with sandbox execution."""

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
        from crewai import Agent, Crew, Process, Task
        from crewai.tools import tool

        @tool("sandbox_execute")
        def sandbox_execute(code: str) -> str:
            """Execute Python code in a secure Roche sandbox. Returns stdout or error."""
            return execute_in_sandbox(code)

        researcher = Agent(
            role="Algorithm Researcher",
            goal="Design Python algorithms for data analysis tasks",
            backstory="You design efficient algorithms and write clean Python code.",
            verbose=True,
        )

        executor = Agent(
            role="Code Executor",
            goal="Run code in a sandbox and validate the results",
            backstory="You execute code safely and verify correctness.",
            tools=[sandbox_execute],
            verbose=True,
        )

        research_task = Task(
            description=(
                "Write a Python function that computes basic statistics "
                "(mean, median, std dev) for the list [4, 8, 15, 16, 23, 42]. "
                "Output the complete code."
            ),
            expected_output="Complete Python code as a string",
            agent=researcher,
        )

        execution_task = Task(
            description=(
                "Take the code from the research task and execute it in the sandbox. "
                "Report the results."
            ),
            expected_output="The computed statistics",
            agent=executor,
        )

        crew = Crew(
            agents=[researcher, executor],
            tasks=[research_task, execution_task],
            process=Process.sequential,
            verbose=True,
        )

        result = crew.kickoff()
        print(f"\nCrew result: {result}")
    else:
        # Simulated: two-step pipeline
        print("[simulated] Researcher generates code, executor runs it...")

        # Step 1: researcher output
        code = """\
import statistics
data = [4, 8, 15, 16, 23, 42]
print(f"Mean:   {statistics.mean(data):.2f}")
print(f"Median: {statistics.median(data):.2f}")
print(f"StdDev: {statistics.stdev(data):.2f}")
"""
        print(f"Researcher code:\n{code}")

        # Step 2: executor runs in sandbox
        output = execute_in_sandbox(code)
        print(f"Executor output:\n{output}")


if __name__ == "__main__":
    main()
