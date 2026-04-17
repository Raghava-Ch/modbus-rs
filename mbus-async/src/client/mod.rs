//! Internal runtime for the async facade.
//!
//! This module contains the worker-thread bridge between async callers and the
//! synchronous `ClientServices` state machine.
//!
//! Public entry points are re-exported from the crate root:
//! - [`AsyncTcpClient`] (TCP)
//! - [`AsyncSerialClient`] (RTU/ASCII)
//!
//! Internal/shared building blocks:
//! - [`AsyncClientCore`] stores worker channel state and implements request methods.
//! - [`WorkerCommand`] and [`WorkerResponse`] carry typed request/response payloads.
//! - [`run_worker`] drives polling and response routing.

use std::collections::HashMap;
#[cfg(feature = "traffic")]
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(feature = "coils")]
use mbus_client::app::CoilResponse;
#[cfg(feature = "diagnostics")]
use mbus_client::app::DiagnosticsResponse;
#[cfg(feature = "discrete-inputs")]
use mbus_client::app::DiscreteInputResponse;
#[cfg(feature = "fifo")]
use mbus_client::app::FifoQueueResponse;
#[cfg(feature = "file-record")]
use mbus_client::app::FileRecordResponse;
#[cfg(feature = "registers")]
use mbus_client::app::RegisterResponse;
use mbus_client::app::RequestErrorNotifier;
#[cfg(feature = "traffic")]
use mbus_client::app::{TrafficDirection, TrafficNotifier};
use mbus_client::services::ClientServices;
#[cfg(feature = "coils")]
use mbus_client::services::coil::Coils;
#[cfg(feature = "diagnostics")]
pub use mbus_client::services::diagnostic::{DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode};
#[cfg(feature = "discrete-inputs")]
use mbus_client::services::discrete_input::DiscreteInputs;
#[cfg(feature = "fifo")]
pub use mbus_client::services::fifo_queue::FifoQueue;
#[cfg(feature = "file-record")]
pub use mbus_client::services::file_record::{SubRequest, SubRequestParams};
#[cfg(feature = "registers")]
use mbus_client::services::register::Registers;
use mbus_core::errors::MbusError;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
#[cfg(feature = "tcp")]
use mbus_core::transport::ModbusTcpConfig;
use mbus_core::transport::{ModbusConfig, TimeKeeper, Transport, UnitIdOrSlaveAddr};
#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
use mbus_core::transport::{ModbusSerialConfig, SerialMode};
#[cfg(feature = "tcp")]
use mbus_network::StdTcpTransport;
#[cfg(feature = "serial-ascii")]
use mbus_serial::StdAsciiTransport;
#[cfg(feature = "serial-rtu")]
use mbus_serial::StdRtuTransport;
#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
use mbus_serial::StdSerialTransport;
use tokio::sync::oneshot;

#[cfg(feature = "diagnostics")]
/// Diagnostics response payload returned by FC 08.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsDataResponse {
    /// Echoed diagnostic sub-function code.
    pub sub_function: DiagnosticSubFunction,
    /// Echoed diagnostic data words.
    pub data: Vec<u16>,
}
#[cfg(feature = "diagnostics")]
/// Communication event log payload `(status, event_count, message_count, events)` returned by FC 12.
pub type CommEventLogResponse = (u16, u16, u16, Vec<u8>);

/// Async facade error type.
#[derive(Debug, PartialEq, Eq)]
pub enum AsyncError {
    /// Error propagated from the underlying Modbus client stack.
    Mbus(MbusError),
    /// Background worker channel is closed or worker thread has stopped.
    WorkerClosed,
    /// Internal response routing mismatch between request and callback payload type.
    UnexpectedResponseType,
}

impl From<MbusError> for AsyncError {
    fn from(value: MbusError) -> Self {
        Self::Mbus(value)
    }
}

impl std::fmt::Display for AsyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mbus(err) => write!(f, "Modbus error: {err}"),
            Self::WorkerClosed => write!(f, "async worker channel closed"),
            Self::UnexpectedResponseType => write!(f, "unexpected response type from worker"),
        }
    }
}

impl std::error::Error for AsyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mbus(err) => Some(err),
            _ => None,
        }
    }
}

