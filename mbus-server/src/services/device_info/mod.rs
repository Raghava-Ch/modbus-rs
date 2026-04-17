//! # Modbus Device Identification Service (server-side)
//!
//! Handles FC 0x2B (Encapsulated Interface Transport), specifically:
//! - **MEI Type 0x0E** — Read Device Identification: returns structured identity
//!   objects (VendorName, ProductCode, MajorMinorRevision, etc.) via the app callback.
//!
//! Works on both TCP and serial transports (unlike the serial-only diagnostics FCs).

use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::{EncapsulatedInterfaceType, FunctionCode};
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use super::framing::{build_fc2b_read_device_id_response, parse_fc2b_request};
use crate::app::ModbusAppHandler;
use crate::services::{ServerServices, server_log_debug};

/// Available bytes in the objects payload of a single MEI 0x0E response PDU.
///
/// PDU max (252) minus the fixed header bytes:
/// MEI_type(1) + code(1) + conformity(1) + more_follows(1) + next_id(1) + n_objects(1) = 6
const MAX_DEVICE_ID_OBJECTS_LEN: usize = MAX_PDU_DATA_LEN - 6;

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC 0x2B (Encapsulated Interface Transport).
    ///
    /// Dispatches to the appropriate MEI-specific handler. Currently supports:
    /// - MEI 0x0E: Read Device Identification
    ///
    /// All other MEI types return an `IllegalFunction` exception.
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_encapsulated_interface_transport_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let mei_payload = match parse_fc2b_request(message) {
            Ok(p) => p,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::EncapsulatedInterfaceTransport,
                    err,
                );
                return;
            }
        };

        match EncapsulatedInterfaceType::try_from(mei_payload.mei_type_byte) {
            Ok(EncapsulatedInterfaceType::ReadDeviceIdentification) => {
                self.handle_read_device_identification(
                    txn_id,
                    unit_id_or_slave_addr,
                    mei_payload.payload,
                );
            }
            _ => {
                server_log_debug!(
                    "FC2B: unsupported or unknown MEI type {:#04x}: txn_id={}",
                    mei_payload.mei_type_byte,
                    txn_id
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::EncapsulatedInterfaceTransport,
                    MbusError::InvalidFunctionCode,
                );
            }
        }
    }

    /// Handles MEI 0x0E (Read Device Identification) sub-request.
    ///
    /// Calls the app callback with a stack-allocated output buffer and then
    /// assembles the response PDU from the returned object triples.
    #[cfg(feature = "diagnostics")]
    fn handle_read_device_identification(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        request_data: &[u8],
    ) {
        // MEI 0x0E request data layout: [read_device_id_code(1), start_object_id(1)]
        if request_data.len() < 2 {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::EncapsulatedInterfaceTransport,
                MbusError::InvalidPduLength,
            );
            return;
        }

        let read_device_id_code = request_data[0];
        let start_object_id = request_data[1];

        // Validate the code byte (0x01–0x04 are the only legal values)
        if mbus_core::models::diagnostic::ReadDeviceIdCode::try_from(read_device_id_code).is_err() {
            server_log_debug!(
                "FC2B/0x0E: invalid device ID code {:#04x}: txn_id={}",
                read_device_id_code,
                txn_id
            );
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::EncapsulatedInterfaceTransport,
                MbusError::InvalidDeviceIdCode,
            );
            return;
        }

        let mut out = [0u8; MAX_DEVICE_ID_OBJECTS_LEN];

        let (bytes_written, conformity_level, more_follows, next_object_id) =
            match self.app.read_device_identification_request(
                txn_id,
                unit_id_or_slave_addr,
                read_device_id_code,
                start_object_id,
                &mut out,
            ) {
                Ok(v) => v,
                Err(err) => {
                    server_log_debug!(
                        "FC2B/0x0E: app callback failed: txn_id={}, error={:?}",
                        txn_id,
                        err
                    );
                    self.send_exception_response(
                        txn_id,
                        unit_id_or_slave_addr,
                        FunctionCode::EncapsulatedInterfaceTransport,
                        err,
                    );
                    return;
                }
            };

        let response = match build_fc2b_read_device_id_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            read_device_id_code,
            conformity_level,
            more_follows,
            next_object_id,
            &out[..bytes_written as usize],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::EncapsulatedInterfaceTransport,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }
}
