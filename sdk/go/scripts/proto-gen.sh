#!/usr/bin/env bash
set -euo pipefail

export PATH="${GOPATH:-$HOME/go}/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROTO_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)/crates/roche-daemon/proto"
OUT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)/gen"

mkdir -p "$OUT_DIR/roche/v1"

protoc \
  -I "$PROTO_DIR" \
  --go_out="$OUT_DIR" \
  --go_opt=paths=source_relative \
  --go_opt=Mroche/v1/sandbox.proto=github.com/substratum-labs/roche-go/gen/roche/v1 \
  --go-grpc_out="$OUT_DIR" \
  --go-grpc_opt=paths=source_relative \
  --go-grpc_opt=Mroche/v1/sandbox.proto=github.com/substratum-labs/roche-go/gen/roche/v1 \
  "$PROTO_DIR/roche/v1/sandbox.proto"
