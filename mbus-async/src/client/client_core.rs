//! Core async client handle shared by all transport flavours.
//!
//! [`AsyncClientCore`] is the single place that owns the channel to the
//! background [`ClientTask`] and implements every Modbus request method.
//! Transport-specific client types (`AsyncTcpClient`, `AsyncSerialClient`)
//! store an `AsyncClientCore` as their only field and expose its API
//! transparently via [`std::ops::Deref`].
//!
//! # Architecture
//!
//! ```text
//! AsyncTcpClient / AsyncSerialClient
//!   └── AsyncClientCore   (this module)
//!         ├── mpsc::Sender<TaskCommand>  ──────► ClientTask::run()  (tokio task)
//!         └── watch::Receiver<usize>            (pending-request count)
//! ```
//!
//! Each public async method:
//! 1. Creates a `oneshot` channel.
//! 2. Sends a [`TaskCommand::Request`] (carrying the oneshot sender) over the mpsc channel.
//! 3. `await`s the oneshot receiver for the reply.
//!
//! [`ClientTask`]: crate::client::task::ClientTask

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode};
#[cfg(feature = "discrete-inputs")]
use mbus_core::models::discrete_input::DiscreteInputs;
#[cfg(feature = "fifo")]
use mbus_core::models::fifo_queue::FifoQueue;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::{SubRequest, SubRequestParams};
#[cfg(feature = "registers")]
use mbus_core::models::register::Registers;

use crate::client::command::{ClientRequest, TaskCommand};
use crate::client::response::ClientResponse;
use crate::client::task::PendingCountReceiver;

#[cfg(feature = "traffic")]
use crate::client::notifier::{AsyncClientNotifier, NotifierStore};

use super::AsyncError;
#[cfg(feature = "diagnostics")]
use super::{CommEventLogResponse, DiagnosticsDataResponse};

// ── Core handle ─────────────────────────────────────────────────────────────

/// Shared async client handle.
///
/// Owns the `mpsc::Sender` that drives the background async task and a
/// `watch::Receiver` used for a synchronous `has_pending_requests()` query.
///
/// Dropping this value closes the channel, which causes the background
/// [`ClientTask`] to exit cleanly via its `cmd_rx.recv()` returning `None`.
///
/// [`ClientTask`]: crate::client::task::ClientTask
pub struct AsyncClientCore {
    cmd_tx: mpsc::Sender<TaskCommand>,
    pending_count_rx: PendingCountReceiver,
    /// Per-request timeout in nanoseconds; 0 = disabled.
    request_timeout_ns: Arc<AtomicU64>,
    #[cfg(feature = "traffic")]
    notifier: NotifierStore,
}

