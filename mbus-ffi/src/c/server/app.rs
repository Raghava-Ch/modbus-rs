//! `CServerApp` — the Rust-side bridge between `mbus-server` and C callbacks.
//!
//! `CServerApp` implements all eight server handler traits by dispatching each
//! incoming Modbus request to the corresponding C function pointer in
//! [`MbusServerHandlers`].  When a slot is `None` (NULL in C), the default
//! trait behaviour applies: the server returns `IllegalFunction`.
//!
//! ## Exception mapping
//!
//! The `mbus-server` runtime converts `MbusError` → `ExceptionCode` via
//! `FunctionCode::exception_code_for_error`. The mapping used here:
//!
//! | `MbusServerExceptionCode` | `MbusError` returned  | Resulting Modbus exception  |
//! |---------------------------|-----------------------|-----------------------------|
//! | `Ok (0)`                  | `Ok(data)`            | no exception                |
//! | `IllegalFunction (1)`     | `InvalidFunctionCode` | `0x01` IllegalFunction      |
//! | `IllegalDataAddress (2)`  | `InvalidAddress`      | `0x02` IllegalDataAddress   |
//! | `IllegalDataValue (3)`    | `InvalidValue`        | `0x03` IllegalDataValue     |
//! | `ServerDeviceFailure (4)` | `Unexpected`          | `0x04` ServerDeviceFailure  |

use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::DiagnosticSubFunction;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{
    ServerCoilHandler, ServerDiagnosticsHandler, ServerDiscreteInputHandler,
    ServerExceptionHandler, ServerFifoHandler, ServerFileRecordHandler,
    ServerHoldingRegisterHandler, ServerInputRegisterHandler,
};

use super::callbacks::*;

// ── Exception mapping helper ──────────────────────────────────────────────────

/// Maps a C exception code to the `MbusError` that produces the correct Modbus
/// exception response when processed by `FunctionCode::exception_code_for_error`.
#[inline(always)]
fn exception_to_error(exc: MbusServerExceptionCode) -> MbusError {
    match exc {
        // Ok is never expected here — callers only pass non-Ok values.
        MbusServerExceptionCode::Ok => MbusError::Unexpected,
        MbusServerExceptionCode::IllegalFunction => MbusError::InvalidFunctionCode,
        MbusServerExceptionCode::IllegalDataAddress => MbusError::InvalidAddress,
        MbusServerExceptionCode::IllegalDataValue => MbusError::InvalidValue,
        MbusServerExceptionCode::ServerDeviceFailure => MbusError::Unexpected,
    }
}

// ── CServerApp ────────────────────────────────────────────────────────────────

/// Modbus server application backed by C function-pointer callbacks.
///
/// Implements all eight `mbus-server` handler traits. Each trait method checks
/// whether the corresponding callback slot is populated; if not, it returns
/// `InvalidFunctionCode` which the server converts to `IllegalFunction`.
///
/// # Safety
///
/// `CServerApp` stores a `MbusServerHandlers` which may contain raw pointers/fn
/// ptrs. It is the caller's responsibility to ensure:
/// - All registered callbacks are valid function pointers for the lifetime of
///   the server.
/// - The `userdata` pointer (if non-null) outlives the server.
/// - No callback re-enters the server's poll loop.
pub struct CServerApp {
    pub(super) handlers: MbusServerHandlers,
}

impl CServerApp {
    pub(super) fn new(handlers: MbusServerHandlers) -> Self {
        Self { handlers }
    }
}

// ── ServerExceptionHandler — default no-op ────────────────────────────────────

impl ServerExceptionHandler for CServerApp {}

// ── ServerCoilHandler ─────────────────────────────────────────────────────────

impl ServerCoilHandler for CServerApp {
    fn read_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_coils else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadCoilsReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            quantity,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }

    fn write_single_coil_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_write_single_coil else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerWriteSingleCoilReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            value,
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }

    fn write_multiple_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_write_multiple_coils else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerWriteMultipleCoilsReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            starting_address,
            quantity,
            values: values.as_ptr(),
            values_len: values.len(),
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }
}

// ── ServerDiscreteInputHandler ────────────────────────────────────────────────

impl ServerDiscreteInputHandler for CServerApp {
    fn read_discrete_inputs_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_discrete_inputs else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadDiscreteInputsReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            quantity,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }
}

// ── ServerHoldingRegisterHandler ──────────────────────────────────────────────

