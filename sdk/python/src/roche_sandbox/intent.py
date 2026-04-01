# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Client-side intent analysis for code execution.

Python uses AST parsing for precise import/call analysis.
Node and Bash use keyword matching.
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass, field


@dataclass
class CodeIntent:
    """Result of analyzing code intent."""

    provider: str = "docker"
    """Recommended provider: 'wasm', 'docker', 'firecracker'."""
    needs_network: bool = False
    network_hosts: list[str] = field(default_factory=list)
    needs_writable: bool = False
    writable_paths: list[str] = field(default_factory=list)
    needs_packages: bool = False
    package_manager: str | None = None
    memory_hint: str | None = None
    language: str = "python"
    confidence: float = 0.5
    reasoning: list[str] = field(default_factory=list)


# URL pattern for host extraction
_URL_RE = re.compile(r"https?://([a-zA-Z0-9.-]+)")

# --- Python AST-based analysis ---

# Modules that imply network access
_PY_NETWORK_MODULES = {
    "requests", "urllib", "urllib.request", "urllib3", "httpx",
    "aiohttp", "socket", "http", "http.client", "http.server",
    "ftplib", "smtplib", "xmlrpc", "grpc",
}

# Modules that imply package installation (heavy deps unlikely pre-installed)
_PY_HEAVY_PACKAGES = {
    "pandas", "numpy", "scipy", "sklearn", "scikit-learn",
    "tensorflow", "torch", "pytorch", "matplotlib", "seaborn",
    "polars", "xgboost", "lightgbm", "transformers", "langchain",
    "openai", "anthropic",
}

# Modules that imply high memory
_PY_HEAVY_MEMORY = {
    "pandas", "numpy", "scipy", "tensorflow", "torch", "polars",
    "xgboost", "lightgbm", "transformers",
}

# Functions/methods that imply filesystem writes
_PY_WRITE_FUNCS = {
    "open",  # checked with write mode
}

_PY_WRITE_METHODS = {
    "write", "writelines", "to_csv", "to_json", "to_parquet",
    "to_excel", "to_pickle", "to_hdf", "save", "savefig",
    "dump",  # json.dump, pickle.dump
}

_PY_WRITE_CALLS = {
    "os.makedirs", "os.mkdir", "os.rename", "os.remove", "os.unlink",
    "shutil.copy", "shutil.move", "shutil.rmtree",
    "pathlib.Path.mkdir", "pathlib.Path.write_text", "pathlib.Path.write_bytes",
}

# Modules that imply subprocess/shell execution
_PY_SUBPROCESS_MODULES = {"subprocess", "os"}

_PY_SUBPROCESS_FUNCS = {
    "subprocess.run", "subprocess.call", "subprocess.Popen",
    "subprocess.check_output", "subprocess.check_call",
    "os.system", "os.popen", "os.exec", "os.execvp",
}


