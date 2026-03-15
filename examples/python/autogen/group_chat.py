#!/usr/bin/env python3
"""AutoGen + Roche: multi-agent group chat with sandbox execution."""

import os

from roche_sandbox import Roche

USE_REAL_LLM = bool(os.environ.get("OPENAI_API_KEY"))


class RocheCodeExecutor:
    """AutoGen-compatible code executor backed by Roche sandbox."""

    def __init__(self, image: str = "python:3.12-slim"):
        self._image = image

    def execute_code_blocks(self, code_blocks: list[tuple[str, str]]) -> dict:
        roche = Roche()
        sandbox = roche.create(image=self._image)
        try:
            outputs = []
            for lang, code in code_blocks:
                cmd = ["python3", "-c", code] if lang.startswith("py") else ["sh", "-c", code]
                result = sandbox.exec(cmd)
                if result.exit_code != 0:
                    return {"exit_code": result.exit_code, "output": result.stderr.strip()}
                outputs.append(result.stdout.strip())
            return {"exit_code": 0, "output": "\n".join(outputs)}
        finally:
            sandbox.destroy()


def main():
    if USE_REAL_LLM:
        from autogen import AssistantAgent, GroupChat, GroupChatManager, UserProxyAgent

        llm_config = {"model": "gpt-4o-mini"}
        executor = RocheCodeExecutor()

        planner = AssistantAgent(
            name="planner",
            system_message=(
                "You plan coding tasks. Break down the problem and describe "
                "what code the coder should write. Do NOT write code yourself."
            ),
            llm_config=llm_config,
        )

        coder = AssistantAgent(
            name="coder",
            system_message=(
                "You write Python code based on the planner's instructions. "
                "Always wrap code in ```python blocks."
            ),
            llm_config=llm_config,
        )

        executor_agent = UserProxyAgent(
            name="executor",
            human_input_mode="NEVER",
            max_consecutive_auto_reply=5,
            code_execution_config={"executor": executor},
        )

        reviewer = AssistantAgent(
            name="reviewer",
            system_message=(
                "You review execution results. If correct, say TERMINATE. "
                "If wrong, explain what to fix."
            ),
            llm_config=llm_config,
        )

        group_chat = GroupChat(
            agents=[planner, coder, executor_agent, reviewer],
            messages=[],
            max_round=10,
        )
        manager = GroupChatManager(groupchat=group_chat, llm_config=llm_config)

        executor_agent.initiate_chat(
            manager,
            message="Sort the list [38, 27, 43, 3, 9, 82, 10] using merge sort and print each step.",
        )
    else:
        # Simulated group chat flow
        executor = RocheCodeExecutor()

        print("=== Simulated Group Chat ===\n")
        print("[planner] Break into: implement merge_sort, add step printing, test it.\n")

        code = """\
def merge_sort(arr, depth=0):
    if len(arr) <= 1:
        return arr
    mid = len(arr) // 2
    left = merge_sort(arr[:mid], depth + 1)
    right = merge_sort(arr[mid:], depth + 1)
    merged = []
    i = j = 0
    while i < len(left) and j < len(right):
        if left[i] <= right[j]:
            merged.append(left[i]); i += 1
        else:
            merged.append(right[j]); j += 1
    merged.extend(left[i:])
    merged.extend(right[j:])
    print(f"{'  ' * depth}merge({left}, {right}) = {merged}")
    return merged

data = [38, 27, 43, 3, 9, 82, 10]
print(f"Input: {data}")
result = merge_sort(data)
print(f"Sorted: {result}")
"""
        print(f"[coder] Writing merge sort...\n")
        result = executor.execute_code_blocks([("python", code)])
        print(f"[executor] exit_code={result['exit_code']}")
        print(f"{result['output']}\n")
        print("[reviewer] Output is correct. TERMINATE.")


if __name__ == "__main__":
    main()
