# xtask

Workspace maintenance and validation commands for the `modbus-rs` repository.

## Run Location

Run all commands from the repository root:

```bash
cargo run -p xtask -- <command>
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

## Commands

### `gen-header`
Regenerates FFI headers.

What it does:
- Runs `scripts/check_header.sh --fix`
- Regenerates `mbus-ffi/include/mbus_ffi_feature_gated.h`

Use when:
- Header definitions changed and you want to update generated output.

### `check-header`
Verifies FFI headers are up to date.

What it does:
- Runs `scripts/check_header.sh`
- Verifies `mbus-ffi/include/mbus_ffi_feature_gated.h` matches generated content

Use when:
- CI/local validation before commit.

### `gen-feature-header`
Regenerates only the feature-gated header.

Output:
- `mbus-ffi/include/mbus_ffi_feature_gated.h`

### `check-feature-header`
Checks only the feature-gated header for drift.

Fails with instruction to run:

```bash
cargo run -p xtask -- gen-feature-header
```

### `build-c-smoke`
Builds and executes the C smoke test project.

What it does:
- `cargo build -p mbus-ffi --features c,full`
- Configures CMake in `mbus-ffi/examples/c_smoke_cmake`
- Builds the C smoke binary
- Runs CTest

Use when:
- Verifying C FFI integration and C smoke test behavior.

### `check-feature-matrix`
Runs feature and package checks across the workspace.

What it does:
- `cargo check --features full`
- `cargo check --workspace --all-features`
- `cargo test -p mbus-client --doc --all-features`
- `cargo test -p mbus-server --all-features`
- `cargo test -p mbus-async --all-features`

Use when:
- Verifying feature compatibility and core package health.

### `validate-docs`
Validates code examples found in Markdown docs.

What it does:
- Scans all `.md` files in the repo
- Extracts and checks `cargo ... --example ...` commands from bash/shell blocks
- Compile-checks Rust fenced blocks (according to tags/markers)
- Cross-references documented examples with `modbus-rs/Cargo.toml` `[[example]]` entries
- Prints colorized pass/fail/warning output in interactive terminals

Markers supported in Markdown:
- `<!-- validate: run -->`
- `<!-- validate: skip -->`
- `<!-- validate: compile -->`

See root documentation for details:
- `VALIDATE_DOCS.md`

Disable colors if needed:

```bash
NO_COLOR=1 cargo run -p xtask -- validate-docs
```

### `check-release`
Runs the release gate sequence.

What it does:
- `check-header`
- `build-c-smoke`
- `check-feature-matrix`

Use when:
- Performing final release-style verification.

## Typical Workflows

Update and verify headers:

```bash
cargo run -p xtask -- gen-header
cargo run -p xtask -- check-header
```

Validate docs/examples:

```bash
cargo run -p xtask -- validate-docs
```

Run everything needed for release checks:

```bash
cargo run -p xtask -- check-release
```

## Troubleshooting

If `cargo` fails on macOS with linker error like:

`ld: library 'System' not found`

set `SDKROOT` in your shell environment to the current macOS SDK path before running commands.
