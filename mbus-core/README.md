# mbus-core

`mbus-core` is a helper crate for [modbus-rs](https://crates.io/crates/modbus-rs).

It provides shared protocol types, transport abstractions, function-code definitions,
and data models used by higher-level crates such as `mbus-client`, `mbus-network`, and
`mbus-serial`.

## What This Crate Provides

- Core Modbus ADU/PDU data structures.
- Function code enums and protocol constants.
- Transport trait and transport configuration types.
- Shared error model (`MbusError`) used across the workspace.
- Feature-gated Modbus data models (coils, registers, etc.).
- `no_std`-friendly implementation for embedded targets.

## Helper Crate Role in the Workspace

`mbus-core` is intentionally low-level and reusable.

- `mbus-client` builds request/response workflows on top of this crate.
- `mbus-network` and `mbus-serial` implement concrete transport layers that satisfy
	interfaces defined here.
- `modbus-rs` re-exports major crates for easier consumption.

This separation keeps protocol definitions centralized and avoids duplication across
client and transport crates.

## Module Overview

- `data_unit`: ADU/PDU structures and framing helpers.
- `errors`: Core error types used by all crates.
- `function_codes`: Public and user-defined function code definitions.
- `models`: Feature-gated Modbus data models.
- `transport`: Transport traits, config types, and transport-related enums/errors.

## Feature Flags

`mbus-core` supports selective compilation to reduce binary size.

Available feature flags:

- `serial-ascii`
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`

Default behavior:

- `default` enables all features above.

`serial-ascii` affects ADU buffer sizing:

- enabled: `MAX_ADU_FRAME_LEN = 513` (ASCII upper bound)
- disabled: `MAX_ADU_FRAME_LEN = 260` (TCP/RTU upper bound)

This reduces stack usage in non-ASCII builds while preserving full ASCII compatibility
when explicitly enabled.

Example with selective features:

```toml
[dependencies]
mbus-core = { version = "0.6.0", default-features = false, features = ["coils", "registers"] }
```

## no_std

This crate is designed for embedded and constrained environments and is compatible
with `no_std` usage patterns.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](../LICENSE) for details.

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

## Disclaimer

This is an independent Rust implementation of the Modbus specification and is not
affiliated with the Modbus Organization.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)