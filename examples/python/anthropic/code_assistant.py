#!/usr/bin/env python3
"""Anthropic API + Roche: multi-turn agentic code assistant."""

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
            "code": {"type": "string", "description": "Python code to execute"}
        },
        "required": ["code"],
    },
}

# Simulated multi-turn: two tool calls then a final text response
SIMULATED_TURNS = [
    {
        "tool_use": {
            "code": "import sys; print(f'Python {sys.version}')"
        },
    },
    {
        "tool_use": {
            "code": (
                "def is_palindrome(s):\n"
                "    s = s.lower().replace(' ', '')\n"
                "    return s == s[::-1]\n\n"
                "tests = ['racecar', 'hello', 'A man a plan a canal Panama']\n"
                "for t in tests:\n"
                "    print(f'{t!r}: {is_palindrome(t)}')\n"
            )
        },
    },
    {"text": "The palindrome checker works correctly on all test cases."},
]


def main():
    roche = Roche()
    sandbox = roche.create(image="python:3.12-slim")

    try:
        print(f"Sandbox created: {sandbox.id}\n")

        if USE_REAL_LLM:
            import anthropic

            client = anthropic.Anthropic()
            messages = [
                {"role": "user", "content": "Write a palindrome checker and test it."}
            ]

            # Agentic loop
            for turn in range(5):
                response = client.messages.create(
                    model="claude-sonnet-4-20250514",
                    max_tokens=1024,
                    tools=[TOOL_SCHEMA],
                    messages=messages,
                )

                # Process response blocks
                tool_results = []
                for block in response.content:
                    if block.type == "text":
                        print(f"Claude: {block.text}")
                    elif block.type == "tool_use":
                        print(f"\n[Turn {turn + 1}] Tool: {block.name}")
                        print(f"Code: {block.input['code'][:80]}...")
                        result = sandbox.exec(["python3", "-c", block.input["code"]])
                        output = result.stdout.strip() if result.exit_code == 0 else (
                            f"ERROR:\n{result.stderr.strip()}"
                        )
                        print(f"Output: {output}")
                        tool_results.append({
                            "type": "tool_result",
                            "tool_use_id": block.id,
                            "content": output,
                        })

                messages.append({"role": "assistant", "content": response.content})
                if tool_results:
                    messages.append({"role": "user", "content": tool_results})

                if response.stop_reason == "end_turn":
                    break
        else:
            # Simulated multi-turn
            for i, turn in enumerate(SIMULATED_TURNS):
                if "tool_use" in turn:
                    code = turn["tool_use"]["code"]
                    print(f"[Turn {i + 1}] Tool: execute_code")
                    print(f"Code: {code[:80]}...")
                    result = sandbox.exec(["python3", "-c", code])
                    output = result.stdout.strip() if result.exit_code == 0 else (
                        f"ERROR:\n{result.stderr.strip()}"
                    )
                    print(f"Output: {output}\n")
                elif "text" in turn:
                    print(f"Claude: {turn['text']}")

    finally:
        sandbox.destroy()
        print("\nSandbox destroyed.")


if __name__ == "__main__":
    main()
