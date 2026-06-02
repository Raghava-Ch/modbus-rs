# xtask

Workspace maintenance, code generation, and C demo tooling for the `modbus-rs` repository.

## Run Location

Run all commands from the repository root:

```bash
cargo run -p xtask -- <command> [OPTIONS]
```

## Quick Start

Show help:

```bash
cargo run -p xtask -- help
```

Run the full release verification pipeline:

```bash
cargo run -p xtask -- check-release
```

---

## Demo Commands

Demos are **auto-discovered** by scanning `mbus-ffi/examples/*/demo.yaml`.  No demo names are hard-coded in xtask.  Adding a new `demo.yaml` to any subdirectory automatically makes it available to all three commands below.

### `list-c-demos`

Print every discovered demo with its run modes:

```bash
cargo run -p xtask -- list-c-demos
```

Example output:
```
c_client_demo  —  Modbus TCP/Serial client C FFI smoke test
  rust_features : c,full
  mode 'serial-pty' [default]  — In-process PTY loopback, no hardware needed

c_server_demo  —  Hand-written Modbus TCP server demo (no codegen)
  rust_features : c-server,full
  mode 'self-test' [default]  — In-process self-test (no network needed)

c_server_demo_yaml  —  YAML-driven Modbus TCP server with generated handler table
  rust_features : c-server,full
  codegen       : yes
  mode 'self-test'  — In-process test, no network needed
  mode 'server' [default]  — Real Modbus TCP server on port 5020 (Ctrl-C to stop)
```

### `build-c-demo`

Build one or all demos.

```bash
# Build a specific demo (static link, run CTest)
cargo run -p xtask -- build-c-demo --demo c_server_demo_yaml --static

# Build all discovered demos
cargo run -p xtask -- build-c-demo

# Build without running CTest
cargo run -p xtask -- build-c-demo --demo c_server_demo --no-test

# Override Rust features
cargo run -p xtask -- build-c-demo --demo c_server_demo_yaml --features c-server

# Skip codegen step (re-use existing generated files)
cargo run -p xtask -- build-c-demo --demo c_server_demo_yaml --skip-gen --static
```

Options:

| Flag | Default | Description |
|---|---|---|
| `--demo <name>` | *(all demos)* | Demo name as declared in `demo.yaml` |
| `--static` | dynamic | Link `libmbus_ffi.a` statically |
| `--features <list>` | from `demo.yaml` | Override Rust crate features for `cargo build -p mbus-ffi` |
| `--skip-gen` | false | Skip the codegen step even if `demo.yaml` declares one |
| `--no-test` | false | Skip CTest after the build |

What it does for each demo:
1. Run codegen (`gen-server-app`) if `demo.yaml` has a `codegen:` section and `--skip-gen` is not set
2. `cargo build -p mbus-ffi --features <features>`
3. `cmake -S . -B <build-dir> -DMBUS_FFI_LINK_STATIC=<ON|OFF>`
4. `cmake --build <build-dir>`
5. `ctest --test-dir <build-dir>` (unless `--no-test`)

### `run-c-demo`

Run a demo binary.  If the binary does not exist yet, it is built automatically first (with `--no-test`).

```bash
# Run the default mode of a demo
cargo run -p xtask -- run-c-demo --demo c_server_demo_yaml

# Run in self-test mode (no network)
cargo run -p xtask -- run-c-demo --demo c_server_demo_yaml --mode self-test

# Run using the static-linked binary
cargo run -p xtask -- run-c-demo --demo c_server_demo_yaml --static --mode server
```

Options:

| Flag | Default | Description |
|---|---|---|
| `--demo <name>` | *(required if >1 demo)* | Demo name as declared in `demo.yaml` |
| `--mode <name>` | `default_mode` in `demo.yaml` | Run mode to use |
| `--static` | false | Use the `build-static/` binary |

Run modes are declared per-demo in `demo.yaml`:

```yaml
run:
  default_mode: server
  modes:
    server:
      args: []
      description: "Real Modbus TCP server on port 5020 (Ctrl-C to stop)"
    self-test:
      args: ["--self-test"]
      description: "In-process test, no network needed"
```

---

## Adding a New Demo

1. Create a directory under `mbus-ffi/examples/`.
2. Add a `demo.yaml` manifest:

