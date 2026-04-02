# GitHub Repo Tester

Clone any public GitHub repo and run it in a sandbox. CI-as-a-function.

## Quick Start

```bash
pip install roche-sandbox

# Test a repo with Dockerfile
python examples/github-tester/tester.py user/flask-app

# Test with explicit command
python examples/github-tester/tester.py user/python-lib --command "pytest -v"

# Test a specific branch
python examples/github-tester/tester.py user/repo --ref feature-branch
```

## How It Works

```
1. git clone --depth 1 https://github.com/user/repo
2. Detect execution strategy:
   ├── Dockerfile found → docker build + run
   ├── --command given  → copy to sandbox + run command
   └── main.py found   → copy + install deps + run
3. Capture stdout/stderr, report pass/fail
4. Clean up (temp dir + sandbox + built image)
```

## Examples

```bash
# A Python project with tests
python examples/github-tester/tester.py psf/requests --command "python -m pytest tests/ -x -q"

# A Node.js project
python examples/github-tester/tester.py expressjs/express --command "npm test"

# A project with Dockerfile
python examples/github-tester/tester.py docker/getting-started
```

## What Roche Does

- **Isolation**: code runs in a locked-down Docker container
- **Auto-detect**: Dockerfile → build+run, otherwise detect language + deps
- **Network**: enabled for dependency installation, repos that need it
- **Timeout**: default 120s, configurable
- **Cleanup**: everything removed after run (temp dir, sandbox, built images)
