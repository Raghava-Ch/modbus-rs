#!/usr/bin/env bash
# Build the modbus-rs Node.js native addon.
#
# Requirements:
#   * Rust stable toolchain (https://rustup.rs/)
#   * Node.js >= 20 LTS and npm
#   * On Linux: `libudev-dev` (Debian/Ubuntu) or `libudev-devel` (RHEL/Fedora).
#     The serialport crate's transitive dependency requires the udev headers.
#       sudo apt-get install -y libudev-dev
#
# Outputs `mbus-ffi/nodejs/modbus-rs.<triple>.node`.

set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v napi >/dev/null 2>&1; then
    echo "napi CLI not found — installing local dev dependencies first..."
    npm install
fi

MODE="${1:-release}"
case "$MODE" in
    release)  EXTRA="--release" ;;
    debug)    EXTRA="" ;;
    *) echo "Unknown mode: $MODE (expected: release | debug)"; exit 1 ;;
esac

# shellcheck disable=SC2086
npx napi build --platform $EXTRA \
    --features nodejs,full \
    --js index.js --dts index.d.ts \
    --cargo-cwd ..

echo
echo "Build complete. Native artifacts:"
ls -la *.node 2>/dev/null || echo "  (no .node file found at $(pwd))"
