#!/usr/bin/env python3
"""Anthropic API + Roche: basic tool_use integration."""

import json
import os

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("ANTHROPIC_API_KEY"))

TOOL_SCHEMA = {
    "name": "execute_code",
    "description": "Execute Python code in a secure Roche sandbox. Returns stdout or error.",
    "input_schema": {
        "type": "object",
        "properties": {
            "code": {
                "type": "string",
                "description": "Python code to execute",
            }
        },
        "required": ["code"],
    },
}


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


def process_tool_call(tool_name: str, tool_input: dict) -> str:
    """Route tool calls to the appropriate handler."""
    if tool_name == "execute_code":
        return execute_in_sandbox(tool_input["code"])
    raise ValueError(f"Unknown tool: {tool_name}")


def main():
    if USE_REAL_LLM:
        import anthropic

        client = anthropic.Anthropic()
        response = client.messages.create(
            model="claude-sonnet-4-20250514",
            max_tokens=1024,
            tools=[TOOL_SCHEMA],
            messages=[
                {"role": "user", "content": "Calculate 2^10 using Python code."}
            ],
        )

        # Process tool use
        for block in response.content:
            if block.type == "tool_use":
                print(f"Tool call: {block.name}({json.dumps(block.input)[:80]})")
                result = process_tool_call(block.name, block.input)
                print(f"Result: {result}")
            elif block.type == "text":
                print(f"Claude: {block.text}")
    else:
        # Simulated: hardcoded tool_use -> execute in sandbox
        print("[simulated] Claude requests tool_use: execute_code")
        simulated_input = {"code": "print(2 ** 10)"}
        print(f"Tool input: {json.dumps(simulated_input)}")
        result = process_tool_call("execute_code", simulated_input)
        print(f"Result: {result}")


if __name__ == "__main__":
    main()
