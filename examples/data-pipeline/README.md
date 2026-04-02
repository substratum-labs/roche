# Data Pipeline Runner

Run data processing scripts safely in sandboxes. Upload a script, get results back — pandas and numpy auto-installed.

## Quick Start

```bash
pip install roche-sandbox

# Run a sample analysis script
python examples/data-pipeline/runner.py examples/data-pipeline/sample_scripts/analyze.py

# Download output files from sandbox
python examples/data-pipeline/runner.py examples/data-pipeline/sample_scripts/transform.py \
  --download /tmp/summary.csv --download /tmp/summary.json

# With dependency caching (fast on repeat runs)
python examples/data-pipeline/runner.py examples/data-pipeline/sample_scripts/analyze.py --cached
```

## What Roche Does

1. **Reads the script** → detects `import pandas` → knows it needs pip install
2. **Detects `.to_csv("/tmp/...")`** → enables writable filesystem for /tmp
3. **No network imports** → keeps network disabled (safe)
4. **Copies script into sandbox** → runs it → captures stdout
5. **Downloads output files** → `--download /tmp/output.csv` copies files back

## Sample Scripts

| Script | What it does |
|---|---|
| `analyze.py` | Generates sample data, computes stats, writes CSV |
| `transform.py` | ETL: group-by aggregation, writes CSV + JSON |

## With Your Own Scripts

```bash
# Your script that reads data and writes results
python examples/data-pipeline/runner.py my_analysis.py \
  --data my_data.csv \
  --download /tmp/results.csv \
  --cached
```

The script receives the data file in the same directory. Roche handles isolation, dependency installation, and file transfer.
