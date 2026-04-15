//! # Modbus FIFO Queue Service (server-side)
//!
//! Handles the Read FIFO Queue (FC 0x18) Modbus request and builds the
//! corresponding variable-length response PDU.

use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use super::framing::{build_fifo_response, parse_read_fifo_pointer_request};
use crate::app::ModbusAppHandler;
use crate::services::{ServerServices, server_log_debug};

/// FC18 maximum number of FIFO entries per response (Modbus specification limit).
const FC18_MAX_FIFO_COUNT: u16 = 31;

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC18 (Read FIFO Queue).
    ///
    /// Parses the FIFO pointer address, invokes the application callback, validates
    /// the returned FIFO count against the Modbus limit of 31 entries, and sends a
    /// byte-count-prefixed variable-length response.
    ///
    /// ## Response PDU layout
    /// ```text
    /// [byte_count_hi, byte_count_lo, fifo_count_hi, fifo_count_lo, value0_hi, value0_lo, ...]
    /// ```
    /// where `byte_count = 2 + fifo_count * 2`.
    ///
    /// ## App callback contract
    /// The application must write into `out`:
    /// - `out[0..1]`: `fifo_count` as a big-endian `u16`.
    /// - `out[2..2 + fifo_count * 2]`: register values, each 2 bytes big-endian.
    ///
    /// And return `Ok(2 + fifo_count * 2)`.
    #[cfg(feature = "fifo")]
    pub(super) fn handle_read_fifo_queue_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let pointer_address = match parse_read_fifo_pointer_request(message) {
            Ok(addr) => addr,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFifoQueue,
                    err,
                );
                return;
            }
        };

        // Buffer: app writes fifo_count(2 bytes) + values(fifo_count * 2 bytes).
        // Maximum: 2 + 31 * 2 = 64 bytes.
        let mut buf = [0u8; MAX_PDU_DATA_LEN];
        let returned_len = match self.app.read_fifo_queue_request(
            txn_id,
            unit_id_or_slave_addr,
            pointer_address,
            &mut buf,
        ) {
            Ok(n) => n as usize,
            Err(err) => {
                server_log_debug!(
                    "FC18: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFifoQueue,
                    err,
                );
                return;
            }
        };

        // Need at least 2 bytes to extract the fifo_count field.
        if returned_len < 2 {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadFifoQueue,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let fifo_count = u16::from_be_bytes([buf[0], buf[1]]);

        // Validate against the Modbus spec limit of 31 entries per response.
        if fifo_count > FC18_MAX_FIFO_COUNT {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadFifoQueue,
                MbusError::InvalidQuantity,
            );
            return;
        }

        // Validate that the app wrote exactly fifo_count * 2 value bytes.
        let expected_len = 2 + fifo_count as usize * 2;
        if returned_len != expected_len {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadFifoQueue,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let response = match build_fifo_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            &buf[..returned_len],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFifoQueue,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }
}
