//! Core async client handle shared by all transport flavours.
//!
//! [`AsyncClientCore`] is the single place that owns the channel to the
//! background `ClientTask` and implements every Modbus request method.
//! Transport-specific client types (`AsyncTcpClient`, `AsyncSerialClient`)
//! store an `AsyncClientCore` as their only field and expose its API
//! transparently via [`std::ops::Deref`].
//!
//! # Architecture
//!
//! ```text
//! AsyncTcpClient / AsyncSerialClient
//!   └── AsyncClientCore   (this module)
//!         ├── mpsc::Sender<TaskCommand>  ──────► `ClientTask::run()`  (tokio task)
//!         └── watch::Receiver<usize>            (pending-request count)
//! ```
//!
//! Each public async method:
//! 1. Creates a `oneshot` channel.
//! 2. Sends a [`TaskCommand::Request`] (carrying the oneshot sender) over the mpsc channel.
//! 3. `await`s the oneshot receiver for the reply.
//!

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
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
#[cfg(feature = "holding-registers")]
use mbus_core::models::register::HoldingRegisters;
#[cfg(feature = "input-registers")]
use mbus_core::models::register::InputRegisters;

use crate::client::command::{ClientRequest, TaskCommand};
use crate::client::response::ClientResponse;
use crate::client::task::PendingCountReceiver;

#[cfg(feature = "traffic")]
use crate::client::notifier::{AsyncClientTrafficNotifier, NotifierStore};

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
/// `ClientTask` to exit cleanly via its `cmd_rx.recv()` returning `None`.
#[derive(Clone)]
pub struct AsyncClientCore {
    cmd_tx: mpsc::Sender<TaskCommand>,
    pending_count_rx: PendingCountReceiver,
    /// Per-request timeout in nanoseconds; 0 = disabled.
    transport_connected: Arc<std::sync::atomic::AtomicBool>,
    response_timeout_ns: Arc<AtomicU64>,
    queue_timeout_ns: Arc<AtomicU64>,
    /// Number of retry attempts.
    retry_attempts: Arc<AtomicU8>,
    /// Delay between retry attempts in milliseconds.
    retry_delay_ms: Arc<AtomicU64>,
    is_closed: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "traffic")]
    notifier: NotifierStore,
}

