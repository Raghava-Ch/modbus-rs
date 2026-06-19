//! Command enum representing requests sent from WasmModbusClient to WasmClientTask.

use super::response::WasmResponse;
use futures_channel::oneshot::Sender as OneshotSender;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "file-record")]
use mbus_client::services::file_record::SubRequestParams;

pub(crate) enum WasmCommand {
    #[allow(dead_code)]
    ReadCoils {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadDiscreteInputs {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadHoldingRegisters {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadInputRegisters {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    WriteSingleCoil {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    WriteSingleRegister {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    WriteMultipleCoils {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        values: Vec<bool>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    WriteMultipleRegisters {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        values: Vec<u16>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadWriteMultipleRegisters {
        unit_id: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: Vec<u16>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadFifoQueue {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    MaskWriteRegister {
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[cfg(feature = "file-record")]
    ReadFileRecord {
        unit_id: UnitIdOrSlaveAddr,
        requests: Vec<SubRequestParams>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[cfg(feature = "file-record")]
    WriteFileRecord {
        unit_id: UnitIdOrSlaveAddr,
        requests: Vec<SubRequestParams>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[allow(dead_code)]
    ReadExceptionStatus {
        unit_id: UnitIdOrSlaveAddr,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        unit_id: UnitIdOrSlaveAddr,
        sub_function: u16,
        data: Vec<u16>,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
    #[cfg(feature = "diagnostics")]
    ReadDeviceIdentification {
        unit_id: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        object_id: u8,
        resp: OneshotSender<Result<WasmResponse, String>>,
    },
}
