# Modbus-rs Quick Start Guide

This guide will help you get started with `modbus-rs`, a `no_std` compatible Modbus client library for Rust, designed for embedded systems and bare-metal environments.

## 1. Project Setup

To use `modbus-rs` in your project, you need to add the necessary crates to your `Cargo.toml` file.

### Add Dependencies

Depending on your needs (e.g., Modbus TCP, Modbus Serial, or just core data structures), you will include `mbus-core` and `modbus-client`. If you plan to use standard TCP or Serial transports, you'll also need `mbus-tcp` or `mbus-serial` respectively.

```toml
[dependencies]
mbus-core = { version = "0.1.0", default-features = false } # Use default-features = false for no_std
modbus-client = { version = "0.1.0", default-features = false } # Use default-features = false for no_std

# If you need Modbus TCP support (requires `std` environment)
mbus-tcp = { version = "0.1.0", optional = true }

# If you need Modbus Serial (RTU/ASCII) support (requires `std` environment)
mbus-serial = { version = "0.1.0", optional = true }

[features]
std = [
    "mbus-core/std",
    "modbus-client/std",
    "mbus-tcp", # Enable mbus-tcp when 'std' feature is active
    "mbus-serial", # Enable mbus-serial when 'std' feature is active
]
```

**Note on `no_std` vs. `std`**:
- By default, `mbus-core` and `modbus-client` are `no_std` compatible.
- If you are developing for a `std` environment (e.g., a desktop application, Raspberry Pi, etc.) and want to use the provided `mbus-tcp` or `mbus-serial` implementations, you will need to enable the `std` feature. This is typically done by adding `default-features = false` and then `features = ["std"]` to your `Cargo.toml` for `mbus-core` and `modbus-client`, and simply including `mbus-tcp` or `mbus-serial` as shown above.

## 2. Basic Usage Example (Modbus TCP)

This example demonstrates how to set up a Modbus TCP client to read multiple coils.

```rust
use mbus_core::transport::{UnitIdOrSlaveAddr, ModbusConfig, ModbusTcpConfig, Transport, TransportType, TimeKeeper};
use mbus_core::errors::MbusError;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use modbus_client::app::{CoilResponse, RequestErrorNotifier};
use modbus_client::services::coil::Coils;
use modbus_client::services::ClientServices;
use heapless::Vec;

// --- Mock Transport for demonstration purposes ---
// In a real application, you would use `mbus-tcp::StdTcpTransport` or `mbus-serial::StdSerialTransport`.
struct MockTransport {
    is_connected: bool,
}

impl MockTransport {
    fn new() -> Self { Self { is_connected: false } }
}

impl Transport for MockTransport {
    type Error = MbusError;
    fn connect(&mut self, _: &ModbusConfig) -> Result<(), Self::Error> {
        self.is_connected = true;
        Ok(())
    }
    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.is_connected = false;
        Ok(())
    }
    fn send(&mut self, _: &[u8]) -> Result<(), Self::Error> { Ok(()) }
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        // In a real scenario, this would read from the network/serial port.
        // For this mock, we return an empty vector or simulate a timeout.
        Err(MbusError::Timeout)
    }
    fn is_connected(&self) -> bool { self.is_connected }
    fn transport_type(&self) -> TransportType { TransportType::StdTcp }
}

// --- Mock TimeKeeper for demonstration purposes ---
struct MockTimeKeeper;
impl TimeKeeper for MockTimeKeeper {
    fn current_millis(&self) -> u64 { 0 } // Always return 0 for simplicity in this mock
}

// 1. Define your application state and implement response traits
struct MyApp;
impl CoilResponse for MyApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        println!("Received Read Coils Response (Txn ID: {}, Unit ID: {}): {:?}", txn_id, unit_id.get(), coils.values());
    }
    // Implement other CoilResponse methods or use default empty implementations if not needed
    fn read_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}
impl RequestErrorNotifier for MyApp {
    fn request_failed(&self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr, error: MbusError) {
        eprintln!("Request failed (Txn ID: {}, Unit ID: {}): {:?}", txn_id, unit_id_slave_addr.get(), error);
    }
}

fn main() -> Result<(), MbusError> {
    // 2. Initialize transport and config
    let transport = MockTransport::new(); // Use a real transport like `mbus-tcp::StdTcpTransport::new()` for actual communication
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);

    // 3. Create the service (N=5 allows 5 concurrent requests)
    let mut client = ClientServices::<_, _, 5>::new(transport, MyApp, config, MockTimeKeeper)?;

    // 4. Send a request (e.g., read 8 coils starting from address 0 on unit ID 1)
    client.read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;

    // 5. Periodically poll to process incoming bytes and handle timeouts
    // In a real application, this would be part of your main event loop.
    loop {
        client.poll();
        // In a real application, you might add a small delay here to avoid busy-waiting
        // e.g., `std::thread::sleep(std::time::Duration::from_millis(10));`
        // For this mock, we'll just break after a few polls to avoid an infinite loop.
        // In a real system, `poll()` would be called continuously by your main loop.
        break;
    }
    Ok(())
}
```

