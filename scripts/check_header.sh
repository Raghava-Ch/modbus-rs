#!/usr/bin/env bash
# check_header.sh — Verify that mbus_ffi.h matches the current Rust source.
#
# Usage:
#   ./scripts/check_header.sh          # exits 1 if the header is stale
#   ./scripts/check_header.sh --fix    # regenerates the header in place
#
# Prerequisites: cbindgen must be on $PATH.
#   cargo install cbindgen --locked

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HEADER="$REPO_ROOT/mbus-ffi/include/mbus_ffi.h"
CBINDGEN_TOML="$REPO_ROOT/mbus-ffi/cbindgen.toml"

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

if [[ "${1:-}" == "--fix" ]]; then
    cp "$TMPFILE" "$HEADER"
    echo "Header regenerated: $HEADER"
    exit 0
fi

if ! diff -u "$HEADER" "$TMPFILE"; then
    echo ""
    echo "ERROR: mbus_ffi.h is out of date with the Rust source."
    echo "Run the following to fix it:"
    echo ""
    echo "  ./scripts/check_header.sh --fix"
    echo ""
    exit 1
fi

echo "OK: mbus_ffi.h is up to date."