impl ServerHoldingRegisterHandler for CServerApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_holding_registers else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadHoldingRegistersReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            quantity,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }

    fn write_single_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_write_single_register else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerWriteSingleRegisterReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            value,
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }

    fn write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_write_multiple_registers else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerWriteMultipleRegistersReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            starting_address,
            values: values.as_ptr(),
            values_len: values.len(),
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }

    fn mask_write_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_mask_write_register else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerMaskWriteRegisterReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            and_mask,
            or_mask,
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn read_write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_write_multiple_registers else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadWriteMultipleRegistersReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            read_address,
            read_quantity,
            write_address,
            write_values: write_values.as_ptr(),
            write_values_len: write_values.len(),
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }
}

// ── ServerInputRegisterHandler ────────────────────────────────────────────────

impl ServerInputRegisterHandler for CServerApp {
    fn read_multiple_input_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_input_registers else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadInputRegistersReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            address,
            quantity,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }
}

// ── ServerFifoHandler ─────────────────────────────────────────────────────────

impl ServerFifoHandler for CServerApp {
    fn read_fifo_queue_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_fifo_queue else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadFifoQueueReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            pointer_address,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }
}

// ── ServerFileRecordHandler ───────────────────────────────────────────────────

impl ServerFileRecordHandler for CServerApp {
    fn read_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_file_record else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadFileRecordReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            file_number,
            record_number,
            record_length,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_byte_count)
    }

    fn write_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: &[u16],
    ) -> Result<(), MbusError> {
        let Some(cb) = self.handlers.on_write_file_record else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let req = MbusServerWriteFileRecordReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            file_number,
            record_number,
            record_length,
            record_data: record_data.as_ptr(),
            record_data_len: record_data.len(),
        };
        let exc = unsafe { cb(&req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(())
    }
}

// ── ServerDiagnosticsHandler ──────────────────────────────────────────────────

impl ServerDiagnosticsHandler for CServerApp {
    fn read_exception_status_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        let Some(cb) = self.handlers.on_read_exception_status else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadExceptionStatusReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            out_status: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_status)
    }

    fn diagnostics_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: u16,
    ) -> Result<u16, MbusError> {
        let Some(cb) = self.handlers.on_diagnostics else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerDiagnosticsReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            sub_function: sub_function as u16,
            data,
            out_result: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok(req.out_result)
    }

    fn get_comm_event_counter_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(u16, u16), MbusError> {
        let Some(cb) = self.handlers.on_get_comm_event_counter else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerGetCommEventCounterReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            out_status: 0,
            out_event_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok((req.out_status, req.out_event_count))
    }

    fn get_comm_event_log_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_events: &mut [u8],
    ) -> Result<(u16, u16, u16, u8), MbusError> {
        let Some(cb) = self.handlers.on_get_comm_event_log else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerGetCommEventLogReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            out_events: out_events.as_mut_ptr(),
            out_events_len: out_events.len(),
            out_status: 0,
            out_event_count: 0,
            out_message_count: 0,
            out_num_events: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok((
            req.out_status,
            req.out_event_count,
            req.out_message_count,
            req.out_num_events,
        ))
    }

    fn report_server_id_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        let Some(cb) = self.handlers.on_report_server_id else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReportServerIdReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            out_server_id: out_server_id.as_mut_ptr(),
            out_server_id_len: out_server_id.len(),
            out_byte_count: 0,
            out_run_indicator_status: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok((req.out_byte_count, req.out_run_indicator_status))
    }

    fn read_device_identification_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        let Some(cb) = self.handlers.on_read_device_identification else {
            return Err(MbusError::InvalidFunctionCode);
        };
        let mut req = MbusServerReadDeviceIdentificationReq {
            unit_id: unit_id_or_slave_addr.get(),
            txn_id,
            read_device_id_code,
            start_object_id,
            out_data: out.as_mut_ptr(),
            out_data_len: out.len(),
            out_conformity_level: 0,
            out_more_follows_object_id: 0,
            out_has_more: false,
            out_next_object_id: 0,
            out_byte_count: 0,
        };
        let exc = unsafe { cb(&mut req, self.handlers.userdata) };
        if exc != MbusServerExceptionCode::Ok {
            return Err(exception_to_error(exc));
        }
        Ok((
            req.out_conformity_level,
            req.out_more_follows_object_id,
            req.out_has_more,
            req.out_next_object_id,
        ))
    }
}

// ── TrafficNotifier (no-op when server-traffic feature is active) ─────────────

#[cfg(feature = "server-traffic")]
impl mbus_server::TrafficNotifier for CServerApp {}
