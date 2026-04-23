# Server Quick Start

Get a synchronous Modbus server running with the current `ServerServices` API.

---

## 1. Add Dependencies

If you are fine with the top-level default stack, this is enough:

```toml
[dependencies]
modbus-rs = "0.7.0"
```

That default includes both client and server support. For a smaller TCP-only server build,
disable defaults and opt in explicitly:

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "server",
    "network-tcp",
    "coils",
    "holding-registers"
] }
```

---

## 2. Write a Server

The current lifecycle is:

1. Build `ModbusConfig`
2. Construct a transport
3. Create `ServerServices::new(transport, app, config, unit_id, resilience)`
4. Call `connect()`
5. Drive `poll()` in a loop

```rust
use modbus_rs::{
    modbus_app, CoilsModel, HoldingRegistersModel, MbusError, ModbusConfig,
    ModbusTcpConfig, ResilienceConfig, ServerServices, StdTcpTransport,
    UnitIdOrSlaveAddr,
};

#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    run_enable: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Setpoints {
    #[reg(addr = 0)]
    target_speed: u16,
}

#[derive(Default)]
#[modbus_app(
    coils(outputs),
    holding_registers(setpoints),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("0.0.0.0", 5502)?);
    let transport = StdTcpTransport::new();
    let app = App::default();

    let mut server = ServerServices::new(
        transport,
        app,
        config,
        UnitIdOrSlaveAddr::new(1)?,
        ResilienceConfig::default(),
    );

    server.connect()?;

    loop {
        server.poll();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
```

The derive macros generate the split handler impls for FC01/FC05/FC0F and
FC03/FC06/FC10/FC16/FC17.

---

## 3. Manual Callbacks

If you do not want macros, implement the split traits directly. The current read callbacks
write encoded bytes into `out` and return the number of bytes written.

```rust
impl modbus_rs::ServerCoilHandler for MyApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _uid: modbus_rs::UnitIdOrSlaveAddr,
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
}
```

For full examples, see [Examples](examples.md) and [Building Applications](building_applications.md).

---

## 4. Test It

Start the server, then use one of the included TCP client examples from the workspace root:

```bash
# Read coils / exercise retry policy example client
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter -- 127.0.0.1 5502 1

# Read holding registers
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers -- 127.0.0.1 5502 1
```

Or run one of the prebuilt server demos directly:

```bash
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo
cargo run -p modbus-rs --example modbus_rs_server_std_transport_client_demo
```

---

## Next Steps

- [Building Applications](building_applications.md) for manual handlers, shared state, and transport setup
- [Macros](macros.md) for generated maps and routing
- [Policies](policies.md) for retry, deadlines, and queue behavior
