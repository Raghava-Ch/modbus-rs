#!/usr/bin/env bash
#
# Build the modbus-rs `mbus_ffi` cdylib + staticlib for the host
# platform and copy the static archive + freshly-generated header into
# the Go module so `go build`/`go test` can find them.
#
# Usage:
#     ./scripts/build_native.sh              # release build
#     PROFILE=debug ./scripts/build_native.sh
#
# The script must be run from anywhere within the modbus-rs workspace.

set -euo pipefail

# ── Locate workspace root ──────────────────────────────────────────────────
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
GO_MODULE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ROOT="$(cd "$GO_MODULE_DIR/../.." && pwd)"

PROFILE="${PROFILE:-release}"
CARGO_FLAG=""
if [[ "$PROFILE" == "release" ]]; then
    CARGO_FLAG="--release"
fi

# ── Detect target triple → Go GOOS_GOARCH ──────────────────────────────────
OS_NAME="$(uname -s)"
ARCH="$(uname -m)"
case "$OS_NAME" in
    Linux)  GOOS=linux ;;
    Darwin) GOOS=darwin ;;
    MINGW*|MSYS*|CYGWIN*) GOOS=windows ;;
    *) echo "Unsupported OS: $OS_NAME" >&2; exit 1 ;;
esac
case "$ARCH" in
    x86_64|amd64) GOARCH=amd64 ;;
    aarch64|arm64) GOARCH=arm64 ;;
    *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac
PLATFORM_DIR="${GOOS}_${GOARCH}"

# ── Build ──────────────────────────────────────────────────────────────────
# On Windows we target x86_64-pc-windows-gnu so that the static archive uses
# the MinGW ABI.  The default MSVC target (x86_64-pc-windows-msvc) emits
# calls to __chkstk (a MSVC CRT symbol) which MinGW's ld — used by cgo —
# cannot resolve.  The GNU target produces a plain libmbus_ffi.a that is
# fully compatible with MinGW.
if [[ "$GOOS" == "windows" ]]; then
    echo "→ Building mbus-ffi (${PROFILE}) for x86_64-pc-windows-gnu"
    rustup target add x86_64-pc-windows-gnu
    (cd "$ROOT" && cargo build $CARGO_FLAG -p mbus-ffi --features go,full \
        --target x86_64-pc-windows-gnu)
    LIB_SRC="$ROOT/target/x86_64-pc-windows-gnu/$PROFILE/libmbus_ffi.a"
else
    echo "→ Building mbus-ffi (${PROFILE}) with --features go,full"
    (cd "$ROOT" && cargo build $CARGO_FLAG -p mbus-ffi --features go,full)
    LIB_SRC="$ROOT/target/$PROFILE/libmbus_ffi.a"
fi

# ── Vendor the artefacts ───────────────────────────────────────────────────
HDR_SRC="$ROOT/target/mbus-ffi/include/modbus_rs_go.h"
LIB_DST_DIR="$GO_MODULE_DIR/internal/cgo/lib/$PLATFORM_DIR"
HDR_DST="$GO_MODULE_DIR/internal/cgo/include/modbus_rs_go.h"

mkdir -p "$LIB_DST_DIR"
cp -v "$LIB_SRC" "$LIB_DST_DIR/libmbus_ffi.a"
cp -v "$HDR_SRC" "$HDR_DST"

echo
echo "✔ Vendored static lib → $LIB_DST_DIR"
echo "✔ Vendored header     → $HDR_DST"
echo
echo "You can now run:   cd $GO_MODULE_DIR && go test ./..."
