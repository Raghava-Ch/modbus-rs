//! # Modbus Client Services Module
//!
//! This module provides the core orchestration logic for a Modbus client. It acts as the
//! bridge between the high-level application logic and the low-level transport protocols.
//!
//! ## Key Components
//! - [`ClientServices`]: The main entry point for sending Modbus requests. It manages
//!   transaction state, handles timeouts, and performs automatic retries.
//! - [`ExpectedResponse`]: A state tracking mechanism that maps outgoing requests to
//!   incoming responses using Transaction IDs (for TCP) or FIFO ordering (for Serial).
//! - Sub-services: Specialized modules (coils, registers, etc.) that handle the
//!   serialization and deserialization of specific Modbus function codes.
//!
//! ## Features
//! - Supports both TCP and Serial (RTU/ASCII) transport types.
//! - Generic over `TRANSPORT` and `APP` traits for maximum flexibility in different environments.
//! - Fixed-capacity response tracking using `heapless` for `no_std` compatibility.

pub mod coil;
pub mod diagnostic;
pub mod discrete_input;
pub mod fifo_queue;
pub mod file_record;
pub mod register;

use crate::app::RequestErrorNotifier;
use diagnostic::ReadDeviceIdCode;
use heapless::Vec;
use mbus_core::data_unit::common::{ModbusMessage, SlaveAddress, derive_length_from_bytes};
use mbus_core::function_codes::public::EncapsulatedInterfaceType;
use mbus_core::transport::{UidSaddrFrom, UnitIdOrSlaveAddr};
use mbus_core::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN},
    errors::MbusError,
    transport::{ModbusConfig, TimeKeeper, Transport, TransportType},
};

type ResponseHandler<T, A, const N: usize> =
    fn(&mut ClientServices<T, A, N>, &ExpectedResponse<T, A, N>, &ModbusMessage);

/// Internal tracking payload for a Single-address operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Single {
    address: u16,
    value: u16,
}
/// Internal tracking payload for a Multiple-address/quantity operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Multiple {
    address: u16,
    quantity: u16,
}
/// Internal tracking payload for a Masking operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mask {
    address: u16,
    and_mask: u16,
    or_mask: u16,
}
/// Internal tracking payload for a Diagnostic/Encapsulated operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Diag {
    device_id_code: ReadDeviceIdCode,
    encap_type: EncapsulatedInterfaceType,
}

/// Metadata required to match responses to requests and properly parse the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OperationMeta {
    Other,
    Single(Single),
    Multiple(Multiple),
    Masking(Mask),
    Diag(Diag),
}

impl OperationMeta {
    fn address(&self) -> u16 {
        match self {
            OperationMeta::Single(s) => s.address,
            OperationMeta::Multiple(m) => m.address,
            OperationMeta::Masking(m) => m.address,
            _ => 0,
        }
    }

    fn value(&self) -> u16 {
        match self {
            OperationMeta::Single(s) => s.value,
            _ => 0,
        }
    }

    fn quantity(&self) -> u16 {
        match self {
            OperationMeta::Single(_) => 1,
            OperationMeta::Multiple(m) => m.quantity,
            _ => 0,
        }
    }

    fn and_mask(&self) -> u16 {
        match self {
            OperationMeta::Masking(m) => m.and_mask,
            _ => 0,
        }
    }

    fn or_mask(&self) -> u16 {
        match self {
            OperationMeta::Masking(m) => m.or_mask,
            _ => 0,
        }
    }

    fn is_single(&self) -> bool {
        match self {
            OperationMeta::Single(_) => true,
            _ => false,
        }
    }

    fn single_value(&self) -> u16 {
        match self {
            OperationMeta::Single(s) => s.value,
            _ => 0,
        }
    }

    fn device_id_code(&self) -> ReadDeviceIdCode {
        match self {
            OperationMeta::Diag(d) => d.device_id_code,
            _ => ReadDeviceIdCode::default(),
        }
    }

    #[allow(dead_code)]
    fn encap_type(&self) -> EncapsulatedInterfaceType {
        match self {
            OperationMeta::Diag(d) => d.encap_type,
            _ => EncapsulatedInterfaceType::default(),
        }
    }
}

/// Represents an outstanding request that the client expects a response for.
///
/// # Generic Parameters
/// * `T` - Transport implementor.
/// * `A` - Application callbacks implementor.
/// * `N` - Max concurrent requests supported (Queue capacity).
#[derive(Debug)]
pub(crate) struct ExpectedResponse<T, A, const N: usize> {
    /// The Modbus TCP transaction identifier (0 for serial).
    pub txn_id: u16,
    /// The destination Modbus Unit ID or Server Address.
    pub unit_id_or_slave_addr: u8,

    /// The fully compiled Application Data Unit to be sent over the wire.
    /// Retained in memory to allow automatic `retries` without recompiling.
    pub original_adu: Vec<u8, MAX_ADU_FRAME_LEN>,

    /// Time stamp when request is posted
    pub sent_timestamp: u64,
    /// The number of retries left for this request.
    pub retries_left: u8,

    /// Pointer to the specific module's parser/handler function for this operation.
    pub handler: ResponseHandler<T, A, N>,

    /// Modbus memory context (address/quantity) needed to validate the response.
    pub operation_meta: OperationMeta,
}

/// Core client services struct that manages the application logic, transport layer, and
/// expected responses for Modbus communication.
/// This is Main entry point for client operations, providing methods to send requests and process responses.
///
/// # Type Parameters
///
/// * `TRANSPORT` - The transport layer implementation (e.g., TCP or RTU) that handles the physical transmission of Modbus frames.
/// * `N` - The maximum number of concurrent outstanding requests (capacity of the expected responses queue). N must be 1 in serial transport type
/// * `APP` - The application layer that handles processed Modbus responses.
#[derive(Debug)]
pub struct ClientServices<TRANSPORT, APP, const N: usize = 1> {
    /// Application layer that implements the CoilResponse trait, used to handle responses and invoke callbacks.
    pub app: APP,
    /// Transport layer used for sending and receiving Modbus frames. Must implement the Transport trait.
    transport: TRANSPORT,

    /// Configuration for the modbus client
    config: ModbusConfig,

    /// A buffer to store the received frame.
    rxed_frame: Vec<u8, MAX_ADU_FRAME_LEN>,

    /// Heapless circular buffer representing the pipelined requests awaiting responses.
    expected_responses: Vec<ExpectedResponse<TRANSPORT, APP, N>, N>,

    /// Cached timestamp of the earliest expected response timeout to avoid O(N) iteration on every poll.
    next_timeout_check: Option<u64>,
}

pub trait ClientCommon: RequestErrorNotifier + TimeKeeper {}

impl<T> ClientCommon for T where T: RequestErrorNotifier + TimeKeeper {}

