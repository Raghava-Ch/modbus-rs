//! Core async client handle shared by all transport flavours.
//!
//! [`AsyncClientCore`] is the single place that owns the channel to the
//! background worker thread and implements every Modbus request method.
//! Transport-specific client types (`AsyncTcpClient`, `AsyncSerialClient`)
//! store an `AsyncClientCore` as their only field and expose its API
//! transparently via [`std::ops::Deref`].
//!
//! # Architecture
//!
//! ```text
//! AsyncTcpClient / AsyncSerialClient
//!   └── AsyncClientCore   (this module)
//!         ├── Sender<WorkerCommand>  ──────► background std::thread
//!         │                                    └── run_worker loop
//!         │                                          └── ClientServices (sync)
//!         └── AtomicU16  (monotonic transaction counter)
//! ```
//!
//! Each public async method:
//! 1. Allocates a `oneshot` channel.
//! 2. Sends a [`WorkerCommand`] (carrying the oneshot sender) over the mpsc channel.
//! 3. `await`s the oneshot receiver for the reply.

use super::*;

// ── Core handle ─────────────────────────────────────────────────────────────

/// Shared async client handle.
///
/// Owns the `mpsc::Sender` that drives the background poll worker and a
/// monotonically-incrementing transaction counter used to correlate pending
/// requests.
///
/// Dropping this value sends a [`WorkerCommand::Shutdown`] to the worker
/// thread so it exits cleanly.
pub struct AsyncClientCore {
    sender: Sender<WorkerCommand>,
    next_txn_id: AtomicU16,
    #[cfg(feature = "traffic")]
    traffic_handler: TrafficHandlerStore,
}

impl AsyncClientCore {
    /// Creates a new core handle from the sending half of an already-spawned
    /// worker channel.  The transaction counter starts at 1.
    #[cfg(feature = "traffic")]
    pub(super) fn new(sender: Sender<WorkerCommand>, traffic_handler: TrafficHandlerStore) -> Self {
        Self {
            sender,
            next_txn_id: AtomicU16::new(1),
            #[cfg(feature = "traffic")]
            traffic_handler,
        }
    }

