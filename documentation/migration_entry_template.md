# Migration Entry Template

Use this template when adding a new version section to
`documentation/migration_guide.md`.

```markdown
### vX.Y.Z (from vA.B.C)

#### Breaking Changes Summary

| Area | Old | New | Action |
|---|---|---|---|
| ... | ... | ... | ... |

#### Rust Migration

- Describe required API changes.
- Include before/after snippets where useful.

#### C/FFI Migration

- Describe ABI/header/callback changes.
- Include before/after snippets where useful.

#### Validation Checklist

```bash
cargo check --workspace
cargo test --workspace
cargo run -p xtask -- check-header
cargo run -p xtask -- build-c-smoke
```

#### Notes

- Mention deprecation windows, removed symbols, and known caveats.
```
