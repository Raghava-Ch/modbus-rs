#!/usr/bin/env bash
set -euo pipefail

# Canonical runner for browser-side WASM E2E tests.
# Run from anywhere inside the repository:
#   bash mbus-ffi/scripts/run_wasm_browser_tests.sh

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR/mbus-ffi"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: required command '$1' not found in PATH" >&2
    exit 1
  fi
}

require_cmd cargo
require_cmd wasm-pack

if ! command -v google-chrome >/dev/null 2>&1 && \
   ! command -v chromium >/dev/null 2>&1 && \
   ! command -v chromium-browser >/dev/null 2>&1; then
  echo "error: Chrome/Chromium not found. Install a Chromium-based browser for --chrome tests." >&2
  exit 1
fi

# wasm-pack browser tests use webdriver under the hood; keep this explicit.
if ! command -v chromedriver >/dev/null 2>&1; then
  echo "error: chromedriver not found in PATH" >&2
  echo "hint: install chromedriver and ensure major version matches your Chrome/Chromium." >&2
  exit 1
fi

echo "[wasm-browser-tests] Running mbus-ffi wasm browser tests (headless Chrome)..."
wasm-pack test --headless --chrome --features wasm,full --test wasm_e2e
