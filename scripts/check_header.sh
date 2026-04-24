#!/usr/bin/env bash
# check_header.sh — Verify that modbus_rs_client.h matches the current Rust source.
#
# Usage:
#   ./scripts/check_header.sh          # exits 1 if the header is stale
#   ./scripts/check_header.sh --fix    # regenerates the header under target/
#
# Prerequisites: cbindgen must be on $PATH.
#   cargo install cbindgen --locked

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HEADER_DIR="$REPO_ROOT/target/mbus-ffi/include"
HEADER="$HEADER_DIR/modbus_rs_client.h"
CBINDGEN_TOML="$REPO_ROOT/mbus-ffi/cbindgen_client.toml"

if ! command -v cbindgen &>/dev/null; then
    echo "ERROR: cbindgen not found. Install with: cargo install cbindgen --locked"
    exit 1
fi

# Regenerate into a temp file so we can diff without touching the tracked file.
TMPFILE="$(mktemp /tmp/mbus_ffi_XXXXXX.h)"
trap 'rm -f "$TMPFILE"' EXIT

cbindgen \
    --config "$CBINDGEN_TOML" \
    --crate mbus-ffi \
    --output "$TMPFILE" \
    --quiet

if [[ ! -f "$HEADER" ]]; then
    mkdir -p "$HEADER_DIR"
    cp "$TMPFILE" "$HEADER"
    echo "Header bootstrapped: $HEADER"
    exit 0
fi

if [[ "${1:-}" == "--fix" ]]; then
    mkdir -p "$HEADER_DIR"
    cp "$TMPFILE" "$HEADER"
    echo "Header regenerated: $HEADER"
    exit 0
fi

if ! diff -u "$HEADER" "$TMPFILE"; then
    echo ""
    echo "ERROR: modbus_rs_client.h is out of date with the Rust source."
    echo "Run the following to fix it:"
    echo ""
    echo "  ./scripts/check_header.sh --fix"
    echo ""
    exit 1
fi

echo "OK: modbus_rs_client.h is up to date."