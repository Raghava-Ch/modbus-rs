//! Demonstrates Serial broadcast write handling in `mbus-server`.
//!
//! Modbus broadcast writes (slave address `0`) are used to update all devices on
//! a Serial bus simultaneously. The server processes the write but **never sends
//! a response** — this is mandated by the Modbus specification for address `0`.
//!
//! ## Supported function codes
//! | FC   | Name                    |
//! |------|-------------------------|
//! | 0x05 | Write Single Coil       |
//! | 0x0F | Write Multiple Coils    |
//! | 0x06 | Write Single Register   |
//! | 0x10 | Write Multiple Registers|
//!
//! ## Transport restriction
//! Broadcast writes are **Serial-only**. TCP transports do not have a broadcast
//! address concept in Modbus and silently drop frames addressed to unit `0`.
//!
//! ## Configuration
//! Enable broadcast handling by setting `enable_broadcast_writes: true` in
//! [`ResilienceConfig`]. The default is `false` (frames addressed to `0` are
//! discarded without invoking any app callback).
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example broadcast_writes --features "coils,holding-registers"
//! ```

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerDiagnosticsHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// A simple in-process controller that tracks the last broadcast values it
/// received.  A real embedded application would write these directly to
/// hardware peripherals or shared state.
#[derive(Debug, Default)]
struct ControllerApp {
    relay_states: [bool; 8],
    setpoints: [u16; 8],
    broadcast_writes_received: u32,
}

impl ServerExceptionHandler for ControllerApp {}

impl ServerCoilHandler for ControllerApp {
    // ── Coil writes ────────────────────────────────────────────────────────

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        if (address as usize) < self.relay_states.len() {
            self.relay_states[address as usize] = value;
            if unit_id_or_slave_addr.is_broadcast() {
                self.broadcast_writes_received += 1;
            }
            Ok(())
        } else {
            Err(MbusError::InvalidAddress)
        }
    }

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        packed_values: &[u8],
    ) -> Result<(), MbusError> {
        for bit_pos in 0..(quantity as usize) {
            let addr = starting_address as usize + bit_pos;
            if addr >= self.relay_states.len() {
                break;
            }
            let byte_idx = bit_pos / 8;
            let bit_idx = bit_pos % 8;
            self.relay_states[addr] = (packed_values[byte_idx] >> bit_idx) & 1 != 0;
        }
        if unit_id_or_slave_addr.is_broadcast() {
            self.broadcast_writes_received += 1;
        }
        Ok(())
    }
}

impl ServerDiscreteInputHandler for ControllerApp {}

impl ServerHoldingRegisterHandler for ControllerApp {
    // ── Register writes ────────────────────────────────────────────────────

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        if (address as usize) < self.setpoints.len() {
            self.setpoints[address as usize] = value;
            if unit_id_or_slave_addr.is_broadcast() {
                self.broadcast_writes_received += 1;
            }
            Ok(())
        } else {
            Err(MbusError::InvalidAddress)
        }
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        for (offset, &v) in values.iter().enumerate() {
            let addr = starting_address as usize + offset;
            if addr >= self.setpoints.len() {
                break;
            }
            self.setpoints[addr] = v;
        }
        if unit_id_or_slave_addr.is_broadcast() {
            self.broadcast_writes_received += 1;
        }
        Ok(())
    }
}

impl ServerInputRegisterHandler for ControllerApp {}

impl ServerFifoHandler for ControllerApp {}

impl ServerFileRecordHandler for ControllerApp {}

impl ServerDiagnosticsHandler for ControllerApp {}

#[cfg(feature = "traffic")]
impl TrafficNotifier for ControllerApp {}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let broadcast = UnitIdOrSlaveAddr::new_broadcast_address();
    let mut app = ControllerApp::default();

    // ── FC05: broadcast single coil ON ────────────────────────────────────
    #[cfg(feature = "coils")]
    {
        app.write_single_coil_request(1, broadcast, 3, true)
            .expect("FC05 broadcast callback should succeed");
        println!(
            "FC05 broadcast → relay[3] = {} (no response sent to master)",
            app.relay_states[3]
        );
    }

    // ── FC0F: broadcast multiple coils ───────────────────────────────────
    #[cfg(feature = "coils")]
    {
        // Set relays 0–3 to ON (packed byte 0b0000_1111)
        app.write_multiple_coils_request(2, broadcast, 0, 4, &[0x0F])
            .expect("FC0F broadcast callback should succeed");
        println!(
            "FC0F broadcast → relays[0..4] = {:?} (no response sent to master)",
            &app.relay_states[..4]
        );
    }

    // ── FC06: broadcast single register ──────────────────────────────────
    #[cfg(feature = "holding-registers")]
    {
        app.write_single_register_request(3, broadcast, 0, 1500)
            .expect("FC06 broadcast callback should succeed");
        println!(
            "FC06 broadcast → setpoint[0] = {} (no response sent to master)",
            app.setpoints[0]
        );
    }

    // ── FC10: broadcast multiple registers ──────────────────────────────
    #[cfg(feature = "holding-registers")]
    {
        app.write_multiple_registers_request(4, broadcast, 0, &[100, 200, 300])
            .expect("FC10 broadcast callback should succeed");
        println!(
            "FC10 broadcast → setpoints[0..3] = {:?} (no response sent to master)",
            &app.setpoints[..3]
        );
    }

    println!(
        "\nTotal broadcast writes received by app: {}",
        app.broadcast_writes_received
    );

    // ── ResilienceConfig setup reminder ──────────────────────────────────
    println!("\n--- ResilienceConfig for broadcast writes ---");
    println!("  ResilienceConfig {{");
    println!("      enable_broadcast_writes: true,  // default: false");
    println!("      ..Default::default()");
    println!("  }}");
    println!("Note: Serial transport must also set SUPPORTS_BROADCAST_WRITES = true.");
    println!("TCP transports always drop frames addressed to unit 0.");
}
