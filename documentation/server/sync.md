# Sync Server Applications

Guide to building poll-driven server applications against the current `mbus-server` runtime.

---

## Runtime Shape

A synchronous server has four pieces:

1. A transport such as `StdTcpTransport`, `StdRtuTransport`, or `StdAsciiTransport`
2. An application that implements the split server traits directly or via `#[modbus_app]`
3. A `ModbusConfig`
4. A `ServerServices` instance that owns the transport and app callback surface

The current lifecycle is:

```rust
let mut server = ServerServices::new(
    transport,
    app,
    config,
    unit_id,
    resilience,
);
server.connect()?;
loop {
    server.poll();
}
```

There is no `bind()` step on `ServerServices`.

---

## Data Models

### Recommended: derive-backed maps

The derive macros generate compact range-checked maps that `#[modbus_app]` can route automatically:

```rust
use modbus_rs::{
    modbus_app, CoilsModel, DiscreteInputsModel, HoldingRegistersModel,
    InputRegistersModel,
};

#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    run_enable: bool,
    #[coil(addr = 1)]
    alarm_reset: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Setpoints {
    #[reg(addr = 0)]
    target_speed: u16,
    #[reg(addr = 1, scale = 10)]
    target_temp_tenths: u16,
}

#[derive(Default, InputRegistersModel)]
struct Sensors {
    #[reg(addr = 0)]
    actual_speed: u16,
}

#[derive(Default, DiscreteInputsModel)]
struct Status {
    #[discrete_input(addr = 0)]
    motor_running: bool,
}

#[derive(Default)]
#[modbus_app(
    coils(outputs),
    holding_registers(setpoints),
    input_registers(sensors),
    discrete_inputs(status),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
    sensors: Sensors,
    status: Status,
}
```

`#[modbus_app]` also supports non-range routing groups:

- `fifo(...)` routes FC18 by `FifoQueue::POINTER_ADDRESS`
- `file_record(...)` routes FC14 and FC15 by `FileRecord::FILE_NUMBER`

### Manual handlers

If you need custom storage or behavior, implement the split traits directly. Read callbacks now write into an output buffer and return the byte count instead of returning wrapper model types.

```rust
impl modbus_rs::ServerCoilHandler for MyApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: modbus_rs::UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, modbus_rs::MbusError> {
        let start = address as usize;
        let end = start + quantity as usize;
        if end > self.coils.len() {
            return Err(modbus_rs::MbusError::InvalidAddress);
        }

        let byte_count = (quantity as usize).div_ceil(8);
        if out.len() < byte_count {
            return Err(modbus_rs::MbusError::BufferTooSmall);
        }

        out[..byte_count].fill(0);
        for (index, value) in self.coils[start..end].iter().enumerate() {
            if *value {
                out[index / 8] |= 1u8 << (index % 8);
            }
        }

        Ok(byte_count as u8)
    }

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        unit_id_or_slave_addr: modbus_rs::UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), modbus_rs::MbusError> {
        let slot = address as usize;
        if slot >= self.coils.len() {
            return Err(modbus_rs::MbusError::InvalidAddress);
        }

        if unit_id_or_slave_addr.is_broadcast() {
            // Apply any broadcast-specific side effects here.
        }

        self.coils[slot] = value;
        Ok(())
    }
}
```

`ServerExceptionHandler` defaults to a no-op, so implement it only if you want exception visibility.

---

## Transport Configuration

### TCP

```rust
use modbus_rs::{ModbusConfig, ModbusTcpConfig, StdTcpTransport};

let transport = StdTcpTransport::new();
let config = ModbusConfig::Tcp(ModbusTcpConfig::new("0.0.0.0", 5502)?);
```

### Serial RTU

