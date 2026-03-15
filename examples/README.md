# Roche Examples

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) installed and running
- `roche` CLI on PATH (`cargo install --path crates/roche-cli`)

## Python — Basic

```bash
pip install -e sdk/python
python examples/python/basic.py
python examples/python/async_context_manager.py
```

## Python — Agent Framework Integrations

Each framework directory contains:
- **basic example** — minimal tool integration (~30-60 lines)
- **advanced example** — multi-step workflow (~60-120 lines)
- **requirements.txt** — framework-specific dependencies

All examples run in **simulated mode** by default (no API key needed). Set the appropriate env var to enable real LLM calls.

### OpenAI Agents SDK

```bash
pip install -r examples/python/openai-agents/requirements.txt
python examples/python/openai-agents/basic_tool.py
python examples/python/openai-agents/code_interpreter.py

# Real LLM mode:
OPENAI_API_KEY=sk-... python examples/python/openai-agents/basic_tool.py
```

### LangChain / LangGraph

```bash
pip install -r examples/python/langchain/requirements.txt
python examples/python/langchain/basic_tool.py
python examples/python/langchain/code_agent.py

# Real LLM mode:
OPENAI_API_KEY=sk-... python examples/python/langchain/basic_tool.py
```

### CrewAI

```bash
pip install -r examples/python/crewai/requirements.txt
python examples/python/crewai/basic_task.py
python examples/python/crewai/research_crew.py

# Real LLM mode:
OPENAI_API_KEY=sk-... python examples/python/crewai/basic_task.py
```

### Anthropic API

```bash
pip install -r examples/python/anthropic/requirements.txt
python examples/python/anthropic/basic_tool.py
python examples/python/anthropic/code_assistant.py

# Real LLM mode:
ANTHROPIC_API_KEY=sk-ant-... python examples/python/anthropic/basic_tool.py
```

### AutoGen

```bash
pip install -r examples/python/autogen/requirements.txt
python examples/python/autogen/basic_executor.py
python examples/python/autogen/group_chat.py

# Real LLM mode:
OPENAI_API_KEY=sk-... python examples/python/autogen/basic_executor.py
```

### Camel-AI

```bash
pip install -r examples/python/camel/requirements.txt
python examples/python/camel/basic_tool.py
python examples/python/camel/coding_agent.py

# Real LLM mode:
OPENAI_API_KEY=sk-... python examples/python/camel/basic_tool.py
```

## TypeScript

```bash
cd sdk/typescript && npm ci && npm run build && cd ../..
npx tsx examples/typescript/basic.ts
```
