# Server Function Codes Reference

Complete reference for all Modbus function codes supported by `mbus-server`.

---

## Function Code Table

| FC | Name | Feature Flag | Callback | Direction |
|----|------|--------------|----------|-----------|
| `0x01` | Read Coils | `coils` | `read_coils_request` | Read |
| `0x02` | Read Discrete Inputs | `discrete-inputs` | `read_discrete_inputs_request` | Read |
| `0x03` | Read Holding Registers | `holding-registers` | `read_multiple_holding_registers_request` | Read |
| `0x04` | Read Input Registers | `input-registers` | `read_input_registers_request` | Read |
| `0x05` | Write Single Coil | `coils` | `write_single_coil_request` | Write* |
| `0x06` | Write Single Register | `holding-registers` | `write_single_register_request` | Write* |
| `0x07` | Read Exception Status | `diagnostics` | `read_exception_status_request` | Read |
| `0x08` | Diagnostics | `diagnostics` | `diagnostics_request` | R/W |
| `0x0B` | Get Comm Event Counter | `diagnostics` | `get_comm_event_counter_request` | Read |
| `0x0C` | Get Comm Event Log | `diagnostics` | `get_comm_event_log_request` | Read |
| `0x0F` | Write Multiple Coils | `coils` | `write_multiple_coils_request` | Write* |
| `0x10` | Write Multiple Registers | `holding-registers` | `write_multiple_registers_request` | Write* |
| `0x11` | Report Server ID | `diagnostics` | `report_server_id_request` | Read |
| `0x14` | Read File Record | `file-record` | `read_file_record_request` | Read |
| `0x15` | Write File Record | `file-record` | `write_file_record_request` | Write* |
| `0x16` | Mask Write Register | `holding-registers` | `mask_write_register_request` | Write |
| `0x17` | Read/Write Multiple Registers | `holding-registers` | `read_write_multiple_registers_request` | R/W |
| `0x18` | Read FIFO Queue | `fifo` | `read_fifo_queue_request` | Read |
| `0x2B` | Read Device Identification | `diagnostics` | `read_device_identification_request` | Read |

\* Broadcast-capable (Serial only)

---

## Callback Signatures

### FC01: Read Coils

```rust
fn read_coils_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    quantity: u16,
) -> Result<Coils, MbusError>;
```

### FC02: Read Discrete Inputs

```rust
fn read_discrete_inputs_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    quantity: u16,
) -> Result<DiscreteInputs, MbusError>;
```

### FC03: Read Holding Registers

```rust
fn read_multiple_holding_registers_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    quantity: u16,
) -> Result<Registers, MbusError>;
```

### FC04: Read Input Registers

```rust
fn read_input_registers_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    quantity: u16,
) -> Result<Registers, MbusError>;
```

### FC05: Write Single Coil

```rust
fn write_single_coil_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
) -> Result<(), MbusError>;
```

### FC06: Write Single Register

```rust
fn write_single_register_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: u16,
) -> Result<(), MbusError>;
```

### FC0F: Write Multiple Coils

```rust
fn write_multiple_coils_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    coils: &Coils,
) -> Result<(), MbusError>;
```

### FC10: Write Multiple Registers

```rust
fn write_multiple_registers_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    registers: &Registers,
) -> Result<(), MbusError>;
```

### FC16: Mask Write Register

```rust
fn mask_write_register_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> Result<u16, MbusError>;  // Returns resulting value
```

### FC17: Read/Write Multiple Registers

```rust
fn read_write_multiple_registers_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    read_start: u16,
    read_quantity: u16,
    write_start: u16,
    write_registers: &Registers,
) -> Result<Registers, MbusError>;
```

### FC18: Read FIFO Queue

```rust
fn read_fifo_queue_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    pointer_address: u16,
) -> Result<FifoQueue, MbusError>;  // Up to 31 u16 values
```

### FC2B: Read Device Identification

```rust
fn read_device_identification_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    object_id: u8,
    output: &mut DeviceIdentification,
) -> Result<(), MbusError>;
```

---

## Broadcast Writes (Serial Only)

FC05, FC06, FC0F, FC10, FC15 may arrive as broadcast frames when `enable_broadcast_writes = true`.

```rust
fn write_single_coil_request(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
) -> Result<(), MbusError> {
    if uid.is_broadcast() {
        // Slave address 0 - broadcast write
        // Apply the change but no response will be sent
    }
    
    self.coils[address as usize] = value;
    Ok(())
}
```

- Broadcast requests have `uid.is_broadcast() == true`
- No response is ever sent for broadcast writes
- TCP broadcast frames (unit ID `0`) are always discarded

---

## Exception Handling

When a callback returns `Err(MbusError::*)`, the server sends an exception response.

### Error to Exception Mapping

| `MbusError` | `ExceptionCode` |
|-------------|-----------------|
| `InvalidAddress` | `IllegalDataAddress` |
| `InvalidData` | `IllegalDataValue` |
| `TooManyRequests` | `ServerDeviceFailure` |
| `NotEnabled` | `IllegalFunction` |
| `ReservedSubFunction(_)` | `IllegalFunction` |
| `DeviceBusy` | `ServerDeviceBusy` |
| other | `ServerDeviceFailure` |

### Exception Callback

```rust
fn on_exception(
    &mut self,
    txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    exception_code: ExceptionCode,
    error: MbusError,
) {
    eprintln!(
        "Exception: FC{:02X} → {:?} ({})",
        function_code as u8, exception_code, error
    );
}
```

---

## FC08: Diagnostics Sub-functions

### Handled by Stack (with `diagnostics-stats`)

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

### Handled by App Callback

| Sub-function | Name |
|-------------|------|
| `0x0000` | Return Query Data |
| All others | Custom handling |

---

## FC2B: Device Identification

Required objects (conformity class `0x01`):

| Object ID | Name |
|-----------|------|
| `0x00` | Vendor Name |
| `0x01` | Product Code |
| `0x02` | Major Minor Revision |

Optional objects:

| Object ID | Name |
|-----------|------|
| `0x03` | Vendor URL |
| `0x04` | Product Name |
| `0x05` | Model Name |
| `0x06` | User Application Name |
| `0x80`+ | Private/vendor-specific |

---

## See Also

- [Building Applications](building_applications.md)
- [Policies](policies.md)
- [Macros](macros.md)
