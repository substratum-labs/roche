#!/usr/bin/env python3
"""LLM Code Interpreter — AI generates code, Roche executes it safely.

A conversational loop: user asks a question → LLM writes Python code →
Roche runs it in a sandbox → result feeds back to LLM → LLM responds.

Works with Anthropic (Claude) or OpenAI. Set one of:
    export ANTHROPIC_API_KEY=sk-ant-...
    export OPENAI_API_KEY=sk-...

Usage:
    python examples/llm-code-interpreter/interpreter.py
    python examples/llm-code-interpreter/interpreter.py --provider openai
    python examples/llm-code-interpreter/interpreter.py --no-sandbox  # unsafe, for comparison
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

from roche_sandbox import run, RunResult

SYSTEM_PROMPT = """You are a helpful assistant with access to a Python code interpreter.

When the user asks a question that benefits from computation, data analysis,
or verification, write Python code to solve it. Wrap your code in a ```python
code block. The code will be executed in a secure sandbox and you'll see the output.

Rules:
- Use print() to show results — you'll see stdout
- You can import standard library modules freely
- For data work, pandas/numpy are available (auto-installed)
- Network is auto-enabled if your code needs it (e.g. import requests)
- Files written to /tmp/ persist within the session
- If execution fails, you'll see the error — fix and retry"""


def extract_code(text: str) -> str | None:
    """Extract Python code from markdown code block."""
    if "```python" not in text:
        return None
    start = text.index("```python") + len("```python")
    end = text.index("```", start)
    return text[start:end].strip()


def execute_code(code: str, use_sandbox: bool = True) -> dict:
    """Execute code, either in Roche sandbox or locally (unsafe)."""
    t0 = time.time()

    if use_sandbox:
        result = run(code, timeout_secs=30, trace_level="standard")
        elapsed = time.time() - t0
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "elapsed_ms": round(elapsed * 1000),
            "sandboxed": True,
        }
    else:
        # Unsafe local execution — for comparison only
        import subprocess
        try:
            proc = subprocess.run(
                ["python3", "-c", code],
                capture_output=True, text=True, timeout=30,
            )
            elapsed = time.time() - t0
            return {
                "stdout": proc.stdout,
                "stderr": proc.stderr,
                "exit_code": proc.returncode,
                "elapsed_ms": round(elapsed * 1000),
                "sandboxed": False,
            }
        except subprocess.TimeoutExpired:
            return {"stdout": "", "stderr": "Timeout", "exit_code": -1,
                    "elapsed_ms": round((time.time() - t0) * 1000), "sandboxed": False}


# ---------------------------------------------------------------------------
# LLM Backends
# ---------------------------------------------------------------------------


def chat_anthropic(messages: list[dict], system: str) -> str:
    """Call Claude API."""
    import anthropic
    client = anthropic.Anthropic()
    response = client.messages.create(
        model="claude-sonnet-4-20250514",
        max_tokens=4096,
        system=system,
        messages=messages,
    )
    return response.content[0].text


def chat_openai(messages: list[dict], system: str) -> str:
    """Call OpenAI API."""
    import openai
    client = openai.OpenAI()
    full_messages = [{"role": "system", "content": system}] + messages
    response = client.chat.completions.create(
        model="gpt-4o",
        messages=full_messages,
        max_tokens=4096,
    )
    return response.choices[0].message.content


def detect_llm_provider() -> str:
    if os.environ.get("ANTHROPIC_API_KEY"):
        return "anthropic"
    if os.environ.get("OPENAI_API_KEY"):
        return "openai"
    return "none"


# ---------------------------------------------------------------------------
# Main loop
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(description="LLM Code Interpreter with Roche sandbox")
    parser.add_argument("--provider", choices=["anthropic", "openai", "auto"], default="auto",
                        help="LLM provider")
    parser.add_argument("--no-sandbox", action="store_true",
                        help="Run code locally without sandbox (unsafe, for comparison)")
    parser.add_argument("--non-interactive", help="Single question, non-interactive mode")
    args = parser.parse_args()

    # Detect LLM provider
    llm_provider = args.provider if args.provider != "auto" else detect_llm_provider()
    if llm_provider == "none":
        print("Set ANTHROPIC_API_KEY or OPENAI_API_KEY to use this example.")
        sys.exit(1)

    chat_fn = chat_anthropic if llm_provider == "anthropic" else chat_openai
    sandbox_mode = "Roche sandbox" if not args.no_sandbox else "LOCAL (unsafe)"

    print(f"LLM Code Interpreter [{llm_provider} + {sandbox_mode}]")
    print("Type your question. Code will be generated and executed automatically.\n")

    messages: list[dict] = []
    use_sandbox = not args.no_sandbox

    # Support non-interactive mode for testing/benchmarking
    questions = [args.non_interactive] if args.non_interactive else None
    q_idx = 0

    while True:
        # Get user input
        if questions:
            if q_idx >= len(questions):
                break
            user_input = questions[q_idx]
            q_idx += 1
            print(f"You: {user_input}")
        else:
            try:
                user_input = input("You: ").strip()
            except (EOFError, KeyboardInterrupt):
                print("\nBye!")
                break

        if not user_input:
            continue
        if user_input.lower() in ("quit", "exit", "q"):
            break

        messages.append({"role": "user", "content": user_input})

        # Call LLM
        t0 = time.time()
        response = chat_fn(messages, SYSTEM_PROMPT)
        llm_ms = round((time.time() - t0) * 1000)

        # Check for code block
        code = extract_code(response)

        if code:
            # Show the code
            print(f"\nAssistant: (generating code, {llm_ms}ms)")
            print(f"```python\n{code}\n```")

            # Execute
            print(f"\nExecuting in {sandbox_mode}...")
            exec_result = execute_code(code, use_sandbox)

            # Show result
            if exec_result["stdout"]:
                print(f"Output:\n{exec_result['stdout']}", end="")
            if exec_result["stderr"]:
                print(f"Error:\n{exec_result['stderr']}", end="")
            print(f"[exit={exec_result['exit_code']}, {exec_result['elapsed_ms']}ms, "
                  f"{'sandboxed' if exec_result['sandboxed'] else 'LOCAL'}]\n")

            # Feed result back to LLM
            exec_summary = f"Code execution result:\nstdout: {exec_result['stdout'][:2000]}"
            if exec_result["stderr"]:
                exec_summary += f"\nstderr: {exec_result['stderr'][:500]}"
            exec_summary += f"\nexit_code: {exec_result['exit_code']}"

            messages.append({"role": "assistant", "content": response})
            messages.append({"role": "user", "content": exec_summary})

            # Get LLM's interpretation
            final = chat_fn(messages, SYSTEM_PROMPT)
            print(f"Assistant: {final}\n")
            messages.append({"role": "assistant", "content": final})
        else:
            # No code — just text response
            print(f"\nAssistant: {response}\n")
            messages.append({"role": "assistant", "content": response})


if __name__ == "__main__":
    main()
