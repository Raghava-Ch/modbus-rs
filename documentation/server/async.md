# Async Server Applications

Guide to building async server applications with `tokio` using the `mbus-async` runtime.

---

## Runtime Shape

An async Modbus server has three pieces:

1. A bind address (TCP `host:port` or serial port path)
2. An application that implements `AsyncAppHandler` (typically via `#[async_modbus_app]`)
3. An `AsyncTcpServer`, `AsyncRtuServer`, or `AsyncAsciiServer` that accepts connections

The current lifecycle is:

```rust,ignore
async fn run_server_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let app = MyApp::default();
    
    // Spawn server forever; each connection gets its own task
    AsyncTcpServer::serve("0.0.0.0:502", app, unit).await?;
    
    Ok(())
}
```

There are no `connect()` or `poll()` calls. The server runs forever in a single `await` expression, spawning a new tokio task for each client connection.

---

## Per-Session vs Shared State

### Per-Session: `serve()`

Each client connection receives its own cloned instance of the app:

```rust,ignore
#[derive(Clone)]
#[derive(Default)]
#[async_modbus_app(...)]
struct MyApp {
    // ...
}

async fn run_per_session_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    
    // Client A gets one clone, Client B gets another clone
    // They do NOT see each other's state changes
    AsyncTcpServer::serve("0.0.0.0:502", MyApp::default(), unit).await?;
    
    Ok(())
}
```

Requirement: `MyApp` must implement `Clone`.

### Shared State: `serve_shared()`

All client connections share a single `Arc<Mutex<APP>>` instance:

```rust,ignore
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
#[async_modbus_app(...)]
struct MyApp {
    // ...
}

async fn run_shared_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let shared = Arc::new(Mutex::new(MyApp::default()));
    
    // All clients access the same app through the mutex
    AsyncTcpServer::serve_shared("0.0.0.0:502", shared, unit).await?;
    
    Ok(())
}
```

Requirements: `MyApp` does not need to be `Clone`; mutex handles sharing.

---

## Data Models

Data models in async servers use the same derive macros as sync servers:

```rust
use modbus_rs::{
    async_modbus_app, CoilsModel, DiscreteInputsModel, HoldingRegistersModel,
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
#[async_modbus_app(
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

---

## Async Write Hooks

The `#[async_modbus_app]` macro supports **async** write hook methods. Unlike sync servers, hooks can freely `.await`:

```rust
#[derive(Default)]
#[async_modbus_app(
    coils(outputs, on_write_0 = on_run_changed, on_batch_write = on_outputs_written),
    holding_registers(setpoints, on_write_1 = on_temp_setpoint_write),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
}

impl App {
    /// Fires when coil 0 is written via FC05 (single write).
    /// Runs pre-commit: returning Err(...) rejects the write.
    async fn on_run_changed(
        &mut self,
        _address: u16,
        old_value: bool,
        new_value: bool,
    ) -> Result<(), modbus_rs::MbusError> {
        println!("Run changed: {} -> {}", old_value, new_value);
        // Can freely .await here: database writes, HTTP calls, etc.
        self.start_or_stop_motor(new_value).await?;
        Ok(())
    }

    /// Fires when multiple coils are written via FC15.
    /// Receives the address range and packed bit values.
    async fn on_outputs_written(
        &mut self,
        address: u16,
        quantity: u16,
        packed_bits: &[u8],
    ) -> Result<(), modbus_rs::MbusError> {
        println!("Batch write: addr={} qty={}", address, quantity);
        self.sync_to_plc(packed_bits).await?;
        Ok(())
    }

    /// Fires when register 1 is written via FC16 (multiple) or FC6 (single with notify_via_batch).
    async fn on_temp_setpoint_write(
        &mut self,
        _address: u16,
        old_value: u16,
        new_value: u16,
    ) -> Result<(), modbus_rs::MbusError> {
        if new_value > 500 {
            // Pre-commit rejection: write never applies
            return Err(modbus_rs::MbusError::InvalidValue);
        }
        println!("Temp setpoint: {} -> {}", old_value, new_value);
        self.update_hvac_controller(new_value).await?;
        Ok(())
    }

    async fn start_or_stop_motor(&mut self, enable: bool) -> Result<(), std::io::Error> {
        // Example async operation
        Ok(())
    }

    async fn sync_to_plc(&mut self, bits: &[u8]) -> Result<(), std::io::Error> {
        // Example async operation
        Ok(())
    }

    async fn update_hvac_controller(&mut self, temp: u16) -> Result<(), std::io::Error> {
        // Example async operation
        Ok(())
    }
}
```

Hook parameters are the same as sync servers, but the function itself is `async fn` and can return a `Future`.

---

## Concurrent Connections And Background Tasks

Async servers can spawn background tasks alongside the main server loop:

