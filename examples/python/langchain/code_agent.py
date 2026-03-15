#!/usr/bin/env python3
"""LangGraph + Roche: stateful code-execute-retry workflow."""

import os
from typing import TypedDict

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))

# Simulated LLM responses: first attempt has a bug, second is correct
SIMULATED_RESPONSES = [
    'result = sum(range(1, 101))\nprint(f"Sum: {reslt}")',  # NameError: reslt
    'result = sum(range(1, 101))\nprint(f"Sum: {result}")',  # correct
]


class AgentState(TypedDict):
    task: str
    code: str
    output: str
    attempt: int
    success: bool


def generate_code(state: AgentState) -> dict:
    """Generate (or fix) code for the task."""
    attempt = state["attempt"]

    if USE_REAL_LLM:
        from langchain_openai import ChatOpenAI

        llm = ChatOpenAI(model="gpt-4o-mini")
        if attempt == 0:
            prompt = f"Write Python code to: {state['task']}. Print the result."
        else:
            prompt = (
                f"This code failed:\n```\n{state['code']}\n```\n"
                f"Error:\n{state['output']}\n\nFix it."
            )
        response = llm.invoke(prompt)
        code = response.content.strip().strip("`").removeprefix("python\n")
    else:
        idx = min(attempt, len(SIMULATED_RESPONSES) - 1)
        code = SIMULATED_RESPONSES[idx]

    print(f"  [generate] attempt {attempt + 1}: {code[:60]}...")
    return {"code": code, "attempt": attempt + 1}


def execute_in_sandbox(state: AgentState) -> dict:
    """Execute code in a Roche sandbox."""
    roche = Roche()
    sandbox = roche.create(image="python:3.12-slim")
    try:
        result = sandbox.exec(["python3", "-c", state["code"]])
        success = result.exit_code == 0
        output = result.stdout.strip() if success else result.stderr.strip()
        print(f"  [execute] exit={result.exit_code}: {output[:80]}")
        return {"output": output, "success": success}
    finally:
        sandbox.destroy()


def should_retry(state: AgentState) -> str:
    """Decide whether to retry or finish."""
    if state["success"]:
        return "done"
    if state["attempt"] >= 3:
        return "done"
    return "retry"


def main():
    from langgraph.graph import END, StateGraph

    graph = StateGraph(AgentState)
    graph.add_node("generate", generate_code)
    graph.add_node("execute", execute_in_sandbox)

    graph.set_entry_point("generate")
    graph.add_edge("generate", "execute")
    graph.add_conditional_edges("execute", should_retry, {"retry": "generate", "done": END})

    app = graph.compile()

    print("Running code-execute-retry workflow...")
    result = app.invoke(
        {"task": "Calculate the sum of 1 to 100", "code": "", "output": "", "attempt": 0, "success": False}
    )
    print(f"\nFinal result: {result['output']}")
    print(f"Attempts: {result['attempt']}, Success: {result['success']}")


if __name__ == "__main__":
    main()
