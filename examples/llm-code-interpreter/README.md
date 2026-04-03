# LLM Code Interpreter

AI generates code. Roche executes it safely. The most natural use case for a sandbox.

## Why a Sandbox Matters Here

Without a sandbox, LLM-generated code runs on your machine with full access. The LLM could generate `os.system("rm -rf /")` or `socket.connect(("evil.com", 80))` — and Open Interpreter will happily execute it.

Roche doesn't. Network is off by default, filesystem is readonly, timeout is enforced. If the code needs network (e.g. `import requests`), Roche detects it and enables only the specific hosts. The LLM doesn't know it's sandboxed.

## Quick Start

    pip install roche-sandbox anthropic
    export ANTHROPIC_API_KEY=sk-ant-...

    python examples/llm-code-interpreter/interpreter.py

Example session:

    You: What's the 100th Fibonacci number?

    Assistant: (generating code, 850ms)
    def fib(n):
        a, b = 0, 1
        for _ in range(n):
            a, b = b, a + b
        return a
    print(fib(100))

    Executing in Roche sandbox...
    Output: 354224848179261915075
    [exit=0, 1200ms, sandboxed]

    Assistant: The 100th Fibonacci number is 354,224,848,179,261,915,075.

## Options

    # Use OpenAI instead of Anthropic
    python interpreter.py --provider openai

    # Run without sandbox (unsafe — for comparison)
    python interpreter.py --no-sandbox

    # Non-interactive (single question)
    python interpreter.py --non-interactive "Calculate pi to 50 digits"

## How It Works

    User question
      |
      v
    LLM generates Python code
      |
      v
    Roche analyzes code intent:
      - needs network? which hosts?
      - needs filesystem writes?
      - needs extra memory?
      |
      v
    Execute in locked-down sandbox
      |
      v
    Feed stdout/stderr back to LLM
      |
      v
    LLM interprets result and responds

## Competitor Comparison

Run `python examples/llm-code-interpreter/comparison.py` for the full formatted table.

|                    | Roche     | E2B          | Open Interpreter | OpenAI CI      | Modal        | Daytona      |
|:-------------------|:---------:|:------------:|:----------------:|:--------------:|:------------:|:------------:|
| Isolation          | Docker/WASM/Firecracker | Firecracker microVM | **none** | managed VM | gVisor | Docker |
| Network control    | allowlist | off by default | **full access** | no network | granular | isolated |
| Intent analysis    | **yes**   | no           | no               | no             | no           | no           |
| Local / self-host  | **yes**   | cloud only   | local (unsafe)   | cloud only     | cloud only   | both         |
| Multi-provider     | **yes** (5) | no         | no               | no             | no           | no           |
| GPU support        | no        | no           | local            | no             | **yes**      | no           |
| Setup              | 1 line    | 4 lines+key  | 2 lines          | tool param     | 5 lines+key  | 4 lines+key  |
| Cold start         | ~1s Docker, <1ms WASM | ~150ms | 0 (unsafe) | n/a      | <1s          | ~90ms        |
| Pricing            | **free**  | ~$0.05/hr    | free (+LLM)      | $0.03/session  | ~$0.14/hr    | ~$0.05/hr    |
| Open source        | yes       | yes          | yes              | no             | no           | yes          |

### Where each wins

- **Roche**: Only option that's isolated + zero config + local + intent-aware. Free. No API key.
- **E2B**: Market leader for cloud agent sandboxes. Fast cold starts. Production-proven.
- **Open Interpreter**: Best UX for personal use. Zero friction. Zero security.
- **OpenAI CI**: Zero setup if you're already using OpenAI. No custom packages.
- **Modal**: Only option with GPU sandboxes. Best for ML workloads. Most expensive.
- **Daytona**: Full dev environments (LSP, SSH, VS Code). Self-hostable. Good for coding agents.
