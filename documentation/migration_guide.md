# Modbus-rs Migration Guide

This is a versioned, append-only migration guide for breaking changes.

When a release introduces a breaking API/ABI change, add a new section under
"Versioned Migrations" rather than rewriting old sections.

## How To Use This Guide

1. Find your current version.
2. Apply each migration section in order until you reach your target version.
3. Run the validation checklist for each section.

## Maintainer Notes

For each future breaking release, append a new section under "Versioned Migrations"
and keep older sections unchanged. Use the entry template at
`documentation/migration_entry_template.md`.

## Versioned Migrations

### v0.6.0 (from v0.5.x)

#### Breaking Changes Summary

| Area | Old | New | Action |
|---|---|---|---|
| Rust transport metadata | `transport.transport_type()` method | `Transport::TRANSPORT_TYPE` associated const | Replace runtime calls with type-level const usage |
| C client ID type | `typedef uint8_t MbusClientId` | `typedef uint16_t MbusClientId` | Update all C/C++ wrappers, structs, and callback signatures |
| C invalid ID sentinel | `MBUS_INVALID_CLIENT_ID = 0xFF` | `MBUS_INVALID_CLIENT_ID = 0xFFFF` | Update comparisons and default initialization |
| FFI serial client pool | single serial pool | split RTU/ASCII pools behind opaque IDs | Treat IDs as opaque and do not assume dense `u8` IDs |
| C lock callbacks | accepted `uint8_t id` in older integrations | header declares `MbusClientId id` | Change callback signatures to `MbusClientId` |

#### Rust Migration

#### `Transport::transport_type()` was removed

The runtime method is no longer part of the trait. Transport kind is now compile-time
through the associated const.

Before:

```rust
fn uses_transport<T: Transport>(transport: &T) {
    let kind = transport.transport_type();
    // ...
}
```

After:

```rust
fn uses_transport<T: Transport>(_transport: &T) {
    let kind = T::TRANSPORT_TYPE;
    // ...
}
```

Notes:

- If a helper only needed transport kind, remove the value parameter entirely.
- For generic impl blocks, replace `self.transport.transport_type()` with
  `TRANSPORT::TRANSPORT_TYPE`.

#### Serial transport specialization

Serial transports are modeled with compile-time RTU/ASCII specialization.
Use aliases for clarity:

- `StdRtuTransport`
- `StdAsciiTransport`

`StdSerialTransport` remains available as the generic type.

#### C/FFI Migration

#### Regenerate and consume current headers

Always use the generated header from this repository version:

- `mbus-ffi/include/mbus_ffi.h`

Recommended validation command from workspace root:

```bash
cargo run -p xtask -- check-header
```

If out of date:

```bash
cargo run -p xtask -- gen-header
```

#### Update `MbusClientId` usage

`MbusClientId` is now `uint16_t`.

Required updates:

- Replace `uint8_t` storage for client IDs with `MbusClientId` or `uint16_t`.
- Replace invalid sentinel checks from `0xFF` to `MBUS_INVALID_CLIENT_ID`.
- Update arrays/maps keyed by ID if they previously assumed 8-bit IDs.

Before:

```c
uint8_t client_id = mbus_tcp_client_new(&cfg, &transport, &callbacks);
if (client_id == 0xFF) { /* invalid */ }
```

After:

```c
MbusClientId client_id = mbus_tcp_client_new(&cfg, &transport, &callbacks);
if (client_id == MBUS_INVALID_CLIENT_ID) { /* invalid */ }
```

#### Update lock callback signatures

If you provide lock hooks, signatures must match the header:

```c
void mbus_client_lock(MbusClientId id);
void mbus_client_unlock(MbusClientId id);
```

If you map IDs into fixed local lock buckets, index using your own strategy.
A common approach is low-byte slot mapping:

```c
uint8_t slot = (uint8_t)(id & 0xFFu);
```

Treat ID layout as opaque protocol between your app and the FFI library.
Do not hard-code pool tags as stable public API.

#### Validation Checklist

From workspace root:

```bash
cargo check --workspace
cargo test -p mbus-ffi
cargo run -p xtask -- check-header
cargo run -p xtask -- build-c-smoke
```

For users with custom native CMake flows, ensure the Rust FFI library is built with the
required features before linking, for example:

```bash
cargo build -p mbus-ffi --features c,full
```

#### FAQ

#### Why was `transport_type()` removed?

To remove runtime transport-kind branching from generic code and make transport-kind
resolution compile-time only.

#### Are `MbusClientId` numeric values stable?

No. IDs are opaque handles. Only equality checks and sentinel checks against
`MBUS_INVALID_CLIENT_ID` are supported.