```rust,ignore
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

async fn run_with_background_tasks() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let shared = Arc::new(Mutex::new(App::default()));
    
    // Spawn a background task to simulate sensor data
    let app_ref = shared.clone();
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            let mut app = app_ref.lock().await;
            // Update sensor readings
            app.sensors.actual_speed = simulate_sensor_read();
        }
    });
    
    // Main server runs forever
    AsyncTcpServer::serve_shared("0.0.0.0:502", shared, unit).await?;
    
    Ok(())
}

fn simulate_sensor_read() -> u16 {
    // Simulate a real sensor
    2500
}
```

Each client connection runs in its own spawned task, completely independent of other clients. The mutex ensures safe access to shared state.

---

## TCP Server Configuration

```rust,ignore
use modbus_rs::UnitIdOrSlaveAddr;
use mbus_async::server::AsyncTcpServer;

async fn run_tcp_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let app = MyApp::default();
    
    // Bind to any interface on port 502
    AsyncTcpServer::serve("0.0.0.0:502", app, unit).await?;
    
    Ok(())
}
```

The address string is any type implementing `ToSocketAddrs`, so you can use:
- `"0.0.0.0:502"` — bind all interfaces
- `"127.0.0.1:5502"` — localhost only
- `"[::]:502"` — IPv6

---

## Serial Server Configuration

### RTU Server

```rust,ignore
use mbus_async::server::AsyncRtuServer;
use modbus_rs::{BaudRate, DataBits, ModbusConfig, ModbusSerialConfig, Parity, SerialMode};

async fn run_rtu_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let config = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: "/dev/ttyUSB0".try_into()?,
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Baud19200,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        ..Default::default()
    });
    let mut server = AsyncRtuServer::new_rtu(&config, unit)?;
    
    server.run(MyApp::default()).await?;
    
    Ok(())
}
```

Note: Async serial servers handle a **single serial connection** (no multi-tasking like TCP).

### ASCII Server

```rust,ignore
use mbus_async::server::AsyncAsciiServer;
use modbus_rs::{BaudRate, DataBits, ModbusConfig, ModbusSerialConfig, Parity, SerialMode};

async fn run_ascii_example() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let config = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: "/dev/ttyUSB0".try_into()?,
        mode: SerialMode::Ascii,
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        ..Default::default()
    });
    let mut server = AsyncAsciiServer::new_ascii(&config, unit)?;
    
    server.run(MyApp::default()).await?;
    
    Ok(())
}
```

---

## Exception Handling

By default, exceptions are silent. To observe them, override `on_exception`:

```rust
use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;

#[derive(Default)]
#[async_modbus_app(...)]
struct App {
    // ...
}

impl App {
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
}
```

Note: `on_exception` is **synchronous** (not async), even in async servers.

---

## Manual `AsyncAppHandler` Implementation

If you don't want to use the `#[async_modbus_app]` derive macro, implement `AsyncAppHandler` directly:

```rust
use mbus_async::server::{AsyncAppHandler, ModbusRequest, ModbusResponse};
use mbus_core::function_codes::public::FunctionCode;

struct MyApp;

impl AsyncAppHandler for MyApp {
    async fn handle(&mut self, req: ModbusRequest) -> ModbusResponse {
        match req {
            ModbusRequest::ReadCoils { .. } => {
                // Implement read logic here
                ModbusResponse::packed_bits(FunctionCode::ReadCoils, &[0b0000_0001])
            }
            ModbusRequest::WriteSingleCoil { address, value, .. } => {
                // Implement write logic here
                ModbusResponse::echo_coil(address, value)
            }
            // Handle all other request types...
            _ => ModbusResponse::NoResponse,
        }
    }
}
```

This approach is more verbose but gives complete control over request dispatch.

---

## Comparison With Sync Servers

| Aspect | Sync (`ServerServices`) | Async (`AsyncTcpServer`) |
|--------|---|---|
| **Runtime** | Poll-driven, single thread | Tokio multi-tasking |
| **Lifecycle** | `server.connect()?; loop { server.poll(); }` | `AsyncTcpServer::serve(...).await?` |
| **Connections** | One per transport | Many concurrent (one per tokio task) |
| **App instances** | Single, shared via `Mutex` | Per-session clones or shared via `Arc<Mutex<>>` |
| **Handler traits** | `ServerCoilHandler`, `ServerHoldingRegisterHandler`, etc. | `AsyncAppHandler` + optional generated split traits |
| **Write hooks** | Synchronous: `fn(...) -> Result<(), _>` | **Async**: `async fn(...) -> Result<(), _>` |
| **Hook semantics** | Pre-commit rejection | Pre-commit rejection (same) |
| **Background tasks** | Manual spawn in separate thread | `tokio::spawn()` alongside main loop |
| **Transport type** | Owns transport, user passes to constructor | Hidden; user only passes bind address |

---

## See Also

- [Quick Start](quick_start.md)
- [Sync Server Applications](sync.md) — poll-driven sync server guide
- [Architecture](architecture.md)
- [Policies](policies.md)
- [Macros](macros.md) — includes `#[async_modbus_app]` reference
- [Write Hooks](write_hooks.md) — hook semantics (same for both sync and async)