```rust
use modbus_rs::{
    BaudRate, DataBits, ModbusConfig, ModbusSerialConfig, Parity, SerialMode,
    StdRtuTransport,
};

let transport = StdRtuTransport::new();
let config = ModbusConfig::Serial(ModbusSerialConfig {
    port_path: "/dev/ttyUSB0".try_into()?,
    mode: SerialMode::Rtu,
    baud_rate: BaudRate::Baud19200,
    data_bits: DataBits::Eight,
    stop_bits: 1,
    parity: Parity::None,
    response_timeout_ms: 100,
    retry_attempts: 1,
    retry_backoff_strategy: modbus_rs::BackoffStrategy::Immediate,
    retry_jitter_strategy: modbus_rs::JitterStrategy::None,
    retry_random_fn: None,
});
```

### Serial ASCII

```rust
use modbus_rs::{ModbusConfig, ModbusSerialConfig, SerialMode, StdAsciiTransport};

let transport = StdAsciiTransport::new();
let config = ModbusConfig::Serial(ModbusSerialConfig {
    mode: SerialMode::Ascii,
    ..Default::default()
});
```

---

## Resilience And Queue Depth

`ServerServices::new(...)` uses the default queue depth of `8`.

If you want a different depth, instantiate the const generic explicitly and call `with_queue_depth(...)`:

```rust
let mut server: modbus_rs::ServerServices<_, _, 16> =
    modbus_rs::ServerServices::with_queue_depth(
        transport,
        app,
        config,
        unit_id,
        resilience,
    );
```

Timeouts, retry cadence, overflow behavior, broadcast writes, and priority dispatch all live under `ResilienceConfig`.

---

## Shared State

`ServerServices` owns the app object. If your application state must also be updated from elsewhere, use `ForwardingApp` with a `ModbusAppAccess` implementation.

```rust
use modbus_rs::{ForwardingApp, ModbusAppAccess};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct SharedApp {
    inner: Arc<Mutex<RealApp>>,
}

impl ModbusAppAccess for SharedApp {
    type App = RealApp;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        let mut guard = self.inner.lock().expect("poisoned app mutex");
        f(&mut guard)
    }
}

let app = ForwardingApp::new(shared.clone());
```

This pattern is what the shared-state TCP example uses.

---

## Addressing And Broadcasts

`ServerServices` is constructed with a single `UnitIdOrSlaveAddr`. Frames for other unit ids are filtered before your callback runs.

Broadcast behavior is separate:

- serial broadcast writes use address `0`
- they are accepted only when `enable_broadcast_writes = true`
- callbacks observe them via `unit_id_or_slave_addr.is_broadcast()`
- no response is sent

TCP unit id `0` is not treated as a writable broadcast.

---

## Error Handling

Return `Err(MbusError::...)` from a callback to emit an exception response.

Typical cases are:

- `MbusError::InvalidAddress` for windows outside your model
- `MbusError::InvalidQuantity` for zero or oversized reads and writes
- `MbusError::InvalidValue`, `InvalidByteCount`, or related validation errors for malformed payloads

To observe emitted exceptions, override the callback on `ServerExceptionHandler`:

```rust
use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;

fn on_exception(
    &mut self,
    txn_id: u16,
    unit_id_or_slave_addr: modbus_rs::UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    exception_code: ExceptionCode,
    error: modbus_rs::MbusError,
) {
    eprintln!(
        "txn={} uid={} fc={:?} exception={:?} error={:?}",
        txn_id,
        unit_id_or_slave_addr.get(),
        function_code,
        exception_code,
        error,
    );
}
```

---

## Poll Loop

The server remains entirely poll-driven. `poll()` performs receive, parse, dispatch, response send, and retry-queue work.

```rust
loop {
    server.poll();
    std::thread::sleep(std::time::Duration::from_millis(1));
}
```

If you need to update application state alongside polling, either:

- do it through shared state with `ForwardingApp`
- update your app before moving it into `ServerServices`
- keep the poll loop thread responsible for both state refresh and `poll()` calls

---

## See Also

- [Quick Start](quick_start.md)
- [Architecture](architecture.md)
- [Policies](policies.md)
- [Macros](macros.md)
- [Write Hooks](write_hooks.md)
- [Async Server Applications](async.md)