type PendingSender = oneshot::Sender<Result<WorkerResponse, MbusError>>;
type PendingStore = Arc<Mutex<HashMap<u16, PendingSender>>>;
#[cfg(feature = "traffic")]
type TrafficHandler = Box<dyn FnMut(&TrafficEvent) + Send + 'static>;
#[cfg(feature = "traffic")]
type TrafficHandlerStore = Arc<Mutex<Option<TrafficHandler>>>;
#[cfg(feature = "traffic")]
type TrafficSender = Sender<TrafficEvent>;

#[cfg(feature = "traffic")]
/// Async traffic event emitted from the worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficEvent {
    /// Outbound or inbound frame direction.
    pub direction: TrafficDirection,
    /// Transaction identifier associated with the request lifecycle.
    pub txn_id: u16,
    /// Unit id (TCP) or slave address (Serial) for this frame.
    pub unit_id_slave_addr: UnitIdOrSlaveAddr,
    /// Raw ADU bytes observed on the wire.
    pub frame: Vec<u8>,
    /// Error details when the traffic event corresponds to a failed TX/RX path.
    pub error: Option<MbusError>,
}

enum WorkerCommand {
    Connect {
        sender: PendingSender,
    },
    HasPendingRequests {
        sender: PendingSender,
    },
    #[cfg(feature = "coils")]
    ReadMultipleCoils {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    ReadHoldingRegisters {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    ReadInputRegisters {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    WriteSingleRegister {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "coils")]
    WriteSingleCoil {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
        sender: PendingSender,
    },
    #[cfg(feature = "coils")]
    WriteMultipleCoils {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        coils: Coils,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    WriteMultipleRegisters {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        values: Vec<u16>,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    ReadWriteMultipleRegisters {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: Vec<u16>,
        sender: PendingSender,
    },
    #[cfg(feature = "registers")]
    MaskWriteRegister {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "discrete-inputs")]
    ReadDiscreteInputs {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "fifo")]
    ReadFifoQueue {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        sender: PendingSender,
    },
    #[cfg(feature = "file-record")]
    ReadFileRecord {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_request: SubRequest,
        sender: PendingSender,
    },
    #[cfg(feature = "file-record")]
    WriteFileRecord {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_request: SubRequest,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    ReadDeviceIdentification {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: Vec<u8>,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    ReadExceptionStatus {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: Vec<u16>,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    GetCommEventCounter {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    GetCommEventLog {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sender: PendingSender,
    },
    #[cfg(feature = "diagnostics")]
    ReportServerId {
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sender: PendingSender,
    },
    Shutdown,
}

enum WorkerResponse {
    Ack,
    HasPendingRequests(bool),
    #[cfg(feature = "coils")]
    Coils(Coils),
    #[cfg(feature = "registers")]
    Registers(Registers),
    #[cfg(feature = "registers")]
    SingleRegisterWrite {
        address: u16,
        value: u16,
    },
    #[cfg(feature = "registers")]
    MaskWriteRegister,
    #[cfg(feature = "discrete-inputs")]
    DiscreteInputs(DiscreteInputs),
    #[cfg(feature = "fifo")]
    FifoQueue(FifoQueue),
    #[cfg(feature = "file-record")]
    FileRecordRead(Vec<SubRequestParams>),
    #[cfg(feature = "file-record")]
    FileRecordWrite,
    #[cfg(feature = "diagnostics")]
    DeviceIdentification(DeviceIdentificationResponse),
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        mei_type: EncapsulatedInterfaceType,
        data: Vec<u8>,
    },
    #[cfg(feature = "diagnostics")]
    ExceptionStatus(u8),
    #[cfg(feature = "diagnostics")]
    DiagnosticsData(DiagnosticsDataResponse),
    #[cfg(feature = "diagnostics")]
    CommEventCounter {
        status: u16,
        event_count: u16,
    },
    #[cfg(feature = "diagnostics")]
    CommEventLog(CommEventLogResponse),
    #[cfg(feature = "diagnostics")]
    ReportServerId(Vec<u8>),
}

struct AsyncApp {
    pending: PendingStore,
    #[cfg(feature = "traffic")]
    traffic_sender: TrafficSender,
}

impl AsyncApp {
    fn complete(&self, txn_id: u16, response: Result<WorkerResponse, MbusError>) {
        if let Ok(mut pending) = self.pending.lock()
            && let Some(sender) = pending.remove(&txn_id)
        {
            let _ = sender.send(response);
        }
    }

    fn resolve(&self, txn_id: u16, response: WorkerResponse) {
        self.complete(txn_id, Ok(response));
    }

    fn reject(&self, txn_id: u16, error: MbusError) {
        self.complete(txn_id, Err(error));
    }

    #[cfg(feature = "traffic")]
    fn emit_traffic(
        &self,
        direction: TrafficDirection,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: Option<MbusError>,
        frame_bytes: &[u8],
    ) {
        let event = TrafficEvent {
            direction,
            txn_id,
            unit_id_slave_addr,
            frame: frame_bytes.to_vec(),
            error,
        };

        let _ = self.traffic_sender.send(event);
    }
}

impl TimeKeeper for AsyncApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl RequestErrorNotifier for AsyncApp {
    fn request_failed(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
    ) {
        self.reject(txn_id, error);
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for AsyncApp {
    fn on_tx_frame(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        frame_bytes: &[u8],
    ) {
        self.emit_traffic(
            TrafficDirection::Tx,
            txn_id,
            unit_id_slave_addr,
            None,
            frame_bytes,
        );
    }

    fn on_rx_frame(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        frame_bytes: &[u8],
    ) {
        self.emit_traffic(
            TrafficDirection::Rx,
            txn_id,
            unit_id_slave_addr,
            None,
            frame_bytes,
        );
    }

    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame_bytes: &[u8],
    ) {
        self.emit_traffic(
            TrafficDirection::Tx,
            txn_id,
            unit_id_slave_addr,
            Some(error),
            frame_bytes,
        );
    }

    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame_bytes: &[u8],
    ) {
        self.emit_traffic(
            TrafficDirection::Rx,
            txn_id,
            unit_id_slave_addr,
            Some(error),
            frame_bytes,
        );
    }
}

#[cfg(feature = "traffic")]
fn run_traffic_dispatcher(receiver: Receiver<TrafficEvent>, traffic_handler: TrafficHandlerStore) {
    while let Ok(event) = receiver.recv() {
        if let Ok(mut handler_slot) = traffic_handler.lock()
            && let Some(handler) = handler_slot.as_mut()
        {
            let _ = catch_unwind(AssertUnwindSafe(|| handler(&event)));
        }
    }
}

#[cfg(feature = "coils")]
impl CoilResponse for AsyncApp {
    fn read_coils_response(&mut self, txn_id: u16, _unit: UnitIdOrSlaveAddr, coils: &Coils) {
        self.resolve(txn_id, WorkerResponse::Coils(coils.clone()));
    }

    fn read_single_coil_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        match Coils::new(address, 1).and_then(|mut c| {
            c.set_value(address, value)?;
            Ok(c)
        }) {
            Ok(coils) => self.resolve(txn_id, WorkerResponse::Coils(coils)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        match Coils::new(address, 1).and_then(|mut c| {
            c.set_value(address, value)?;
            Ok(c)
        }) {
            Ok(coils) => self.resolve(txn_id, WorkerResponse::Coils(coils)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        match Coils::new(address, quantity) {
            Ok(coils) => self.resolve(txn_id, WorkerResponse::Coils(coils)),
            Err(err) => self.reject(txn_id, err),
        }
    }
}

#[cfg(feature = "registers")]
impl RegisterResponse for AsyncApp {
    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        self.resolve(txn_id, WorkerResponse::Registers(registers.clone()));
    }

    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        match Registers::new(address, 1).and_then(|mut r| {
            r.set_value(address, value)?;
            Ok(r)
        }) {
            Ok(registers) => self.resolve(txn_id, WorkerResponse::Registers(registers)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        self.resolve(txn_id, WorkerResponse::Registers(registers.clone()));
    }

    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::SingleRegisterWrite { address, value },
        );
    }

    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
    ) {
        match Registers::new(starting_address, quantity) {
            Ok(registers) => self.resolve(txn_id, WorkerResponse::Registers(registers)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        self.resolve(txn_id, WorkerResponse::Registers(registers.clone()));
    }

    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        match Registers::new(address, 1).and_then(|mut r| {
            r.set_value(address, value)?;
            Ok(r)
        }) {
            Ok(registers) => self.resolve(txn_id, WorkerResponse::Registers(registers)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn read_single_register_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        match Registers::new(address, 1).and_then(|mut r| {
            r.set_value(address, value)?;
            Ok(r)
        }) {
            Ok(registers) => self.resolve(txn_id, WorkerResponse::Registers(registers)),
            Err(err) => self.reject(txn_id, err),
        }
    }

    fn mask_write_register_response(&mut self, txn_id: u16, _unit: UnitIdOrSlaveAddr) {
        self.resolve(txn_id, WorkerResponse::MaskWriteRegister);
    }
}

#[cfg(feature = "discrete-inputs")]
impl DiscreteInputResponse for AsyncApp {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        discrete_inputs: &DiscreteInputs,
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::DiscreteInputs(discrete_inputs.clone()),
        );
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        let bit = if value { 0b0000_0001 } else { 0 };
        match DiscreteInputs::new(address, 1).and_then(|d| d.with_values(&[bit], 1)) {
            Ok(discrete_inputs) => {
                self.resolve(txn_id, WorkerResponse::DiscreteInputs(discrete_inputs))
            }
            Err(err) => self.reject(txn_id, err),
        }
    }
}

#[cfg(feature = "fifo")]
impl FifoQueueResponse for AsyncApp {
    fn read_fifo_queue_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        fifo_queue: &FifoQueue,
    ) {
        self.resolve(txn_id, WorkerResponse::FifoQueue(fifo_queue.clone()));
    }
}

#[cfg(feature = "file-record")]
impl FileRecordResponse for AsyncApp {
    fn read_file_record_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        data: &[SubRequestParams],
    ) {
        self.resolve(txn_id, WorkerResponse::FileRecordRead(data.to_vec()));
    }

    fn write_file_record_response(&mut self, txn_id: u16, _unit: UnitIdOrSlaveAddr) {
        self.resolve(txn_id, WorkerResponse::FileRecordWrite);
    }
}

#[cfg(feature = "diagnostics")]
impl DiagnosticsResponse for AsyncApp {
    fn read_device_identification_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        response: &DeviceIdentificationResponse,
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::DeviceIdentification(response.clone()),
        );
    }

    fn encapsulated_interface_transport_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::EncapsulatedInterfaceTransport {
                mei_type,
                data: data.to_vec(),
            },
        );
    }

