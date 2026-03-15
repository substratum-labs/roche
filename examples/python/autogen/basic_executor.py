#!/usr/bin/env python3
"""AutoGen + Roche: custom code executor integration."""

import os

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))


class RocheCodeExecutor:
    """AutoGen-compatible code executor backed by Roche sandbox."""

    def __init__(self, image: str = "python:3.12-slim"):
        self._image = image

    def execute_code_blocks(self, code_blocks: list[tuple[str, str]]) -> dict:
        """Execute code blocks in a Roche sandbox.

        Args:
            code_blocks: List of (language, code) tuples.

        Returns:
            Dict with exit_code, output, and image fields.
        """
        roche = Roche()
        sandbox = roche.create(image=self._image)
        outputs = []

        try:
            for lang, code in code_blocks:
                if lang in ("python", "py", "python3"):
                    result = sandbox.exec(["python3", "-c", code])
                elif lang in ("bash", "sh", "shell"):
                    result = sandbox.exec(["sh", "-c", code])
                else:
                    outputs.append(f"Unsupported language: {lang}")
                    continue

                if result.exit_code != 0:
                    return {
                        "exit_code": result.exit_code,
                        "output": result.stderr.strip(),
                        "image": self._image,
                    }
                outputs.append(result.stdout.strip())

            return {
                "exit_code": 0,
                "output": "\n".join(outputs),
                "image": self._image,
            }
        finally:
            sandbox.destroy()


def main():
    executor = RocheCodeExecutor()

    if USE_REAL_LLM:
        # With real LLM, integrate into AutoGen agent flow
        from autogen import AssistantAgent, UserProxyAgent

        assistant = AssistantAgent(
            name="coder",
            llm_config={"model": "gpt-4o-mini"},
        )

        user_proxy = UserProxyAgent(
            name="user",
            human_input_mode="NEVER",
            max_consecutive_auto_reply=3,
            code_execution_config={"executor": executor},
        )

        user_proxy.initiate_chat(
            assistant,
            message="Write Python to find the 20th Fibonacci number and print it.",
        )
    else:
        # Simulated: directly test the executor
        print("[simulated] Running code blocks via RocheCodeExecutor...")

        blocks = [
            ("python", "print('Hello from Roche + AutoGen!')"),
            ("python", "import sys; print(f'Python {sys.version_info[:2]}')"),
            ("bash", "echo 'Shell works too'"),
        ]

        for lang, code in blocks:
            print(f"\n[{lang}] {code}")
            result = executor.execute_code_blocks([(lang, code)])
            print(f"  exit_code: {result['exit_code']}")
            print(f"  output: {result['output']}")


if __name__ == "__main__":
    main()
