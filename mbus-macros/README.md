# mbus-macros

`mbus-macros` provides the procedural macros used by `mbus-server` and `mbus-async`
to build compile-time-safe Modbus app mappings.

## Available Macros

Derive macros:

- `CoilsModel`
- `DiscreteInputsModel`
- `HoldingRegistersModel`
- `InputRegistersModel`

Attribute macros:

- `modbus_app`
- `async_modbus_app`

`fifo` and `file_record` are intentionally not listed here as standalone macros.
They are group selectors inside `#[modbus_app(...)]` / `#[async_modbus_app(...)]`,
for example `#[modbus_app(fifo(history), file_record(files))]`.

## What These Macros Generate

- Address-mapped model implementations for coil/register/discrete-input maps.
- Compile-time validation for duplicate addresses and invalid field attributes.
- `modbus_app` routing implementations for selected map groups.
- Optional write-hook wiring for writable groups using:
	- `on_write_<addr> = handler`
	- `on_batch_write = handler`

## modbus_app Group Support

Supported groups inside `modbus_app(...)`:

- `holding_registers(...)` — FC03/FC06/FC10/FC16/FC17; writable, supports `on_write_N` and `on_batch_write` hooks
- `input_registers(...)` — FC04; read-only
- `coils(...)` — FC01/FC05/FC0F; writable, supports `on_write_N` and `on_batch_write` hooks
- `discrete_inputs(...)` — FC02; read-only
- `fifo(...)` — FC18 Read FIFO Queue; each listed field must implement `FifoQueue`
- `file_record(...)` — FC14/FC15 Read/Write File Record; each listed field must implement `FileRecord`

When `fifo(...)` is provided, the macro generates a `ServerFifoHandler` impl that
dispatches to the matching field by `FifoQueue::POINTER_ADDRESS`.

When `file_record(...)` is provided, the macro generates a `ServerFileRecordHandler`
impl that dispatches to the matching field by `FileRecord::FILE_NUMBER`.

If neither `fifo` nor `file_record` is listed, both handlers are still auto-implemented
with empty bodies (returning "function not supported").

## Test Coverage

Macro behavior is validated with `trybuild` UI tests in `mbus-macros/tests/ui`,
including positive cases and compile-fail diagnostics for unsupported/invalid forms.