    fn read_exception_status_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        status: u8,
    ) {
        self.resolve(txn_id, WorkerResponse::ExceptionStatus(status));
    }

    fn diagnostics_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::DiagnosticsData(DiagnosticsDataResponse {
                sub_function,
                data: data.to_vec(),
            }),
        );
    }

    fn get_comm_event_counter_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::CommEventCounter {
                status,
                event_count,
            },
        );
    }

    fn get_comm_event_log_response(
        &mut self,
        txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
        message_count: u16,
        events: &[u8],
    ) {
        self.resolve(
            txn_id,
            WorkerResponse::CommEventLog((status, event_count, message_count, events.to_vec())),
        );
    }

    fn report_server_id_response(&mut self, txn_id: u16, _unit: UnitIdOrSlaveAddr, data: &[u8]) {
        self.resolve(txn_id, WorkerResponse::ReportServerId(data.to_vec()));
    }
}

fn register_pending(
    pending: &PendingStore,
    txn_id: u16,
    sender: PendingSender,
) -> Result<(), MbusError> {
    let mut guard = pending.lock().map_err(|_| MbusError::Unexpected)?;
    guard.insert(txn_id, sender);
    Ok(())
}

fn reject_pending(pending: &PendingStore, txn_id: u16, error: MbusError) {
    if let Ok(mut guard) = pending.lock()
        && let Some(sender) = guard.remove(&txn_id)
    {
        let _ = sender.send(Err(error));
    }
}

