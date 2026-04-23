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

### `check-server-gen`

Verify that the generated C header (`mbus_server_app.h`) matches the current YAML config (used in CI):

```bash
cargo run -p xtask -- check-server-gen
```

---

## FFI Header Commands

### `gen-header`
Regenerate `modbus_rs_client.h` and `modbus_rs_client_feature_gated.h`:

```bash
cargo run -p xtask -- gen-header
```

### `check-header`
Verify the headers are up to date (CI):

```bash
cargo run -p xtask -- check-header
```

### `gen-feature-header` / `check-feature-header`
Regenerate or verify only `modbus_rs_client_feature_gated.h`.

---

## Validation Commands

### `check-feature-matrix`
Run feature and package checks across the workspace:

```bash
cargo run -p xtask -- check-feature-matrix
```

### `validate-docs`
Validate code examples in Markdown docs.  Disable colors with `NO_COLOR=1`.

### `check-release`
Run the full release gate: `check-header` → `check-server-gen` → `build-c-smoke` → `build-c-demo --demo c_server_demo` → `check-feature-matrix`.

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
