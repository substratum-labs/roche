"""Sample ETL transform — reads CSV, transforms, writes output.

Demonstrates file I/O inside a sandbox. Roche auto-detects:
- needs pandas → installs it
- writes to /tmp → enables writable filesystem
- no network needed → keeps network disabled
"""

import pandas as pd
import json

# Read input (would be uploaded via --data in real usage)
# For demo, generate sample data
data = {
    "name": ["Alice", "Bob", "Charlie", "Diana", "Eve"],
    "score": [85, 92, 78, 95, 88],
    "department": ["engineering", "marketing", "engineering", "marketing", "engineering"],
}
df = pd.DataFrame(data)

# Transform
result = (
    df.groupby("department")
    .agg(
        avg_score=("score", "mean"),
        max_score=("score", "max"),
        count=("name", "count"),
        top_performer=("name", lambda x: x.iloc[df.loc[x.index, "score"].argmax()]),
    )
    .round(1)
)

print("=== Department Summary ===")
print(result.to_string())

# Write outputs
result.to_csv("/tmp/summary.csv")
result.to_json("/tmp/summary.json", orient="records", indent=2)

print(f"\nWrote /tmp/summary.csv and /tmp/summary.json")
