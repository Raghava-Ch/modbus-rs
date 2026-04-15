//! # Modbus Diagnostics Service (server-side)
//!
//! Handles diagnostics-related function codes:
//! - **FC07 (Read Exception Status)**: Returns device-specific 8-bit exception status
//! - **FC08 (Diagnostics)**: Comprehensive diagnostic function with multiple sub-functions.
//!   When the `diagnostics-stats` feature is enabled, the server automatically tracks
//!   and returns protocol-level statistics (frame counts, error counts, overrun flags).

use heapless::Vec;
use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::{DiagnosticSubFunction, FunctionCode};
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use super::framing::{
    build_byte_count_prefixed_response, build_diagnostics_response, build_single_byte_response,
    build_two_u16_response, parse_diagnostics_request, parse_empty_request,
};
use crate::app::ModbusAppHandler;
use crate::services::{ServerServices, server_log_debug};

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC07 (Read Exception Status).
    ///
    /// Request payload must be empty. Response payload is one status byte where
    /// each bit is device-specific.
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_read_exception_status_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        // FC07 is a serial-line-only function code (Modbus spec §6.7).
        if !TRANSPORT::TRANSPORT_TYPE.is_serial_type() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadExceptionStatus,
                mbus_core::errors::MbusError::InvalidFunctionCode,
            );
            return;
        }

        if let Err(err) = parse_empty_request(message) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadExceptionStatus,
                err,
            );
            return;
        }

        let status = match self
            .app
            .read_exception_status_request(txn_id, unit_id_or_slave_addr)
        {
            Ok(v) => v,
            Err(err) => {
                server_log_debug!(
                    "FC07: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadExceptionStatus,
                    err,
                );
                return;
            }
        };

        let response = match build_single_byte_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::ReadExceptionStatus,
            status,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadExceptionStatus,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles FC0B (Get Comm Event Counter).
    ///
    /// Serial line only (Modbus spec §6.9).
    /// Request payload must be empty.
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_get_comm_event_counter_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        if !TRANSPORT::TRANSPORT_TYPE.is_serial_type() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::GetCommEventCounter,
                MbusError::InvalidFunctionCode,
            );
            return;
        }

        if let Err(err) = parse_empty_request(message) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::GetCommEventCounter,
                err,
            );
            return;
        }

        let default_status_word = 0x8000;
        let default_event_count = self.comm_event_counter;

        let (status_word, event_count) = match self
            .app
            .get_comm_event_counter_request(txn_id, unit_id_or_slave_addr)
        {
            Ok(values) => values,
            Err(MbusError::InvalidFunctionCode) => (default_status_word, default_event_count),
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::GetCommEventCounter,
                    err,
                );
                return;
            }
        };

        let response = match build_two_u16_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::GetCommEventCounter,
            status_word,
            event_count,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::GetCommEventCounter,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles FC0C (Get Comm Event Log).
    ///
    /// Serial line only (Modbus spec §6.10).
    /// Request payload must be empty.
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_get_comm_event_log_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        if !TRANSPORT::TRANSPORT_TYPE.is_serial_type() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::GetCommEventLog,
                MbusError::InvalidFunctionCode,
            );
            return;
        }

        if let Err(err) = parse_empty_request(message) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::GetCommEventLog,
                err,
            );
            return;
        }

        let mut default_events = [0u8; 64];
        let mut default_event_len: u8 = 0;
        for event in self.comm_event_log.iter() {
            default_events[default_event_len as usize] = *event;
            default_event_len = default_event_len.saturating_add(1);
        }

        let default_status_word = 0x8000;
        let default_event_count = self.comm_event_counter;
        let default_message_count = self.comm_message_count;

        let mut app_events = [0u8; 64];
        let (status_word, event_count, message_count, events_slice) = match self
            .app
            .get_comm_event_log_request(txn_id, unit_id_or_slave_addr, &mut app_events)
        {
            Ok((status, event_count, message_count, event_len)) => {
                let event_len = event_len.min(app_events.len() as u8);
                (
                    status,
                    event_count,
                    message_count,
                    &app_events[..event_len as usize],
                )
            }
            Err(MbusError::InvalidFunctionCode) => (
                default_status_word,
                default_event_count,
                default_message_count,
                &default_events[..default_event_len as usize],
            ),
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::GetCommEventLog,
                    err,
                );
                return;
            }
        };

        let mut payload: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        if payload
            .extend_from_slice(&status_word.to_be_bytes())
            .and_then(|_| payload.extend_from_slice(&event_count.to_be_bytes()))
            .and_then(|_| payload.extend_from_slice(&message_count.to_be_bytes()))
            .and_then(|_| payload.extend_from_slice(events_slice))
            .is_err()
        {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::GetCommEventLog,
                MbusError::BufferTooSmall,
            );
            return;
        }

        let response = match build_byte_count_prefixed_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::GetCommEventLog,
            payload.as_slice(),
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::GetCommEventLog,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles FC11 (Report Server ID).
    ///
    /// Serial line only (Modbus spec §6.13).
    /// Request payload must be empty.
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_report_server_id_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        if !TRANSPORT::TRANSPORT_TYPE.is_serial_type() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReportServerId,
                MbusError::InvalidFunctionCode,
            );
            return;
        }

        if let Err(err) = parse_empty_request(message) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReportServerId,
                err,
            );
            return;
        }

        let mut server_id_bytes = [0u8; MAX_PDU_DATA_LEN - 2];
        let (server_id_len, run_indicator_status) = match self.app.report_server_id_request(
            txn_id,
            unit_id_or_slave_addr,
            &mut server_id_bytes,
        ) {
            Ok(values) => values,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReportServerId,
                    err,
                );
                return;
            }
        };

        let server_id_len = (server_id_len as usize).min(server_id_bytes.len());
        let mut payload: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        if payload
            .extend_from_slice(&server_id_bytes[..server_id_len])
            .is_err()
            || payload.push(run_indicator_status).is_err()
        {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReportServerId,
                MbusError::BufferTooSmall,
            );
            return;
        }

        let response = match build_byte_count_prefixed_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::ReportServerId,
            payload.as_slice(),
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReportServerId,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles FC08 (Diagnostics).
    ///
    /// Serial line only (Modbus spec §6.8).
    ///
    /// **Built-in sub-functions** (handled by stack):
    /// - 0x0000: Return Query Data (loopback)
    /// - 0x0001: Restart Communications Option (clears listen-only mode)
    /// - 0x0004: Force Listen Only Mode (sets flag, no response)
    /// - 0x000A: Clear Counters and Diagnostic Register (clears statistics, if feature enabled)
    /// - 0x000B-0x000E, 0x0010-0x0012: Return various counters (if `diagnostics-stats` enabled)
    /// - 0x0014: Clear Overrun Counter and Flag (if feature enabled)
    ///
    /// **App-delegated sub-functions** (forward to app callback):
    /// - 0x0002: Return Diagnostic Register (custom device state)
    /// - 0x0003: Change ASCII Input Delimiter (ASCII mode)
    /// - All other unrecognized sub-functions
    #[cfg(feature = "diagnostics")]
    pub(super) fn handle_diagnostics_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        // FC08 is a serial-line-only function code (Modbus spec §6.8).
        if !TRANSPORT::TRANSPORT_TYPE.is_serial_type() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::Diagnostics,
                MbusError::InvalidFunctionCode,
            );
            return;
        }

        let (sub_function_raw, data) = match parse_diagnostics_request(message) {
            Ok(tuple) => tuple,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::Diagnostics,
                    err,
                );
                return;
            }
        };

        let sub_function = match DiagnosticSubFunction::try_from(sub_function_raw) {
            Ok(sf) => sf,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::Diagnostics,
                    err,
                );
                return;
            }
        };

        self.dispatch_diagnostics_sub_function(
            txn_id,
            unit_id_or_slave_addr,
            sub_function,
            sub_function_raw,
            data,
        );
    }

    /// Routes a validated FC08 sub-function to its built-in handler or the app callback.
    #[cfg(feature = "diagnostics")]
    fn dispatch_diagnostics_sub_function(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        sub_function_raw: u16,
        data: u16,
    ) {
        use DiagnosticSubFunction::*;
        match sub_function {
            ReturnQueryData => {
                server_log_debug!("FC08 0x0000: loopback; txn_id={}", txn_id);
                self.send_diagnostics_value_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    sub_function_raw,
                    data,
                );
            }
            RestartCommunicationsOption => {
                self.listen_only_mode = false;
                server_log_debug!("FC08 0x0001: listen-only mode cleared; txn_id={}", txn_id);
                self.send_diagnostics_value_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    sub_function_raw,
                    data,
                );
            }
            ForceListenOnlyMode => {
                self.enable_listen_only_mode(txn_id);
            }
            _ => {
                #[cfg(feature = "diagnostics-stats")]
                if self.try_handle_stats_sub_function(
                    txn_id,
                    unit_id_or_slave_addr,
                    sub_function,
                    sub_function_raw,
                    data,
                ) {
                    return;
                }
                self.forward_diagnostics_to_app(
                    txn_id,
                    unit_id_or_slave_addr,
                    sub_function,
                    sub_function_raw,
                    data,
                );
            }
        }
    }

    /// Handles all `diagnostics-stats`-gated FC08 sub-functions.
    ///
    /// Returns `true` if the sub-function was handled; `false` to fall through to the app.
    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    fn try_handle_stats_sub_function(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        sub_function_raw: u16,
        data: u16,
    ) -> bool {
        use DiagnosticSubFunction::*;
        let counter = match sub_function {
            ClearCountersAndDiagnosticRegister => {
                self.clear_diagnostics_counters(
                    txn_id,
                    unit_id_or_slave_addr,
                    sub_function_raw,
                    data,
                );
                return true;
            }
            ClearOverrunCounterAndFlag => {
                self.clear_overrun_flag(txn_id, unit_id_or_slave_addr, sub_function_raw, data);
                return true;
            }
            ReturnBusMessageCount => self.stats.message_count,
            ReturnBusCommunicationErrorCount => self.stats.comm_error_count,
            ReturnBusExceptionErrorCount => self.stats.exception_error_count,
            ReturnServerMessageCount => self.stats.server_message_count,
            ReturnServerNoResponseCount => self.stats.no_response_count,
            ReturnServerNakCount => self.stats.nak_count,
            ReturnServerBusyCount => self.stats.busy_count,
            ReturnBusCharacterOverrunCount => self.stats.character_overrun_count,
            _ => return false,
        };
        self.send_diagnostics_counter_response(
            txn_id,
            unit_id_or_slave_addr,
            sub_function_raw,
            counter,
        );
        true
    }

    #[cfg(feature = "diagnostics")]
    fn send_diagnostics_value_response(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function_raw: u16,
        value: u16,
    ) {
        let response = match build_diagnostics_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            sub_function_raw,
            value,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::Diagnostics,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    #[cfg(feature = "diagnostics")]
    fn enable_listen_only_mode(&mut self, txn_id: u16) {
        self.listen_only_mode = true;
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_no_response_count();

        server_log_debug!(
            "FC08 0x0004: listen-only mode enabled; txn_id={} (no response sent per spec)",
            txn_id
        );
    }

    #[cfg(feature = "diagnostics")]
    fn forward_diagnostics_to_app(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        sub_function_raw: u16,
        data: u16,
    ) {
        let result =
            match self
                .app
                .diagnostics_request(txn_id, unit_id_or_slave_addr, sub_function, data)
            {
                Ok(value) => value,
                Err(err) => {
                    server_log_debug!(
                        "FC08 0x{:04X}: app callback failed: txn_id={}, error={:?}",
                        sub_function_raw,
                        txn_id,
                        err
                    );
                    self.send_exception_response(
                        txn_id,
                        unit_id_or_slave_addr,
                        FunctionCode::Diagnostics,
                        err,
                    );
                    return;
                }
            };

        self.send_diagnostics_value_response(
            txn_id,
            unit_id_or_slave_addr,
            sub_function_raw,
            result,
        );
    }

    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    fn send_diagnostics_counter_response(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function_raw: u16,
        counter_value: u16,
    ) {
        server_log_debug!(
            "FC08 0x{:04X}: counter={}; txn_id={}",
            sub_function_raw,
            counter_value,
            txn_id
        );
        self.send_diagnostics_value_response(
            txn_id,
            unit_id_or_slave_addr,
            sub_function_raw,
            counter_value,
        );
    }

    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    fn clear_diagnostics_counters(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function_raw: u16,
        data: u16,
    ) {
        self.stats.clear();
        server_log_debug!("FC08 0x000A: statistics cleared; txn_id={}", txn_id);
        self.send_diagnostics_value_response(txn_id, unit_id_or_slave_addr, sub_function_raw, data);
    }

    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    fn clear_overrun_flag(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function_raw: u16,
        data: u16,
    ) {
        self.stats.clear_overrun_flag();
        server_log_debug!("FC08 0x0014: overrun flag cleared; txn_id={}", txn_id);
        self.send_diagnostics_value_response(txn_id, unit_id_or_slave_addr, sub_function_raw, data);
    }
}