```yaml
name: my_demo
description: "One-line description"
binary: my_demo          # CMake executable target name
rust_features: "c-server,full"

# Optional — omit if no codegen needed
codegen:
  config: mbus-ffi/examples/my_demo/my_device.yaml
  out_dir: mbus-ffi/src/c/server_gen
  header: target/mbus-ffi/include/mbus_server_app.h

run:
  default_mode: self-test
  modes:
    self-test:
      args: ["--self-test"]
      description: "In-process test"
    server:
      args: []
      description: "Real TCP server on port 5020"
```

3. Add a `CMakeLists.txt` that builds the binary target named `my_demo`.

The demo is now automatically visible to `list-c-demos`, `build-c-demo`, and `run-c-demo`.

---

## Codegen Commands

### `gen-server-app`

Two‑mode operation:

#### Codegen Mode

Generate the C header from a YAML device config:

```bash
cargo run -p xtask -- gen-server-app \
  --config mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml \
  --emit-c-header target/mbus-ffi/include/mbus_server_app.h
```

The Rust dispatcher (`generated_server.rs`) is **not** written by this command — it is
generated at compile time by `mbus-ffi/build.rs` when `MBUS_SERVER_APP_CONFIG` is set.
Pass `--out-dir <path>` only if you need a standalone Rust file outside the normal build.

Flags: `--check` (verify without writing), `--dry-run` (print what would be written).

#### Full Mode (cross‑compile + bundle)

Parse the YAML, generate artifacts, cross‑compile `mbus-ffi` with **only** the
features required by the YAML config, and bundle the result:

```bash
cargo run -p xtask -- gen-server-app \
  --config mbus-ffi/examples/my_device.yaml \
  --target thumbv7em-none-eabi \
  --profile release \
  /path/to/output-dir
```

This will:
1. Parse the YAML and determine which memory‑map sections are non‑empty
2. Build `mbus-ffi` with features like `c-server,server-coils,server-holding-registers`
   (instead of the catch‑all `c-server,full`)
3. Populate `<output-dir>/` with:
   - `include/mbus_server_app.h` — generated C header
   - `include/modbus_rs_server.h` — server FFI header (from cbindgen)
   - `lib/libmbus_ffi.a` — cross‑compiled static library
   - `generated_server.rs` — Rust dispatcher for reference

| Flag | Default | Description |
|---|---|---|
| `--config <path>` | *(required)* | Path to YAML server app config |
| `--target <triple>` | *(required for full mode)* | Target triple for cross‑compilation |
| `--profile release\|debug` | `release` | Build profile |
| `--emit-c-header <path>` | — | Also write a standalone copy of the C header |
| `--dry-run` | false | Print what would be done without executing |

### `check-server-gen`

Verify that the generated C header (`mbus_server_app.h`) matches the current YAML config (used in CI):

```bash
cargo run -p xtask -- check-server-gen
```

---

## FFI Header Commands

### `gen-client-lib`
Regenerate the client FFI header (`modbus_rs_client.h`), compile `mbus-ffi` in the selected profile mode (release or debug) for the selected target, and bundle the header and compiled libraries (both static and dynamic formats) into the output directory:

```bash
# Default: generate and bundle under target/mbus-ffi/ (release profile)
cargo run -p xtask -- gen-client-lib

# Generate and bundle under a custom directory (creates include/ and library/)
cargo run -p xtask -- gen-client-lib --out-dir /path/to/output

# Generate and compile in debug mode
cargo run -p xtask -- gen-client-lib --profile debug

# Cross-compile for a specific target (e.g. thumbv7em-none-eabi) in release mode
cargo run -p xtask -- gen-client-lib --target thumbv7em-none-eabi
```

This will automatically:
1. Generate the C FFI client header (`modbus_rs_client.h`).
2. Run `cargo build -p mbus-ffi` (with `--release` if release profile, and with `--target` if specified) with the chosen features (defaults to `full`).
3. Create `include/` and `library/` folders under the output directory.
4. Copy the header to `<out-dir>/include/` and all compiled libraries (`libmbus_ffi.a`, `libmbus_ffi.dylib`, `libmbus_ffi.so`, `mbus_ffi.dll`, `mbus_ffi.lib` depending on the platform) from the correct build folder to `<out-dir>/library/`.

