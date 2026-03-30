# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

"""Client-side intent analysis for code execution.

Mirrors the Rust intent engine (roche-core/src/intent.rs) logic.
Analyzes code to infer provider, permissions, and resource hints.
"""

from __future__ import annotations

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

# Network indicators by language
_NETWORK_INDICATORS: dict[str, list[str]] = {
    "python": ["import requests", "import urllib", "import httpx", "import aiohttp",
               "from requests", "from urllib", "from httpx", "from aiohttp",
               "import socket", "import http.client"],
    "node": ["require('http')", "require('https')", "require('axios')",
             "require('node-fetch')", "import fetch", "import axios"],
    "bash": ["curl ", "wget ", "nc ", "ssh "],
}

# Package indicators: (language, package_manager, indicators)
_PACKAGE_INDICATORS: list[tuple[str, str, list[str]]] = [
    ("python", "pip", ["pip install", "pip3 install"]),
    ("python", "pip", ["import pandas", "import numpy", "import scipy",
                       "import sklearn", "import tensorflow", "import torch"]),
    ("node", "npm", ["npm install", "npx "]),
    ("bash", "apt", ["apt-get install", "apt install", "yum install", "apk add"]),
]

# Write indicators
_WRITE_INDICATORS: dict[str, list[str]] = {
    "python": ["open(", "with open", ".write(", ".to_csv(", ".to_json(",
               "os.makedirs(", "os.mkdir("],
    "node": ["fs.writeFile", "fs.writeSync", "fs.mkdir", "createWriteStream"],
    "bash": [" > ", " >> ", "mkdir ", "touch ", "tee "],
}

# Data-heavy libraries that suggest more memory
_HEAVY_LIBS = ["pandas", "numpy", "scipy", "tensorflow", "torch", "polars"]

# Package registries
_REGISTRIES = {"pip": "pypi.org", "npm": "registry.npmjs.org"}


def analyze(code: str, language: str = "python") -> CodeIntent:
    """Analyze code and infer execution intent."""
    intent = CodeIntent(language=language, provider="wasm")

    _analyze_network(intent, code, language)
    _analyze_packages(intent, code, language)
    _analyze_filesystem(intent, code, language)
    _analyze_resources(intent, code)
    _determine_provider(intent)

    return intent


def _analyze_network(intent: CodeIntent, code: str, language: str) -> None:
    for lang, indicators in _NETWORK_INDICATORS.items():
        if lang != language and language != "auto":
            continue
        for indicator in indicators:
            if indicator in code:
                intent.needs_network = True
                intent.reasoning.append(f"Network needed: `{indicator}`")
                break

    # Extract hosts from URLs
    for match in _URL_RE.finditer(code):
        host = match.group(1)
        if host not in intent.network_hosts:
            intent.network_hosts.append(host)
            intent.needs_network = True
            intent.reasoning.append(f"Detected host: {host}")


def _analyze_packages(intent: CodeIntent, code: str, language: str) -> None:
    for lang, pm, indicators in _PACKAGE_INDICATORS:
        if lang != language and language != "auto":
            continue
        for indicator in indicators:
            if indicator in code:
                intent.needs_packages = True
                intent.needs_network = True
                intent.package_manager = pm
                intent.reasoning.append(f"Package install: `{indicator}`")
                registry = _REGISTRIES.get(pm, "")
                if registry and registry not in intent.network_hosts:
                    intent.network_hosts.append(registry)
                break


def _analyze_filesystem(intent: CodeIntent, code: str, language: str) -> None:
    for lang, indicators in _WRITE_INDICATORS.items():
        if lang != language and language != "auto":
            continue
        for indicator in indicators:
            if indicator in code:
                intent.needs_writable = True
                intent.reasoning.append(f"FS write: `{indicator}`")
                break

    # Extract common paths
    for path in ["/tmp", "/output", "/data", "/workspace"]:
        if path in code and intent.needs_writable and path not in intent.writable_paths:
            intent.writable_paths.append(path)

    if intent.needs_writable and not intent.writable_paths:
        intent.writable_paths.append("/tmp")


def _analyze_resources(intent: CodeIntent, code: str) -> None:
    for lib in _HEAVY_LIBS:
        if lib in code:
            intent.memory_hint = "512m"
            intent.reasoning.append(f"Memory hint 512m: `{lib}` detected")
            break


def _determine_provider(intent: CodeIntent) -> None:
    if intent.needs_network or intent.needs_packages or intent.needs_writable:
        intent.provider = "docker"
        intent.confidence = 0.8
    else:
        intent.provider = "wasm"
        intent.confidence = 0.7
    intent.reasoning.append(f"Provider: {intent.provider} ({intent.confidence:.0%})")
