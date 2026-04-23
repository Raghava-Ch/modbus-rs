# Server Function Codes Reference

Current Modbus function-code coverage for `mbus-server`.

---

## Function Code Table

| FC | Name | Feature Flag | Callback | Direction |
|----|------|--------------|----------|-----------|
| `0x01` | Read Coils | `coils` | `read_coils_request` | Read |
| `0x02` | Read Discrete Inputs | `discrete-inputs` | `read_discrete_inputs_request` | Read |
| `0x03` | Read Holding Registers | `holding-registers` | `read_multiple_holding_registers_request` | Read |
| `0x04` | Read Input Registers | `input-registers` | `read_multiple_input_registers_request` | Read |
| `0x05` | Write Single Coil | `coils` | `write_single_coil_request` | Write |
| `0x06` | Write Single Register | `holding-registers` | `write_single_register_request` | Write |
| `0x07` | Read Exception Status | `diagnostics` | `read_exception_status_request` | Read |
| `0x08` | Diagnostics | `diagnostics` | `diagnostics_request` | R/W |
| `0x0B` | Get Comm Event Counter | `diagnostics` | `get_comm_event_counter_request` | Read |
| `0x0C` | Get Comm Event Log | `diagnostics` | `get_comm_event_log_request` | Read |
| `0x0F` | Write Multiple Coils | `coils` | `write_multiple_coils_request` | Write |
| `0x10` | Write Multiple Registers | `holding-registers` | `write_multiple_registers_request` | Write |
| `0x11` | Report Server ID | `diagnostics` | `report_server_id_request` | Read |
| `0x14` | Read File Record | `file-record` | `read_file_record_request` | Read |
| `0x15` | Write File Record | `file-record` | `write_file_record_request` | Write |
| `0x16` | Mask Write Register | `holding-registers` | `mask_write_register_request` | Write |
| `0x17` | Read/Write Multiple Registers | `holding-registers` | `read_write_multiple_registers_request` | R/W |
| `0x18` | Read FIFO Queue | `fifo` | `read_fifo_queue_request` | Read |
| `0x2B / 0x0E` | Read Device Identification | `diagnostics` | `read_device_identification_request` | Read |

`mbus-server` receives FC `0x2B` as Encapsulated Interface Transport and currently
routes only MEI `0x0E` to the application callback. Other MEI types are rejected by the stack.

---

## Read Callback Shape

Most read-style callbacks write encoded payload bytes into an output buffer and return the
number of bytes written:

```rust
fn read_...(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
    out: &mut [u8],
) -> Result<u8, MbusError>;
```

This applies to:

- FC01 `read_coils_request`
- FC02 `read_discrete_inputs_request`
- FC03 `read_multiple_holding_registers_request`
- FC04 `read_multiple_input_registers_request`
- FC18 `read_fifo_queue_request`
- FC14 `read_file_record_request`
- FC17 `read_write_multiple_registers_request` for the read portion

For coil and discrete-input reads, `out` contains packed bits. For register-style reads,
`out` contains big-endian `u16` words.

---

## Write Callback Shape

Single-write callbacks mutate application state directly and return `Result<(), MbusError>`:

```rust
fn write_single_coil_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
) -> Result<(), MbusError>;

fn write_single_register_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: u16,
) -> Result<(), MbusError>;
```

Multi-write callbacks receive already-decoded payloads:

```rust
fn write_multiple_coils_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    starting_address: u16,
    quantity: u16,
    values: &[u8],
) -> Result<(), MbusError>;

fn write_multiple_registers_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    starting_address: u16,
    values: &[u16],
) -> Result<(), MbusError>;
```

Additional write-capable callbacks:

```rust
fn mask_write_register_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> Result<(), MbusError>;

fn write_file_record_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    file_number: u16,
    record_number: u16,
    record_length: u16,
    record_data: &[u16],
) -> Result<(), MbusError>;
```

---

## Diagnostics Callback Shape

`ServerDiagnosticsHandler` uses a few specialized return shapes:

```rust
fn read_exception_status_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
) -> Result<u8, MbusError>;

fn diagnostics_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    sub_function: DiagnosticSubFunction,
    data: u16,
) -> Result<u16, MbusError>;

fn get_comm_event_counter_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
) -> Result<(u16, u16), MbusError>;

fn get_comm_event_log_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    out_events: &mut [u8],
) -> Result<(u16, u16, u16, u8), MbusError>;

fn report_server_id_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    out_server_id: &mut [u8],
) -> Result<(u8, u8), MbusError>;

fn read_device_identification_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    read_device_id_code: u8,
    start_object_id: u8,
    out: &mut [u8],
) -> Result<(u8, u8, bool, u8), MbusError>;
```

With `diagnostics-stats`, the stack auto-handles the FC08 counter-oriented sub-functions:

| Sub-function | Name |
|-------------|------|
| `0x000A` | Clear Counters |
| `0x000B` | Bus Message Count |
| `0x000C` | Bus Comm Error Count |
| `0x000D` | Bus Exception Error Count |
| `0x000E` | Server Message Count |
| `0x000F` | Server No-Response Count |
| `0x0010` | Server NAK Count |
| `0x0011` | Server Busy Count |
| `0x0012` | Bus Character Overrun Count |
| `0x0014` | Clear Overrun Counter/Flag |

Other FC08 sub-functions continue to flow through `diagnostics_request`.

---

## Broadcast Writes

Serial broadcast writes are supported only when
`ResilienceConfig::enable_broadcast_writes = true`.

- Broadcast address is `0`
- Only write-oriented requests are processed
- No response is sent
- TCP unit id `0` is still discarded

Your callback can detect the case with `uid.is_broadcast()`.

---

## Exceptions

Returning `Err(MbusError::...)` causes the server to build a Modbus exception response and
then invoke `ServerExceptionHandler::on_exception(...)`.

```rust
fn on_exception(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    exception_code: ExceptionCode,
    error: MbusError,
)
```

Common outcomes are:

- unsupported or disabled handlers map to `IllegalFunction`
- invalid address windows map to `IllegalDataAddress`
- invalid quantities, values, or malformed sub-function payloads map to `IllegalDataValue`
- internal queue-pressure or transport-side failures surface as server-side exceptions

If you need exact per-service behavior, use the service modules in `mbus-server/src/services` as
the source of truth.

---

## See Also

- [Building Applications](building_applications.md)
- [Policies](policies.md)
- [Macros](macros.md)
