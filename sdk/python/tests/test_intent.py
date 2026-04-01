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
        intent = analyze("pip install pandas", "python")
        assert intent.needs_packages
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

    # --- AST-based analysis tests (v0.6) ---

    def test_ast_ignores_comments(self):
        """Comments should NOT trigger network detection via AST."""
        code = "# import requests\nprint('hello')"
        intent = analyze(code, "python")
        assert not intent.needs_network
        assert intent.provider == "wasm"

    def test_ast_open_write_mode(self):
        """open('x.txt', 'w') should trigger writable."""
        code = "f = open('x.txt', 'w')\nf.write('data')\nf.close()"
        intent = analyze(code, "python")
        assert intent.needs_writable

    def test_ast_open_read_only(self):
        """open('x.txt', 'r') should NOT trigger writable."""
        code = "f = open('x.txt', 'r')\ndata = f.read()\nf.close()"
        intent = analyze(code, "python")
        assert not intent.needs_writable

    def test_ast_open_keyword_mode(self):
        """open('x.txt', mode='w') should trigger writable via keyword arg."""
        code = "f = open('x.txt', mode='w')\nf.write('data')\nf.close()"
        intent = analyze(code, "python")
        assert intent.needs_writable

    def test_ast_stdout_write_not_writable(self):
        """sys.stdout.write() should NOT trigger writable."""
        code = "import sys\nsys.stdout.write('hello')"
        intent = analyze(code, "python")
        assert not intent.needs_writable

    def test_ast_to_csv(self):
        """df.to_csv('out.csv') should trigger writable."""
        code = "import pandas as pd\ndf = pd.DataFrame()\ndf.to_csv('out.csv')"
        intent = analyze(code, "python")
        assert intent.needs_writable

    def test_ast_subprocess_pip(self):
        """subprocess.run('pip install x') should trigger needs_packages."""
        code = "import subprocess\nsubprocess.run('pip install requests', shell=True)"
        intent = analyze(code, "python")
        assert intent.needs_packages
        assert intent.package_manager == "pip"

    def test_ast_fallback_on_syntax_error(self):
        """Non-Python code should fall back to keyword matching."""
        code = "pip install pandas"
        intent = analyze(code, "python")
        # Keyword fallback should detect 'pip install'
        assert intent.needs_packages
        assert any("fell back" in r.lower() or "keyword" in r.lower() for r in intent.reasoning)

    def test_ast_url_in_string(self):
        """URLs in string literals should be extracted."""
        code = "url = 'https://api.example.com/data'\nprint(url)"
        intent = analyze(code, "python")
        assert intent.needs_network
        assert "api.example.com" in intent.network_hosts

    def test_ast_heavy_package_memory(self):
        """import torch should set memory_hint to 512m."""
        code = "import torch\nx = torch.tensor([1, 2, 3])"
        intent = analyze(code, "python")
        assert intent.memory_hint == "512m"


if __name__ == "__main__":
    unittest.main()
