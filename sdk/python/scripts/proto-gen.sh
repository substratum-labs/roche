#!/usr/bin/env bash
set -euo pipefail

PROTO_DIR="$(cd "$(dirname "$0")/../../.." && pwd)/proto"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/src/roche_sandbox/generated"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/roche/v1"
touch "$OUT_DIR/__init__.py"
touch "$OUT_DIR/roche/__init__.py"
touch "$OUT_DIR/roche/v1/__init__.py"

python -m grpc_tools.protoc \
  -I "$PROTO_DIR" \
  --python_out="$OUT_DIR" \
  --grpc_python_out="$OUT_DIR" \
  --pyi_out="$OUT_DIR" \
  "$PROTO_DIR/roche/v1/sandbox.proto"