impl<T, APP, const N: usize> ClientServices<T, APP, N>
where
    T: Transport,
    APP: ClientCommon,
{
    fn dispatch_response(&mut self, message: &ModbusMessage) {
        let txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let index = if self.transport.transport_type().is_tcp_type() {
            self.expected_responses.iter().position(|r| {
                r.txn_id == txn_id && r.unit_id_or_slave_addr == unit_id_or_slave_addr.into()
            })
        } else {
            self.expected_responses
                .iter()
                .position(|r| r.unit_id_or_slave_addr == unit_id_or_slave_addr.into())
        };

        let expected = match index {
            Some(i) => self.expected_responses.swap_remove(i),
            None => return,
        };

        // If the Modbus server replied with an exception, notify the application layer
        // immediately instead of attempting to parse it as a successful response.
        if let Some(exception_code) = message.pdu().error_code() {
            self.app.request_failed(
                txn_id,
                unit_id_or_slave_addr,
                MbusError::ModbusException(exception_code),
            );
            return;
        }

        (expected.handler)(self, &expected, message);
    }
}

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: RequestErrorNotifier + TimeKeeper,
{
    /// Polls the transport layer for incoming Modbus frames and processes them.
    /// It also handles timeouts and retries for outstanding requests, using the application's `TimeKeeper` for current time.
    ///
    /// # Arguments
    /// * `current_millis` - The current monotonic time in milliseconds.
    pub fn poll(&mut self) {
        // 1. Attempt to receive a frame
        match self.transport.recv() {
            Ok(frame) => {
                if self.rxed_frame.extend_from_slice(frame.as_slice()).is_err() {
                    // Buffer overflowed without forming a valid frame. Must be noise.
                    self.rxed_frame.clear();
                }

                // Process as many pipelined/concatenated frames as exist in the buffer
                while !self.rxed_frame.is_empty() {
                    match self.ingest_frame() {
                        Ok(consumed) => {
                            let len = self.rxed_frame.len();
                            if consumed < len {
                                // Shift array to the left to drain processed bytes (no_std compatible)
                                for i in 0..(len - consumed) {
                                    self.rxed_frame[i] = self.rxed_frame[consumed + i];
                                }
                                self.rxed_frame.truncate(len - consumed);
                            } else {
                                self.rxed_frame.clear();
                            }
                        }
                        Err(MbusError::BufferTooSmall) => {
                            // Reached an incomplete frame, break and wait for more bytes
                            break;
                        }
                        Err(_) => {
                            // Garbage or parsing error, drop the first byte and try again to resync the stream
                            let len = self.rxed_frame.len();
                            if len > 1 {
                                for i in 0..(len - 1) {
                                    self.rxed_frame[i] = self.rxed_frame[1 + i];
                                }
                                self.rxed_frame.truncate(len - 1);
                            } else {
                                self.rxed_frame.clear();
                            }
                        }
                    }
                }
            }
            Err(_) => {
                // Only log non-timeout errors for now. Timeouts are handled below.

                // FUTURE: Consider more robust error handling, e.g., disconnecting
                // and notifying the application if the connection is lost.
                // eprintln!("Transport receive error: {:?}", e);
            }
        }

        // 2. Check for timed-out requests and handle retries for all outstanding requests
        self.handle_timeouts();
    }

    /// Evaluates all pending requests to determine if any have exceeded their response timeout.
    ///
    /// This method is designed to be efficient:
    /// 1. It immediately returns if there are no pending requests.
    /// 2. It utilizes a fast-path cache (`next_timeout_check`) to skip an O(N) linear scan if the nearest
    ///    timeout in the future hasn't been reached yet.
    /// 3. If the cache expires, it iterates linearly over `expected_responses` to check the `sent_timestamp`
    ///    against `current_millis`.
    /// 4. If a request is timed out and has retries remaining, it automatically retransmits the original ADU
    ///    via the transport layer. If the re-send fails, it is dropped and reported as `SendFailed`.
    /// 5. If no retries remain, the request is removed from the pending queue and `NoRetriesLeft` is reported.
    /// 6. Finally, it recalculates the `next_timeout_check` state to schedule the next evaluation interval.
    fn handle_timeouts(&mut self) {
        if self.expected_responses.is_empty() {
            self.next_timeout_check = None;
            return;
        }

        let current_millis = self.app.current_millis();

        // Fast-path: Skip O(N) iteration if the earliest timeout has not yet been reached
        if let Some(check_at) = self.next_timeout_check {
            if current_millis < check_at {
                return;
            }
        }

        let response_timeout_ms = self.response_timeout_ms();
        let expected_responses = &mut self.expected_responses;
        let mut i = 0;
        let mut new_next_check = u64::MAX;

        while i < expected_responses.len() {
            let expected_response = &mut expected_responses[i];
            let elapsed = current_millis
                .checked_sub(expected_response.sent_timestamp)
                .unwrap_or(0);

            if elapsed > response_timeout_ms {
                // Request timed out
                if expected_response.retries_left > 0 {
                    // Retry the request
                    expected_response.retries_left -= 1;
                    expected_response.sent_timestamp = current_millis;
                    // Re-send the original ADU
                    if let Err(_e) = self.transport.send(&expected_response.original_adu) {
                        // If re-sending fails, treat as a failed request
                        let response = expected_responses.swap_remove(i);
                        self.app.request_failed(
                            response.txn_id,
                            UnitIdOrSlaveAddr::from_u8(response.unit_id_or_slave_addr),
                            MbusError::SendFailed,
                        );
                        continue; // Move to the next item in the (now shorter) vec
                    }

                    let expires_at = current_millis.saturating_add(response_timeout_ms);
                    if expires_at < new_next_check {
                        new_next_check = expires_at;
                    }
                } else {
                    // No retries left, report timeout to application
                    let response = expected_responses.swap_remove(i); // Remove the timed-out request
                    self.app.request_failed(
                        response.txn_id,
                        UnitIdOrSlaveAddr::from_u8(response.unit_id_or_slave_addr),
                        MbusError::NoRetriesLeft,
                    );
                    continue; // Move to the next item in the (now shorter) vec
                }
            } else {
                // Not yet timed out, recalculate the next check time
                let expires_at = expected_response
                    .sent_timestamp
                    .saturating_add(response_timeout_ms);
                if expires_at < new_next_check {
                    new_next_check = expires_at;
                }
            }
            i += 1;
        }

        if new_next_check != u64::MAX {
            self.next_timeout_check = Some(new_next_check);
        } else {
            self.next_timeout_check = None;
        }
    }

    fn add_an_expectation(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        frame: &heapless::Vec<u8, MAX_ADU_FRAME_LEN>,
        operation_meta: OperationMeta,
        handler: ResponseHandler<TRANSPORT, APP, N>,
    ) -> Result<(), MbusError> {
        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: handler,
                operation_meta: operation_meta,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        Ok(())
    }
}

/// Implementation of core client services, including methods for sending requests and processing responses.
impl<TRANSPORT: Transport, APP: ClientCommon, const N: usize> ClientServices<TRANSPORT, APP, N> {
    /// Creates a new instance of ClientServices, connecting to the transport layer with the provided configuration.
    pub fn new(
        mut transport: TRANSPORT,
        app: APP,
        config: ModbusConfig,
    ) -> Result<Self, MbusError> {
        let transport_type = transport.transport_type();
        if matches!(
            transport_type,
            TransportType::StdSerial(_) | TransportType::CustomSerial(_)
        ) {
            if N != 1 {
                return Err(MbusError::InvalidNumOfExpectedRsps);
            }
        }

        transport
            .connect(&config)
            .map_err(|_e| MbusError::ConnectionFailed)?;

        Ok(Self {
            app,
            transport,
            rxed_frame: Vec::new(),
            config,
            expected_responses: Vec::new(),
            next_timeout_check: None,
        })
    }

    /// Returns the configured response timeout in milliseconds.
    fn response_timeout_ms(&self) -> u64 {
        match &self.config {
            ModbusConfig::Tcp(config) => config.response_timeout_ms as u64,
            ModbusConfig::Serial(config) => config.response_timeout_ms as u64,
        }
    }

    /// Returns the configured number of retries for outstanding requests.
    fn retry_attempts(&self) -> u8 {
        match &self.config {
            ModbusConfig::Tcp(config) => config.retry_attempts,
            ModbusConfig::Serial(config) => config.retry_attempts,
        }
    }

    /// Ingests received Modbus frames from the transport layer.
    pub fn ingest_frame(&mut self) -> Result<usize, MbusError> {
        let frame = self.rxed_frame.as_slice();
        let transport_type = self.transport.transport_type();

        let expected_length = match derive_length_from_bytes(frame, transport_type) {
            Some(len) => len,
            None => return Err(MbusError::BufferTooSmall),
        };

        if expected_length > MAX_ADU_FRAME_LEN {
            return Err(MbusError::BasicParseError);
        }

        if self.rxed_frame.len() < expected_length {
            return Err(MbusError::BufferTooSmall);
        }

        let message = match common::decompile_adu_frame(&frame[..expected_length], transport_type) {
            Ok(value) => value,
            Err(err) => {
                return Err(err); // Malformed frame or parsing error, frame is dropped.
            }
        };
        use mbus_core::data_unit::common::AdditionalAddress;
        use mbus_core::transport::TransportType::*;
        let message = match self.transport.transport_type() {
            StdTcp | CustomTcp => {
                let mbap_header = match message.additional_address() {
                    AdditionalAddress::MbapHeader(header) => header,
                    _ => return Ok(expected_length),
                };
                let additional_addr = AdditionalAddress::MbapHeader(*mbap_header);
                ModbusMessage::new(additional_addr, message.pdu)
            }
            StdSerial(_) | CustomSerial(_) => {
                let slave_addr = match message.additional_address() {
                    AdditionalAddress::SlaveAddress(addr) => addr.address(),
                    _ => return Ok(expected_length),
                };

                let additional_address =
                    AdditionalAddress::SlaveAddress(SlaveAddress::new(slave_addr)?);
                ModbusMessage::new(additional_address, message.pdu)
            }
        };

        self.dispatch_response(&message);

        Ok(expected_length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CoilResponse;
    use crate::app::DiagnosticsResponse;
    use crate::app::DiscreteInputResponse;
    use crate::app::FifoQueueResponse;
    use crate::app::FileRecordResponse;
    use crate::app::RegisterResponse;
    use crate::services::coil::Coils;

    use crate::services::diagnostic::ConformityLevel;
    use crate::services::diagnostic::DeviceIdentificationResponse;
    use crate::services::diagnostic::ObjectId;
    use crate::services::discrete_input::DiscreteInputs;
    use crate::services::fifo_queue::FifoQueue;
    use crate::services::file_record::MAX_SUB_REQUESTS_PER_PDU;
    use crate::services::file_record::SubRequest;
    use crate::services::file_record::SubRequestParams;
    use crate::services::register::Registers;
    use core::cell::RefCell; // `core::cell::RefCell` is `no_std` compatible
    use heapless::Deque;
    use heapless::Vec;
    use mbus_core::errors::MbusError;
    use mbus_core::function_codes::public::DiagnosticSubFunction;
    use mbus_core::transport::TransportType;
    use mbus_core::transport::{ModbusConfig, ModbusTcpConfig};

    const MOCK_DEQUE_CAPACITY: usize = 10; // Define a capacity for the mock deques

    // --- Mock Transport Implementation ---
    #[derive(Debug, Default)]
    struct MockTransport {
        pub sent_frames: RefCell<Deque<Vec<u8, MAX_ADU_FRAME_LEN>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
        pub recv_frames: RefCell<Deque<Vec<u8, MAX_ADU_FRAME_LEN>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
        pub connect_should_fail: bool,
        pub send_should_fail: bool,
        pub is_connected_flag: RefCell<bool>,
        pub transport_type: Option<TransportType>,
    }

    impl Transport for MockTransport {
        type Error = MbusError;

        fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
            if self.connect_should_fail {
                return Err(MbusError::ConnectionFailed);
            }
            *self.is_connected_flag.borrow_mut() = true;
            Ok(())
        }

        fn disconnect(&mut self) -> Result<(), Self::Error> {
            *self.is_connected_flag.borrow_mut() = false;
            Ok(())
        }

        fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
            if self.send_should_fail {
                return Err(MbusError::SendFailed);
            }
            let mut vec_adu = Vec::new();
            vec_adu
                .extend_from_slice(adu)
                .map_err(|_| MbusError::BufferLenMissmatch)?;
            self.sent_frames
                .borrow_mut()
                .push_back(vec_adu)
                .map_err(|_| MbusError::BufferLenMissmatch)?;
            Ok(())
        }

        fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
            self.recv_frames
                .borrow_mut()
                .pop_front()
                .ok_or(MbusError::Timeout)
        }

