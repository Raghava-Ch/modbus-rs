# Building Client Applications

Complete guide to building production-ready Modbus client applications.

---

## Table of Contents

1. [Application Structure](#application-structure)
2. [Transport Configuration](#transport-configuration)
3. [Implementing Callbacks](#implementing-callbacks)
4. [The Poll Loop](#the-poll-loop)
5. [Error Handling](#error-handling)
6. [Connection Lifecycle](#connection-lifecycle)

---

## Application Structure

A Modbus client application consists of:

1. **Transport** — The communication layer (TCP, Serial RTU, Serial ASCII)
2. **App** — Your struct implementing response callbacks
3. **ClientServices** — The orchestrator managing requests, timeouts, retries
4. **Config** — Transport and protocol parameters

<!-- validate: skip -->
```rust
use modbus_rs::{
    ClientServices, ModbusConfig, ModbusTcpConfig, StdTcpTransport,
    CoilResponse, RegisterResponse, RequestErrorNotifier, TimeKeeper,
    Coils, Registers, MbusError, UnitIdOrSlaveAddr,
};

// Your application state
struct App {
    latest_coils: Option<Coils>,
    latest_registers: Option<Registers>,
}

// Implement required traits...

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
    let transport = StdTcpTransport::new();
    let app = App { latest_coils: None, latest_registers: None };
    
    // Queue depth of 4 outstanding requests
    let mut client = ClientServices::<_, _, 4>::new(transport, app, config)?;
    
    client.connect()?;
    
    // Send requests...
    // Poll for responses...
    
    Ok(())
}
```

---

## Transport Configuration

### TCP Transport

```rust
use modbus_rs::{ModbusTcpConfig, BackoffStrategy, JitterStrategy};

let mut config = ModbusTcpConfig::new("192.168.1.10", 502)?;

// Timeouts
config.response_timeout_ms = 1500;

// Retry policy
config.retry_attempts = 3;
config.retry_backoff_strategy = BackoffStrategy::Exponential {
    base_delay_ms: 100,
    max_delay_ms: 3000,
};
config.retry_jitter_strategy = JitterStrategy::Percentage { percent: 20 };
config.retry_random_fn = Some(my_random_u32);  // fn() -> u32

let config = ModbusConfig::Tcp(config);
```

### Serial RTU Transport

```rust
use modbus_rs::{
    ModbusSerialConfig, SerialMode, BaudRate, DataBits, Parity,
    BackoffStrategy, JitterStrategy,
};

let config = ModbusSerialConfig {
    port_path: "/dev/ttyUSB0".try_into()?,
    mode: SerialMode::Rtu,
    baud_rate: BaudRate::Baud19200,
    data_bits: DataBits::Eight,
    stop_bits: 1,
    parity: Parity::Even,
    response_timeout_ms: 1000,
    retry_attempts: 3,
    retry_backoff_strategy: BackoffStrategy::Fixed { delay_ms: 200 },
    retry_jitter_strategy: JitterStrategy::None,
    retry_random_fn: None,
};

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

## Implementing Callbacks

### Required: RequestErrorNotifier

Called when a request fails (timeout, protocol error, retries exhausted).

```rust
impl RequestErrorNotifier for App {
    fn request_failed(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, error: MbusError) {
        eprintln!("Request {} to unit {} failed: {:?}", txn_id, uid.get(), error);
    }
}
```

### Required: TimeKeeper

Provides monotonic timestamps for timeout tracking.

```rust
impl TimeKeeper for App {
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
```

For embedded/bare-metal:

```rust
impl TimeKeeper for App {
    fn current_millis(&self) -> u64 {
        // Return your hardware timer tick converted to milliseconds
        hal::timer::millis()
    }
}
```

### Optional: Function Code Callbacks

Implement only the traits for function codes you use:

```rust
// Coils (FC01, FC05, FC0F)
impl CoilResponse for App {
    fn read_coils_response(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, coils: &Coils) {
        println!("Coils: {:?}", coils.values());
    }
    fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, addr: u16, value: bool) {
        println!("Wrote coil {} = {}", addr, value);
    }
    fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, start: u16, qty: u16) {
        println!("Wrote {} coils starting at {}", qty, start);
    }
}

// Registers (FC03, FC04, FC06, FC10)
impl RegisterResponse for App {
    fn read_multiple_holding_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, regs: &Registers) {
        for addr in regs.from_address()..regs.from_address() + regs.quantity() {
            println!("Register {}: {}", addr, regs.value(addr).unwrap());
        }
    }
    fn read_multiple_input_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &Registers) {}
    fn read_single_holding_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn read_single_input_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_multiple_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

// Discrete Inputs (FC02)
impl DiscreteInputResponse for App {
    fn read_discrete_inputs_response(&self, _: u16, _: UnitIdOrSlaveAddr, inputs: &DiscreteInputs) {
        println!("Discrete inputs: {:?}", inputs.values);
    }
}
```

### Optional: Traffic Observability

When `traffic` feature is enabled:

```rust
use modbus_rs::{TrafficNotifier, TrafficDirection};

impl TrafficNotifier for App {
    fn on_tx_frame(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("TX [{}]: {:02X?}", txn_id, frame);
    }
    
    fn on_rx_frame(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("RX [{}]: {:02X?}", txn_id, frame);
    }
    
    fn on_tx_error(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, error: MbusError, _frame: &[u8]) {
        eprintln!("TX error [{}]: {:?}", txn_id, error);
    }
    
    fn on_rx_error(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, error: MbusError, _frame: &[u8]) {
        eprintln!("RX error [{}]: {:?}", txn_id, error);
    }
}
```

---

## Understanding Response Trait Method Signatures

### Method Receiver Types

The different callback traits use **different receiver types**:

| Trait | Method Receiver | Can Mutate State? | Use Case |
|-------|-----------------|-------------------|----------|
| `RequestErrorNotifier` | `&self` | ❌ No | Error logging only |
| `CoilResponse` | `&mut self` | ✅ Yes | Store coil states, update app |
| `RegisterResponse` | `&mut self` | ✅ Yes | Store register values, update app |
| `TimeKeeper` | `&self` | ❌ No | Read-only time access |
| `TrafficNotifier` | `&self` | ❌ No | Frame logging only |

### Using `&mut self` to Update Application State

All response methods (`read_coils_response`, `write_single_register_response`, etc.) use **`&mut self`**. This allows your app to update state when responses arrive:

```rust
pub struct App {
    latest_coils: Option<Coils>,
    latest_registers: Option<Registers>,
    error_count: u32,
}

impl CoilResponse for App {
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        self.latest_coils = Some(coils.clone());  // ✅ Can update state with &mut self
        println!("Updated coils at {}", coils.from_address());
    }
}

impl RegisterResponse for App {
    fn read_multiple_holding_registers_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, regs: &Registers) {
        self.latest_registers = Some(regs.clone());  // ✅ Can mutate
    }
}

impl RequestErrorNotifier for MyApp {
    fn request_failed(&self, txn_id: u16, uid: UnitIdOrSlaveAddr, error: MbusError) {
        eprintln!("Error: {:?}", error);  // ❌ Cannot mutate with &self
        // To update state on error, use a RefCell or Mutex if needed
    }
}
```

### TimeKeeper Return Type

The `TimeKeeper::current_millis()` method must return a **`u64` in milliseconds**:

```rust
impl TimeKeeper for App {
    /// Returns milliseconds since UNIX_EPOCH
    /// Used internally for timeout tracking and retry scheduling
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
```

For **testing or embedded systems without system time**, use a counter or fixed value:

```rust
// For testing: fixed time
fn current_millis(&self) -> u64 {
    0  // Fixed timestamp for deterministic testing
}

// For embedded: monotonic counter
static mut MILLIS: u64 = 0;
fn current_millis(&self) -> u64 {
    unsafe { MILLIS }  // Increment via timer ISR
}
```

---

## The Poll Loop

The client is **poll-driven** — no internal threads or blocking. Call `poll()` repeatedly:

```rust
// Send some requests
client.coils().read_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 16)?;
client.registers().read_holding_registers(2, UnitIdOrSlaveAddr::new(1)?, 0, 10)?;

// Poll loop
loop {
    while client.has_pending_requests() {
        client.poll();
    }
    
    // On std: sleep briefly to avoid busy-wait
    std::thread::sleep(std::time::Duration::from_millis(10));
    
    // On embedded: wait for timer interrupt or yield
}
```

### What `poll()` Does

1. Checks transport for incoming data
2. Parses complete ADU frames
3. Matches responses to pending requests
4. Invokes your callbacks
5. Handles timeouts and triggers retries
6. Updates internal state machine

---

## Error Handling

### Request-Level Errors

Handled via `RequestErrorNotifier::request_failed`:

```rust
fn request_failed(&self, txn_id: u16, uid: UnitIdOrSlaveAddr, error: MbusError) {
    match error {
        MbusError::Timeout => {
            // Request timed out after all retries
        }
        MbusError::ExceptionResponse(code) => {
            // Server returned an exception
        }
        MbusError::ConnectionLost => {
            // Connection dropped
        }
        _ => {
            // Other errors
        }
    }
}
```

### Connection-Level Errors

Check `is_connected()` and handle reconnection:

```rust
if !client.is_connected() {
    match client.reconnect() {
        Ok(_) => println!("Reconnected"),
        Err(e) => eprintln!("Reconnect failed: {:?}", e),
    }
}
```

---

## Connection Lifecycle

### Explicit Connection

```rust
// Construction doesn't connect
let mut client = ClientServices::new(transport, app, config)?;

// Explicit connect
client.connect()?;

// Check status
assert!(client.is_connected());

// Reconnect (flushes pending requests with ConnectionLost)
client.reconnect()?;

// Disconnect
client.disconnect()?;
```

### Automatic Reconnection Pattern

```rust
loop {
    if !client.is_connected() {
        if let Err(e) = client.reconnect() {
            eprintln!("Reconnect failed: {:?}", e);
            std::thread::sleep(Duration::from_secs(5));
            continue;
        }
    }
    
    client.poll();
    
    // Send requests when connected...
}
```

---

## See Also

- [Feature Flags](feature_flags.md) — Customize your build
- [Architecture](architecture.md) — Internal design
- [Policies](policies.md) — Retry and timeout configuration
- [Async Development](async.md) — Tokio async APIs
