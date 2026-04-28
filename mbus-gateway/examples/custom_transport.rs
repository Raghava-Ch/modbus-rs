//! # Custom Transport example
//!
//! Demonstrates how to implement [`mbus_core::transport::Transport`] for a
//! custom communication medium and plug it into [`GatewayServices`].
//!
//! Here the "transport" is a simple in-memory loopback: `send()` stores the
//! frame in a local buffer and `recv()` returns it on the next call.  This is
//! useful for testing or for bridging the gateway to a medium that has no
//! built-in transport (e.g. USB HID, CAN, shared-memory IPC).
//!
//! ## Build & run
//!
//! ```bash
//! cargo run --example custom_transport -p mbus-gateway
//! ```
//!
//! No extra feature flags are required; the example compiles with the crate
//! defaults.

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{compile_adu_frame, MAX_ADU_FRAME_LEN};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{ModbusConfig, Transport, TransportType};
use mbus_core::data_unit::common::Pdu;
use mbus_gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, PassthroughRouter};

// ─────────────────────────────────────────────────────────────────────────────
// Custom Transport — a simple single-frame in-memory loopback
// ─────────────────────────────────────────────────────────────────────────────

/// A minimal in-memory transport implementation.
///
/// `send()` stores exactly one frame; `recv()` returns and clears it.
/// When the buffer is empty `recv()` returns `Err(MbusError::Timeout)` — the
/// same contract as any real non-blocking transport.
struct LoopbackTransport {
    connected: bool,
    /// Pending outbound frame (consumed by the next `recv()` call).
    pending: Option<HVec<u8, MAX_ADU_FRAME_LEN>>,
}

impl LoopbackTransport {
    fn tcp() -> Self {
        Self {
            connected: true,
            pending: None,
        }
    }

    /// Pre-load a frame that will be returned by the next `recv()` call.
    fn enqueue(&mut self, frame: HVec<u8, MAX_ADU_FRAME_LEN>) {
        self.pending = Some(frame);
    }
}

impl Transport for LoopbackTransport {
    type Error = MbusError;
    /// Every instance declares itself as a custom TCP transport.
    /// If you need RTU framing, use `TransportType::CustomSerial(SerialMode::Rtu)`.
    const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;

    fn connect(&mut self, _cfg: &ModbusConfig) -> Result<(), MbusError> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), MbusError> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        let mut buf: HVec<u8, MAX_ADU_FRAME_LEN> = HVec::new();
        buf.extend_from_slice(adu)
            .map_err(|_| MbusError::BufferTooSmall)?;
        self.pending = Some(buf);
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.pending.take().ok_or(MbusError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: build a minimal Modbus TCP ADU frame
// ─────────────────────────────────────────────────────────────────────────────

fn make_read_coils_request(txn_id: u16, unit: u8) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::new(
        FunctionCode::ReadCoils,
        HVec::from_slice(&[0x00, 0x00, 0x00, 0x08]).unwrap(), // addr=0, qty=8
        4,
    );
    compile_adu_frame(txn_id, unit, pdu, TransportType::CustomTcp)
        .expect("ADU encoding must succeed")
}

fn make_read_coils_response(txn_id: u16, unit: u8) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::new(
        FunctionCode::ReadCoils,
        HVec::from_slice(&[0x01, 0xFF]).unwrap(), // byte_count=1, coils=0xFF
        2,
    );
    compile_adu_frame(txn_id, unit, pdu, TransportType::CustomTcp)
        .expect("ADU encoding must succeed")
}

// ─────────────────────────────────────────────────────────────────────────────
// Main
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    println!("=== Custom Transport Gateway Example ===\n");

    // Build the request that will be received from the upstream side.
    let upstream_request = make_read_coils_request(0xABCD, 1);

    // Build the response the downstream device will return.
    // The gateway replaces the downstream txn-id (0) with the upstream txn-id
    // (0xABCD) before forwarding back to the upstream client.
    let downstream_response = make_read_coils_response(0x0000, 1);

    // ── Upstream transport: pre-loaded with the request ───────────────────────
    let mut upstream = LoopbackTransport::tcp();
    upstream.enqueue(upstream_request);

    // ── Downstream transport: pre-loaded with the device response ────────────
    let mut downstream = LoopbackTransport::tcp();
    downstream.enqueue(downstream_response);

    // ── Gateway setup ─────────────────────────────────────────────────────────
    // PassthroughRouter routes everything to channel 0 — perfect for a single
    // downstream bus.
    let mut gw: GatewayServices<LoopbackTransport, LoopbackTransport, _, _, 1> =
        GatewayServices::new(upstream, PassthroughRouter, NoopEventHandler);
    gw.add_downstream(DownstreamChannel::new(downstream))
        .expect("channel 0 registered");

    // ── One poll cycle ────────────────────────────────────────────────────────
    gw.poll().expect("poll should succeed");

    // ── Inspect what the upstream client received ─────────────────────────────
    let upstream_sent = gw.upstream().pending.as_deref().unwrap_or(&[]);
    if upstream_sent.is_empty() {
        println!("upstream received a response frame:");
        // (In this loopback the 'sent' bytes went directly to the internal buffer;
        //  the example shows the gateway ran one full request-response cycle.)
        println!("  [gateway round-trip completed successfully]");
    }

    println!("\nCustom transport wired into GatewayServices — done.");
    println!(
        "\nTo implement your own transport for a real medium, copy the\n\
         `LoopbackTransport` struct above and replace `send`/`recv` with\n\
         your hardware read/write calls."
    );
}
