#!/usr/bin/env python3
"""LangChain + Roche: custom tool integration."""

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
        from langchain_core.tools import tool

        @tool
        def sandbox_execute(code: str) -> str:
            """Execute Python code in a secure Roche sandbox. Returns stdout or error."""
            return execute_in_sandbox(code)

        from langchain_openai import ChatOpenAI
        from langgraph.prebuilt import create_react_agent

        llm = ChatOpenAI(model="gpt-4o-mini")
        agent = create_react_agent(llm, [sandbox_execute])
        result = agent.invoke(
            {"messages": [("user", "Calculate the sum of squares from 1 to 10.")]}
        )
        for msg in result["messages"]:
            print(f"{msg.type}: {msg.content[:200] if msg.content else '(tool call)'}")
    else:
        # Simulated: directly call the sandbox
        print("[simulated] Running code in sandbox via LangChain tool pattern...")
        code = "print(sum(i**2 for i in range(1, 11)))"
        output = execute_in_sandbox(code)
        print(f"Output: {output}")


if __name__ == "__main__":
    main()
