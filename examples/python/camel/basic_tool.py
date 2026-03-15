#!/usr/bin/env python3
"""Camel-AI + Roche: basic toolkit integration."""

import os

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))


class RocheToolkit:
    """Camel-AI compatible toolkit for Roche sandbox operations."""

    def __init__(self, image: str = "python:3.12-slim"):
        self._image = image

    def execute_code(self, code: str) -> str:
        """Execute Python code in a secure Roche sandbox.

        Args:
            code: Python code to execute.

        Returns:
            stdout output or error message.
        """
        roche = Roche()
        sandbox = roche.create(image=self._image)
        try:
            result = sandbox.exec(["python3", "-c", code])
            if result.exit_code != 0:
                return f"ERROR (exit {result.exit_code}):\n{result.stderr.strip()}"
            return result.stdout.strip()
        finally:
            sandbox.destroy()

    def list_sandboxes(self) -> str:
        """List all active Roche sandboxes.

        Returns:
            Formatted list of sandbox IDs and statuses.
        """
        roche = Roche()
        sandboxes = roche.list()
        if not sandboxes:
            return "No active sandboxes."
        lines = [f"  {s.id} ({s.status})" for s in sandboxes]
        return f"Active sandboxes:\n" + "\n".join(lines)

    def get_tools(self) -> list:
        """Return tool functions for agent integration."""
        return [self.execute_code, self.list_sandboxes]


def main():
    toolkit = RocheToolkit()

    if USE_REAL_LLM:
        from camel.agents import ChatAgent
        from camel.messages import BaseMessage

        agent = ChatAgent(
            system_message=BaseMessage.make_assistant_message(
                role_name="Coder",
                content="You write and execute Python code using available tools.",
            ),
            tools=toolkit.get_tools(),
        )

        user_msg = BaseMessage.make_user_message(
            role_name="User",
            content="Write code to generate the first 10 triangular numbers and print them.",
        )
        response = agent.step(user_msg)
        print(f"Agent: {response.msg.content}")
    else:
        # Simulated: directly use the toolkit
        print("[simulated] Using RocheToolkit...")
        code = """\
triangular = [n * (n + 1) // 2 for n in range(1, 11)]
print(f"First 10 triangular numbers: {triangular}")
"""
        output = toolkit.execute_code(code)
        print(f"execute_code output: {output}")

        sandboxes = toolkit.list_sandboxes()
        print(f"list_sandboxes: {sandboxes}")


if __name__ == "__main__":
    main()