impl AsyncClientCore {
    /// Creates a new core handle wired to an already-spawned [`ClientTask`].
    ///
    /// [`ClientTask`]: crate::client::task::ClientTask
    pub(super) fn new(
        cmd_tx: mpsc::Sender<TaskCommand>,
        pending_count_rx: PendingCountReceiver,
        #[cfg(feature = "traffic")] notifier: NotifierStore,
    ) -> Self {
        Self {
            cmd_tx,
            pending_count_rx,
            request_timeout_ns: Arc::new(AtomicU64::new(0)),
            #[cfg(feature = "traffic")]
            notifier,
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Sends a [`ClientRequest`] to the background task and awaits the reply.
    ///
    /// If a per-request timeout is set via [`set_request_timeout`](Self::set_request_timeout)
    /// and no response arrives within that deadline, returns [`AsyncError::Timeout`].
    async fn send_request(&self, params: ClientRequest) -> Result<ClientResponse, AsyncError> {
        let (resp_tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TaskCommand::Request { params, resp_tx })
            .await
            .map_err(|_| AsyncError::WorkerClosed)?;

        let timeout_ns = self.request_timeout_ns.load(Ordering::Relaxed);
        if timeout_ns > 0 {
            let outcome = tokio::time::timeout(Duration::from_nanos(timeout_ns), rx).await;
            if outcome.is_err() {
                // Transport may be hung.  Send a non-blocking Disconnect so the
                // background task drains the pipeline and closes the transport;
                // the caller can then call connect() to recover.
                let _ = self.cmd_tx.try_send(TaskCommand::Disconnect);
                return Err(AsyncError::Timeout);
            }
            outcome
                .unwrap()
                .map_err(|_| AsyncError::WorkerClosed)?
                .map_err(AsyncError::Mbus)
        } else {
            rx.await
                .map_err(|_| AsyncError::WorkerClosed)?
                .map_err(AsyncError::Mbus)
        }
    }

    // ── Connection ───────────────────────────────────────────────────────

    /// Establishes the underlying transport connection.
    ///
    /// Must be called once before issuing Modbus requests.  Can be called
    /// again after a disconnect to reconnect.
    pub async fn connect(&self) -> Result<(), AsyncError> {
        let (resp_tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(TaskCommand::Connect { resp_tx })
            .await
            .map_err(|_| AsyncError::WorkerClosed)?;
        rx.await
            .map_err(|_| AsyncError::WorkerClosed)?
            .map_err(AsyncError::Mbus)
    }

    /// Returns `true` when there are requests in-flight awaiting a response.
    ///
    /// This is a **synchronous** check — no `.await` required.
    pub fn has_pending_requests(&self) -> bool {
        *self.pending_count_rx.borrow() > 0
    }
    // ── Request timeout ──────────────────────────────────────────────────────────

    /// Sets a per-request deadline applied to every subsequent request call.
    ///
    /// If a response is not received within `timeout`, the method returns
    /// [`AsyncError::Timeout`].  The in-flight entry remains in the background
    /// task until the transport delivers or errors; calling
    /// [`connect`](Self::connect) resets transport state.
    ///
    /// The timeout can be updated at any time and takes effect on the next
    /// request.  Call [`clear_request_timeout`](Self::clear_request_timeout) to
    /// remove it.
    pub fn set_request_timeout(&self, timeout: Duration) {
        self.request_timeout_ns.store(
            u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    /// Removes the per-request timeout set by
    /// [`set_request_timeout`](Self::set_request_timeout), allowing requests to
    /// wait indefinitely for a server response.
    pub fn clear_request_timeout(&self) {
        self.request_timeout_ns.store(0, Ordering::Relaxed);
    }
    // ── Traffic notifier ─────────────────────────────────────────────────

    /// Registers (or replaces) an [`AsyncClientNotifier`] for traffic events.
    ///
    /// The notifier is invoked from the background task on every transmitted
    /// and received frame.
    #[cfg(feature = "traffic")]
    pub fn set_traffic_notifier<N: AsyncClientNotifier + Send + 'static>(&self, notifier: N) {
        if let Ok(mut g) = self.notifier.try_lock() {
            *g = Some(Box::new(notifier));
        }
    }

    /// Removes any previously registered traffic notifier.
    #[cfg(feature = "traffic")]
    pub fn clear_traffic_notifier(&self) {
        if let Ok(mut g) = self.notifier.try_lock() {
            *g = None;
        }
    }

    // ── Coil methods ─────────────────────────────────────────────────────

    /// Reads multiple coils (FC 01) from `address` with the given `quantity`.
    ///
    /// Returns the coil values packed into a [`Coils`] object.
    #[cfg(feature = "coils")]
    pub async fn read_multiple_coils(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<Coils, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadMultipleCoils {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::Coils(coils) => Ok(coils),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes a single coil (FC 05) at `address` with the given boolean `value`.
    ///
    /// Returns `(address, value)` echoed back by the server.
    #[cfg(feature = "coils")]
    pub async fn write_single_coil(
        &self,
        unit_id: u8,
        address: u16,
        value: bool,
    ) -> Result<(u16, bool), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::WriteSingleCoil {
                unit,
                address,
                value,
            })
            .await?
        {
            ClientResponse::Coils(coils) => {
                let v = coils.value(coils.from_address()).unwrap_or(false);
                Ok((coils.from_address(), v))
            }
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes multiple coils (FC 15) starting at `address`.
    ///
    /// Returns `(starting_address, quantity)` echoed back by the server.
    #[cfg(feature = "coils")]
    pub async fn write_multiple_coils(
        &self,
        unit_id: u8,
        address: u16,
        coils: &Coils,
    ) -> Result<(u16, u16), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::WriteMultipleCoils {
                unit,
                address,
                coils: coils.clone(),
            })
            .await?
        {
            ClientResponse::Coils(coils) => Ok((coils.from_address(), coils.quantity())),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── Register methods ──────────────────────────────────────────────────

    /// Reads holding registers (FC 03) from `address` with the given `quantity`.
    ///
    /// Returns the register values as a [`Registers`] object.
    #[cfg(feature = "registers")]
    pub async fn read_holding_registers(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<Registers, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadHoldingRegisters {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::Registers(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads input registers (FC 04) from `address` with the given `quantity`.
    ///
    /// Returns the register values as a [`Registers`] object.
    #[cfg(feature = "registers")]
    pub async fn read_input_registers(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<Registers, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadInputRegisters {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::Registers(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes a single holding register (FC 06) at `address` with `value`.
    ///
    /// Returns `(address, value)` echoed back by the server.
    #[cfg(feature = "registers")]
    pub async fn write_single_register(
        &self,
        unit_id: u8,
        address: u16,
        value: u16,
    ) -> Result<(u16, u16), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::WriteSingleRegister {
                unit,
                address,
                value,
            })
            .await?
        {
            ClientResponse::SingleRegisterWrite { address, value } => Ok((address, value)),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes multiple holding registers (FC 16) starting at `address`.
    ///
    /// Returns `(starting_address, quantity)` echoed back by the server.
    #[cfg(feature = "registers")]
    pub async fn write_multiple_registers(
        &self,
        unit_id: u8,
        address: u16,
        values: &[u16],
    ) -> Result<(u16, u16), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        let hv =
            heapless::Vec::<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::from_slice(
                values,
            )
            .map_err(|_| AsyncError::Mbus(MbusError::BufferTooSmall))?;
        match self
            .send_request(ClientRequest::WriteMultipleRegisters {
                unit,
                address,
                values: hv,
            })
            .await?
        {
            ClientResponse::Registers(regs) => Ok((regs.from_address(), regs.quantity())),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Performs a combined read/write on holding registers (FC 23).
    ///
    /// Reads `read_quantity` registers starting at `read_address` and
    /// simultaneously writes `write_values` starting at `write_address`.
    /// Returns the read registers.
    #[cfg(feature = "registers")]
    pub async fn read_write_multiple_registers(
        &self,
        unit_id: u8,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<Registers, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        let hv =
            heapless::Vec::<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::from_slice(
                write_values,
            )
            .map_err(|_| AsyncError::Mbus(MbusError::BufferTooSmall))?;
        match self
            .send_request(ClientRequest::ReadWriteMultipleRegisters {
                unit,
                read_address,
                read_quantity,
                write_address,
                write_values: hv,
            })
            .await?
        {
            ClientResponse::Registers(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Applies an AND/OR bitmask to a holding register (FC 22).
    ///
    /// The resulting register value is `(current & and_mask) | (or_mask & !and_mask)`.
    #[cfg(feature = "registers")]
    pub async fn mask_write_register(
        &self,
        unit_id: u8,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::MaskWriteRegister {
                unit,
                address,
                and_mask,
                or_mask,
            })
            .await?
        {
            ClientResponse::MaskWriteRegister => Ok(()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── Discrete input methods ────────────────────────────────────────────

    /// Reads discrete inputs (FC 02) from `address` with the given `quantity`.
    ///
    /// Returns the input states as a [`DiscreteInputs`] object.
    #[cfg(feature = "discrete-inputs")]
    pub async fn read_discrete_inputs(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<DiscreteInputs, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadDiscreteInputs {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::DiscreteInputs(di) => Ok(di),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── FIFO methods ──────────────────────────────────────────────────────

    /// Reads the FIFO queue (FC 24) at `address`.
    ///
    /// Returns up to 31 words from the FIFO queue as a [`FifoQueue`] object.
    #[cfg(feature = "fifo")]
    pub async fn read_fifo_queue(
        &self,
        unit_id: u8,
        address: u16,
    ) -> Result<FifoQueue, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadFifoQueue { unit, address })
            .await?
        {
            ClientResponse::FifoQueue(queue) => Ok(queue),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── File record methods ───────────────────────────────────────────────

    /// Reads a file record (FC 20) described by `sub_request`.
    ///
    /// Returns the sub-request response parameters for each requested record.
    #[cfg(feature = "file-record")]
    pub async fn read_file_record(
        &self,
        unit_id: u8,
        sub_request: &SubRequest,
    ) -> Result<Vec<SubRequestParams>, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadFileRecord {
                unit,
                sub_request: sub_request.clone(),
            })
            .await?
        {
            ClientResponse::FileRecordRead(data) => Ok(data.into_iter().collect()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes a file record (FC 21) described by `sub_request`.
    #[cfg(feature = "file-record")]
    pub async fn write_file_record(
        &self,
        unit_id: u8,
        sub_request: &SubRequest,
    ) -> Result<(), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::WriteFileRecord {
                unit,
                sub_request: sub_request.clone(),
            })
            .await?
        {
            ClientResponse::FileRecordWrite => Ok(()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── Diagnostics methods ───────────────────────────────────────────────

    /// Reads device identification objects (FC 43 / MEI 14).
    #[cfg(feature = "diagnostics")]
    pub async fn read_device_identification(
        &self,
        unit_id: u8,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<DeviceIdentificationResponse, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadDeviceIdentification {
                unit,
                read_device_id_code,
                object_id,
            })
            .await?
        {
            ClientResponse::DeviceIdentification(resp) => Ok(resp),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Sends an encapsulated interface transport request (FC 43).
    ///
    /// Returns the `(mei_type, data)` pair from the server response.
    #[cfg(feature = "diagnostics")]
    pub async fn encapsulated_interface_transport(
        &self,
        unit_id: u8,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) -> Result<(EncapsulatedInterfaceType, Vec<u8>), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        let hv =
            heapless::Vec::<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::from_slice(
                data,
            )
            .map_err(|_| AsyncError::Mbus(MbusError::BufferTooSmall))?;
        match self
            .send_request(ClientRequest::EncapsulatedInterfaceTransport {
                unit,
                mei_type,
                data: hv,
            })
            .await?
        {
            ClientResponse::EncapsulatedInterfaceTransport { mei_type, data } => {
                Ok((mei_type, data.as_slice().to_vec()))
            }
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads the device exception status (FC 07).
    #[cfg(feature = "diagnostics")]
    pub async fn read_exception_status(&self, unit_id: u8) -> Result<u8, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadExceptionStatus { unit })
            .await?
        {
            ClientResponse::ExceptionStatus(status) => Ok(status),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Sends a diagnostics request (FC 08).
    ///
    /// Returns [`DiagnosticsDataResponse`] with echoed `sub_function` and `data`.
    #[cfg(feature = "diagnostics")]
    pub async fn diagnostics(
        &self,
        unit_id: u8,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<DiagnosticsDataResponse, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        let hv =
            heapless::Vec::<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::from_slice(
                data,
            )
            .map_err(|_| AsyncError::Mbus(MbusError::BufferTooSmall))?;
        match self
            .send_request(ClientRequest::Diagnostics {
                unit,
                sub_function,
                data: hv,
            })
            .await?
        {
            ClientResponse::DiagnosticsData { sub_function, data } => Ok(DiagnosticsDataResponse {
                sub_function,
                data: data.as_slice().to_vec(),
            }),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads the communication event counter (FC 11).
    ///
    /// Returns `(status_word, event_count)`.
    #[cfg(feature = "diagnostics")]
    pub async fn get_comm_event_counter(&self, unit_id: u8) -> Result<(u16, u16), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::GetCommEventCounter { unit })
            .await?
        {
            ClientResponse::CommEventCounter {
                status,
                event_count,
            } => Ok((status, event_count)),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads the communication event log (FC 12).
    ///
    /// Returns `(status, event_count, message_count, events)`.
    #[cfg(feature = "diagnostics")]
    pub async fn get_comm_event_log(
        &self,
        unit_id: u8,
    ) -> Result<CommEventLogResponse, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::GetCommEventLog { unit })
            .await?
        {
            ClientResponse::CommEventLog {
                status,
                event_count,
                message_count,
                events,
            } => Ok((
                status,
                event_count,
                message_count,
                events.as_slice().to_vec(),
            )),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Requests the server identifier data (FC 17).
    ///
    /// Returns the raw server ID byte array.
    #[cfg(feature = "diagnostics")]
    pub async fn report_server_id(&self, unit_id: u8) -> Result<Vec<u8>, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReportServerId { unit })
            .await?
        {
            ClientResponse::ReportServerId(data) => Ok(data.as_slice().to_vec()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }
}
