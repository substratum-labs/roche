#!/usr/bin/env python3
"""Camel-AI + Roche: role-playing coding agent session."""

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


# Simulated role-playing rounds
SIMULATED_ROUNDS = [
    {
        "user": "Write a function to check if a number is a perfect square.",
        "assistant_code": """\
import math

def is_perfect_square(n):
    if n < 0:
        return False
    root = int(math.isqrt(n))
    return root * root == n

# Test
test_cases = [0, 1, 4, 9, 15, 16, 25, 26, 100]
for n in test_cases:
    print(f"{n}: {is_perfect_square(n)}")
""",
    },
    {
        "user": "Now extend it to also find the next perfect square above a given number.",
        "assistant_code": """\
import math

def is_perfect_square(n):
    if n < 0:
        return False
    root = int(math.isqrt(n))
    return root * root == n

def next_perfect_square(n):
    root = int(math.isqrt(n))
    if root * root == n:
        root += 1
    else:
        root += 1
    return root * root

# Test
for n in [0, 1, 5, 15, 16, 24, 25, 99]:
    nps = next_perfect_square(n)
    print(f"next_perfect_square({n}) = {nps}")
""",
    },
]


def main():
    if USE_REAL_LLM:
        from camel.agents import ChatAgent
        from camel.messages import BaseMessage
        from camel.societies import RolePlaying

        task_prompt = (
            "Write and test a Python perfect square checker, "
            "then extend it to find the next perfect square."
        )

        role_play = RolePlaying(
            assistant_role_name="Python Developer",
            user_role_name="Code Reviewer",
            task_prompt=task_prompt,
        )

        print(f"Task: {task_prompt}\n")

        for round_num in range(3):
            user_msg, _ = role_play.step()
            if "CAMEL_TASK_DONE" in (user_msg.content or ""):
                print("Task completed!")
                break

            assistant_response, _ = role_play.step()
            content = assistant_response.msg.content

            # Extract and execute code blocks
            if "```python" in content:
                code = content.split("```python")[1].split("```")[0].strip()
                print(f"[Round {round_num + 1}] Executing code...")
                output = execute_in_sandbox(code)
                print(f"Output: {output}\n")
    else:
        # Simulated role-playing
        print("=== Simulated Role-Playing Session ===\n")

        for i, round_data in enumerate(SIMULATED_ROUNDS, 1):
            print(f"--- Round {i} ---")
            print(f"[User] {round_data['user']}")
            print(f"[Assistant] Writing code...")
            output = execute_in_sandbox(round_data["assistant_code"])
            print(f"[Sandbox Output]\n{output}\n")

        print("Task completed!")


if __name__ == "__main__":
    main()
