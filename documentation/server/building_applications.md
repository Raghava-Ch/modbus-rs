# Building Server Applications

Complete guide to building production-ready Modbus server applications.

---

## Table of Contents

1. [Application Structure](#application-structure)
2. [Data Models](#data-models)
3. [Transport Configuration](#transport-configuration)
4. [Request Handling](#request-handling)
5. [Error Handling](#error-handling)
6. [The Poll Loop](#the-poll-loop)

---

## Application Structure

A Modbus server application consists of:

1. **Transport** — The communication layer (TCP, Serial RTU, Serial ASCII)
2. **App** — Your struct implementing `ModbusAppHandler` or using `#[modbus_app]`
3. **ServerServices** — The orchestrator managing requests and responses
4. **Config** — Transport and protocol parameters

<!-- validate: skip -->
```rust
use modbus_rs::{
    ServerServices, ModbusConfig, ModbusTcpConfig, StdTcpTransport,
    ModbusAppHandler, MbusError, UnitIdOrSlaveAddr, Coils, Registers,
};

// Application with in-memory data
struct App {
    coils: [bool; 100],
    holding_registers: [u16; 100],
}

impl ModbusAppHandler for App {
    // Implement callbacks for each function code...
}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("0.0.0.0", 502)?);
    let transport = StdTcpTransport::new();
    let app = App {
        coils: [false; 100],
        holding_registers: [0; 100],
    };
    
    // Queue depth of 4 pending responses
    let mut server = ServerServices::<_, _, 4>::new(transport, app, config)?;
    
    server.bind()?;
    
    loop {
        server.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
```

---

## Data Models

### Using Derive Macros (Recommended)

The derive macros generate protocol handling automatically:

```rust
use modbus_rs::{modbus_app, CoilsModel, HoldingRegistersModel, InputRegistersModel, DiscreteInputsModel};

// Coils map (FC01, FC05, FC0F)
#[derive(Default, CoilsModel)]
struct DeviceOutputs {
    #[coil(addr = 0)]
    motor_enable: bool,
    #[coil(addr = 1)]
    heater_enable: bool,
    #[coil(addr = 2)]
    alarm_ack: bool,
}

// Holding registers map (FC03, FC06, FC10)
#[derive(Default, HoldingRegistersModel)]
struct DeviceSetpoints {
    #[reg(addr = 0)]
    target_speed: u16,
    #[reg(addr = 1, scale = 10)]
    target_temp: u16,  // 0.1°C resolution
    #[reg(addr = 2, unit = "RPM")]
    max_speed: u16,
}

// Input registers map (FC04) - read-only
#[derive(Default, InputRegistersModel)]
struct DeviceSensors {
    #[reg(addr = 0, scale = 10)]
    current_temp: u16,  // 0.1°C resolution
    #[reg(addr = 1)]
    current_speed: u16,
}

// Discrete inputs map (FC02) - read-only
#[derive(Default, DiscreteInputsModel)]
struct DeviceStatus {
    #[discrete_input(addr = 0)]
    motor_running: bool,
    #[discrete_input(addr = 1)]
    temp_alarm: bool,
    #[discrete_input(addr = 2)]
    door_open: bool,
}

// Combine into application
#[modbus_app(
    coils(outputs),
    holding_registers(setpoints),
    input_registers(sensors),
    discrete_inputs(status),
)]
struct App {
    outputs: DeviceOutputs,
    setpoints: DeviceSetpoints,
    sensors: DeviceSensors,
    status: DeviceStatus,
}
```

### Manual Callback Implementation

For full control, implement `ModbusAppHandler` directly:

```rust
impl ModbusAppHandler for App {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        start_address: u16,
        quantity: u16,
    ) -> Result<Coils, MbusError> {
        // Validate range
        let start = start_address as usize;
        let qty = quantity as usize;
        if start + qty > self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        
        // Return coils
        Ok(Coils::from_values(start_address, &self.coils[start..start + qty]))
    }
    
    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        uid: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        // Handle broadcast (no response sent)
        if uid.is_broadcast() {
            // Apply write but don't send response
        }
        
        let addr = address as usize;
        if addr >= self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        
        self.coils[addr] = value;
        Ok(())
    }
    
    // Implement other callbacks...
}
```

---

## Transport Configuration

### TCP Transport

```rust
use modbus_rs::{ModbusTcpConfig, ResilienceConfig, TimeoutConfig};

let config = ModbusTcpConfig::new("0.0.0.0", 502)?;

// With resilience settings
let resilience = ResilienceConfig {
    timeouts: TimeoutConfig {
        app_callback_ms: 20,
        send_ms: 50,
        request_deadline_ms: 500,
        ..Default::default()
    },
    clock_fn: Some(my_clock),
    max_send_retries: 3,
    ..Default::default()
};

let config = ModbusConfig::Tcp(config);
```

### Serial RTU Transport

```rust
use modbus_rs::{ModbusSerialConfig, SerialMode, BaudRate, DataBits, Parity, StdRtuTransport};

let config = ModbusSerialConfig {
    port_path: "/dev/ttyUSB0".try_into()?,
    mode: SerialMode::Rtu,
    baud_rate: BaudRate::Baud19200,
    data_bits: DataBits::Eight,
    stop_bits: 1,
    parity: Parity::Even,
    ..Default::default()
};

let transport = StdRtuTransport::new();
let config = ModbusConfig::Serial(config);
```

### Serial ASCII Transport

```rust
use modbus_rs::{ModbusSerialConfig, SerialMode, StdAsciiTransport};

let config = ModbusSerialConfig {
    mode: SerialMode::Ascii,
    // ... same as RTU
    ..Default::default()
};

let transport = StdAsciiTransport::new();
```

---

## Request Handling

### Unit ID / Slave Address Filtering

```rust
fn read_coils_request(
    &mut self,
    _txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    start_address: u16,
    quantity: u16,
) -> Result<Coils, MbusError> {
    // Only respond to unit ID 1
    if uid.get() != 1 {
        return Err(MbusError::NotAddressedToMe);
    }
    
    // ...
}
```

### Broadcast Handling (Serial)

```rust
fn write_single_coil_request(
    &mut self,
    _txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
) -> Result<(), MbusError> {
    // Check if this is a broadcast write
    if uid.is_broadcast() {
        // Apply the write but don't expect a response
        self.coils[address as usize] = value;
        return Ok(());
    }
    
    // Normal unicast request
    self.coils[address as usize] = value;
    Ok(())
}
```

Broadcast writes require `ResilienceConfig::enable_broadcast_writes = true`.

---

## Error Handling

### Returning Exceptions

Return `MbusError` to send an exception response:

```rust
fn read_coils_request(&mut self, ...) -> Result<Coils, MbusError> {
    // Invalid address
    if start_address >= 1000 {
        return Err(MbusError::InvalidAddress);  // → IllegalDataAddress
    }
    
    // Invalid quantity
    if quantity == 0 || quantity > 2000 {
        return Err(MbusError::InvalidData);  // → IllegalDataValue
    }
    
    // Device busy
    if self.is_busy {
        return Err(MbusError::DeviceBusy);  // → ServerDeviceBusy
    }
    
    Ok(...)
}
```

### Exception Callback

Implement `on_exception` to log or track exceptions:

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
        "Exception on FC{:02X} for unit {}: {:?} (caused by {:?})",
        function_code as u8, uid.get(), exception_code, error
    );
}
```

---

## The Poll Loop

The server is **poll-driven** — no internal threads:

<!-- validate: skip -->
```rust
fn main() -> Result<(), MbusError> {
    let mut server = ServerServices::<_, _, 4>::new(transport, app, config)?;
    server.bind()?;
    
    loop {
        server.poll();
        
        // Optional: Update sensor values, check alarms, etc.
        server.app_mut().update_sensors();
        
        // On std: sleep briefly
        std::thread::sleep(Duration::from_millis(10));
    }
}
```

### What `poll()` Does

1. Checks transport for incoming data
2. Parses complete ADU frames
3. Validates unit ID / slave address
4. Dispatches to your callback
5. Builds and sends response
6. Handles retry queue for failed sends

---

## See Also

- [Feature Flags](feature_flags.md) — Customize your build
- [Architecture](architecture.md) — Internal design
- [Policies](policies.md) — Timeouts, retry queues
- [Macros](macros.md) — Derive macro details
- [Write Hooks](write_hooks.md) — React to writes