impl AsyncClientCore {
    /// Creates a new core handle wired to an already-spawned `ClientTask`.
    pub(super) fn new(
        cmd_tx: mpsc::Sender<TaskCommand>,
        pending_count_rx: PendingCountReceiver,
        transport_connected: Arc<std::sync::atomic::AtomicBool>,
        #[cfg(feature = "traffic")] notifier: NotifierStore,
    ) -> Self {
        Self {
            cmd_tx,
            pending_count_rx,
            transport_connected,
            response_timeout_ns: Arc::new(AtomicU64::new(1_000_000_000)), // 1 second default
            queue_timeout_ns: Arc::new(AtomicU64::new(0)), // 0 = unbounded queue wait by default
            retry_attempts: Arc::new(AtomicU8::new(0)),
            retry_delay_ms: Arc::new(AtomicU64::new(0)),
            is_closed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "traffic")]
            notifier,
        }
    }

    /// Sets the retry options for subsequent requests.
    pub fn set_retry_options(&self, attempts: u8, delay: Duration) {
        self.retry_attempts.store(attempts, Ordering::Relaxed);
        self.retry_delay_ms
            .store(delay.as_millis() as u64, Ordering::Relaxed);
    }

    /// Checks if the client is connected to the transport and the background task is running.
    pub fn is_connected(&self) -> bool {
        self.transport_connected.load(Ordering::Relaxed)
            && !self.is_closed.load(Ordering::Relaxed)
            && !self.cmd_tx.is_closed()
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Sends a [`ClientRequest`] to the background task and awaits the reply.
    ///
    /// The request timeout deadline begins only when the request is dispatched to the wire
    /// by the background task, avoiding premature timeouts while queued.
    async fn send_request(&self, params: ClientRequest) -> Result<ClientResponse, AsyncError> {
        let (resp_tx, rx) = oneshot::channel();
        let retry_attempts = self.retry_attempts.load(Ordering::Relaxed);
        let retry_delay_ms = self.retry_delay_ms.load(Ordering::Relaxed);
        let resp_timeout_ns = self.response_timeout_ns.load(Ordering::Relaxed);
        let response_timeout_ms = resp_timeout_ns / 1_000_000;
        let queue_timeout_ns = self.queue_timeout_ns.load(Ordering::Relaxed);
        let queue_deadline = if queue_timeout_ns > 0 {
            Some(tokio::time::Instant::now() + Duration::from_nanos(queue_timeout_ns))
        } else {
            None
        };

        self.cmd_tx
            .send(TaskCommand::Request {
                params,
                resp_tx,
                retry_attempts,
                retry_delay_ms,
                response_timeout_ms,
                queue_deadline,
            })
            .await
            .map_err(|_| AsyncError::WorkerClosed)?;

        rx.await
            .map_err(|_| AsyncError::WorkerClosed)?
            .map_err(|e| match e {
                MbusError::Timeout => AsyncError::Timeout,
                other => AsyncError::Mbus(other),
            })
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

    /// Disconnects the underlying transport.
    ///
    /// Drains all in-flight and queued requests with
    /// [`MbusError::ConnectionClosed`] and closes the transport.  After this
    /// call, [`connect`](Self::connect) can be called to reconnect.
    ///
    /// This is an explicit, graceful disconnect.  The background task continues
    /// running so the client can be reconnected later.  Dropping the client
    /// handle entirely also stops the background task.
    pub async fn disconnect(&self) -> Result<(), AsyncError> {
        self.cmd_tx
            .send(TaskCommand::Disconnect)
            .await
            .map_err(|_| AsyncError::WorkerClosed)
    }

    /// Permanently shuts down the communication task.
    ///
    /// Drains all pending and queued requests with
    /// [`MbusError::ConnectionClosed`] and terminates the background loop.
    pub async fn shutdown(&self) -> Result<(), AsyncError> {
        self.is_closed.store(true, Ordering::Relaxed);
        self.cmd_tx
            .send(TaskCommand::Shutdown)
            .await
            .map_err(|_| AsyncError::WorkerClosed)
    }

    /// Returns `true` when there are requests in-flight awaiting a response.
    ///
    /// This is a **synchronous** check — no `.await` required.
    pub fn has_pending_requests(&self) -> bool {
        *self.pending_count_rx.borrow() > 0
    }
    // ── Timeout configuration ───────────────────────────────────────────────────

    /// Sets the wire response turnaround timeout applied once a request is dispatched to the wire.
    ///
    /// If a response is not received within `timeout`, the method returns
    /// [`AsyncError::Timeout`].
    pub fn set_response_timeout(&self, timeout: Duration) {
        self.response_timeout_ns.store(
            u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    /// Clears the wire response turnaround timeout.
    pub fn clear_response_timeout(&self) {
        self.response_timeout_ns.store(0, Ordering::Relaxed);
    }

    /// Sets the queue waiting timeout.
    ///
    /// If a request sits in the client queue longer than `timeout` before being
    /// dispatched to the physical wire, it fails early with [`AsyncError::Timeout`]
    /// without being transmitted over the wire.
    pub fn set_queue_timeout(&self, timeout: Duration) {
        self.queue_timeout_ns.store(
            u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    /// Clears the queue waiting timeout, allowing requests to wait in queue until a transmission slot opens.
    pub fn clear_queue_timeout(&self) {
        self.queue_timeout_ns.store(0, Ordering::Relaxed);
    }

    /// Sets a per-request deadline applied to every subsequent request call.
    ///
    /// Alias for [`set_response_timeout`](Self::set_response_timeout).
    pub fn set_request_timeout(&self, timeout: Duration) {
        self.set_response_timeout(timeout);
    }

    /// Removes the per-request response timeout.
    ///
    /// Alias for [`clear_response_timeout`](Self::clear_response_timeout).
    pub fn clear_request_timeout(&self) {
        self.clear_response_timeout();
    }
    // ── Traffic notifier ─────────────────────────────────────────────────

    /// Registers (or replaces) an [`AsyncClientTrafficNotifier`] for traffic events.
    ///
    /// The notifier is invoked from the background task on every transmitted
    /// and received frame.
    #[cfg(feature = "traffic")]
    pub fn set_traffic_notifier<N: AsyncClientTrafficNotifier + Send + 'static>(
        &self,
        notifier: N,
    ) {
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
        #[allow(unreachable_patterns)]
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

    /// Writes a single coil (FC 05) at `address` with the given `CoilState`.
    ///
    /// Returns `(address, CoilState)` echoed back by the server.
    #[cfg(feature = "coils")]
    pub async fn write_single_coil(
        &self,
        unit_id: u8,
        address: u16,
        value: mbus_core::models::coil::CoilState,
    ) -> Result<(u16, mbus_core::models::coil::CoilState), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        #[allow(unreachable_patterns)]
        match self
            .send_request(ClientRequest::WriteSingleCoil {
                unit,
                address,
                value,
            })
            .await?
        {
            ClientResponse::Coils(coils) => {
                let v = coils
                    .value(coils.from_address())
                    .unwrap_or(mbus_core::models::coil::CoilState::Off);
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
        #[allow(unreachable_patterns)]
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
    /// Returns the register values as a [`HoldingRegisters`] object.
    #[cfg(feature = "holding-registers")]
    pub async fn read_holding_registers(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<HoldingRegisters, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadHoldingRegisters {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::HoldingRegisters(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads input registers (FC 04) from `address` with the given `quantity`.
    ///
    /// Returns the register values as an [`InputRegisters`] object.
    #[cfg(feature = "input-registers")]
    pub async fn read_input_registers(
        &self,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<InputRegisters, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::Mbus)?;
        match self
            .send_request(ClientRequest::ReadInputRegisters {
                unit,
                address,
                quantity,
            })
            .await?
        {
            ClientResponse::InputRegisters(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Writes a single holding register (FC 06) at `address` with `value`.
    ///
    /// Returns `(address, value)` echoed back by the server.
    #[cfg(feature = "holding-registers")]
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
    #[cfg(feature = "holding-registers")]
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
            ClientResponse::HoldingRegisters(regs) => Ok((regs.from_address(), regs.quantity())),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Performs a combined read/write on holding registers (FC 23).
    ///
    /// Reads `read_quantity` registers starting at `read_address` and
    /// simultaneously writes `write_values` starting at `write_address`.
    /// Returns the read registers.
    #[cfg(feature = "holding-registers")]
    pub async fn read_write_multiple_registers(
        &self,
        unit_id: u8,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<HoldingRegisters, AsyncError> {
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
            ClientResponse::HoldingRegisters(regs) => Ok(regs),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Applies an AND/OR bitmask to a holding register (FC 22).
    ///
    /// The resulting register value is `(current & and_mask) | (or_mask & !and_mask)`.
    #[cfg(feature = "holding-registers")]
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
