//! Minimal `Read FIFO Queue` (FC 0x18) server-side example.
//!
//! Demonstrates implementing [`ModbusAppHandler::read_fifo_queue_request`] to
//! service FC 0x18 requests.  The application maps a pointer address to a
//! small in-memory queue and serialises the queue into the response buffer.
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example fifo_queue --features fifo
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

/// A simple FIFO backed by a fixed-size array.
///
/// Supports a single queue at pointer address `0x0100`; any other address
/// returns [`MbusError::InvalidAddress`].
struct FifoServer {
    /// Up to 8 entries in the queue.
    queue: [u16; 8],
    /// Current number of valid entries.
    count: usize,
}

impl FifoServer {
    fn new() -> Self {
        Self {
            queue: [
                0xAAAA, 0xBBBB, 0xCCCC, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
            ],
            count: 3,
        }
    }
}

impl ServerExceptionHandler for FifoServer {}

impl ServerCoilHandler for FifoServer {}

impl ServerDiscreteInputHandler for FifoServer {}

impl ServerHoldingRegisterHandler for FifoServer {}

impl ServerInputRegisterHandler for FifoServer {}

impl ServerFifoHandler for FifoServer {
    /// FC 0x18 — return the queue contents for the given pointer address.
    ///
    /// The server writes into `out`:
    /// - `out[0..1]`: `fifo_count` as a big-endian `u16`.
    /// - `out[2..2 + fifo_count * 2]`: register values, big-endian.
    ///
    /// Returns `Ok(2 + fifo_count * 2)`.
    fn read_fifo_queue_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if pointer_address != 0x0100 {
            return Err(MbusError::InvalidAddress);
        }

        let count = self.count as u16;
        out[0] = (count >> 8) as u8;
        out[1] = count as u8;

        for (i, &v) in self.queue[..self.count].iter().enumerate() {
            out[2 + i * 2] = (v >> 8) as u8;
            out[2 + i * 2 + 1] = v as u8;
        }

        Ok(2 + count as u8 * 2)
    }
}

impl ServerFileRecordHandler for FifoServer {}

impl ServerDiagnosticsHandler for FifoServer {}

#[cfg(feature = "traffic")]
impl TrafficNotifier for FifoServer {}

fn main() {
    let mut app = FifoServer::new();
    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");

    // Simulate a successful FIFO read at the known pointer address.
    let mut out = [0u8; 64];
    let n = app
        .read_fifo_queue_request(0, uid, 0x0100, &mut out)
        .expect("read_fifo_queue_request should succeed");

    let fifo_count = u16::from_be_bytes([out[0], out[1]]);
    println!("FC18: fifo_count={fifo_count}, byte_count={n}");
    for i in 0..fifo_count as usize {
        let v = u16::from_be_bytes([out[2 + i * 2], out[2 + i * 2 + 1]]);
        println!("  value[{i}] = {v:#06X}");
    }

    // Simulate an unknown pointer address → exception.
    let err = app
        .read_fifo_queue_request(0, uid, 0x9999, &mut out)
        .expect_err("unknown pointer should fail");
    println!("FC18: unknown pointer → {err:?}");
}
