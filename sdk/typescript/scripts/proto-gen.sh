#!/usr/bin/env bash
set -euo pipefail

# Requires: protoc installed on the system (e.g., brew install protobuf)
PROTO_DIR="$(cd "$(dirname "$0")/../../.." && pwd)/proto"
OUT_DIR="$(cd "$(dirname "$0")/.." && pwd)/src/generated"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

protoc \
  --plugin=protoc-gen-ts_proto=./node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out="$OUT_DIR" \
  --ts_proto_opt=outputServices=grpc-js \
  --ts_proto_opt=esModuleInterop=true \
  --ts_proto_opt=snakeToCamel=true \
  --ts_proto_opt=forceLong=number \
  -I "$PROTO_DIR" \
  "$PROTO_DIR/roche/v1/sandbox.proto"