        fn is_connected(&self) -> bool {
            *self.is_connected_flag.borrow()
        }

        fn transport_type(&self) -> TransportType {
            self.transport_type.unwrap_or(TransportType::StdTcp)
        }
    }

    // --- Mock App Implementation ---
    #[derive(Debug, Default)]
    struct MockApp {
        pub received_coil_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Coils), 10>>, // Corrected duplicate
        pub received_write_single_coil_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, bool), 10>>,
        pub received_write_multiple_coils_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16), 10>>,
        pub received_discrete_input_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, DiscreteInputs, u16), 10>>,
        pub received_holding_register_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, Registers, u16), 10>>,
        pub received_input_register_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, Registers, u16), 10>>,
        pub received_write_single_register_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16), 10>>,
        pub received_write_multiple_register_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16), 10>>,
        pub received_read_write_multiple_registers_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, Registers), 10>>,
        pub received_mask_write_register_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr), 10>>,
        pub received_read_fifo_queue_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, FifoQueue), 10>>,
        pub received_read_file_record_responses: RefCell<
            Vec<
                (
                    u16,
                    UnitIdOrSlaveAddr,
                    Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>,
                ),
                10,
            >,
        >,
        pub received_write_file_record_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr), 10>>,
        pub received_read_device_id_responses:
            RefCell<Vec<(u16, UnitIdOrSlaveAddr, DeviceIdentificationResponse), 10>>,
        pub failed_requests: RefCell<Vec<(u16, UnitIdOrSlaveAddr, MbusError), 10>>,

        pub current_time: RefCell<u64>, // For simulating time in tests
    }

    impl CoilResponse for MockApp {
        fn read_coils_response(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            coils: &Coils,
        ) {
            self.received_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, coils.clone()))
                .unwrap();
        }

        fn read_single_coil_response(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: bool,
        ) {
            // For single coil, we create a Coils struct with quantity 1 and the single value
            let mut values_vec = [0x00, 1];
            values_vec[0] = if value { 0x01 } else { 0x00 }; // Store the single bit in a byte
            let mut coils = Coils::new(address, 1);
            let _ = coils.set_values(&values_vec, 1);
            self.received_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, coils))
                .unwrap();
        }

        fn write_single_coil_response(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: bool,
        ) {
            self.received_write_single_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, address, value))
                .unwrap();
        }

        fn write_multiple_coils_response(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            quantity: u16,
        ) {
            self.received_write_multiple_coils_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, address, quantity))
                .unwrap();
        }
    }

    impl DiscreteInputResponse for MockApp {
        fn read_multiple_discrete_inputs_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            inputs: &DiscreteInputs,
        ) {
            self.received_discrete_input_responses
                .borrow_mut()
                .push((
                    txn_id,
                    unit_id_slave_addr,
                    inputs.clone(),
                    inputs.quantity(),
                ))
                .unwrap();
        }

        fn read_single_discrete_input_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: bool,
        ) {
            let mut values = Vec::new();
            values.push(if value { 0x01 } else { 0x00 }).unwrap();
            let inputs = DiscreteInputs::new(address, 1, values);
            self.received_discrete_input_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, inputs, 1))
                .unwrap();
        }
    }

    impl RequestErrorNotifier for MockApp {
        fn request_failed(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            error: MbusError,
        ) {
            self.failed_requests
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, error))
                .unwrap();
            println!("Request failed: {:?}", error);
        }
    }

    impl RegisterResponse for MockApp {
        fn read_multiple_holding_registers_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            registers: &Registers,
        ) {
            let quantity = registers.quantity();
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers.clone(), quantity))
                .unwrap();
        }

        fn read_single_input_register_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_input_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers, 1))
                .unwrap();
        }

        fn read_single_holding_register_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers, 1))
                .unwrap();
        }

        fn read_multiple_input_registers_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            registers: &Registers,
        ) {
            let quantity = registers.quantity();
            self.received_input_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers.clone(), quantity))
                .unwrap();
        }

        fn write_single_register_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: u16,
        ) {
            self.received_write_single_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, address, value))
                .unwrap();
        }

        fn write_multiple_registers_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            quantity: u16,
        ) {
            self.received_write_multiple_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, address, quantity))
                .unwrap();
        }

        fn read_write_multiple_registers_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            registers: &Registers,
        ) {
            self.received_read_write_multiple_registers_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers.clone()))
                .unwrap();
        }

        fn mask_write_register_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
        ) {
            self.received_mask_write_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr))
                .unwrap();
        }

        fn read_single_register_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, registers, 1))
                .unwrap();
        }
    }

    impl FifoQueueResponse for MockApp {
        fn read_fifo_queue_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            fifo_queue: &FifoQueue,
        ) {
            self.received_read_fifo_queue_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, fifo_queue.clone()))
                .unwrap();
        }
    }

    impl FileRecordResponse for MockApp {
        fn read_file_record_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            data: &[SubRequestParams],
        ) {
            let mut vec = Vec::new();
            vec.extend_from_slice(data).unwrap();
            self.received_read_file_record_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, vec))
                .unwrap();
        }
        fn write_file_record_response(
            &mut self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
        ) {
            self.received_write_file_record_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr))
                .unwrap();
        }
    }

    impl DiagnosticsResponse for MockApp {
        fn read_device_identification_response(
            &self,
            txn_id: u16,
            unit_id_slave_addr: UnitIdOrSlaveAddr,
            response: &DeviceIdentificationResponse,
        ) {
            self.received_read_device_id_responses
                .borrow_mut()
                .push((txn_id, unit_id_slave_addr, response.clone()))
                .unwrap();
        }

        fn encapsulated_interface_transport_response(
            &self,
            _: u16,
            _: UnitIdOrSlaveAddr,
            _: EncapsulatedInterfaceType,
            _: &[u8],
        ) {
        }

        fn diagnostics_response(
            &self,
            _: u16,
            _: UnitIdOrSlaveAddr,
            _: DiagnosticSubFunction,
            _: &[u16],
        ) {
        }

        fn get_comm_event_counter_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}

        fn get_comm_event_log_response(
            &self,
            _: u16,
            _: UnitIdOrSlaveAddr,
            _: u16,
            _: u16,
            _: u16,
            _: &[u8],
        ) {
        }

        fn read_exception_status_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u8) {}

        fn report_server_id_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: &[u8]) {}
    }

    impl TimeKeeper for MockApp {
        fn current_millis(&self) -> u64 {
            *self.current_time.borrow()
        }
    }

    // --- ClientServices Tests ---

    /// Test case: `ClientServices::new` successfully connects to the transport.
    #[test]
    fn test_client_services_new_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());

        let client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config);
        assert!(client_services.is_ok());
        assert!(client_services.unwrap().transport.is_connected());
    }

    /// Test case: `ClientServices::new` returns an error if transport connection fails.
    #[test]
    fn test_client_services_new_connection_failure() {
        let mut transport = MockTransport::default();
        transport.connect_should_fail = true;
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());

        let client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config);
        assert!(client_services.is_err());
        assert_eq!(client_services.unwrap_err(), MbusError::ConnectionFailed);
    }

    /// Test case: `read_multiple_coils` sends a valid ADU over the transport.
    #[test]
    fn test_read_multiple_coils_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 8;
        client_services
            .read_multiple_coils(txn_id, unit_id, address, quantity)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0001), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x01), Addr(0x0000), Qty(0x0008)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Quantity = 6)
            0x01,       // Unit ID
            0x01,       // Function Code (Read Coils)
            0x00, 0x00, // Starting Address
            0x00, 0x08, // Quantity of Coils
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `read_multiple_coils` returns an error for an invalid quantity.
    #[test]
    fn test_read_multiple_coils_invalid_quantity() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 0; // Invalid quantity

        let result = client_services.read_multiple_coils(txn_id, unit_id, address, quantity); // current_millis() is called internally
        assert_eq!(result.unwrap_err(), MbusError::InvalidQuantity);
    }

    /// Test case: `read_multiple_coils` returns an error if sending fails.
    #[test]
    fn test_read_multiple_coils_send_failure() {
        let mut transport = MockTransport::default();
        transport.send_should_fail = true;
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 8;

        let result = client_services.read_multiple_coils(txn_id, unit_id, address, quantity); // current_millis() is called internally
        assert_eq!(result.unwrap_err(), MbusError::SendFailed);
    }

    /// Test case: `ingest_frame` ignores responses with wrong function code.
    #[test]
    fn test_ingest_frame_wrong_fc() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        // ADU with FC 0x03 (Read Holding Registers) instead of 0x01 (Read Coils)
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x03, 0x01, 0xB3];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores malformed ADUs.
    #[test]
    fn test_ingest_frame_malformed_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        // Malformed ADU (too short)
        let malformed_adu = [0x01, 0x02, 0x03];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&malformed_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores responses for unknown transaction IDs.
    #[test]
    fn test_ingest_frame_unknown_txn_id() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        // No request was sent, so no expected response is in the queue.
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0xB3];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores responses that fail PDU parsing.
    #[test]
    fn test_ingest_frame_pdu_parse_failure() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 8;
        client_services
            .read_multiple_coils(txn_id, unit_id, address, quantity) // current_millis() is called internally
            .unwrap();

        // Craft a PDU that will cause `parse_read_coils_response` to fail.
        // For example, byte count mismatch: PDU indicates 1 byte of data, but provides 2.
        // ADU: TID(0x0001), PID(0x0000), Length(0x0005), UnitID(0x01), FC(0x01), Byte Count(0x01), Data(0xB3, 0x00)
        let response_adu = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x01, 0x01, 0xB3, 0x00,
        ]; // Corrected duplicate

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
        // The expected response should still be removed even if PDU parsing fails.
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `ClientServices` successfully sends a Read Single Coil request and processes a valid response.
    #[test]
    fn test_client_services_read_single_coil_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0002;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0005;

        // 1. Send a Read Single Coil request
        client_services // current_millis() is called internally
            .read_single_coil(txn_id, unit_id, address)
            .unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services
            .transport
            .sent_frames
            .borrow_mut()
            .pop_front()
            .unwrap();
        // Expected ADU for Read Coils (FC 0x01) with quantity 1
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x02, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (Unit ID + FC + Addr + Qty=1)
            0x01,       // Unit ID
            0x01,       // Function Code (Read Coils)
            0x00, 0x05, // Starting Address
            0x00, 0x01, // Quantity of Coils (1)
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // 2. Manually construct a valid Read Coils response ADU for a single coil
        // Response for reading 1 coil at 0x0005, value: true (0x01)
        // ADU: TID(0x0002), PID(0x0000), Length(0x0004), UnitID(0x01), FC(0x01), Byte Count(0x01), Coil Data(0x01)
        let response_adu = [0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0x01];

        // Simulate receiving the frame
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        // 3. Assert that the MockApp's read_single_coil_response callback was invoked with correct data
        let received_responses = client_services.app.received_coil_responses.borrow();
        assert_eq!(received_responses.len(), 1);

        let (rcv_txn_id, rcv_unit_id, rcv_coils) = &received_responses[0];
        let rcv_quantity = rcv_coils.quantity();
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), 1); // Quantity should be 1
        assert_eq!(&rcv_coils.values()[..1], &[0x01]); // Value should be 0x01 for true
        assert_eq!(rcv_quantity, 1);

        // 4. Assert that the expected response was removed from the queue
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `read_single_coil_request` sends a valid ADU over the transport.
    #[test]
    fn test_read_single_coil_request_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0002;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0005;

        client_services
            .read_single_coil(txn_id, unit_id, address) // current_millis() is called internally
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0002), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x01), Addr(0x0005), Qty(0x0001)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x02, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Quantity = 6)
            0x01,       // Unit ID
            0x01,       // Function Code (Read Coils)
            0x00, 0x05, // Starting Address
            0x00, 0x01, // Quantity of Coils (1)
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // Verify that the expected response was recorded with single_read = true
        assert_eq!(client_services.expected_responses.len(), 1); // Corrected: Removed duplicate pop_front()
        let single_read = client_services.expected_responses[0]
            .operation_meta
            .is_single();
        assert!(single_read);
    }

    /// Test case: `write_single_coil` sends a valid ADU over the transport.
    #[test]
    fn test_write_single_coil_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0003;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x000A;
        let value = true;

        client_services
            .write_single_coil(txn_id, unit_id, address, value) // current_millis() is called internally
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0003), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x05), Addr(0x000A), Value(0xFF00)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x03, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Value = 6)
            0x01,       // Unit ID
            0x05,       // Function Code (Write Single Coil)
            0x00, 0x0A, // Address
            0xFF, 0x00, // Value (ON)
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // Verify that the expected response was recorded
        assert_eq!(client_services.expected_responses.len(), 1);
        let expected_address = client_services.expected_responses[0]
            .operation_meta
            .address();
        let expected_value = client_services.expected_responses[0].operation_meta.value() != 0;

        assert_eq!(expected_address, address);
        assert_eq!(expected_value, value);
    }

    /// Test case: `ClientServices` successfully sends a Write Single Coil request and processes a valid response.
    #[test]
    fn test_client_services_write_single_coil_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0003;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x000A;
        let value = true;

        // 1. Send a Write Single Coil request
        client_services // current_millis() is called internally
            .write_single_coil(txn_id, unit_id, address, value)
            .unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services
            .transport
            .sent_frames
            .borrow_mut()
            .pop_front()
            .unwrap();
        #[rustfmt::skip]
        let expected_request_adu: [u8; 12] = [
            0x00, 0x03, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length
            0x01,       // Unit ID
            0x05,       // Function Code (Write Single Coil)
            0x00, 0x0A, // Address
            0xFF, 0x00, // Value (ON)
        ];
        assert_eq!(sent_adu.as_slice(), &expected_request_adu);

        // 2. Manually construct a valid Write Single Coil response ADU
        // ADU: TID(0x0003), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x05), Address(0x000A), Value(0xFF00)
        let response_adu = [
            0x00, 0x03, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0x0A, 0xFF, 0x00,
        ];

        // Simulate receiving the frame
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        // 3. Assert that the MockApp's write_single_coil_response callback was invoked with correct data
        let received_responses = client_services
            .app
            .received_write_single_coil_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);

        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_value) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_value, value);

        // 4. Assert that the expected response was removed from the queue
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `write_multiple_coils` sends a valid ADU over the transport.
    #[test]
    fn test_write_multiple_coils_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0004;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 10;

        // Initialize a Coils instance with alternating true/false values to produce 0x55, 0x01
        let mut values = Coils::new(address, quantity);
        for i in 0..quantity {
            values.set_value(address + i, i % 2 == 0).unwrap();
        }

        client_services
            .write_multiple_coils(txn_id, unit_id, address, &values) // current_millis() is called internally
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0004), PID(0x0000), Length(0x0009), UnitID(0x01), FC(0x0F), Addr(0x0000), Qty(0x000A), Byte Count(0x02), Data(0x55, 0x01)
        #[rustfmt::skip]
        let expected_adu: [u8; 15] = [
            0x00, 0x04, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x09, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Quantity + 1 byte Byte Count + 2 bytes Data = 9)
            0x01,       // Unit ID
            0x0F,       // Function Code (Write Multiple Coils)
            0x00, 0x00, // Address
            0x00, 0x0A, // Quantity
            0x02,       // Byte Count
            0x55, 0x01, // Data
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // Verify that the expected response was recorded
        assert_eq!(client_services.expected_responses.len(), 1);
        let expected_address = client_services.expected_responses[0]
            .operation_meta
            .address();
        let expected_quantity = client_services.expected_responses[0]
            .operation_meta
            .quantity();
        assert_eq!(expected_address, address);
        assert_eq!(expected_quantity, quantity);
    }

    /// Test case: `ClientServices` successfully sends a Write Multiple Coils request and processes a valid response.
    #[test]
    fn test_client_services_write_multiple_coils_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0004;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 10;

        // Initialize a Coils instance with alternating true/false values
        let mut values = Coils::new(address, quantity);
        for i in 0..quantity {
            values.set_value(address + i, i % 2 == 0).unwrap();
        }

        // 1. Send a Write Multiple Coils request
        client_services // current_millis() is called internally
            .write_multiple_coils(txn_id, unit_id, address, &values)
            .unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services
            .transport
            .sent_frames
            .borrow_mut()
            .pop_front()
            .unwrap();
        #[rustfmt::skip]
        let expected_request_adu: [u8; 15] = [
            0x00, 0x04, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x09, // Length
            0x01,       // Unit ID
            0x0F,       // Function Code (Write Multiple Coils)
            0x00, 0x00, // Address
            0x00, 0x0A, // Quantity
            0x02,       // Byte Count
            0x55, 0x01, // Data
        ];
        assert_eq!(sent_adu.as_slice(), &expected_request_adu);

        // 2. Manually construct a valid Write Multiple Coils response ADU
        // ADU: TID(0x0004), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x0F), Address(0x0000), Quantity(0x000A)
        let response_adu = [
            0x00, 0x04, 0x00, 0x00, 0x00, 0x06, 0x01, 0x0F, 0x00, 0x00, 0x00, 0x0A,
        ];

        // Simulate receiving the frame
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        // 3. Assert that the MockApp's write_multiple_coils_response callback was invoked with correct data
        let received_responses = client_services
            .app
            .received_write_multiple_coils_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);

        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_quantity, quantity);

        // 4. Assert that the expected response was removed from the queue
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `ClientServices` successfully sends a Read Coils request and processes a valid response.
    #[test]
    fn test_client_services_read_coils_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 8;
        client_services
            .read_multiple_coils(txn_id, unit_id, address, quantity) // current_millis() is called internally
            .unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services
            .transport
            .sent_frames
            .borrow_mut()
            .pop_front()
            .unwrap(); // Corrected: Removed duplicate pop_front()
        // Expected ADU: TID(0x0001), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x01), Addr(0x0000), Qty(0x0008)
        assert_eq!(
            sent_adu.as_slice(),
            &[
                0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x01, 0x00, 0x00, 0x00, 0x08
            ]
        );

        // Verify that the expected response was recorded
        assert_eq!(client_services.expected_responses.len(), 1); // Corrected: Removed duplicate pop_front()
        let from_address = client_services.expected_responses[0]
            .operation_meta
            .address();
        let expected_quantity = client_services.expected_responses[0]
            .operation_meta
            .quantity();

        assert_eq!(expected_quantity, quantity);
        assert_eq!(from_address, address);

        // 2. Manually construct a valid Read Coils response ADU
        // Response for reading 8 coils, values: 10110011 (0xB3)
        // ADU: TID(0x0001), PID(0x0000), Length(0x0004 = Unit ID + FC + Byte Count + Coil Data), UnitID(0x01), FC(0x01), Byte Count(0x01), Coil Data(0xB3)
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0xB3];

        // Simulate receiving the frame
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll(); // Call poll to ingest frame and process

        // Advance time to ensure any potential timeouts are processed (though not expected here)

        // 3. Assert that the MockApp's callback was invoked with correct data
        let received_responses = client_services.app.received_coil_responses.borrow();
        assert_eq!(received_responses.len(), 1);

        let (rcv_txn_id, rcv_unit_id, rcv_coils) = &received_responses[0];
        let rcv_quantity = rcv_coils.quantity();
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), quantity);
        assert_eq!(&rcv_coils.values()[..1], &[0xB3]);
        assert_eq!(rcv_quantity, quantity);

        // 4. Assert that the expected response was removed from the queue
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `poll` handles a timed-out request with retries.
    #[test]
    fn test_client_services_timeout_with_retry() {
        let transport = MockTransport::default();
        // Simulate no response from the server initially
        transport.recv_frames.borrow_mut().clear();
        let app = MockApp::default();
        let mut tcp_config = ModbusTcpConfig::new("127.0.0.1", 502).unwrap();
        tcp_config.response_timeout_ms = 100; // Short timeout for testing
        tcp_config.retry_attempts = 1; // One retry
        let config = ModbusConfig::Tcp(tcp_config);

        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0005;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;

        client_services
            .read_single_coil(txn_id, unit_id, address)
            .unwrap();

        // Advance time past timeout for the first time
        *client_services.app.current_time.borrow_mut() = 150;
        // Simulate time passing beyond timeout, but with retries left
        client_services.poll(); // First timeout, should retry

        // Verify that the request was re-sent (2 frames: initial + retry)
        assert_eq!(client_services.transport.sent_frames.borrow().len(), 2);
        assert_eq!(client_services.expected_responses.len(), 1); // Still waiting for response
        assert_eq!(client_services.expected_responses[0].retries_left, 0); // One retry used

        // Advance time past timeout for the second time
        *client_services.app.current_time.borrow_mut() = 300;
        // Simulate more time passing, exhausting retries
        client_services.poll(); // Second timeout, should fail

        // Verify that the request is no longer expected and an error was reported
        assert!(client_services.expected_responses.is_empty());
        // In a real scenario, MockApp::request_failed would be checked.
    }

    /// Test case: `poll` correctly handles multiple concurrent requests timing out simultaneously.
    #[test]
    fn test_client_services_concurrent_timeouts() {
        let transport = MockTransport::default();
        let app = MockApp::default();

        // Configure a short timeout and 1 retry for testing purposes
        let mut tcp_config = ModbusTcpConfig::new("127.0.0.1", 502).unwrap();
        tcp_config.response_timeout_ms = 100;
        tcp_config.retry_attempts = 1;
        let config = ModbusConfig::Tcp(tcp_config);

        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();

        // 1. Send two simultaneous requests
        client_services
            .read_single_coil(1, unit_id, 0x0000)
            .unwrap();
        client_services
            .read_single_coil(2, unit_id, 0x0001)
            .unwrap();

        // Verify both requests are queued and sent once
        assert_eq!(client_services.expected_responses.len(), 2);
        assert_eq!(client_services.transport.sent_frames.borrow().len(), 2);

        // 2. Advance time past the timeout threshold for both requests
        *client_services.app.current_time.borrow_mut() = 150;

        // 3. Poll the client. Both requests should be evaluated, found timed out, and retried.
        client_services.poll();

        // Verify both requests are STILL in the queue (waiting for retry responses)
        assert_eq!(client_services.expected_responses.len(), 2);
        assert_eq!(client_services.expected_responses[0].retries_left, 0);
        assert_eq!(client_services.expected_responses[1].retries_left, 0);

        // Verify both requests were transmitted again (Total sent frames = 2 original + 2 retries = 4)
        assert_eq!(client_services.transport.sent_frames.borrow().len(), 4);

        // 4. Advance time again past the retry timeout threshold
        *client_services.app.current_time.borrow_mut() = 300;

        // 5. Poll the client. Both requests should exhaust their retries and be dropped.
        client_services.poll();

        // Verify the queue is now completely empty
        assert!(client_services.expected_responses.is_empty());

        // Verify the application was notified of BOTH failures
        let failed_requests = client_services.app.failed_requests.borrow();
        assert_eq!(failed_requests.len(), 2);

        // Ensure both specific transaction IDs were reported as having no retries left
        let has_txn_1 = failed_requests
            .iter()
            .any(|(txn, _, err)| *txn == 1 && *err == MbusError::NoRetriesLeft);
        let has_txn_2 = failed_requests
            .iter()
            .any(|(txn, _, err)| *txn == 2 && *err == MbusError::NoRetriesLeft);
        assert!(has_txn_1, "Transaction 1 should have failed");
        assert!(has_txn_2, "Transaction 2 should have failed");
    }

    /// Test case: `read_multiple_coils` returns `MbusError::TooManyRequests` when the queue is full.
    #[test]
    fn test_too_many_requests_error() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        // Create a client with a small capacity for expected responses
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 1>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        // Send one request, which should fill the queue
        client_services
            .read_multiple_coils(1, unit_id, 0, 1)
            .unwrap();
        assert_eq!(client_services.expected_responses.len(), 1);

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        // Attempt to send another request, which should fail due to full queue
        let result = client_services.read_multiple_coils(2, unit_id, 0, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), MbusError::TooManyRequests);
        assert_eq!(client_services.expected_responses.len(), 1); // Queue size remains 1
    }

    /// Test case: `read_holding_registers` sends a valid ADU over the transport.
    #[test]
    fn test_read_holding_registers_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0005;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 2;
        client_services
            .read_holding_registers(txn_id, unit_id, address, quantity)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0005), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x03), Addr(0x0000), Qty(0x0002)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x05, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Quantity = 6)
            0x01,       // Unit ID
            0x03,       // Function Code (Read Holding Registers)
            0x00, 0x00, // Starting Address
            0x00, 0x02, // Quantity of Registers
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `ClientServices` successfully sends a Read Holding Registers request and processes a valid response.
    #[test]
    fn test_client_services_read_holding_registers_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0005;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 2;
        client_services
            .read_holding_registers(txn_id, unit_id, address, quantity)
            .unwrap();

        // Simulate response
        // ADU: TID(0x0005), PID(0x0000), Length(0x0007), UnitID(0x01), FC(0x03), Byte Count(0x04), Data(0x1234, 0x5678)
        let response_adu = [
            0x00, 0x05, 0x00, 0x00, 0x00, 0x07, 0x01, 0x03, 0x04, 0x12, 0x34, 0x56, 0x78,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_holding_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_registers, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_registers.from_address(), address);
        assert_eq!(rcv_registers.quantity(), quantity);
        assert_eq!(rcv_registers.values().as_slice(), &[0x1234, 0x5678]);
        assert_eq!(*rcv_quantity, quantity);
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `read_input_registers` sends a valid ADU over the transport.
    #[test]
    fn test_read_input_registers_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0006;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 2;
        client_services
            .read_input_registers(txn_id, unit_id, address, quantity)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0006), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x04), Addr(0x0000), Qty(0x0002)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x06, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Quantity = 6)
            0x01,       // Unit ID
            0x04,       // Function Code (Read Input Registers)
            0x00, 0x00, // Starting Address
            0x00, 0x02, // Quantity of Registers
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `ClientServices` successfully sends a Read Input Registers request and processes a valid response.
    #[test]
    fn test_client_services_read_input_registers_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0006;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 2;
        client_services
            .read_input_registers(txn_id, unit_id, address, quantity)
            .unwrap();

        // Simulate response
        // ADU: TID(0x0006), PID(0x0000), Length(0x0007), UnitID(0x01), FC(0x04), Byte Count(0x04), Data(0xAABB, 0xCCDD)
        let response_adu = [
            0x00, 0x06, 0x00, 0x00, 0x00, 0x07, 0x01, 0x04, 0x04, 0xAA, 0xBB, 0xCC, 0xDD,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_input_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_registers, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_registers.from_address(), address);
        assert_eq!(rcv_registers.quantity(), quantity);
        assert_eq!(rcv_registers.values().as_slice(), &[0xAABB, 0xCCDD]);
        assert_eq!(*rcv_quantity, quantity);
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `write_single_register` sends a valid ADU over the transport.
    #[test]
    fn test_write_single_register_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0007;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0001;
        let value = 0x1234;
        client_services
            .write_single_register(txn_id, unit_id, address, value)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0007), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x06), Addr(0x0001), Value(0x1234)
        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x07, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x06, // Length (1 byte Unit ID + 1 byte FC + 2 bytes Address + 2 bytes Value = 6)
            0x01,       // Unit ID
            0x06,       // Function Code (Write Single Register)
            0x00, 0x01, // Address
            0x12, 0x34, // Value
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `ClientServices` successfully sends a Write Single Register request and processes a valid response.
    #[test]
    fn test_client_services_write_single_register_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0007;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0001;
        let value = 0x1234;
        client_services
            .write_single_register(txn_id, unit_id, address, value)
            .unwrap();

        // Simulate response
        // ADU: TID(0x0007), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x06), Address(0x0001), Value(0x1234)
        let response_adu = [
            0x00, 0x07, 0x00, 0x00, 0x00, 0x06, 0x01, 0x06, 0x00, 0x01, 0x12, 0x34,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_write_single_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_value) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_value, value);
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `write_multiple_registers` sends a valid ADU over the transport.
    #[test]
    fn test_write_multiple_registers_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0008;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0001;
        let quantity = 2;
        let values = [0x1234, 0x5678];
        client_services
            .write_multiple_registers(txn_id, unit_id, address, quantity, &values)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        // Expected ADU: TID(0x0008), PID(0x0000), Length(0x0009), UnitID(0x01), FC(0x10), Addr(0x0001), Qty(0x0002), Byte Count(0x04), Data(0x1234, 0x5678)
        #[rustfmt::skip]
        let expected_adu: [u8; 17] = [ // Total ADU length is 17 bytes
            0x00, 0x08, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x0B, // Length (UnitID(1) + PDU(10) = 11)
            0x01,       // Unit ID
            0x10,       // Function Code (Write Multiple Registers)
            0x00, 0x01, // Address
            0x00, 0x02, // Quantity
            0x04,       // Byte Count
            0x12, 0x34, 0x56, 0x78, // Data
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `ClientServices` successfully sends a Write Multiple Registers request and processes a valid response.
    #[test]
    fn test_client_services_write_multiple_registers_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0008;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0001;
        let quantity = 2;
        let values = [0x1234, 0x5678];
        client_services
            .write_multiple_registers(txn_id, unit_id, address, quantity, &values)
            .unwrap();

        // Simulate response
        // ADU: TID(0x0008), PID(0x0000), Length(0x0006), UnitID(0x01), FC(0x10), Address(0x0001), Quantity(0x0002)
        let response_adu = [
            0x00, 0x08, 0x00, 0x00, 0x00, 0x06, 0x01, 0x10, 0x00, 0x01, 0x00, 0x02,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_write_multiple_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_quantity, quantity);
        assert!(client_services.expected_responses.is_empty());
    }

    /// Test case: `ClientServices` correctly handles a Modbus exception response.
    #[test]
    fn test_client_services_handles_exception_response() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0009;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 0x0000;
        let quantity = 1;

        client_services
            .read_holding_registers(txn_id, unit_id, address, quantity)
            .unwrap();

        // Simulate an exception response (e.g., Illegal Data Address)
        // FC = 0x83 (0x03 + 0x80), Exception Code = 0x02
        let exception_adu = [
            0x00, 0x09, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x03, // Length
            0x01, // Unit ID
            0x83, // Function Code (0x03 + 0x80 Error Mask)
            0x02, // Exception Code (Illegal Data Address)
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&exception_adu).unwrap())
            .unwrap();
        client_services.poll();

        // Verify that no successful response was recorded
        assert!(
            client_services
                .app
                .received_holding_register_responses
                .borrow()
                .is_empty()
        );
        // Verify that the failure was reported to the app
        assert_eq!(client_services.app.failed_requests.borrow().len(), 1);
        let (failed_txn, failed_unit, failed_err) =
            &client_services.app.failed_requests.borrow()[0];
        assert_eq!(*failed_txn, txn_id);
        assert_eq!(*failed_unit, unit_id);
        assert_eq!(*failed_err, MbusError::ModbusException(0x02));
    }

    /// Test case: `read_single_holding_register` sends a valid ADU.
    #[test]
    fn test_read_single_holding_register_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        client_services
            .read_single_holding_register(10, unit_id, 100)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x0A, // TID
            0x00, 0x00, // PID
            0x00, 0x06, // Length
            0x01,       // Unit ID
            0x03,       // FC
            0x00, 0x64, // Address
            0x00, 0x01, // Quantity
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // Verify expected response
        assert_eq!(client_services.expected_responses.len(), 1);
        let single_read = client_services.expected_responses[0]
            .operation_meta
            .is_single();
        assert!(single_read);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_single_holding_register` request.
    #[test]
    fn test_client_services_read_single_holding_register_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 10;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 100;

        client_services
            .read_single_holding_register(txn_id, unit_id, address)
            .unwrap();

        // Simulate response
        let response_adu = [
            0x00, 0x0A, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0x12, 0x34,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_holding_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_registers, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_registers.from_address(), address);
        assert_eq!(rcv_registers.quantity(), 1);
        assert_eq!(rcv_registers.values().as_slice(), &[0x1234]);
        assert_eq!(*rcv_quantity, 1);
    }

    /// Test case: `read_single_input_register` sends a valid ADU.
    #[test]
    fn test_read_single_input_register_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        client_services
            .read_single_input_register(10, unit_id, 100)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        #[rustfmt::skip]
        let expected_adu: [u8; 12] = [
            0x00, 0x0A, // TID
            0x00, 0x00, // PID
            0x00, 0x06, // Length
            0x01,       // Unit ID
            0x04,       // FC (Read Input Registers)
            0x00, 0x64, // Address
            0x00, 0x01, // Quantity
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);

        // Verify expected response
        assert_eq!(client_services.expected_responses.len(), 1);
        let single_read = client_services.expected_responses[0]
            .operation_meta
            .is_single();
        assert!(single_read);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_single_input_register` request.
    #[test]
    fn test_client_services_read_single_input_register_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 10;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 100;

        client_services
            .read_single_input_register(txn_id, unit_id, address)
            .unwrap();

        // Simulate response
        // ADU: TID(10), PID(0), Len(5), Unit(1), FC(4), ByteCount(2), Data(0x1234)
        let response_adu = [
            0x00, 0x0A, 0x00, 0x00, 0x00, 0x05, 0x01, 0x04, 0x02, 0x12, 0x34,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_input_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_registers, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_registers.from_address(), address);
        assert_eq!(rcv_registers.quantity(), 1);
        assert_eq!(rcv_registers.values().as_slice(), &[0x1234]);
        assert_eq!(*rcv_quantity, 1);
    }

    /// Test case: `read_write_multiple_registers` sends a valid ADU.
    #[test]
    fn test_read_write_multiple_registers_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let write_values = [0xAAAA, 0xBBBB];
        client_services
            .read_write_multiple_registers(11, unit_id, 10, 2, 20, &write_values)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        #[rustfmt::skip]
        let expected_adu: [u8; 21] = [
            0x00, 0x0B, // TID
            0x00, 0x00, // PID
            0x00, 0x0F, // Length
            0x01,       // Unit ID
            0x17,       // FC
            0x00, 0x0A, // Read Address
            0x00, 0x02, // Read Quantity
            0x00, 0x14, // Write Address
            0x00, 0x02, // Write Quantity
            0x04,       // Write Byte Count
            0xAA, 0xAA, // Write Value 1
            0xBB, 0xBB, // Write Value 2
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `mask_write_register` sends a valid ADU.
    #[test]
    fn test_mask_write_register_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        client_services
            .mask_write_register(12, unit_id, 30, 0xF0F0, 0x0F0F)
            .unwrap();

        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        let sent_adu = sent_frames.front().unwrap();

        #[rustfmt::skip]
        let expected_adu: [u8; 14] = [
            0x00, 0x0C, // TID
            0x00, 0x00, // PID
            0x00, 0x08, // Length
            0x01,       // Unit ID
            0x16,       // FC
            0x00, 0x1E, // Address
            0xF0, 0xF0, // AND mask
            0x0F, 0x0F, // OR mask
        ];
        assert_eq!(sent_adu.as_slice(), &expected_adu);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_write_multiple_registers` request.
    #[test]
    fn test_client_services_read_write_multiple_registers_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 11;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let read_address = 10;
        let read_quantity = 2;
        let write_address = 20;
        let write_values = [0xAAAA, 0xBBBB];

        client_services
            .read_write_multiple_registers(
                txn_id,
                unit_id,
                read_address,
                read_quantity,
                write_address,
                &write_values,
            )
            .unwrap();

        // Simulate response
        let response_adu = [
            0x00, 0x0B, 0x00, 0x00, 0x00, 0x07, 0x01, 0x17, 0x04, 0x12, 0x34, 0x56, 0x78,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_read_write_multiple_registers_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_registers) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_registers.from_address(), read_address);
        assert_eq!(rcv_registers.quantity(), read_quantity);
        assert_eq!(rcv_registers.values().as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `ClientServices` successfully sends and processes a `mask_write_register` request.
    #[test]
    fn test_client_services_mask_write_register_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 12;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 30;
        let and_mask = 0xF0F0;
        let or_mask = 0x0F0F;

        client_services
            .mask_write_register(txn_id, unit_id, address, and_mask, or_mask)
            .unwrap();

        // Simulate response
        let response_adu = [
            0x00, 0x0C, 0x00, 0x00, 0x00, 0x08, 0x01, 0x16, 0x00, 0x1E, 0xF0, 0xF0, 0x0F, 0x0F,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_mask_write_register_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_fifo_queue` request.
    #[test]
    fn test_client_services_read_fifo_queue_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 13;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 40;

        client_services
            .read_fifo_queue(txn_id, unit_id, address)
            .unwrap();

        // Simulate response
        #[rustfmt::skip]
        let response_adu = [
            0x00, 0x0D, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x0A, // Length (Unit ID + PDU)
            0x01,       // Unit ID
            0x18,       // Function Code (Read FIFO Queue)
            0x00, 0x06, // FIFO Byte Count (2 bytes for FIFO Count + 2 * 2 bytes for values)
            0x00, 0x02, // FIFO Count (2 registers)
            0xAA, 0xAA, // Register Value 1
            0xBB, 0xBB, // Register Value 2
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_read_fifo_queue_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_fifo_queue) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_fifo_queue.values.len(), 2);
        assert_eq!(rcv_fifo_queue.values.as_slice(), &[0xAAAA, 0xBBBB]);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_file_record` request.
    #[test]
    fn test_client_services_read_file_record_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 14;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let mut sub_req = SubRequest::new();
        sub_req.add_read_sub_request(4, 1, 2).unwrap();

        client_services
            .read_file_record(txn_id, unit_id, &sub_req)
            .unwrap();

        // Simulate response: FC(20), ByteCount(7), SubReqLen(6), Ref(6), Data(0x1234, 0x5678)
        // Note: ByteCount = 1 (SubReqLen) + 1 (Ref) + 4 (Data) + 1 (SubReqLen for next?) No.
        // Response format: ByteCount, [Len, Ref, Data...]
        // Len = 1 (Ref) + 4 (Data) = 5.
        // ByteCount = 1 (Len) + 5 = 6.
        let response_adu = [
            0x00, 0x0E, 0x00, 0x00, 0x00, 0x09, 0x01, 0x14, 0x06, 0x05, 0x06, 0x12, 0x34, 0x56,
            0x78,
        ];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_read_file_record_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_data) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_data.len(), 1);
        assert_eq!(
            rcv_data[0].record_data.as_ref().unwrap().as_slice(),
            &[0x1234, 0x5678]
        );
    }

    /// Test case: `ClientServices` successfully sends and processes a `write_file_record` request.
    #[test]
    fn test_client_services_write_file_record_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 15;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let mut sub_req = SubRequest::new();
        let mut data = Vec::new();
        data.push(0x1122).unwrap();
        sub_req.add_write_sub_request(4, 1, 1, data).unwrap();

        client_services
            .write_file_record(txn_id, unit_id, &sub_req)
            .unwrap();

        // Simulate response (Echo of request)
        // FC(21), ByteCount(9), Ref(6), File(4), Rec(1), Len(1), Data(0x1122)
        let response_adu = [
            0x00, 0x0F, 0x00, 0x00, 0x00, 0x0C, 0x01, 0x15, 0x09, 0x06, 0x00, 0x04, 0x00, 0x01,
            0x00, 0x01, 0x11, 0x22,
        ];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_write_file_record_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_discrete_inputs` request.
    #[test]
    fn test_client_services_read_discrete_inputs_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 16;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 50;
        let quantity = 8;

        client_services
            .read_discrete_inputs(txn_id, unit_id, address, quantity)
            .unwrap();

        // Simulate response: FC(02), ByteCount(1), Data(0xAA)
        let response_adu = [0x00, 0x10, 0x00, 0x00, 0x00, 0x04, 0x01, 0x02, 0x01, 0xAA];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_discrete_input_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_inputs, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_inputs.from_address(), address);
        assert_eq!(rcv_inputs.quantity(), quantity);
        assert_eq!(rcv_inputs.values().as_slice(), &[0xAA]);
        assert_eq!(*rcv_quantity, quantity);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_single_discrete_input` request.
    #[test]
    fn test_client_services_read_single_discrete_input_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 17;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let address = 10;

        client_services
            .read_single_discrete_input(txn_id, unit_id, address)
            .unwrap();

        // Verify request ADU
        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        // MBAP(7) + PDU(5) = 12 bytes
        // MBAP: 00 11 00 00 00 06 01
        // PDU: 02 00 0A 00 01
        let expected_request = [
            0x00, 0x11, 0x00, 0x00, 0x00, 0x06, 0x01, 0x02, 0x00, 0x0A, 0x00, 0x01,
        ];
        assert_eq!(sent_frames.front().unwrap().as_slice(), &expected_request);
        drop(sent_frames);

        // Simulate response: FC(02), ByteCount(1), Data(0x01) -> Input ON
        let response_adu = [0x00, 0x11, 0x00, 0x00, 0x00, 0x04, 0x01, 0x02, 0x01, 0x01];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_discrete_input_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_inputs, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_inputs.from_address(), address);
        assert_eq!(rcv_inputs.quantity(), 1);
        assert_eq!(rcv_inputs.value(address).unwrap(), true);
        assert_eq!(*rcv_quantity, 1);
    }

    /// Test case: `ClientServices` successfully sends and processes a `read_device_identification` request.
    #[test]
    fn test_client_services_read_device_identification_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 20;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let read_code = ReadDeviceIdCode::Basic;
        let object_id = ObjectId::from(0x00);

        client_services
            .read_device_identification(txn_id, unit_id, read_code, object_id)
            .unwrap();

        // Verify request ADU
        let sent_frames = client_services.transport.sent_frames.borrow();
        assert_eq!(sent_frames.len(), 1);
        // MBAP(7) + PDU(4) = 11 bytes
        // MBAP: 00 14 00 00 00 05 01
        // PDU: 2B 0E 01 00
        let expected_request = [
            0x00, 0x14, 0x00, 0x00, 0x00, 0x05, 0x01, 0x2B, 0x0E, 0x01, 0x00,
        ];
        assert_eq!(sent_frames.front().unwrap().as_slice(), &expected_request);
        drop(sent_frames);

        // Simulate response:
        // MEI(0E), Code(01), Conf(81), More(00), Next(00), Num(01), Obj0(00), Len(03), Val("Foo")
        // PDU Len = 1(MEI) + 1(Code) + 1(Conf) + 1(More) + 1(Next) + 1(Num) + 1(Id) + 1(Len) + 3(Val) = 11
        // MBAP Len = 1(Unit) + 1(FC) + 11 = 13
        let response_adu = [
            0x00, 0x14, 0x00, 0x00, 0x00, 0x0D, 0x01, 0x2B, 0x0E, 0x01, 0x81, 0x00, 0x00, 0x01,
            0x00, 0x03, 0x46, 0x6F, 0x6F,
        ];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();
        client_services.poll();

        let received_responses = client_services
            .app
            .received_read_device_id_responses
            .borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_resp) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_resp.read_device_id_code, ReadDeviceIdCode::Basic);
        assert_eq!(
            rcv_resp.conformity_level,
            ConformityLevel::BasicStreamAndIndividual
        );
        assert_eq!(rcv_resp.objects_data.len(), 5); // Id(1)+Len(1)+Val(3)
    }

    /// Test case: `ClientServices` handles multiple concurrent `read_device_identification` requests.
    #[test]
    fn test_client_services_read_device_identification_multi_transaction() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        // Request 1
        let txn_id_1 = 21;
        client_services
            .read_device_identification(
                txn_id_1,
                unit_id,
                ReadDeviceIdCode::Basic,
                ObjectId::from(0x00),
            )
            .unwrap();

        // Request 2
        let txn_id_2 = 22;
        client_services
            .read_device_identification(
                txn_id_2,
                unit_id,
                ReadDeviceIdCode::Regular,
                ObjectId::from(0x00),
            )
            .unwrap();

        assert_eq!(client_services.transport.sent_frames.borrow().len(), 2);

        // Response for Request 2 (Out of order arrival)
        // MEI(0E), Code(02), Conf(82), More(00), Next(00), Num(00)
        // PDU Len = 6. MBAP Len = 1 + 1 + 6 = 8.
        let response_adu_2 = [
            0x00, 0x16, 0x00, 0x00, 0x00, 0x08, 0x01, 0x2B, 0x0E, 0x02, 0x82, 0x00, 0x00, 0x00,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu_2).unwrap())
            .unwrap();

        client_services.poll();

        {
            let received_responses = client_services
                .app
                .received_read_device_id_responses
                .borrow();
            assert_eq!(received_responses.len(), 1);
            assert_eq!(received_responses[0].0, txn_id_2);
            assert_eq!(
                received_responses[0].2.read_device_id_code,
                ReadDeviceIdCode::Regular
            );
        }

        // Response for Request 1
        // MEI(0E), Code(01), Conf(81), More(00), Next(00), Num(00)
        let response_adu_1 = [
            0x00, 0x15, 0x00, 0x00, 0x00, 0x08, 0x01, 0x2B, 0x0E, 0x01, 0x81, 0x00, 0x00, 0x00,
        ];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu_1).unwrap())
            .unwrap();

        client_services.poll();

        {
            let received_responses = client_services
                .app
                .received_read_device_id_responses
                .borrow();
            assert_eq!(received_responses.len(), 2);
            assert_eq!(received_responses[1].0, txn_id_1);
            assert_eq!(
                received_responses[1].2.read_device_id_code,
                ReadDeviceIdCode::Basic
            );
        }
    }

    /// Test case: `ClientServices` rejects a response where the echoed Read Device ID Code does not match the request.
    #[test]
    fn test_client_services_read_device_identification_mismatch_code() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 30;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        // We request BASIC (0x01)
        client_services
            .read_device_identification(
                txn_id,
                unit_id,
                ReadDeviceIdCode::Basic,
                ObjectId::from(0x00),
            )
            .unwrap();

        // Server responds with REGULAR (0x02) - This is a protocol violation or mismatch
        // MEI(0E), Code(02), Conf(81), More(00), Next(00), Num(00)
        let response_adu = [
            0x00, 0x1E, 0x00, 0x00, 0x00, 0x08, 0x01, 0x2B, 0x0E, 0x02, 0x81, 0x00, 0x00, 0x00,
        ];

        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&response_adu).unwrap())
            .unwrap();

        client_services.poll();

        // Verify success callback was NOT called
        assert!(
            client_services
                .app
                .received_read_device_id_responses
                .borrow()
                .is_empty()
        );

        // Verify failure callback WAS called with UnexpectedResponse
        let failed = client_services.app.failed_requests.borrow();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].2, MbusError::InvalidDeviceIdentification);
    }

    /// Test case: `read_exception_status` sends a valid ADU and processes response.
    #[test]
    fn test_client_services_read_exception_status_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 40;
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();

        let err = client_services.read_exception_status(txn_id, unit_id).err();
        // Error is expected since the service only available in serial transport.
        assert_eq!(err, Some(MbusError::InvalidTransport));
    }

    /// Test case: `diagnostics` (Sub-function 00) Query Data sends valid ADU.
    #[test]
    fn test_client_services_diagnostics_query_data_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let data = [0x1234, 0x5678];
        let sub_function = DiagnosticSubFunction::ReturnQueryData;
        let err = client_services
            .diagnostics(50, unit_id, sub_function, &data)
            .err();
        assert_eq!(err, Some(MbusError::InvalidTransport));
    }

    /// Test case: `get_comm_event_counter` sends valid ADU.
    #[test]
    fn test_client_services_get_comm_event_counter_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();
        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let err = client_services.get_comm_event_counter(60, unit_id).err();

        assert_eq!(err, Some(MbusError::InvalidTransport));
    }

    /// Test case: `report_server_id` sends valid ADU.
    #[test]
    fn test_client_services_report_server_id_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let unit_id = UnitIdOrSlaveAddr::new(0x01).unwrap();
        let err = client_services.report_server_id(70, unit_id).err();

        assert_eq!(err, Some(MbusError::InvalidTransport));
    }

    // --- Broadcast Tests ---

    /// Test case: Broadcast read multiple coils is not allowed
    #[test]
    fn test_broadcast_read_multiple_coils_not_allowed() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
        let address = 0x0000;
        let quantity = 8;
        let res = client_services.read_multiple_coils(txn_id, unit_id, address, quantity);
        assert_eq!(res.unwrap_err(), MbusError::BoradcastNotAllowed);
    }

    /// Test case: Broadcast write single coil on TCP is not allowed
    #[test]
    fn test_broadcast_write_single_coil_tcp_not_allowed() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0002;
        let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
        let res = client_services.write_single_coil(txn_id, unit_id, 0x0000, true);
        assert_eq!(res.unwrap_err(), MbusError::BoradcastNotAllowed);
    }

    /// Test case: Broadcast write multiple coils on TCP is not allowed
    #[test]
    fn test_broadcast_write_multiple_coils_tcp_not_allowed() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0003;
        let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
        let mut values = Coils::new(0x0000, 2);
        values.set_value(0x0000, true).unwrap();
        values.set_value(0x0001, false).unwrap();

        let res = client_services.write_multiple_coils(txn_id, unit_id, 0x0000, &values);
        assert_eq!(res.unwrap_err(), MbusError::BoradcastNotAllowed);
    }

    /// Test case: Broadcast read discrete inputs is not allowed
    #[test]
    fn test_broadcast_read_discrete_inputs_not_allowed() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        let txn_id = 0x0006;
        let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
        let res = client_services.read_discrete_inputs(txn_id, unit_id, 0x0000, 2);
        assert_eq!(res.unwrap_err(), MbusError::BoradcastNotAllowed);
    }

    /// Test case: `poll` clears the internal receive buffer if it overflows with garbage bytes.
    /// This simulates a high-noise environment where fragments accumulate beyond `MAX_ADU_FRAME_LEN`.
    #[test]
    fn test_client_services_clears_buffer_on_overflow() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 10>::new(transport, app, config).unwrap();

        // Fill the internal buffer close to its capacity (MAX_ADU_FRAME_LEN = 513) with unparsable garbage
        let initial_garbage = [0xFF; MAX_ADU_FRAME_LEN - 10];
        client_services.rxed_frame.extend_from_slice(&initial_garbage).unwrap();

        // Inject another chunk of bytes that will cause an overflow when appended
        let chunk = [0xAA; 20];
        client_services
            .transport
            .recv_frames
            .borrow_mut()
            .push_back(Vec::from_slice(&chunk).unwrap())
            .unwrap();

        // Poll should attempt to extend the buffer, fail because 503 + 20 > 513, and clear the buffer to recover.
        client_services.poll();

        assert!(
            client_services.rxed_frame.is_empty(),
            "Buffer should be cleared on overflow to prevent crashing and recover from stream noise."
        );
    }
}