You can select a custom feature set by using the `--features` option:

```bash
cargo run -p xtask -- gen-client-lib --features coils,registers
```

### `check-client-header`
Verify the header is up to date (CI):

```bash
cargo run -p xtask -- check-client-header
```

Like the generator command, you can verify with a specific feature set:

```bash
cargo run -p xtask -- check-client-header --features coils,registers
```

You can also pass `--target` and `--profile` options to ensure command-line compatibility with generator invocations:

```bash
cargo run -p xtask -- check-client-header --target thumbv7em-none-eabi --profile debug
```

---

## Validation Commands

### `check-feature-matrix`
Run a coarse workspace-wide feature check (`--all-features` + doc tests):

```bash
cargo run -p xtask -- check-feature-matrix
```

### `check-feature-subsets`
Run the full per-feature subset matrix — `cargo check`, `cargo clippy -D warnings`, `cargo build`, and `cargo test` over every meaningful single-feature and combined-feature slice across `mbus-core`, `mbus-client`, `mbus-server`, `mbus-async`, `mbus-gateway`, and `mbus-ffi`.

```bash
# Fast mode: check + clippy only (~54 steps, skips build and test)
cargo run -p xtask -- check-feature-subsets --fast

# Full mode: check + clippy + build + test (~70 steps)
cargo run -p xtask -- check-feature-subsets
```

Use `--fast` for quick pre-push sweeps; omit it in nightly or release pipelines to also run the build and integration test slices.

**Covered combinations include** (non-exhaustive):

| Crate | Feature slices |
|-------|---------------|
| `mbus-core` | `coils`, `registers`, `diagnostics`, `coils,registers` |
| `mbus-client` | `coils`, `registers`, `diagnostics`, `coils,registers,traffic` |
| `mbus-server` | each FC feature in isolation + `coils,holding-registers`; all examples; focused integration tests |
| `mbus-async` | `network-tcp,coils`, `network-tcp,registers`, `+traffic`, `+coils+registers` |
| `mbus-gateway` | no-features, `network`, `serial-rtu`, `async`, `ws-server`, `network,serial-rtu` |
| `mbus-ffi` | `c,coils,registers`; `c,c-server,full`; `c,c-gateway,full` |

### `validate-docs`
Validate code examples in Markdown docs.  Disable colors with `NO_COLOR=1`.

```bash
# Validate all markdown docs
cargo run -p xtask -- validate-docs

# Validate only selected files
cargo run -p xtask -- validate-docs --file README.md
cargo run -p xtask -- validate-docs -f documentation/client/async.md -f documentation/server/async.md
```

Notes:
- `--file` / `-f` is repeatable
- Paths may be relative to the repo root or absolute
- Cross-reference checks are skipped when `--file` is used

### `check-doc-links`
Validate that local Markdown links point to existing files.

```bash
# Check links across all markdown docs
cargo run -p xtask -- check-doc-links

# Check only selected files
cargo run -p xtask -- check-doc-links --file README.md
cargo run -p xtask -- check-doc-links -f documentation/README.md -f mbus-ffi/README.md
```

`check-doc-links` ignores:
- External links (`http://`, `https://`, `mailto:`)
- Anchor-only links (`#section`)
- Links inside fenced code blocks

### `check-release`
Run the full release gate: `check-client-header` → `check-server-gen` → `build-c-smoke` → `build-c-demo --demo c_server_demo` → `check-feature-matrix`.

---

## Legacy Aliases

These names still work for backward compatibility:

| Alias | Equivalent |
|---|---|
| `build-c-smoke` | `build-c-demo --demo c_client_demo` |
| `build-c-server-demo` | `build-c-demo --demo c_server_demo` |
| `build-c-server-demo-static` | `build-c-demo --demo c_server_demo --static` |
| `build-c-server-demo-yaml` | `build-c-demo --demo c_server_demo_yaml` |
| `build-c-server-demo-yaml-static` | `build-c-demo --demo c_server_demo_yaml --static` |

---

## Troubleshooting

If `cargo` fails on macOS with:

```
ld: library 'System' not found
```

Set `SDKROOT` to the current macOS SDK path before running:

```bash
export SDKROOT=$(xcrun --sdk macosx --show-sdk-path)
```
