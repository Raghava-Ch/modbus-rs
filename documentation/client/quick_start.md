# Client Quick Start

Get your first Modbus client running in 5 minutes.

---

## Prerequisites

- Rust 1.75+ (2024 edition)
- A Modbus server/device to connect to (or use a simulator)

---

## 1. Add Dependencies

### Full Default Stack (TCP + Serial RTU + All FCs)

```toml
[dependencies]
modbus-rs = "0.7.0"
```

### Minimal TCP Client (Coils Only)

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "client",
    "network-tcp",
    "coils"
] }
```

### Minimal Serial RTU Client (Registers Only)

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "client",
    "serial-rtu",
    "registers"
] }
```

---

## 2. Write Your First Client

### TCP Client Example

```rust
use modbus_rs::{
    ClientServices, MbusError, ModbusConfig, ModbusTcpConfig,
    CoilResponse, Coils, RequestErrorNotifier, TimeKeeper,
    UnitIdOrSlaveAddr, StdTcpTransport,
};

// Application struct implementing Modbus callback traits.
// Note: All response methods use &mut self to allow state mutations.
struct App;

impl RequestErrorNotifier for App {
    // Called when a request fails (timeout, CRC error, etc.)
    fn request_failed(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, err: MbusError) {
        eprintln!("Request {} failed: {:?}", txn_id, err);
    }
}

// CoilResponse trait: Handles responses from coil read/write operations (FC01, FC05, FC0F)
// All methods use &mut self to allow state updates in your application
impl CoilResponse for App {
    fn read_coils_response(&mut self, txn_id: u16, uid: UnitIdOrSlaveAddr, coils: &Coils) {
        println!("Received {} coils: {:?}", coils.quantity(), coils.values());
    }
    fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl TimeKeeper for App {
    // Returns current time in milliseconds since UNIX_EPOCH (u64)
    // Used internally for timeout tracking and retry scheduling
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for App {}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
    let transport = StdTcpTransport::new();
    let mut client = ClientServices::<_, _, 4>::new(transport, App, config)?;
    
    client.connect()?;
    
    // Read 8 coils starting at address 0
    client.coils().read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;
    
    // Poll until response arrives
    for _ in 0..100 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    
    Ok(())
}
```

### Serial RTU Client Example

```rust
use modbus_rs::{
    ClientServices, MbusError, ModbusConfig, ModbusSerialConfig,
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, Parity, SerialMode, StdRtuTransport,
    RegisterResponse, Registers, RequestErrorNotifier, TimeKeeper,
    UnitIdOrSlaveAddr,
};

struct App;

impl RequestErrorNotifier for App {
    fn request_failed(&mut self, _: u16, _: UnitIdOrSlaveAddr, err: MbusError) {
        eprintln!("Error: {:?}", err);
    }
}

// RegisterResponse trait: Handles responses from register read/write operations (FC03, FC04, FC06, FC10)
// All methods use &mut self to allow state updates in your application
impl RegisterResponse for App {
    fn read_multiple_holding_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, regs: &Registers) {
        println!("Holding registers: {:?}", regs.values());
    }
    fn read_multiple_input_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &Registers) {}
    fn read_write_multiple_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &Registers) {}
    fn read_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn read_single_holding_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn read_single_input_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_multiple_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn mask_write_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr) {}
}

impl TimeKeeper for App {
    fn current_millis(&self) -> u64 { 0 }
}

#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for App {}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: "/dev/ttyUSB0".try_into().expect("static port path fits"),
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Baud19200,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::Even,
        response_timeout_ms: 1000,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    });
    
    let transport = StdRtuTransport::new();
    let mut client = ClientServices::<_, _, 4>::new(transport, App, config)?;
    
    client.connect()?;
    client.registers().read_holding_registers(1, UnitIdOrSlaveAddr::new(1)?, 0, 10)?;
    
    loop {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
```

---

## 3. Run an Example

From the workspace root:

```bash
# TCP coils example
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 192.168.1.10 502 1

# Serial RTU registers example  
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers \
    --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
```

---

---

## Important Notes on Callback Traits

### Response Trait Methods Use `&mut self`

All response callback methods (`CoilResponse`, `RegisterResponse`, etc.) use **`&mut self`** instead of `&self`. This allows your application to update internal state when responses arrive:

```rust
impl CoilResponse for MyApp {
    // ✅ Correct: &mut self allows state mutation
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        self.latest_coils = Some(coils.clone());  // Can update state
    }
}
```

If you only need to read data without mutations, you can use `&self` in your closure, but the trait definition requires `&mut self`.

### TimeKeeper::current_millis() Signature

The `TimeKeeper` trait returns the current time as a **`u64` in milliseconds**:

```rust
impl TimeKeeper for MyApp {
    // Must return: milliseconds since UNIX_EPOCH (typically)
    // Used internally for timeout tracking and retry scheduling
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}
```

For testing or embedded systems without system time, you can return a fixed value or increment a counter instead.

---

## Next Steps

- [Examples Reference](examples.md) — Find an example for your use case
- [Building Applications](building_applications.md) — Production-ready setup
- [Feature Flags](feature_flags.md) — Minimize binary size
- [Async Development](async.md) — Use Tokio async APIs