def _analyze_python_ast(intent: CodeIntent, code: str) -> bool:
    """Analyze Python code via AST. Returns True if successful."""
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return False

    imports = _extract_imports(tree)
    calls = _extract_calls(tree)
    strings = _extract_strings(tree)

    # --- Imports → network, packages, memory ---
    for mod in imports:
        top = mod.split(".")[0]

        if mod in _PY_NETWORK_MODULES or top in _PY_NETWORK_MODULES:
            intent.needs_network = True
            intent.reasoning.append(f"Network: import `{mod}`")

        if top in _PY_HEAVY_PACKAGES:
            intent.needs_packages = True
            intent.needs_network = True
            intent.package_manager = "pip"
            intent.reasoning.append(f"Package: import `{mod}`")
            if "pypi.org" not in intent.network_hosts:
                intent.network_hosts.append("pypi.org")

        if top in _PY_HEAVY_MEMORY and intent.memory_hint is None:
            intent.memory_hint = "512m"
            intent.reasoning.append(f"Memory 512m: `{top}`")

    # --- Calls → filesystem writes, subprocess ---
    for call_name, args in calls:
        # open() with write mode
        if call_name == "open" and len(args) >= 2:
            mode = args[1]
            if isinstance(mode, str) and any(c in mode for c in "wax+"):
                intent.needs_writable = True
                intent.reasoning.append(f"FS write: open({args[0]!r}, {mode!r})")
                if isinstance(args[0], str):
                    _add_writable_path(intent, args[0])

        # .write(), .to_csv(), etc.
        parts = call_name.rsplit(".", 1)
        if len(parts) == 2 and parts[1] in _PY_WRITE_METHODS:
            intent.needs_writable = True
            intent.reasoning.append(f"FS write: .{parts[1]}()")

        # os.makedirs, shutil.copy, etc.
        if call_name in _PY_WRITE_CALLS:
            intent.needs_writable = True
            intent.reasoning.append(f"FS write: {call_name}()")

        # subprocess → likely needs network or writable
        if call_name in _PY_SUBPROCESS_FUNCS:
            intent.reasoning.append(f"Subprocess: {call_name}()")
            # Check if subprocess runs pip/apt
            if args and isinstance(args[0], (str, list)):
                cmd_str = str(args[0])
                if "pip install" in cmd_str or "pip3 install" in cmd_str:
                    intent.needs_packages = True
                    intent.needs_network = True
                    intent.package_manager = "pip"
                    intent.reasoning.append("Package: subprocess pip install")
                if any(x in cmd_str for x in ["apt-get install", "apt install"]):
                    intent.needs_packages = True
                    intent.needs_network = True
                    intent.package_manager = "apt"

    # --- Strings → URLs/hosts ---
    for s in strings:
        for match in _URL_RE.finditer(s):
            host = match.group(1)
            if host not in intent.network_hosts:
                intent.network_hosts.append(host)
                intent.needs_network = True
                intent.reasoning.append(f"Host: {host}")

    # --- String paths → writable paths ---
    if intent.needs_writable:
        for s in strings:
            _add_writable_path(intent, s)

    if intent.needs_writable and not intent.writable_paths:
        intent.writable_paths.append("/tmp")

    intent.confidence = 0.9  # AST is high confidence
    return True


def _extract_imports(tree: ast.AST) -> set[str]:
    """Extract all imported module names from AST."""
    modules: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            for alias in node.names:
                modules.add(alias.name)
        elif isinstance(node, ast.ImportFrom):
            if node.module:
                modules.add(node.module)
    return modules


def _extract_calls(tree: ast.AST) -> list[tuple[str, list]]:
    """Extract function calls with their string/list arguments."""
    calls: list[tuple[str, list]] = []
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        name = _call_name(node)
        if name is None:
            continue
        # Extract constant args for analysis
        args = []
        for arg in node.args:
            if isinstance(arg, ast.Constant) and isinstance(arg.value, (str, int, float)):
                args.append(arg.value)
            elif isinstance(arg, ast.List):
                elts = [e.value for e in arg.elts if isinstance(e, ast.Constant)]
                args.append(elts)
            else:
                args.append(None)
        calls.append((name, args))
    return calls


def _call_name(node: ast.Call) -> str | None:
    """Extract dotted call name: 'os.makedirs', 'open', 'df.to_csv'."""
    func = node.func
    if isinstance(func, ast.Name):
        return func.id
    if isinstance(func, ast.Attribute):
        parts = [func.attr]
        obj = func.value
        while isinstance(obj, ast.Attribute):
            parts.append(obj.attr)
            obj = obj.value
        if isinstance(obj, ast.Name):
            parts.append(obj.id)
        parts.reverse()
        return ".".join(parts)
    return None


def _extract_strings(tree: ast.AST) -> list[str]:
    """Extract all string constants from AST."""
    strings: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str) and len(node.value) > 3:
            strings.append(node.value)
    return strings


def _add_writable_path(intent: CodeIntent, s: str) -> None:
    """Add writable path if string looks like a path."""
    for prefix in ["/tmp", "/output", "/data", "/workspace", "/app", "/home"]:
        if s.startswith(prefix) and prefix not in intent.writable_paths:
            intent.writable_paths.append(prefix)


# --- Non-Python: keyword-based analysis ---

