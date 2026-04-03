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

Run the comparison table:

    python examples/llm-code-interpreter/comparison.py

|                  | Roche  | E2B    | Open Interpreter | OpenAI CI | Docker DIY |
|:-----------------|:------:|:------:|:----------------:|:---------:|:----------:|
| Isolated         | yes    | yes    | **no**           | yes       | yes        |
| Network control  | yes    | ~      | **no**           | no        | yes        |
| Intent analysis  | yes    | no     | no               | no        | no         |
| Local execution  | yes    | no     | yes              | no        | yes        |
| Multi-provider   | yes    | no     | no               | no        | no         |
| Custom packages  | yes    | yes    | yes              | limited   | yes        |
| Open source      | yes    | yes    | yes              | no        | n/a        |
| Zero config      | yes    | no     | yes              | yes       | no         |
| Setup lines      | 1      | 3+key  | 2                | 1         | ~20        |

**Roche is the only solution that combines isolation + zero config + local execution + intent analysis.**

- **vs E2B**: Roche runs locally, no API key needed, auto-detects permissions
- **vs Open Interpreter**: Roche actually isolates code — Open Interpreter runs everything on your machine
- **vs OpenAI Code Interpreter**: Roche is self-hosted, supports any LLM, custom packages
- **vs Docker DIY**: Roche is one line, not twenty — intent analysis handles config automatically
