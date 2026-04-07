# Contributing

Thanks for contributing to `modbus-rs`.

## Licensing

This project is dual-licensed (GPLv3 + Commercial).

By submitting a contribution, you agree that:

- Your contribution is licensed under GPL v3
- You grant the project owner the right to relicense your contribution may be used in a commercial version of the project, which may be licensed under different terms (e.g., a commercial license).

If you prefer not to proceed under these terms, we respect your decision and welcome other forms of contribution such as bug reports, feature requests, and feedback.

Issues and discussions are always welcome.

## Scope

This workspace contains multiple crates for the Modbus core, sync client, async facade, transport implementations, examples, integration tests, and FFI bindings. Keep changes focused to the smallest set of crates necessary.

## Development Setup

- Rust toolchain with Cargo
- On macOS, ensure your local toolchain and SDK environment are configured correctly for native builds
- For FFI smoke validation, you also need a C compiler and CMake available in `PATH`

## Recommended Workflow

1. Make the smallest change that fixes the root cause.
2. Keep public API changes synchronized with examples and docs.
3. Avoid unrelated refactors in the same change.
4. Prefer crate-local tests when iterating, then finish with workspace validation.

## Validation

Run these from the workspace root before opening a PR:

```bash
cargo fmt --check
cargo clippy --workspace
cargo test --workspace
```

For example and FFI-sensitive changes, also run:

```bash
cargo check -p modbus-rs --examples --all-features
cargo test -p mbus-ffi
cargo run -p xtask -- build-c-smoke
```

## Documentation Expectations

- Update the relevant README and `documentation/` pages when changing public APIs or workflows.
- Keep sync examples aligned with the explicit `connect()` lifecycle.
- Keep async examples aligned with the explicit `connect().await?` lifecycle.
- Keep `mbus-ffi/README.md` aligned with the actual native C test commands.

## Testing Guidance

- Prefer targeted crate tests during development, for example:

```bash
cargo test -p mbus-client --lib
cargo test -p mbus-async
cargo test -p mbus-ffi
```

- Use full workspace tests before merging.
- If you change example code or top-level docs, verify the examples still compile.

## Pull Requests

- Describe the user-visible change and the affected crates.
- Call out any breaking API behavior explicitly.
- Include the validation commands you ran.
- Mention documentation updates in the PR summary when applicable.

## Release-related Changes

If your change affects public behavior, examples, feature flags, or packaging, update:

- `CHANGELOG.md`
- any impacted crate README files
- `documentation/` pages

For release execution itself, use `RELEASE.md`.