    /// Creates a new core handle from the sending half of an already-spawned
    /// worker channel. The transaction counter starts at 1.
    #[cfg(not(feature = "traffic"))]
    pub(super) fn new(sender: Sender<WorkerCommand>) -> Self {
        Self {
            sender,
            next_txn_id: AtomicU16::new(1),
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Returns the next transaction id (wraps on overflow).
    fn next_txn_id(&self) -> u16 {
        self.next_txn_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Sends a command to the worker and awaits the corresponding response.
    ///
    /// `build` receives the oneshot sender and must return the [`WorkerCommand`]
    /// that wraps it so the worker can resolve the future later.
    async fn request_with<F>(&self, build: F) -> Result<WorkerResponse, AsyncError>
    where
        F: FnOnce(PendingSender) -> WorkerCommand,
    {
        let (sender, receiver) = oneshot::channel();
        let command = build(sender);

        self.sender
            .send(command)
            .map_err(|_| AsyncError::WorkerClosed)?;

        receiver
            .await
            .map_err(|_| AsyncError::WorkerClosed)?
            .map_err(AsyncError::from)
    }

    /// Establishes the underlying transport connection.
    ///
    /// Async client constructors only build the worker and state machine. Call
    /// this method before issuing Modbus requests.
    pub async fn connect(&self) -> Result<(), AsyncError> {
        let response = self
            .request_with(|sender| WorkerCommand::Connect { sender })
            .await?;

        match response {
            WorkerResponse::Ack => Ok(()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Returns `true` when the underlying sync client still has in-flight
    /// requests waiting for response/timeout resolution.
    pub async fn has_pending_requests(&self) -> Result<bool, AsyncError> {
        let response = self
            .request_with(|sender| WorkerCommand::HasPendingRequests { sender })
            .await?;

        match response {
            WorkerResponse::HasPendingRequests(value) => Ok(value),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    #[cfg(feature = "traffic")]
    /// Registers (or replaces) a dedicated traffic-dispatcher callback.
    ///
    /// The callback is invoked from a dedicated dispatcher thread and should
    /// remain lightweight and non-blocking.
    pub fn set_traffic_handler<F>(&self, handler: F)
    where
        F: FnMut(&TrafficEvent) + Send + 'static,
    {
        if let Ok(mut slot) = self.traffic_handler.lock() {
            *slot = Some(Box::new(handler));
        }
    }

    #[cfg(feature = "traffic")]
    /// Removes any previously registered traffic callback.
    pub fn clear_traffic_handler(&self) {
        if let Ok(mut slot) = self.traffic_handler.lock() {
            *slot = None;
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadMultipleCoils {
                txn_id: self.next_txn_id(),
                unit,
                address,
                quantity,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Coils(coils) => Ok(coils),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::WriteSingleCoil {
                txn_id: self.next_txn_id(),
                unit,
                address,
                value,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Coils(coils) => {
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::WriteMultipleCoils {
                txn_id: self.next_txn_id(),
                unit,
                address,
                coils: coils.clone(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Coils(coils) => Ok((coils.from_address(), coils.quantity())),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadHoldingRegisters {
                txn_id: self.next_txn_id(),
                unit,
                address,
                quantity,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Registers(registers) => Ok(registers),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadInputRegisters {
                txn_id: self.next_txn_id(),
                unit,
                address,
                quantity,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Registers(registers) => Ok(registers),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::WriteSingleRegister {
                txn_id: self.next_txn_id(),
                unit,
                address,
                value,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::SingleRegisterWrite { address, value } => Ok((address, value)),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::WriteMultipleRegisters {
                txn_id: self.next_txn_id(),
                unit,
                address,
                values: values.to_vec(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Registers(regs) => Ok((regs.from_address(), regs.quantity())),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadWriteMultipleRegisters {
                txn_id: self.next_txn_id(),
                unit,
                read_address,
                read_quantity,
                write_address,
                write_values: write_values.to_vec(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::Registers(regs) => Ok(regs),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::MaskWriteRegister {
                txn_id: self.next_txn_id(),
                unit,
                address,
                and_mask,
                or_mask,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::MaskWriteRegister => Ok(()),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadDiscreteInputs {
                txn_id: self.next_txn_id(),
                unit,
                address,
                quantity,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::DiscreteInputs(di) => Ok(di),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadFifoQueue {
                txn_id: self.next_txn_id(),
                unit,
                address,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::FifoQueue(queue) => Ok(queue),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadFileRecord {
                txn_id: self.next_txn_id(),
                unit,
                sub_request: sub_request.clone(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::FileRecordRead(data) => Ok(data),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::WriteFileRecord {
                txn_id: self.next_txn_id(),
                unit,
                sub_request: sub_request.clone(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::FileRecordWrite => Ok(()),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    // ── Diagnostics methods ───────────────────────────────────────────────

    /// Reads device identification objects (FC 43 / MEI 14).
    ///
    /// `read_device_id_code` controls the conformity level to query and
    /// `object_id` selects the first object to read.
    #[cfg(feature = "diagnostics")]
    pub async fn read_device_identification(
        &self,
        unit_id: u8,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<DeviceIdentificationResponse, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadDeviceIdentification {
                txn_id: self.next_txn_id(),
                unit,
                read_device_id_code,
                object_id,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::DeviceIdentification(resp) => Ok(resp),
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::EncapsulatedInterfaceTransport {
                txn_id: self.next_txn_id(),
                unit,
                mei_type,
                data: data.to_vec(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::EncapsulatedInterfaceTransport { mei_type, data } => {
                Ok((mei_type, data))
            }
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads the device exception status (FC 07).
    ///
    /// Returns the 8-bit exception status byte.
    #[cfg(feature = "diagnostics")]
    pub async fn read_exception_status(&self, unit_id: u8) -> Result<u8, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReadExceptionStatus {
                txn_id: self.next_txn_id(),
                unit,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::ExceptionStatus(status) => Ok(status),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Sends a diagnostics request (FC 08) with the given `sub_function` and
    /// `data` words.
    ///
    /// Returns [`DiagnosticsDataResponse`] with echoed `sub_function` and `data`.
    #[cfg(feature = "diagnostics")]
    pub async fn diagnostics(
        &self,
        unit_id: u8,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<DiagnosticsDataResponse, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::Diagnostics {
                txn_id: self.next_txn_id(),
                unit,
                sub_function,
                data: data.to_vec(),
                sender,
            })
            .await?;

        match response {
            WorkerResponse::DiagnosticsData(resp) => Ok(resp),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Reads the communication event counter (FC 11).
    ///
    /// Returns `(status_word, event_count)`.
    #[cfg(feature = "diagnostics")]
    pub async fn get_comm_event_counter(&self, unit_id: u8) -> Result<(u16, u16), AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::GetCommEventCounter {
                txn_id: self.next_txn_id(),
                unit,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::CommEventCounter {
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
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::GetCommEventLog {
                txn_id: self.next_txn_id(),
                unit,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::CommEventLog(resp) => Ok(resp),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }

    /// Requests the server identifier data (FC 17).
    ///
    /// Returns the raw server ID byte array.
    #[cfg(feature = "diagnostics")]
    pub async fn report_server_id(&self, unit_id: u8) -> Result<Vec<u8>, AsyncError> {
        let unit = UnitIdOrSlaveAddr::new(unit_id).map_err(AsyncError::from)?;
        let response = self
            .request_with(|sender| WorkerCommand::ReportServerId {
                txn_id: self.next_txn_id(),
                unit,
                sender,
            })
            .await?;

        match response {
            WorkerResponse::ReportServerId(data) => Ok(data),
            _ => Err(AsyncError::UnexpectedResponseType),
        }
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

impl Drop for AsyncClientCore {
    /// Signals the background worker thread to stop.
    ///
    /// The send may fail if the worker already exited (e.g. transport error);
    /// that is silently ignored because the thread is already gone.
    fn drop(&mut self) {
        let _ = self.sender.send(WorkerCommand::Shutdown);
    }
}
