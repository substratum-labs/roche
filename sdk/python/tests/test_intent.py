# SPDX-License-Identifier: Apache-2.0
# Copyright 2025 Substratum Labs

import unittest

from roche_sandbox.intent import analyze


class TestIntentAnalysis(unittest.TestCase):
    def test_pure_compute(self):
        intent = analyze("print(2 + 2)", "python")
        assert intent.provider == "wasm"
        assert not intent.needs_network
        assert not intent.needs_writable

    def test_requests_detects_network(self):
        code = "import requests\nr = requests.get('https://api.openai.com/v1/chat')"
        intent = analyze(code, "python")
        assert intent.provider == "docker"
        assert intent.needs_network
        assert "api.openai.com" in intent.network_hosts

    def test_pip_install(self):
        intent = analyze("pip install pandas", "bash")
        assert intent.needs_packages or intent.needs_network
        assert intent.provider == "docker"

    def test_pandas_memory_hint(self):
        intent = analyze("import pandas as pd\ndf = pd.read_csv('data.csv')", "python")
        assert intent.memory_hint == "512m"

    def test_file_write(self):
        intent = analyze("with open('/tmp/out.txt', 'w') as f:\n    f.write('hi')", "python")
        assert intent.needs_writable
        assert "/tmp" in intent.writable_paths

    def test_curl_bash(self):
        intent = analyze("curl https://api.github.com/repos | jq '.'", "bash")
        assert intent.needs_network
        assert "api.github.com" in intent.network_hosts

    def test_explicit_opts_override_intent(self):
        """Verify the override logic works (tested via run.py integration)."""
        intent = analyze("print('hello')", "python")
        assert intent.provider == "wasm"
        # But user can override to docker — tested in test_run.py

    def test_multiple_urls(self):
        code = """
import requests
a = requests.get('https://api.openai.com/v1/models')
b = requests.get('https://cdn.example.com/data.json')
"""
        intent = analyze(code, "python")
        assert "api.openai.com" in intent.network_hosts
        assert "cdn.example.com" in intent.network_hosts

    def test_reasoning_populated(self):
        intent = analyze("import requests", "python")
        assert len(intent.reasoning) > 0


if __name__ == "__main__":
    unittest.main()
