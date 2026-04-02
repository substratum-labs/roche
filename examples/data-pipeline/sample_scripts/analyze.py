"""Sample data analysis script — generates summary statistics.

This runs inside a Roche sandbox with pandas auto-installed.
"""

import pandas as pd
import numpy as np

# Generate sample data
np.random.seed(42)
df = pd.DataFrame({
    "date": pd.date_range("2024-01-01", periods=100),
    "revenue": np.random.normal(1000, 200, 100).round(2),
    "users": np.random.randint(50, 500, 100),
    "region": np.random.choice(["US", "EU", "APAC"], 100),
})

# Analysis
print("=== Revenue Summary ===")
print(df.groupby("region")["revenue"].describe().round(2))

print("\n=== Top 5 Days by Revenue ===")
top = df.nlargest(5, "revenue")[["date", "revenue", "users", "region"]]
print(top.to_string(index=False))

print(f"\n=== Totals ===")
print(f"Total revenue: ${df['revenue'].sum():,.2f}")
print(f"Total users:   {df['users'].sum():,}")
print(f"Period:        {df['date'].min().date()} to {df['date'].max().date()}")

# Write output
df.to_csv("/tmp/output.csv", index=False)
print("\nWrote /tmp/output.csv")
