#!/usr/bin/env python3
"""Side-by-side comparison: Roche vs competitors for LLM code execution.

Shows setup code and features for each approach. Run to see a formatted table.

Usage:
    python examples/llm-code-interpreter/comparison.py
"""


def print_comparison():
    print("""
╔══════════════════════════════════════════════════════════════════════════════╗
║                  LLM Code Interpreter — Approach Comparison                ║
╚══════════════════════════════════════════════════════════════════════════════╝

┌─────────────────┬──────────────────────────────────────────────────────────┐
│ Roche           │ result = run("print(2+2)")                             │
│                 │                                                        │
│ Setup: 1 line   │ • Intent analysis: auto-detects network/FS/memory     │
│ pip install     │ • 5 providers: Docker, WASM, Firecracker, E2B, K8s    │
│ roche-sandbox   │ • Network off by default, allowlist auto-inferred     │
│                 │ • Filesystem readonly by default                       │
│                 │ • Sub-ms WASM for pure compute                        │
│                 │ • Local — no cloud dependency, no API key needed      │
│                 │ • Open source (Apache 2.0)                            │
│ Cost: Free      │ • Warm pool for pre-created sandboxes                 │
├─────────────────┼──────────────────────────────────────────────────────────┤
│ E2B             │ sandbox = Sandbox()                                    │
│                 │ result = sandbox.run_code("print(2+2)")                │
│ Setup: 3 lines  │                                                        │
│ pip install e2b │ • Cloud-hosted sandboxes (Firecracker microVMs)       │
│ + API key       │ • Network enabled by default                           │
│                 │ • Custom Dockerfile templates                          │
│                 │ • No local option — requires internet + API key       │
│                 │ • No intent analysis — manual config                   │
│ Cost: Usage-    │ • Good for cloud-native, bad for offline/local dev    │
│ based ($)       │                                                        │
├─────────────────┼──────────────────────────────────────────────────────────┤
│ Open Interpreter│ interpreter.chat("plot sin(x)")                        │
│                 │                                                        │
│ Setup: 2 lines  │ • Runs code on YOUR machine — no isolation            │
│ pip install     │ • Full filesystem access                               │
│ open-interpreter│ • Full network access                                  │
│                 │ • Can install packages, delete files, anything         │
│                 │ • Convenient but dangerous for untrusted code          │
│ Cost: Free +    │ • Great UX, zero security                             │
│ LLM API costs   │                                                        │
├─────────────────┼──────────────────────────────────────────────────────────┤
│ OpenAI Code     │ response = client.chat(tools=[{"type":                 │
│ Interpreter     │   "code_interpreter"}])                                │
│                 │                                                        │
│ Setup: Built-in │ • Runs inside OpenAI's infrastructure                 │
│ (ChatGPT/API)  │ • Sandboxed but opaque — no control over environment  │
│                 │ • No custom packages (limited to pre-installed)       │
│                 │ • No network access inside sandbox                     │
│                 │ • Can't bring your own LLM (OpenAI only)             │
│ Cost: Included  │ • No local execution, no self-hosting                 │
│ in API price    │                                                        │
├─────────────────┼──────────────────────────────────────────────────────────┤
│ Docker (DIY)    │ subprocess.run(["docker", "run", "--rm",               │
│                 │   "--network=none", "python:3.12",                     │
│                 │   "python", "-c", code])                               │
│ Setup: ~20 lines│                                                        │
│ (roll your own) │ • Full control, full responsibility                   │
│                 │ • Manual network/filesystem/timeout config             │
│                 │ • No intent analysis                                   │
│                 │ • No warm pool, no caching, no streaming              │
│                 │ • Container startup overhead on every call            │
│ Cost: Free      │ • Works but lots of boilerplate                       │
└─────────────────┴──────────────────────────────────────────────────────────┘

Summary:
┌────────────────────┬────────┬──────────┬─────────┬───────────┬───────────┐
│                    │ Roche  │ E2B      │ Open    │ OpenAI CI │ Docker    │
│                    │        │          │ Interp. │           │ DIY       │
├────────────────────┼────────┼──────────┼─────────┼───────────┼───────────┤
│ Isolated           │   ✓    │    ✓     │    ✗    │     ✓     │     ✓     │
│ Network control    │   ✓    │    ~     │    ✗    │     ✗     │     ✓     │
│ Intent analysis    │   ✓    │    ✗     │    ✗    │     ✗     │     ✗     │
│ Local execution    │   ✓    │    ✗     │    ✓    │     ✗     │     ✓     │
│ Multi-provider     │   ✓    │    ✗     │    ✗    │     ✗     │     ✗     │
│ Warm pool          │   ✓    │    ✓     │    n/a  │     n/a   │     ✗     │
│ Custom packages    │   ✓    │    ✓     │    ✓    │     ~     │     ✓     │
│ Open source        │   ✓    │    ✓     │    ✓    │     ✗     │     n/a   │
│ Zero config        │   ✓    │    ✗     │    ✓    │     ✓     │     ✗     │
│ Setup lines        │   1    │    3     │    2    │     1     │    ~20    │
└────────────────────┴────────┴──────────┴─────────┴───────────┴───────────┘
""")


if __name__ == "__main__":
    print_comparison()
