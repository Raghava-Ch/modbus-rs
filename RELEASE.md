# Release Checklist

Use this checklist before tagging a new workspace release.

## Pre-release validation

- Update `CHANGELOG.md` with user-visible changes and breaking behavior.
- Review root `README.md`, crate READMEs, and `documentation/` for API drift.
- Update `documentation/migration_guide.md` for any new breaking changes.
- Run `cargo fmt --check`.
- Run `cargo clippy --workspace`.
- Run `cargo test --workspace`.

## FFI validation

- Run `cargo test -p mbus-ffi`.
- Run `cargo run -p xtask -- build-c-smoke`.
- Verify the standalone native C binding-layer test command documented in `mbus-ffi/README.md` still passes.

## Packaging review

- Confirm crate versions, README links, and feature flags are consistent.
- Confirm examples match the current connection lifecycle.
- Confirm no machine-local-only configuration is required for published artifacts.

## Release

- Tag the release with the workspace version.
- Publish crates in dependency order as needed.
- Attach release notes summarizing breaking changes, migration steps, and validation status.