fn submit_or_reject(pending: &PendingStore, txn_id: u16, result: Result<(), MbusError>) {
    if let Err(err) = result {
        reject_pending(pending, txn_id, err);
    }
}

fn handle_command<TRANSPORT, const N: usize>(
    client: &mut ClientServices<TRANSPORT, AsyncApp, N>,
    pending: &PendingStore,
    command: WorkerCommand,
) where
    TRANSPORT: Transport,
{
    match command {
        WorkerCommand::Connect { sender } => {
            let _ = sender.send(client.connect().map(|_| WorkerResponse::Ack));
        }
        WorkerCommand::HasPendingRequests { sender } => {
            let _ = sender.send(Ok(WorkerResponse::HasPendingRequests(
                client.has_pending_requests(),
            )));
        }
        #[cfg(feature = "coils")]
        WorkerCommand::ReadMultipleCoils {
            txn_id,
            unit,
            address,
            quantity,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_multiple_coils(txn_id, unit, address, quantity);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::ReadHoldingRegisters {
            txn_id,
            unit,
            address,
            quantity,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_holding_registers(txn_id, unit, address, quantity);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::ReadInputRegisters {
            txn_id,
            unit,
            address,
            quantity,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_input_registers(txn_id, unit, address, quantity);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::WriteSingleRegister {
            txn_id,
            unit,
            address,
            value,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.write_single_register(txn_id, unit, address, value);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "coils")]
        WorkerCommand::WriteSingleCoil {
            txn_id,
            unit,
            address,
            value,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.write_single_coil(txn_id, unit, address, value);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "coils")]
        WorkerCommand::WriteMultipleCoils {
            txn_id,
            unit,
            address,
            coils,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.write_multiple_coils(txn_id, unit, address, &coils);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::WriteMultipleRegisters {
            txn_id,
            unit,
            address,
            values,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.write_multiple_registers(
                    txn_id,
                    unit,
                    address,
                    values.len() as u16,
                    &values,
                );
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::ReadWriteMultipleRegisters {
            txn_id,
            unit,
            read_address,
            read_quantity,
            write_address,
            write_values,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_write_multiple_registers(
                    txn_id,
                    unit,
                    read_address,
                    read_quantity,
                    write_address,
                    &write_values,
                );
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "registers")]
        WorkerCommand::MaskWriteRegister {
            txn_id,
            unit,
            address,
            and_mask,
            or_mask,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.mask_write_register(txn_id, unit, address, and_mask, or_mask);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "discrete-inputs")]
        WorkerCommand::ReadDiscreteInputs {
            txn_id,
            unit,
            address,
            quantity,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_discrete_inputs(txn_id, unit, address, quantity);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "fifo")]
        WorkerCommand::ReadFifoQueue {
            txn_id,
            unit,
            address,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_fifo_queue(txn_id, unit, address);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "file-record")]
        WorkerCommand::ReadFileRecord {
            txn_id,
            unit,
            sub_request,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_file_record(txn_id, unit, &sub_request);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "file-record")]
        WorkerCommand::WriteFileRecord {
            txn_id,
            unit,
            sub_request,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.write_file_record(txn_id, unit, &sub_request);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::ReadDeviceIdentification {
            txn_id,
            unit,
            read_device_id_code,
            object_id,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result =
                    client.read_device_identification(txn_id, unit, read_device_id_code, object_id);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::EncapsulatedInterfaceTransport {
            txn_id,
            unit,
            mei_type,
            data,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.encapsulated_interface_transport(txn_id, unit, mei_type, &data);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::ReadExceptionStatus {
            txn_id,
            unit,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.read_exception_status(txn_id, unit);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::Diagnostics {
            txn_id,
            unit,
            sub_function,
            data,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.diagnostics(txn_id, unit, sub_function, &data);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::GetCommEventCounter {
            txn_id,
            unit,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.get_comm_event_counter(txn_id, unit);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::GetCommEventLog {
            txn_id,
            unit,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.get_comm_event_log(txn_id, unit);
                submit_or_reject(pending, txn_id, result);
            }
        }
        #[cfg(feature = "diagnostics")]
        WorkerCommand::ReportServerId {
            txn_id,
            unit,
            sender,
        } => {
            if register_pending(pending, txn_id, sender).is_ok() {
                let result = client.report_server_id(txn_id, unit);
                submit_or_reject(pending, txn_id, result);
            }
        }
        WorkerCommand::Shutdown => {}
    }
}

fn run_worker<TRANSPORT, const N: usize>(
    mut client: ClientServices<TRANSPORT, AsyncApp, N>,
    pending: PendingStore,
    receiver: Receiver<WorkerCommand>,
    poll_interval: Duration,
) where
    TRANSPORT: Transport,
{
    loop {
        // Drain all queued commands first so newly enqueued requests are
        // visible before we decide whether to poll or sleep.
        loop {
            match receiver.try_recv() {
                Ok(WorkerCommand::Shutdown) => return,
                Ok(command) => handle_command(&mut client, &pending, command),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        let should_poll = client.is_connected() && client.has_pending_requests();

        if should_poll {
            client.poll();

            // Active mode: sleep up to `poll_interval`, but wake early when a
            // new command arrives.
            match receiver.recv_timeout(poll_interval) {
                Ok(WorkerCommand::Shutdown) => return,
                Ok(command) => handle_command(&mut client, &pending, command),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }

            continue;
        }

        // Idle mode: block until a command arrives (event-driven wakeup).
        match receiver.recv() {
            Ok(WorkerCommand::Shutdown) => return,
            Ok(command) => handle_command(&mut client, &pending, command),
            Err(_) => return,
        }
    }
}

// ── Submodules ───────────────────────────────────────────────────────────────

mod client_core;
mod network_client;
mod serial_client;

pub(crate) use client_core::AsyncClientCore;
pub use network_client::AsyncTcpClient;
pub use serial_client::AsyncSerialClient;

// ── Note: SubRequest, SubRequestParams, FifoQueue, DeviceIdentificationResponse,
// ObjectId, ReadDeviceIdCode are already re-exported via their `pub use` imports above.

#[cfg(all(test, feature = "traffic"))]
mod tests {
    use super::*;

    #[test]
    fn test_async_app_emits_traffic_event_to_channel() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (traffic_sender, traffic_receiver) = mpsc::channel();

        let mut app = AsyncApp {
            pending,
            traffic_sender,
        };

        let unit = UnitIdOrSlaveAddr::new(1).unwrap();
        app.on_tx_frame(42, unit, &[0xAA, 0x55]);

        let event = traffic_receiver
            .recv_timeout(Duration::from_millis(100))
            .unwrap();
        assert_eq!(event.direction, TrafficDirection::Tx);
        assert_eq!(event.txn_id, 42);
        assert_eq!(event.unit_id_slave_addr, unit);
        assert_eq!(event.frame, vec![0xAA, 0x55]);
        assert_eq!(event.error, None);
    }

    #[test]
    fn test_async_app_emits_traffic_error_event_to_channel() {
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let (traffic_sender, traffic_receiver) = mpsc::channel();

        let mut app = AsyncApp {
            pending,
            traffic_sender,
        };

        let unit = UnitIdOrSlaveAddr::new(1).unwrap();
        app.on_rx_error(77, unit, MbusError::ChecksumError, &[0xAB]);

        let event = traffic_receiver
            .recv_timeout(Duration::from_millis(100))
            .unwrap();
        assert_eq!(event.direction, TrafficDirection::Rx);
        assert_eq!(event.txn_id, 77);
        assert_eq!(event.unit_id_slave_addr, unit);
        assert_eq!(event.frame, vec![0xAB]);
        assert_eq!(event.error, Some(MbusError::ChecksumError));
    }

    #[test]
    fn test_async_client_core_set_and_clear_traffic_handler() {
        let (sender, _receiver) = mpsc::channel();
        let traffic_handler: TrafficHandlerStore = Arc::new(Mutex::new(None));
        let core = AsyncClientCore::new(sender, traffic_handler.clone());

        core.set_traffic_handler(|_evt| {});
        assert!(traffic_handler.lock().unwrap().is_some());

        core.clear_traffic_handler();
        assert!(traffic_handler.lock().unwrap().is_none());
    }
}
