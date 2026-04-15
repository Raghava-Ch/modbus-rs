//! # Modbus File Record Service (server-side)
//!
//! Handles Read File Record (FC14) and Write File Record (FC15) requests.

use heapless::Vec;
use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::models::file_record::FILE_RECORD_REF_TYPE;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use super::framing::{
    build_file_record_read_response, build_file_record_write_echo_response,
    parse_file_record_read_request, parse_file_record_write_request,
};
use crate::app::ModbusAppHandler;
use crate::services::{ServerServices, server_log_debug};

const FILE_RECORD_MAX_RESPONSE_PAYLOAD_LEN: usize = MAX_PDU_DATA_LEN - 1;

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC14 (Read File Record).
    ///
    /// Each request can contain multiple sub-requests. The server invokes the
    /// app callback once per sub-request and concatenates all sub-responses.
    #[cfg(feature = "file-record")]
    pub(super) fn handle_read_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let sub_requests = match parse_file_record_read_request(message) {
            Ok(v) => v,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    err,
                );
                return;
            }
        };

        let mut payload: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        let mut app_out = [0u8; MAX_PDU_DATA_LEN];

        for sub in sub_requests {
            if sub
                .record_number
                .checked_add(sub.record_length.saturating_sub(1))
                .is_none()
            {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::InvalidAddress,
                );
                return;
            }

            let written = match self.app.read_file_record_request(
                txn_id,
                unit_id_or_slave_addr,
                sub.file_number,
                sub.record_number,
                sub.record_length,
                &mut app_out,
            ) {
                Ok(n) => n,
                Err(err) => {
                    server_log_debug!(
                        "FC14: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                        txn_id,
                        unit_id_or_slave_addr.get(),
                        err
                    );
                    self.send_exception_response(
                        txn_id,
                        unit_id_or_slave_addr,
                        FunctionCode::ReadFileRecord,
                        err,
                    );
                    return;
                }
            };

            let expected_data_len = sub.record_length as usize * 2;
            if written as usize != expected_data_len || expected_data_len > app_out.len() {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::InvalidByteCount,
                );
                return;
            }

            let Some(sub_len) = 1usize.checked_add(expected_data_len) else {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::InvalidByteCount,
                );
                return;
            };
            if sub_len > u8::MAX as usize {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::InvalidByteCount,
                );
                return;
            }

            let Some(projected) = payload.len().checked_add(1 + sub_len) else {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::FileReadPduOverflow,
                );
                return;
            };
            if projected > FILE_RECORD_MAX_RESPONSE_PAYLOAD_LEN {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::FileReadPduOverflow,
                );
                return;
            }

            if payload.push(sub_len as u8).is_err()
                || payload.push(FILE_RECORD_REF_TYPE).is_err()
                || payload
                    .extend_from_slice(&app_out[..written as usize])
                    .is_err()
            {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    MbusError::BufferTooSmall,
                );
                return;
            }
        }

        let response = match build_file_record_read_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            &payload,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadFileRecord,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles FC15 (Write File Record).
    ///
    /// Each request can contain multiple sub-requests. The server invokes the
    /// app callback once per sub-request and, on success, echoes the request payload.
    #[cfg(feature = "file-record")]
    pub(super) fn handle_write_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let sub_requests = match parse_file_record_write_request(message) {
            Ok(v) => v,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteFileRecord,
                    err,
                );
                return;
            }
        };

        let mut registers = [0u16; MAX_PDU_DATA_LEN / 2];
        for sub in sub_requests {
            if sub
                .record_number
                .checked_add(sub.record_length.saturating_sub(1))
                .is_none()
            {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteFileRecord,
                    MbusError::InvalidAddress,
                );
                return;
            }

            let expected_bytes = sub.record_length as usize * 2;
            if sub.record_data_bytes.len() != expected_bytes {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteFileRecord,
                    MbusError::InvalidByteCount,
                );
                return;
            }

            for (index, chunk) in sub.record_data_bytes.chunks_exact(2).enumerate() {
                registers[index] = u16::from_be_bytes([chunk[0], chunk[1]]);
            }

            if let Err(err) = self.app.write_file_record_request(
                txn_id,
                unit_id_or_slave_addr,
                sub.file_number,
                sub.record_number,
                sub.record_length,
                &registers[..sub.record_length as usize],
            ) {
                server_log_debug!(
                    "FC15: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteFileRecord,
                    err,
                );
                return;
            }
        }

        let pdu_data = &message.pdu.data()[..message.pdu.data_len() as usize];
        let response = match build_file_record_write_echo_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            pdu_data,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteFileRecord,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }
}
