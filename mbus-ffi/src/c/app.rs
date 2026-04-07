use mbus_client::app::RequestErrorNotifier;
use mbus_core::errors::MbusError;
use mbus_core::transport::TimeKeeper;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "discrete-inputs")]
use super::callbacks::MbusReadDiscreteInputsCtx;
#[cfg(feature = "fifo")]
use super::callbacks::MbusReadFifoQueueCtx;
use super::callbacks::{MbusCallbacks, MbusRequestFailedCtx};
#[cfg(feature = "diagnostics")]
use super::callbacks::{
    MbusCommEventCounterCtx, MbusCommEventLogCtx, MbusDiagnosticsCtx, MbusReadDeviceIdCtx,
    MbusReadExceptionStatusCtx, MbusReportServerIdCtx,
};
#[cfg(feature = "file-record")]
use super::callbacks::{MbusFileRecordResult, MbusReadFileRecordCtx, MbusWriteFileRecordCtx};
#[cfg(feature = "registers")]
use super::callbacks::{
    MbusMaskWriteRegisterCtx, MbusReadHoldingRegistersCtx, MbusReadInputRegistersCtx,
    MbusReadWriteMultipleRegistersCtx, MbusWriteMultipleRegistersCtx, MbusWriteSingleRegisterCtx,
};
#[cfg(feature = "coils")]
use super::callbacks::{MbusReadCoilsCtx, MbusWriteMultipleCoilsCtx, MbusWriteSingleCoilCtx};
use super::error::MbusStatusCode;
#[cfg(feature = "coils")]
use super::models::coils::MbusCoils;
#[cfg(feature = "discrete-inputs")]
use super::models::discrete_inputs::MbusDiscreteInputs;
#[cfg(feature = "fifo")]
use super::models::fifo::MbusFifoQueue;
#[cfg(feature = "registers")]
use super::models::registers::MbusRegisters;

// ── CApp ──────────────────────────────────────────────────────────────────────

/// Internal App implementation that dispatches Modbus responses to C callbacks.
pub(super) struct CApp {
    pub(super) callbacks: MbusCallbacks,
}

impl CApp {
    pub(super) fn new(callbacks: MbusCallbacks) -> Self {
        Self { callbacks }
    }
}

// ── TimeKeeper ────────────────────────────────────────────────────────────────

impl TimeKeeper for CApp {
    fn current_millis(&self) -> u64 {
        if let Some(cb) = self.callbacks.on_current_millis {
            unsafe { cb(self.callbacks.userdata) }
        } else {
            0
        }
    }
}

// ── RequestErrorNotifier ──────────────────────────────────────────────────────

impl RequestErrorNotifier for CApp {
    fn request_failed(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
    ) {
        if let Some(cb) = self.callbacks.on_request_failed {
            let ctx = MbusRequestFailedCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                error: MbusStatusCode::from(error),
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusRequestFailedCtx);
            }
        }
    }
}

#[cfg(feature = "traffic")]
impl mbus_client::app::TrafficNotifier for CApp {}

// ── CoilResponse ─────────────────────────────────────────────────────────────

#[cfg(feature = "coils")]
impl mbus_client::app::CoilResponse for CApp {
    fn read_coils_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        coils: &mbus_client::services::coil::Coils,
    ) {
        if let Some(cb) = self.callbacks.on_read_coils {
            let opaque_coils = MbusCoils(coils.clone());
            let ctx = MbusReadCoilsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                coils: &opaque_coils as *const MbusCoils,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadCoilsCtx);
            }
        }
    }

    fn read_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: bool,
    ) {
        // Single coil: pack into the general read_coils callback with quantity=1
        if let Some(cb) = self.callbacks.on_read_coils {
            let mut coils = mbus_client::services::coil::Coils::new(_address, 1).unwrap();
            coils.set_value(_address, value).unwrap();
            let opaque_coils = MbusCoils(coils);
            let ctx = MbusReadCoilsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                coils: &opaque_coils as *const MbusCoils,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadCoilsCtx);
            }
        }
    }

    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        if let Some(cb) = self.callbacks.on_write_single_coil {
            let ctx = MbusWriteSingleCoilCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                address,
                value: if value { 1 } else { 0 },
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusWriteSingleCoilCtx);
            }
        }
    }

    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        if let Some(cb) = self.callbacks.on_write_multiple_coils {
            let ctx = MbusWriteMultipleCoilsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                address,
                quantity,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusWriteMultipleCoilsCtx);
            }
        }
    }
}

// ── RegisterResponse ──────────────────────────────────────────────────────────

