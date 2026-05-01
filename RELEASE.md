# Release Checklist

Use this checklist before tagging a new workspace release.

---

## 1. Documentation review

- Review root `README.md`, each crate's `README.md`, and `documentation/` for API drift.
- Update `documentation/migration_guide.md` for any breaking changes.

---

## 2. Code quality

```bash
cargo fmt --check
cargo clippy --workspace
cargo test --workspace
```

> **Do not use `--all-features` here.**  The `c-server` feature in `mbus-ffi` requires
> `MBUS_SERVER_APP_CONFIG` to be set at build time; activating it without that env var
> causes a deliberate build-script panic.  The FFI feature matrix and C demos are
> validated separately in steps 4–6 below.

---

## 3. FFI — header checks

Verify the generated headers match the current Rust API:

```bash
cargo run -p xtask -- check-header          # modbus_rs_client.h + feature-gated variant
cargo run -p xtask -- check-server-gen      # mbus_server_app.h matches the example YAML
```

Regenerate if needed:

```bash
cargo run -p xtask -- gen-header
cargo run -p xtask -- gen-server-app \
  --config mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml \
  --emit-c-header target/mbus-ffi/include/mbus_server_app.h
```

---

## 4. FFI — feature matrix

Ensure every feature combination compiles without errors or unexpected warnings:

```bash
cargo run -p xtask -- check-feature-matrix
```

Spot-check the key `mbus-ffi` combinations manually:

```bash
cargo check -p mbus-ffi                              # no features
cargo check -p mbus-ffi --features c,full            # client only
cargo check -p mbus-ffi --features c-server,full     # requires MBUS_SERVER_APP_CONFIG — expected build-script panic
MBUS_SERVER_APP_CONFIG=mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml \
  cargo check -p mbus-ffi --features c,c-server,full # client + server
```

> `--features c-server` without `MBUS_SERVER_APP_CONFIG` is expected to panic with a clear
> error — that is correct behavior, not a bug.

---

## 5. FFI — C demos

Build and test all three C demo targets:

```bash
# Client smoke test (PTY loopback, no hardware)
cargo run -p xtask -- build-c-demo c_client_demo

# Hand-written server demo (in-process self-test)
cargo run -p xtask -- build-c-demo c_server_demo

# YAML-driven server demo (static link, CTest self-test)
cargo run -p xtask -- build-c-demo c_server_demo_yaml --static
```

All three must exit with `CTest: all tests passed` or equivalent success output.

Additionally verify the standalone native C binding-layer test as documented in
`mbus-ffi/README.md` (manual compile + run step).

---

## 6. FFI — Rust unit tests

```bash
cargo test -p mbus-ffi
```

---

## 7. Packaging review

- Confirm workspace version is bumped consistently across all `Cargo.toml` files.
- Confirm `mbus-codegen` is listed as `publish = false` (it is an internal workspace crate).
- Confirm crate `README.md` links, feature flag tables, and example commands are accurate.
- Confirm examples match the current connection lifecycle.
- Confirm no machine-local configuration is required for published crate artifacts.

---

## 8. Full release gate (automated)

Run the single xtask command that exercises all of the above in sequence:

```bash
cargo run -p xtask -- check-release
```

This runs: `check-header` → `check-server-gen` → `build-c-demo c_client_demo` →
`build-c-demo c_server_demo` → `check-feature-matrix`.

> Note: `c_server_demo_yaml` is not in the automated gate because it requires
> `MBUS_SERVER_APP_CONFIG` to be set. Run it manually (step 5 above) before tagging.

---

## 9. Tag and publish

- Tag the release: `git tag v<version>`
- Publish crates in dependency order:
  `mbus-core` → `mbus-network` → `mbus-serial` → `mbus-client` → `mbus-server` →
  `mbus-async` → `mbus-macros` → `mbus-ffi` → `modbus-rs`
  (`mbus-codegen` is `publish = false` — skip it)
- Attach release notes summarising breaking changes, migration steps, and validation status.

### Go module tag

The Go bindings live at `mbus-ffi/go/` and are published as a sub-module:
`github.com/Raghava-Ch/modbus-rs/mbus-ffi/go`. Tag it alongside the workspace
release using the path-prefixed scheme that Go modules require:

```sh
git tag mbus-ffi/go/v<version>
git push origin mbus-ffi/go/v<version>
```

This makes `go get github.com/Raghava-Ch/modbus-rs/mbus-ffi/go@v<version>`
resolve to that commit.


