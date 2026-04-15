# Server Function Codes Reference

This document lists every Modbus function code supported by `mbus-server`,
the feature flag that gates it, the `ModbusAppHandler` callback name, and a
brief description of the request and response payloads.

## Reference Table

| FC     | Name                          | Feature flag        | Callback                                        | Notes                         |
|--------|-------------------------------|---------------------|-------------------------------------------------|-------------------------------|
| `0x01` | Read Coils                    | `coils`             | `read_coils_request`                            |                               |
| `0x02` | Read Discrete Inputs          | `discrete-inputs`   | `read_discrete_inputs_request`                  |                               |
| `0x03` | Read Holding Registers        | `holding-registers` | `read_multiple_holding_registers_request`       |                               |
| `0x04` | Read Input Registers          | `input-registers`   | `read_input_registers_request`                  |                               |
| `0x05` | Write Single Coil             | `coils`             | `write_single_coil_request`                     | broadcast-capable             |
| `0x06` | Write Single Register         | `holding-registers` | `write_single_register_request`                 | broadcast-capable             |
| `0x07` | Read Exception Status         | `diagnostics`       | `read_exception_status_request`                 | returns 8-bit mask            |
| `0x08` | Diagnostics                   | `diagnostics`       | `diagnostics_request`                           | sub-function dispatched       |
| `0x0B` | Get Comm Event Counter        | `diagnostics`       | `get_comm_event_counter_request`                |                               |
| `0x0C` | Get Comm Event Log            | `diagnostics`       | `get_comm_event_log_request`                    |                               |
| `0x0F` | Write Multiple Coils          | `coils`             | `write_multiple_coils_request`                  | broadcast-capable             |
| `0x10` | Write Multiple Registers      | `holding-registers` | `write_multiple_registers_request`              | broadcast-capable             |
| `0x11` | Report Server ID              | `diagnostics`       | `report_server_id_request`                      | variable-length response      |
| `0x14` | Read File Record              | `file-record`       | `read_file_record_request`                      | multiple sub-requests         |
| `0x15` | Write File Record             | `file-record`       | `write_file_record_request`                     | broadcast-capable             |
| `0x16` | Mask Write Register           | `holding-registers` | `mask_write_register_request`                   | AND-mask + OR-mask            |
| `0x17` | Read/Write Multiple Registers | `holding-registers` | `read_write_multiple_registers_request`         | write first, then read        |
| `0x18` | Read FIFO Queue               | `fifo`              | `read_fifo_queue_request`                       | up to 31 u16 values           |
| `0x2B` | Read Device Identification    | `diagnostics`       | `read_device_identification_request`            | MEI type `0x0E`               |

## Broadcast Writes (Serial only)

FC05, FC06, FC0F, FC15, and FC10 may arrive as broadcast frames (slave address `0`)
when `ResilienceConfig::enable_broadcast_writes` is `true`.

- Broadcast requests are delivered to the callback with
  `unit_id_or_slave_addr.is_broadcast() == true`.
- No response is ever sent for a broadcast write.
- TCP broadcast frames (unit id `0` over TCP) are silently discarded even when
  broadcast writes are enabled.

## Exception Handling

If any callback returns `Err(MbusError::*)`, the server maps the error to the
appropriate Modbus exception code and sends an exception response.

The `on_exception` callback is invoked after every exception response:

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

Common error → exception code mappings:

| `MbusError` variant            | `ExceptionCode`            |
|-------------------------------|----------------------------|
| `InvalidAddress`               | `IllegalDataAddress`       |
| `InvalidData`                  | `IllegalDataValue`         |
| `TooManyRequests`              | `ServerDeviceFailure`      |
| `NotEnabled`                   | `IllegalFunction`          |
| `ReservedSubFunction(_)`       | `IllegalFunction`          |
| any other error                | `ServerDeviceFailure`      |

## `diagnostics-stats` Sub-function Handling

When `diagnostics-stats` is enabled, FC08 sub-functions `0x000A`–`0x0014` are
handled automatically by the stack.  Your `diagnostics_request` callback is only
invoked for sub-functions the stack does not handle:

| Sub-function | Name                          | Handled by stack |
|-------------|-------------------------------|-----------------|
| `0x0000`    | Return Query Data             | ❌ (app callback) |
| `0x000A`    | Clear Counters                | ✅               |
| `0x000B`    | Bus Message Count             | ✅               |
| `0x000C`    | Bus Comm Error Count          | ✅               |
| `0x000D`    | Bus Exception Error Count     | ✅               |
| `0x000E`    | Server Message Count          | ✅               |
| `0x000F`    | Server No-Response Count      | ✅               |
| `0x0010`    | Server NAK Count              | ✅               |
| `0x0011`    | Server Busy Count             | ✅               |
| `0x0012`    | Bus Character Overrun Count   | ✅               |
| `0x0014`    | Clear Overrun Counter/Flag    | ✅               |

## Read Device Identification (FC 0x2B / MEI 0x0E)

The callback receives an output buffer and a `next_object_id: u8` cursor so the
response can be built iteratively across multiple requests if conformity level
`0x82` (stream) or `0x83` (individual) is used.

Required (conformity class `0x01`):
- `0x00` — Vendor Name
- `0x01` — Product Code
- `0x02` — Major Minor Revision

Optional (conformity class `0x02`):
- `0x03` — Vendor URL
- `0x04` — Product Name
- `0x05` — Model Name
- `0x06` — User Application Name

Private (conformity class `0x03`):
- `0x80`–`0xFF` — user-defined

The callback writes each object using the `DeviceIdWriter` helper:
```rust
writer.write_object(0x00, b"Vendor Name")?;
```