#[cfg(feature = "registers")]
impl mbus_client::app::RegisterResponse for CApp {
    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &mbus_client::services::register::Registers,
    ) {
        if let Some(cb) = self.callbacks.on_read_holding_registers {
            let opaque_registers = MbusRegisters(registers.clone());
            let ctx = MbusReadHoldingRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadHoldingRegistersCtx);
            }
        }
    }

    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        _value: u16,
    ) {
        if let Some(cb) = self.callbacks.on_read_holding_registers {
            let registers = mbus_client::services::register::Registers::new(address, 1).unwrap();
            let opaque_registers = MbusRegisters(registers);
            let ctx = MbusReadHoldingRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadHoldingRegistersCtx);
            }
        }
    }

    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &mbus_client::services::register::Registers,
    ) {
        if let Some(cb) = self.callbacks.on_read_input_registers {
            let opaque_registers = MbusRegisters(registers.clone());
            let ctx = MbusReadInputRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadInputRegistersCtx);
            }
        }
    }

    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        _value: u16,
    ) {
        if let Some(cb) = self.callbacks.on_read_input_registers {
            let registers = mbus_client::services::register::Registers::new(address, 1).unwrap();
            let opaque_registers = MbusRegisters(registers);
            let ctx = MbusReadInputRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadInputRegistersCtx);
            }
        }
    }

    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        if let Some(cb) = self.callbacks.on_write_single_register {
            let ctx = MbusWriteSingleRegisterCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                address,
                value,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusWriteSingleRegisterCtx);
            }
        }
    }

    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
    ) {
        if let Some(cb) = self.callbacks.on_write_multiple_registers {
            let ctx = MbusWriteMultipleRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                address: starting_address,
                quantity,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusWriteMultipleRegistersCtx);
            }
        }
    }

    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &mbus_client::services::register::Registers,
    ) {
        if let Some(cb) = self.callbacks.on_read_write_multiple_registers {
            let opaque_registers = MbusRegisters(registers.clone());
            let ctx = MbusReadWriteMultipleRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadWriteMultipleRegistersCtx);
            }
        }
    }

    fn read_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        _value: u16,
    ) {
        // Route single-read responses through the generic holding register callback
        if let Some(cb) = self.callbacks.on_read_holding_registers {
            let registers = mbus_client::services::register::Registers::new(address, 1).unwrap();
            let opaque_registers = MbusRegisters(registers);
            let ctx = MbusReadHoldingRegistersCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                registers: &opaque_registers as *const MbusRegisters,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadHoldingRegistersCtx);
            }
        }
    }

    fn mask_write_register_response(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr) {
        if let Some(cb) = self.callbacks.on_mask_write_register {
            let ctx = MbusMaskWriteRegisterCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusMaskWriteRegisterCtx);
            }
        }
    }
}

// ── DiscreteInputResponse ─────────────────────────────────────────────────────

#[cfg(feature = "discrete-inputs")]
impl mbus_client::app::DiscreteInputResponse for CApp {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        discrete_inputs: &mbus_client::services::discrete_input::DiscreteInputs,
    ) {
        if let Some(cb) = self.callbacks.on_read_discrete_inputs {
            let opaque_discrete_inputs = MbusDiscreteInputs::new(discrete_inputs.clone());
            let ctx = MbusReadDiscreteInputsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                discrete_inputs: &opaque_discrete_inputs as *const MbusDiscreteInputs,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadDiscreteInputsCtx);
            }
        }
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        if let Some(cb) = self.callbacks.on_read_discrete_inputs {
            let discrete_inputs =
                mbus_client::services::discrete_input::DiscreteInputs::new(address, 1).unwrap();
            let byte_val: u8 = if value { 1 } else { 0 };
            let discrete_inputs = discrete_inputs.with_values(&[byte_val], 1).unwrap();
            let opaque_discrete_inputs = MbusDiscreteInputs::new(discrete_inputs);
            let ctx = MbusReadDiscreteInputsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                discrete_inputs: &opaque_discrete_inputs as *const MbusDiscreteInputs,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadDiscreteInputsCtx);
            }
        }
    }
}

// ── FifoQueueResponse ─────────────────────────────────────────────────────────

#[cfg(feature = "fifo")]
impl mbus_client::app::FifoQueueResponse for CApp {
    fn read_fifo_queue_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        fifo_queue: &mbus_client::services::fifo_queue::FifoQueue,
    ) {
        if let Some(cb) = self.callbacks.on_read_fifo_queue {
            let opaque_fifo = MbusFifoQueue::new(fifo_queue.clone());
            let ctx = MbusReadFifoQueueCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                fifo_queue: &opaque_fifo as *const MbusFifoQueue,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadFifoQueueCtx);
            }
        }
    }
}