_KW_NETWORK: dict[str, list[str]] = {
    "python": ["import requests", "import urllib", "import httpx", "import aiohttp",
               "from requests", "from urllib", "from httpx", "from aiohttp",
               "import socket", "import http.client"],
    "node": ["require('http')", "require('https')", "require('axios')",
             "require('node-fetch')", "import fetch", "import axios"],
    "bash": ["curl ", "wget ", "nc ", "ssh "],
}
_KW_WRITE: dict[str, list[str]] = {
    "python": ["open(", "with open", ".write(", ".to_csv(", ".to_json(",
               "os.makedirs(", "os.mkdir("],
    "node": ["fs.writeFile", "fs.writeSync", "fs.mkdir", "createWriteStream"],
    "bash": [" > ", " >> ", "mkdir ", "touch ", "tee "],
}
_KW_HEAVY_LIBS = ["pandas", "numpy", "scipy", "tensorflow", "torch", "polars"]

_PACKAGE_INDICATORS: list[tuple[str, str, list[str]]] = [
    ("python", "pip", ["pip install", "pip3 install"]),
    ("python", "pip", ["import pandas", "import numpy", "import scipy",
                       "import sklearn", "import tensorflow", "import torch"]),
    ("node", "npm", ["npm install", "npx "]),
    ("bash", "apt", ["apt-get install", "apt install", "yum install", "apk add"]),
]

# Package registries
_REGISTRIES = {"pip": "pypi.org", "npm": "registry.npmjs.org"}


def _analyze_keyword(intent: CodeIntent, code: str, language: str) -> None:
    """Keyword-based analysis (fallback for Python, primary for Node/Bash)."""
    # Network
    for lang, indicators in _KW_NETWORK.items():
        if lang != language and language != "auto":
            continue
        for ind in indicators:
            if ind in code:
                intent.needs_network = True
                intent.reasoning.append(f"Network: `{ind}`")
                break

    # Filesystem writes
    for lang, indicators in _KW_WRITE.items():
        if lang != language and language != "auto":
            continue
        for ind in indicators:
            if ind in code:
                intent.needs_writable = True
                intent.reasoning.append(f"FS write: `{ind}`")
                break

    # Memory hints
    for lib in _KW_HEAVY_LIBS:
        if lib in code and intent.memory_hint is None:
            intent.memory_hint = "512m"
            intent.reasoning.append(f"Memory 512m: `{lib}`")
            break

    # Package install
    for lang, pm, inds in _PACKAGE_INDICATORS:
        if lang != language and language != "auto":
            continue
        for ind in inds:
            if ind in code:
                intent.needs_packages = True
                intent.needs_network = True
                intent.package_manager = pm
                intent.reasoning.append(f"Package: `{ind}`")
                registry = _REGISTRIES.get(pm, "")
                if registry and registry not in intent.network_hosts:
                    intent.network_hosts.append(registry)
                break

    # URLs
    for match in _URL_RE.finditer(code):
        host = match.group(1)
        if host not in intent.network_hosts:
            intent.network_hosts.append(host)
            intent.needs_network = True
            intent.reasoning.append(f"Host: {host}")

    # Writable paths
    if intent.needs_writable:
        for path in ["/tmp", "/output", "/data", "/workspace"]:
            if path in code and path not in intent.writable_paths:
                intent.writable_paths.append(path)
        if not intent.writable_paths:
            intent.writable_paths.append("/tmp")


# --- Main entry point ---


def analyze(code: str, language: str = "python") -> CodeIntent:
    """Analyze code and infer execution intent.

    Python: uses AST parsing (precise, no false positives from comments).
    Node/Bash: uses keyword matching.
    """
    intent = CodeIntent(language=language, provider="wasm")

    if language == "python" or language == "auto":
        if _analyze_python_ast(intent, code):
            # AST succeeded — also do URL extraction from non-Python if auto
            if language == "auto":
                _analyze_keyword(intent, code, "auto")
        else:
            # AST failed (syntax error) — fall back to keywords
            _analyze_keyword(intent, code, "python")
            intent.reasoning.append("AST parse failed, fell back to keyword matching")
    else:
        _analyze_keyword(intent, code, language)

    _determine_provider(intent)
    return intent


def _determine_provider(intent: CodeIntent) -> None:
    if intent.needs_network or intent.needs_packages or intent.needs_writable:
        intent.provider = "docker"
        if intent.confidence < 0.8:
            intent.confidence = 0.8
    else:
        intent.provider = "wasm"
        if intent.confidence < 0.7:
            intent.confidence = 0.7
    intent.reasoning.append(f"Provider: {intent.provider} ({intent.confidence:.0%})")
