# Server Quick Start

Get your first Modbus server running in 5 minutes.

---

## Prerequisites

- Rust 1.75+ (2024 edition)
- A Modbus client/master to connect (or use an included client example)

---

## 1. Add Dependencies

### Full Default Stack (TCP + All FCs)

```toml
[dependencies]
modbus-rs = { version = "0.7.0", features = ["server"] }
```

### Minimal TCP Server (Coils + Registers Only)

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "server",
    "tcp",
    "coils",
    "holding-registers"
] }
```

---

## 2. Write Your First Server

### TCP Server with Derive Macros

```rust
use modbus_rs::{
    modbus_app, CoilsModel, HoldingRegistersModel,
    ServerServices, MbusError, ModbusConfig, ModbusTcpConfig,
    ResilienceConfig, StdTcpTransport, UnitIdOrSlaveAddr,
};

// Define coils data model
#[derive(Default, CoilsModel)]
struct MyCoils {
    #[coil(addr = 0)]
    output_enable: bool,
    #[coil(addr = 1)]
    alarm_reset: bool,
}

// Define holding registers data model
#[derive(Default, HoldingRegistersModel)]
struct MyRegisters {
    #[reg(addr = 0)]
    setpoint: u16,
    #[reg(addr = 1, scale = 10)]
    temperature: u16,  // 0.1°C resolution
}

// Create application handler
#[modbus_app(
    coils(coils),
    holding_registers(registers),
)]
struct MyApp {
    coils: MyCoils,
    registers: MyRegisters,
}

#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for MyApp {
    fn on_rx_frame(&mut self, _txn_id: u16, _uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("RX frame ({} bytes)", frame.len());
    }

    fn on_tx_frame(&mut self, _txn_id: u16, _uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("TX frame ({} bytes)", frame.len());
    }

    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        println!("RX error {:?} on frame ({} bytes)", error, frame.len());
    }

    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        println!("TX error {:?} on frame ({} bytes)", error, frame.len());
    }
}

impl MyApp {
    fn new() -> Self {
        Self {
            coils: MyCoils::default(),
            registers: MyRegisters {
                setpoint: 250,      // 25.0
                temperature: 235,   // 23.5°C
            },
        }
    }
}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("0.0.0.0", 502)?);
    let transport = StdTcpTransport::new();
    let app = MyApp::new();
    
    let mut server = ServerServices::new(
        transport,
        app,
        config,
        UnitIdOrSlaveAddr::new(1)?,
        ResilienceConfig::default(),
    );
    
    server.connect()?;
    
    println!("Modbus TCP server listening on port 502");
    
    loop {
        server.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
```

### TCP Server with Manual Callbacks

<!-- validate: skip -->
```rust
use modbus_rs::{
    ServerServices, MbusError, ModbusConfig, ModbusTcpConfig,
    StdTcpTransport, ServerCoilHandler, ServerHoldingRegisterHandler,
    ServerExceptionHandler, UnitIdOrSlaveAddr, Coils, Registers,
};

struct MyApp {
    coils: [bool; 16],
    registers: [u16; 10],
}

impl ServerExceptionHandler for MyApp {}

impl ServerCoilHandler for MyApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        start_address: u16,
        quantity: u16,
    ) -> Result<Coils, MbusError> {
        let start = start_address as usize;
        let qty = quantity as usize;
        
        if start + qty > self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        
        Ok(Coils::from_values(start_address, &self.coils[start..start + qty]))
    }
    
    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let addr = address as usize;
        
        if addr >= self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        
        self.coils[addr] = value;
        Ok(())
    }
}

impl ServerHoldingRegisterHandler for MyApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        start_address: u16,
        quantity: u16,
    ) -> Result<Registers, MbusError> {
        let start = start_address as usize;
        let qty = quantity as usize;
        
        if start + qty > self.registers.len() {
            return Err(MbusError::InvalidAddress);
        }
        
        Ok(Registers::from_values(start_address, &self.registers[start..start + qty]))
    }
}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("0.0.0.0", 502)?);
    let transport = StdTcpTransport::new();
    let app = MyApp {
        coils: [false; 16],
        registers: [0; 10],
    };
    
    let mut server = ServerServices::<_, _, 4>::new(transport, app, config)?;
    server.bind()?;
    
    loop {
        server.poll();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
```

---

## 3. Test Your Server

From the workspace root, use the client examples:

```bash
# Read 8 coils starting at address 0
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 127.0.0.1 502 1

# Read 10 holding registers starting at address 0
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers -- 127.0.0.1 502 1
```

Or use any Modbus client tool (e.g., `modpoll`).

---

## 4. Run a Pre-Built Server Example

```bash
# TCP demo server
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo

# TCP shared-state demo server
cargo run -p modbus-rs --example modbus_rs_server_std_transport_client_demo
```

---

## Next Steps

- [Examples Reference](examples.md) — Find an example for your use case
- [Building Applications](building_applications.md) — Production-ready setup
- [Macros](macros.md) — Learn the derive macro system
- [Write Hooks](write_hooks.md) — React to client writes
- [Policies](policies.md) — Configure timeouts and retry queues