// ── FileRecordResponse ────────────────────────────────────────────────────────

#[cfg(feature = "file-record")]
impl mbus_client::app::FileRecordResponse for CApp {
    fn read_file_record_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[mbus_core::models::file_record::SubRequestParams],
    ) {
        if let Some(cb) = self.callbacks.on_read_file_record {
            // Build a stack-allocated array of view structs pointing into `data`.
            // Stack size: 35 * 16 bytes = 560 bytes — acceptable.
            let mut results: [MbusFileRecordResult;
                mbus_core::models::file_record::MAX_SUB_REQUESTS_PER_PDU] =
                core::array::from_fn(|_| MbusFileRecordResult {
                    record_number: 0,
                    data: core::ptr::null(),
                    data_len: 0,
                });

            let count = data.len().min(results.len());
            for (i, sub) in data[..count].iter().enumerate() {
                results[i].record_number = sub.record_number;
                if let Some(rd) = &sub.record_data {
                    results[i].data = rd.as_ptr();
                    results[i].data_len = rd.len() as u16;
                }
            }

            let ctx = MbusReadFileRecordCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                results: results.as_ptr(),
                count: count as u16,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadFileRecordCtx);
            }
        }
    }

    fn write_file_record_response(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr) {
        if let Some(cb) = self.callbacks.on_write_file_record {
            let ctx = MbusWriteFileRecordCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusWriteFileRecordCtx);
            }
        }
    }
}

// ── DiagnosticsResponse ───────────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
impl mbus_client::app::DiagnosticsResponse for CApp {
    fn read_device_identification_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        response: &mbus_client::services::diagnostic::DeviceIdentificationResponse,
    ) {
        if let Some(cb) = self.callbacks.on_read_device_id {
            let len = response.objects_data.len() as u16;
            let ctx = MbusReadDeviceIdCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                read_device_id_code: response.read_device_id_code as u8,
                conformity_level: response.conformity_level as u8,
                more_follows: if response.more_follows { 1 } else { 0 },
                objects: response.objects_data.as_ptr(),
                objects_len: len,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadDeviceIdCtx);
            }
        }
    }

    fn encapsulated_interface_transport_response(
        &mut self,
        _txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _mei_type: mbus_core::function_codes::public::EncapsulatedInterfaceType,
        _data: &[u8],
    ) {
        // Not exposed as a dedicated C callback; callers using EIT directly will
        // receive device_id results through on_read_device_id when MEI type is 0x0E.
    }

    fn read_exception_status_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u8,
    ) {
        if let Some(cb) = self.callbacks.on_read_exception_status {
            let ctx = MbusReadExceptionStatusCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                status,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReadExceptionStatusCtx);
            }
        }
    }

    fn diagnostics_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_function: mbus_core::function_codes::public::DiagnosticSubFunction,
        data: &[u16],
    ) {
        if let Some(cb) = self.callbacks.on_diagnostics {
            let ctx = MbusDiagnosticsCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                sub_fn: u16::from(sub_function),
                data: data.as_ptr(),
                data_len: data.len() as u16,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusDiagnosticsCtx);
            }
        }
    }

    fn get_comm_event_counter_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
    ) {
        if let Some(cb) = self.callbacks.on_comm_event_counter {
            let ctx = MbusCommEventCounterCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                status,
                event_count,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusCommEventCounterCtx);
            }
        }
    }

    fn get_comm_event_log_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
        message_count: u16,
        events: &[u8],
    ) {
        if let Some(cb) = self.callbacks.on_comm_event_log {
            let ctx = MbusCommEventLogCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                status,
                event_count,
                message_count,
                events: events.as_ptr(),
                events_len: events.len() as u16,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusCommEventLogCtx);
            }
        }
    }

    fn report_server_id_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[u8],
    ) {
        if let Some(cb) = self.callbacks.on_report_server_id {
            // Modbus spec: byte 0 = server ID, byte 1 = run indicator, rest = device info.
            let server_id = data.first().copied().unwrap_or(0);
            let run_indicator: u8 = data.get(1).copied().unwrap_or(0);
            let device_identifier = if data.len() > 2 { &data[2..] } else { &[] };
            let ctx = MbusReportServerIdCtx {
                txn_id,
                unit_id: unit_id_slave_addr.get(),
                server_id,
                run_indicator,
                device_identifier: device_identifier.as_ptr(),
                identifier_len: device_identifier.len() as u16,
                userdata: self.callbacks.userdata,
            };
            unsafe {
                cb(&ctx as *const MbusReportServerIdCtx);
            }
        }
    }
}
