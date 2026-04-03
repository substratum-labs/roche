#!/usr/bin/env python3
"""Side-by-side comparison: Roche vs competitors for LLM code execution.

Shows real setup code and features for each approach.

Usage:
    python examples/llm-code-interpreter/comparison.py
"""


def print_comparison():
    print("""
╔══════════════════════════════════════════════════════════════════════════════════╗
║                    LLM Code Execution — Landscape (April 2026)                 ║
╚══════════════════════════════════════════════════════════════════════════════════╝

┌─────────────────────────────────────────────────────────────────────────────────┐
│  ROCHE — 1 line, local, intent-aware                                           │
│                                                                                 │
│  from roche_sandbox import run                                                  │
│  result = run("print(2+2)")                                                     │
│                                                                                 │
│  ✓ 5 providers (Docker, WASM, Firecracker, E2B, K8s)                           │
│  ✓ Intent analysis: auto-detects network/FS/memory needs                       │
│  ✓ Network off by default, allowlist auto-inferred from code                   │
│  ✓ WASM: sub-ms startup for pure compute (~1000x faster than Docker)           │
│  ✓ Local — no cloud, no API key, no internet required                          │
│  ✓ Free, open source (Apache 2.0)                                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│  E2B — 4 lines, cloud, Firecracker microVMs                                    │
│                                                                                 │
│  from e2b import Sandbox                                                        │
│  sandbox = Sandbox.create()                                                     │
│  result = sandbox.commands.run('echo "Hello"')                                  │
│  print(result.stdout)                                                           │
│                                                                                 │
│  ✓ Firecracker microVMs (hardware isolation, own kernel per sandbox)            │
│  ✓ ~150ms cold start                                                            │
│  ✓ ~$0.05/hr per vCPU. Free tier: $100 credit                                  │
│  ✗ Cloud-only — requires internet + API key                                    │
│  ✗ No intent analysis — manual permission config                               │
│  ✗ No GPU support                                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│  OPEN INTERPRETER — 2 lines, local, NO isolation                                │
│                                                                                 │
│  from interpreter import interpreter                                            │
│  interpreter.chat("plot sin(x)")                                                │
│                                                                                 │
│  ✓ Best UX — conversational, natural                                           │
│  ✓ Local, free (+ LLM API costs)                                               │
│  ✗ NO SANDBOX — code runs on your machine with full access                     │
│  ✗ Can delete files, access network, install anything                          │
│  ✗ Not safe for autonomous agents or production                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│  OPENAI CODE INTERPRETER — built-in, managed                                    │
│                                                                                 │
│  response = client.responses.create(                                            │
│      model="gpt-4.1",                                                           │
│      tools=[{"type": "code_interpreter"}],                                      │
│      input="solve 3x + 11 = 14")                                               │
│                                                                                 │
│  ✓ Zero setup — built into the API                                              │
│  ✓ Sandboxed VM per session                                                     │
│  ✗ $0.03/container + $0.03 per 20-min session                                  │
│  ✗ Python only, limited pre-installed packages                                 │
│  ✗ No network, no custom environments                                          │
│  ✗ OpenAI models only — can't use Claude or local LLMs                         │
│  ✗ 20-min inactivity timeout, no persistent storage                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│  MODAL — 5 lines, cloud, gVisor + GPU                                           │
│                                                                                 │
│  import modal                                                                   │
│  app = modal.App.lookup("my-app", create_if_missing=True)                       │
│  sandbox = modal.Sandbox.create(app=app)                                        │
│  process = sandbox.exec("python", "-c", "print('hello')")                       │
│  print(process.stdout.read())                                                   │
│                                                                                 │
│  ✓ GPU support (A100, H100) — unique advantage                                 │
│  ✓ gVisor isolation, SOC2 + HIPAA compliant                                    │
│  ✓ 50K+ concurrent containers, sub-second cold start                           │
│  ✗ ~$0.14/hr per core (3x more than E2B/Daytona)                              │
│  ✗ Cloud-only, requires account + app setup                                    │
│  ✗ No intent analysis                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│  DAYTONA — 4 lines, self-hostable, full dev environments                        │
│                                                                                 │
│  from daytona import Daytona                                                    │
│  daytona = Daytona()                                                            │
│  sandbox = daytona.create()                                                     │
│  response = sandbox.process.code_run('print("Hello")')                          │
│                                                                                 │
│  ✓ Self-hostable or cloud. ~90ms cold start                                    │
│  ✓ Full dev environments: LSP, git, SSH, VS Code in browser                    │
│  ✓ Open source, ~$0.05/hr                                                       │
│  ✗ Docker isolation (weaker than Firecracker/gVisor)                           │
│  ✗ No intent analysis                                                           │
│  ✗ Newer entrant — ecosystem still maturing                                    │
└─────────────────────────────────────────────────────────────────────────────────┘

Feature Matrix:
┌─────────────────────┬───────┬───────┬──────────┬──────────┬───────┬─────────┐
│                     │ Roche │  E2B  │ Open Int │ OpenAI   │ Modal │ Daytona │
├─────────────────────┼───────┼───────┼──────────┼──────────┼───────┼─────────┤
│ Isolated            │  ✓    │  ✓    │    ✗     │    ✓     │   ✓   │    ✓    │
│ Network control     │  ✓    │  ✓    │    ✗     │    ✗     │   ✓   │    ✓    │
│ Intent analysis     │  ✓    │  ✗    │    ✗     │    ✗     │   ✗   │    ✗    │
│ Local / self-host   │  ✓    │  ✗    │    ✓     │    ✗     │   ✗   │    ✓    │
│ Multi-provider      │  ✓    │  ✗    │    ✗     │    ✗     │   ✗   │    ✗    │
│ GPU support         │  ✗    │  ✗    │  local   │    ✗     │   ✓   │    ✗    │
│ Custom packages     │  ✓    │  ✓    │    ✓     │    ~     │   ✓   │    ✓    │
│ Open source         │  ✓    │  ✓    │    ✓     │    ✗     │   ✗   │    ✓    │
│ Zero config         │  ✓    │  ✗    │    ✓     │    ✓     │   ✗   │    ✗    │
│ Cold start          │ <1ms* │ 150ms │    0     │   n/a    │  <1s  │  90ms   │
│ Cost/hr             │ free  │$0.05  │   free   │$0.03/ses │ $0.14 │  $0.05  │
│ Setup lines         │  1    │   4   │    2     │    3     │   5   │    4    │
└─────────────────────┴───────┴───────┴──────────┴──────────┴───────┴─────────┘
  * WASM provider for pure compute. Docker provider: ~1s cold start.
""")


if __name__ == "__main__":
    print_comparison()