## 3. Where to Look for Examples

The `modbus-rs` project typically includes an `examples/` directory at the root of the repository. This directory contains various examples demonstrating how to use the library with different Modbus function codes and transport types (TCP, Serial).

You can usually find examples like:
- `tcp_read_coils.rs`: Demonstrates reading coils over Modbus TCP.
- `serial_write_registers.rs`: Shows writing to registers over Modbus RTU/ASCII.
- `diagnostics_example.rs`: Illustrates using diagnostic function codes.

## 4. How to Run Examples

To run an example, navigate to the root directory of the `modbus-rs` project in your terminal. You can then use `cargo run --example` command.

**For `no_std` examples (using mock transports):**

```bash
cargo run --example <example_name>
```

**For `std` examples (using `mbus-tcp` or `mbus-serial`):**

If an example uses the `std` feature (e.g., for actual network or serial communication), you'll need to enable it:

```bash
cargo run --example <example_name> --features "std"
```

Replace `<example_name>` with the actual name of the example file (e.g., `tcp_read_coils`).

## 5. Setup for Running Examples

To successfully run examples that interact with a real Modbus device, you'll need a Modbus server running.

### Modbus TCP Examples

1.  **Modbus Server Simulator**: You can use a software Modbus TCP simulator. Popular options include:
    *   **ModbusPal**: A free, open-source Modbus slave simulator (Java-based).
    *   **Simply Modbus TCP Slave**: A commercial tool with a free trial.
    *   **Python `pymodbus`**: You can quickly set up a simple Modbus TCP server using a Python script.

    Ensure the simulator is running on `127.0.0.1` (localhost) or an accessible IP address, and listening on port `502` (the default Modbus TCP port) or another configured port.

### Modbus Serial (RTU/ASCII) Examples

1.  **Serial Port**: You'll need access to a serial port.
    *   **Physical Port**: A USB-to-serial adapter or a built-in serial port on your development machine.
    *   **Virtual Serial Port**: Tools like `socat` on Linux or `com0com` on Windows can create virtual serial port pairs, allowing you to connect a Modbus simulator to one end and your Rust application to the other.

2.  **Modbus RTU/ASCII Server Simulator**: Similar to TCP, you'll need a simulator that supports Modbus RTU or ASCII over a serial port. ModbusPal also supports serial modes.

Ensure the serial port settings (baud rate, parity, data bits, stop bits) in your example code match those of your Modbus server.

By following these steps, you should be able to set up your environment, integrate `modbus-rs` into your project, and run the provided examples to understand its functionality.

---

**Note**: The `mbus-tcp` and `mbus-serial` crates are mentioned as providing standard transport implementations. If these crates are not yet fully implemented or available, you might need to provide your own `Transport` implementation as shown in the basic usage example, or use a mock for `no_std` environments.

The `modbus-client/src/lib.rs` example uses a `YourTransport` mock. For a real `std` application, you would replace `YourTransport` with `mbus_tcp::StdTcpTransport` (after adding `mbus-tcp` to your `Cargo.toml` and enabling the `std` feature).

Example of using `StdTcpTransport`:

```rust
// ... (other imports)
use mbus_tcp::StdTcpTransport; // Import the concrete TCP transport

fn main() -> Result<(), MbusError> {
    // ...
    let transport = StdTcpTransport::new(); // Use the real TCP transport
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);

    let mut client = ClientServices::<_, _, 5>::new(transport, MyApp, config, MockTimeKeeper)?;
    // ...
    Ok(())
}
```