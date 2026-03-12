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

use heapless::Vec;

use crate::{
    app::{
        CoilResponse, DiagnosticsResponse, DiscreteInputResponse, FifoQueueResponse,
        FileRecordResponse, RegisterResponse, RequestErrorNotifier,
    },
    client::services::{diagnostics::DiagnosticsService, file_record::SubRequest},
    data_unit::{
        common::{self, MAX_ADU_FRAME_LEN, MbapHeader},
        tcp::ModbusTcpMessage,
    },
    device_identification::{ObjectId, ReadDeviceIdCode},
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode},
    transport::{ModbusConfig, TimeKeeper, Transport, TransportType},
};

pub mod coils;
pub mod diagnostics;
pub mod discrete_inputs;
pub mod fifo;
pub mod file_record;
pub mod registers;

/// Represents the type of response we expect for a given request,
/// along with any necessary metadata to validate and process the response when it arrives,
/// and for handling retries and timeouts.
#[derive(Debug, Default)]
enum ExpectedResponseType {
    /// Undefined response type, used as a default value. Should not be used in practice.
    /// This variant is marked as `#[default]` for `ExpectedResponse` to derive `Default`.
    /// It should ideally never be used in a live `ExpectedResponse` instance.
    #[default]
    Undefined,
    /// Expected response for a Read Coils request, includes metadata to validate the response.
    ReadCoils {
        /// The quantity of coils expected in the response.
        expected_quantity: u16,
        /// The starting address of the coils read.
        from_address: u16,
        /// Indicates if the original request was for a single coil.
        single_read: bool,
    },
    /// Expected response for a Read Discrete Inputs request, includes metadata to validate the response.
    ReadDiscreteInputs {
        /// The quantity of discrete inputs expected in the response.
        expected_quantity: u16,
        /// The starting address of the discrete inputs read.
        from_address: u16,
        /// Indicates if the original request was for a single discrete input.
        single_read: bool,
    },
    /// Expected response for a Write Single Coil request, includes metadata to validate the response.
    WriteSingleCoil { address: u16, value: bool },
    /// Expected response for a Write Multiple Coils request, includes metadata to validate the response.
    WriteMultipleCoils { address: u16, quantity: u16 },
    /// Expected response for a Read Holding Registers request, includes metadata to validate the response.
    ReadHoldingRegisters {
        /// The quantity of holding registers expected in the response.
        expected_quantity: u16,
        /// The starting address of the holding registers read.
        from_address: u16,
        /// Indicates if the original request was for a single holding register.
        single_read: bool,
    },
    /// Expected response for a Read Input Registers request, includes metadata to validate the response.
    ReadInputRegisters {
        /// The quantity of input registers expected in the response.
        expected_quantity: u16,
        /// The starting address of the input registers read.
        from_address: u16,
        /// Indicates if the original request was for a single input register.
        single_read: bool,
    },
    /// Expected response for a Write Single Register request, includes metadata to validate the response.
    WriteSingleRegister { address: u16, value: u16 },
    /// Expected response for a Write Multiple Registers request, includes metadata to validate the response.
    WriteMultipleRegisters { address: u16, quantity: u16 },
    /// Expected response for a Read/Write Multiple Registers request, includes metadata to validate the response.
    ReadWriteMultipleRegisters {
        /// The starting address of the registers to read.
        read_address: u16,
        /// The quantity of registers to read.
        read_quantity: u16,
    },
    /// Expected response for a Mask Write Register request.
    MaskWriteRegister {
        /// The address of the register to mask write.
        address: u16,
        /// The AND mask used in the request.
        and_mask: u16,
        /// The OR mask used in the request.
        or_mask: u16,
    },
    /// Expected response for a Read FIFO Queue request.
    ReadFifoQueue,
    /// Expected response for a Read File Record request.
    ReadFileRecord,
    /// Expected response for a Write File Record request.
    WriteFileRecord,
    /// Expected response for a Read Device Identification request.
    ReadDeviceIdentification {
        read_device_id_code: ReadDeviceIdCode,
    },
    /// Expected response for a generic Encapsulated Interface Transport request.
    EncapsulatedInterfaceTransport { mei_type: EncapsulatedInterfaceType },
    /// Expected response for Read Exception Status (FC 07).
    ReadExceptionStatus,
    /// Expected response for Diagnostics (FC 08).
    Diagnostics { sub_function: DiagnosticSubFunction },
    /// Expected response for Get Comm Event Counter (FC 11).
    GetCommEventCounter,
    /// Expected response for Get Comm Event Log (FC 12).
    GetCommEventLog,
    /// Expected response for Report Server ID (FC 17).
    ReportServerId,
}

/// Represents an expected response for a previously sent request,
/// including the transaction ID, unit ID, and the type of response expected.
#[derive(Debug, Default)]
pub struct ExpectedResponse {
    txn_id: u16,
    unit_id: u8,
    original_adu: Vec<u8, MAX_ADU_FRAME_LEN>, // Store the original ADU for re-sending on retry
    sent_timestamp: u64, // Timestamp when the request was last sent (in milliseconds)
    retries_left: u8,    // Number of retries remaining
    response_type: ExpectedResponseType,
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
    /// A buffer to store the received frame.
    rxed_frame: Vec<u8, MAX_ADU_FRAME_LEN>,

    /// Service struct for constructing coil-related requests and parsing responses.
    coil_service: coils::CoilService,
    /// Service struct for constructing register-related requests and parsing responses.
    register_service: registers::RegisterService,
    /// Service struct for constructing FIFO queue-related requests and parsing responses.
    fifo_queue_service: fifo::FifoQueueService,
    /// Service struct for constructing file record-related requests and parsing responses.
    file_record_service: file_record::FileRecordService,
    /// Service struct for constructing discrete input-related requests and parsing responses.
    discrete_input_service: discrete_inputs::DiscreteInputService,
    /// Service struct for constructing diagnostics-related requests and parsing responses.
    diagnostics_service: DiagnosticsService,

    config: ModbusConfig,

    /// Queue of expected responses for sent requests, used to match incoming responses with their corresponding requests.
    expected_responses: Vec<ExpectedResponse, N>,
}

/// Implementation of core client services, including methods for sending requests and processing responses.
impl<
    TRANSPORT: Transport,
    APP: RequestErrorNotifier
        + CoilResponse
        + RegisterResponse
        + FifoQueueResponse
        + FileRecordResponse
        + DiscreteInputResponse
        + DiagnosticsResponse
        + TimeKeeper,
    const N: usize,
> ClientServices<TRANSPORT, APP, N>
{
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
            register_service: registers::RegisterService::new(),
            coil_service: coils::CoilService::new(),
            fifo_queue_service: fifo::FifoQueueService::new(),
            discrete_input_service: discrete_inputs::DiscreteInputService::new(),
            file_record_service: file_record::FileRecordService::new(),
            diagnostics_service: DiagnosticsService::new(),
            config,
            expected_responses: Vec::new(),
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

    /// Polls the transport layer for incoming Modbus frames and processes them.
    /// It also handles timeouts and retries for outstanding requests, using the application's `TimeKeeper` for current time.
    ///
    /// # Arguments
    /// * `current_millis` - The current monotonic time in milliseconds.
    pub fn poll(&mut self) {
        // 1. Attempt to receive a frame
        match self.transport.recv() {
            Ok(frame) => {
                self.rxed_frame.extend(frame);

                // If a frame is received, ingest it
                match self.ingest_frame() {
                    Ok(_) => {
                        self.rxed_frame.clear();
                    }
                    Err(_) => {}
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
        let current_millis = self.app.current_millis();
        let response_timeout_ms = self.response_timeout_ms();
        let expected_responses = &mut self.expected_responses;
        let mut i = 0;
        while i < expected_responses.len() {
            let expected_response = &mut expected_responses[i];
            if current_millis
                .checked_sub(expected_response.sent_timestamp)
                .unwrap_or(0)
                > response_timeout_ms
            {
                // Request timed out
                if expected_response.retries_left > 0 {
                    // Retry the request
                    expected_response.retries_left -= 1;
                    expected_response.sent_timestamp = current_millis;
                    // Re-send the original ADU
                    if let Err(_e) = self.transport.send(&expected_response.original_adu) {
                        // If re-sending fails
                        // If re-sending fails, treat as a failed request
                        let response = expected_responses.swap_remove(i);
                        self.app.request_failed(
                            response.txn_id,
                            response.unit_id,
                            MbusError::SendFailed,
                        );
                        continue; // Move to the next item in the (now shorter) vec
                    }
                } else {
                    // No retries left, report timeout to application
                    let response = expected_responses.swap_remove(i); // Remove the timed-out request
                    self.app.request_failed(
                        response.txn_id,
                        response.unit_id,
                        MbusError::NoRetriesLeft,
                    );
                    continue; // Move to the next item in the (now shorter) vec
                }
            }
            i += 1;
        }
    }

    /// Sends a Read Coils request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the coils to read.
    /// - `quantity`: The number of coils to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = self.coil_service.read_coils(
            txn_id,
            unit_id,
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadCoils {
                    expected_quantity: quantity,
                    from_address: address,
                    single_read: false,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Single Coil request to the specified unit ID and address, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the coil to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    ///
    /// This method is a convenience wrapper around `read_multiple_coils` for reading a single coil, which simplifies the application logic when only one coil needs to be read.
    pub fn read_single_coil(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = self
            .coil_service
            .read_coils(txn_id, unit_id, address, 1, transport_type)?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadCoils {
                    expected_quantity: 1,
                    from_address: address,
                    single_read: true,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Discrete Inputs request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the inputs to read.
    /// - `quantity`: The number of inputs to read.
    pub fn read_discrete_inputs(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = self.discrete_input_service.read_discrete_inputs(
            txn_id,
            unit_id,
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadDiscreteInputs {
                    expected_quantity: quantity,
                    from_address: address,
                    single_read: false,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Discrete Inputs request for a single input.
    pub fn read_single_discrete_input(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        let frame = self.discrete_input_service.read_discrete_inputs(
            txn_id,
            unit_id,
            address,
            1,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadDiscreteInputs {
                    expected_quantity: 1,
                    from_address: address,
                    single_read: true,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Write Single Coil request to the specified unit ID and address with the given value, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the coil to write.
    /// - `value`: The boolean value to write to the coil (true for ON, false for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_single_coil(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let frame =
            self.coil_service
                .write_single_coil(txn_id, unit_id, address, value, transport_type)?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::WriteSingleCoil { address, value },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write Multiple Coils request to the specified unit ID and address with the given values, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the coils to write.
    /// - `quantity`: The number of coils to write.
    /// - `values`: A slice of boolean values to write to the coils (true for ON, false for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[bool],
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let frame = self.coil_service.write_multiple_coils(
            txn_id,
            unit_id,
            address,
            quantity,
            values,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::WriteMultipleCoils { address, quantity },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read Holding Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the holding registers to read.
    /// - `quantity`: The number of holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_holding_registers(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = self.register_service.read_holding_registers(
            txn_id,
            unit_id,
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadHoldingRegisters {
                    expected_quantity: quantity,
                    from_address: address,
                    single_read: false,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Holding Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_single_holding_register(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        let frame = self.register_service.read_holding_registers(
            txn_id,
            unit_id,
            address,
            1, // quantity = 1
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadHoldingRegisters {
                    expected_quantity: 1,
                    from_address: address,
                    single_read: true,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Input Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the input registers to read.
    /// - `quantity`: The number of input registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_input_registers(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = self.register_service.read_input_registers(
            txn_id,
            unit_id,
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadInputRegisters {
                    expected_quantity: quantity,
                    from_address: address,
                    single_read: false,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Input Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the input registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_single_input_register(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        let frame = self.register_service.read_input_registers(
            txn_id,
            unit_id,
            address,
            1,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadInputRegisters {
                    expected_quantity: 1,
                    from_address: address,
                    single_read: true,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Write Single Register request to the specified unit ID and address with the given value, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the register to write.
    /// - `value`: The `u16` value to write to the register.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_single_register(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = self.register_service.write_single_register(
            txn_id,
            unit_id,
            address,
            value,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::WriteSingleRegister { address, value },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write Multiple Registers request to the specified unit ID and address with the given values, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the registers to write.
    /// - `quantity`: The number of registers to write.
    /// - `values`: A slice of `u16` values to write to the registers.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_multiple_registers(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = self.register_service.write_multiple_registers(
            txn_id,
            unit_id,
            address,
            quantity,
            values,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::WriteMultipleRegisters { address, quantity },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read/Write Multiple Registers request.
    pub fn read_write_multiple_registers(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = self.register_service.read_write_multiple_registers(
            txn_id,
            unit_id,
            read_address,
            read_quantity,
            write_address,
            write_values,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadWriteMultipleRegisters {
                    read_address,
                    read_quantity,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Mask Write Register request.
    pub fn mask_write_register(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        let frame = self.register_service.mask_write_register(
            txn_id,
            unit_id,
            address,
            and_mask,
            or_mask,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::MaskWriteRegister {
                    address,
                    and_mask,
                    or_mask,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read FIFO Queue request.
    pub fn read_fifo_queue(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        let frame = self.fifo_queue_service.read_fifo_queue(
            txn_id,
            unit_id,
            address,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadFifoQueue,
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read File Record request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `sub_request`: The sub-request structure containing file/record details.
    pub fn read_file_record(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        let frame = self.file_record_service.read_file_record(
            txn_id,
            unit_id,
            sub_request,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadFileRecord,
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write File Record request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `sub_request`: The sub-request structure containing file/record details and data.
    pub fn write_file_record(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        let frame = self.file_record_service.write_file_record(
            txn_id,
            unit_id,
            sub_request,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::WriteFileRecord,
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read Device Identification request (FC 43 / 14).
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `read_device_id_code`: The type of access (01=Basic, 02=Regular, 03=Extended, 04=Specific).
    /// - `object_id`: The object ID to start reading from.
    pub fn read_device_identification(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<(), MbusError> {
        let frame = self.diagnostics_service.read_device_identification(
            txn_id,
            unit_id,
            read_device_id_code,
            object_id,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadDeviceIdentification {
                    read_device_id_code,
                },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a generic Encapsulated Interface Transport request (FC 43).
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `mei_type`: The MEI type (e.g., `CanopenGeneralReference`).
    /// - `data`: The data payload to be sent with the request.
    pub fn encapsulated_interface_transport(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) -> Result<(), MbusError> {
        let frame = self.diagnostics_service.encapsulated_interface_transport(
            txn_id,
            unit_id,
            mei_type,
            data,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::EncapsulatedInterfaceTransport { mei_type },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read Exception Status request (FC 07). Serial Line only.
    pub fn read_exception_status(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = self
            .diagnostics_service
            .read_exception_status(unit_id, self.transport.transport_type())?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReadExceptionStatus,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Diagnostics request (FC 08). Serial Line only.
    pub fn diagnostics(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<(), MbusError> {
        let frame = self.diagnostics_service.diagnostics(
            unit_id,
            sub_function,
            data,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::Diagnostics { sub_function },
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Counter request (FC 11). Serial Line only.
    pub fn get_comm_event_counter(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = self
            .diagnostics_service
            .get_comm_event_counter(unit_id, self.transport.transport_type())?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::GetCommEventCounter,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Log request (FC 12). Serial Line only.
    pub fn get_comm_event_log(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = self
            .diagnostics_service
            .get_comm_event_log(unit_id, self.transport.transport_type())?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::GetCommEventLog,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Report Server ID request (FC 17). Serial Line only.
    pub fn report_server_id(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = self
            .diagnostics_service
            .report_server_id(unit_id, self.transport.transport_type())?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                response_type: ExpectedResponseType::ReportServerId,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Ingests received Modbus frames from the transport layer.
    fn ingest_frame(&mut self) -> Result<(), MbusError> {
        let frame = self.rxed_frame.as_slice();
        // Changed to &mut self, removed transport param
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let message = match common::decompile_adu_frame(frame, transport_type) {
            Ok(value) => value,
            Err(err) => {
                return Err(err); // Malformed frame or parsing error, frame is dropped.
            }
        };
        use crate::data_unit::common::AdditionalAddress;
        use crate::transport::TransportType::*;
        let message = match self.transport.transport_type() {
            StdTcp | CustomTcp => {
                let mbap_header = match message.additional_address() {
                    AdditionalAddress::MbapHeader(header) => header,
                    _ => return Ok(()),
                };
                ModbusTcpMessage::new(*mbap_header, message.pdu)
            }
            StdSerial(_) | CustomSerial(_) => {
                let slave_addr = match message.additional_address() {
                    AdditionalAddress::SlaveAddress(addr) => addr.address(),
                    _ => return Ok(()),
                };

                // For Serial, we assume the response matches the first expected response (FIFO).
                // We borrow the TxnID from there to satisfy the matcher.
                let txn_id = self
                    .expected_responses
                    .first()
                    .map(|r| r.txn_id)
                    .unwrap_or(0);

                let mbap = MbapHeader::new(txn_id, 0, slave_addr);
                ModbusTcpMessage::new(mbap, message.pdu)
            }
        };

        let mbap_header = message.mbap_header();
        let function_code = message.function_code();
        self.handle_response(&message, mbap_header, function_code);

        Ok(())
    }

    /// Handles incoming Modbus responses by matching them with expected responses and invoking the appropriate application callbacks.
    ///
    /// # Parameters
    /// - `message`: The decoded Modbus message containing the MBAP header and PDU.
    /// - `mbap_header`: The MBAP header extracted from the message, used to match the response with the corresponding request.
    /// - `function_code`: The function code from the PDU, used to determine how to process the response.
    ///
    /// This method looks up the expected response based on the transaction ID and unit ID from the MBAP header. If a matching expected response is found, it processes the response according to its type (e.g., Read Coils, Write Single Coil) and invokes the corresponding callback on the application layer. If no matching expected response is found, it ignores the response (as it may be unsolicited or a late response to a previous request).
    fn handle_response(
        &mut self,
        message: &ModbusTcpMessage,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
    ) {
        // Find the matching expected response and its index
        let index = self
            .expected_responses
            .iter()
            .enumerate()
            .find(|(_idx, r)| {
                r.txn_id == mbap_header.transaction_id && r.unit_id == mbap_header.unit_id
            })
            .map(|(idx, _)| idx);

        let expected_response = match index {
            Some(idx) => self.expected_responses.swap_remove(idx),
            None => {
                // No matching request found, ignore response (e.g., unsolicited, or already timed out/retried out)
                // FUTURE: Potentially log this as an unexpected response.
                return;
            }
        };
        let pdu = message.pdu();

        if pdu.error_code().is_some() {
            // The response is an error response. Notify the application of the error.
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::ModbusException(pdu.error_code().unwrap()),
            );
            return; // No further processing needed for error responses.
        }

        match expected_response.response_type {
            ExpectedResponseType::ReadCoils {
                expected_quantity,
                from_address,
                single_read,
            } => {
                self.handle_read_coils_response(
                    mbap_header,
                    function_code,
                    pdu,
                    expected_quantity,
                    from_address,
                    single_read,
                );
            }
            ExpectedResponseType::ReadDiscreteInputs {
                expected_quantity,
                from_address,
                single_read,
            } => {
                self.handle_read_discrete_inputs_response(
                    mbap_header,
                    function_code,
                    pdu,
                    expected_quantity,
                    from_address,
                    single_read,
                );
            }
            ExpectedResponseType::WriteSingleCoil { address, value } => {
                self.handle_write_single_coil_response(
                    mbap_header,
                    function_code,
                    pdu,
                    address,
                    value,
                );
            }
            ExpectedResponseType::WriteMultipleCoils { address, quantity } => {
                self.handle_write_multiple_coils_response(
                    mbap_header.transaction_id,
                    mbap_header.unit_id,
                    function_code,
                    pdu,
                    address,
                    quantity,
                );
            }
            ExpectedResponseType::ReadHoldingRegisters {
                expected_quantity,
                from_address,
                single_read,
            } => {
                self.handle_read_holding_registers_response(
                    mbap_header,
                    function_code,
                    pdu,
                    expected_quantity,
                    from_address,
                    single_read,
                );
            }
            ExpectedResponseType::ReadInputRegisters {
                expected_quantity,
                from_address,
                single_read,
            } => {
                self.handle_read_input_registers_response(
                    mbap_header,
                    function_code,
                    pdu,
                    expected_quantity,
                    from_address,
                    single_read,
                );
            }
            ExpectedResponseType::WriteSingleRegister { address, value } => {
                self.handle_write_single_register_response(
                    mbap_header,
                    function_code,
                    pdu,
                    address,
                    value,
                );
            }
            ExpectedResponseType::WriteMultipleRegisters { address, quantity } => {
                self.handle_write_multiple_registers_response(
                    mbap_header.transaction_id,
                    mbap_header.unit_id,
                    function_code,
                    pdu,
                    address,
                    quantity,
                );
            }
            ExpectedResponseType::ReadWriteMultipleRegisters {
                read_address,
                read_quantity,
            } => {
                self.handle_read_write_multiple_registers_response(
                    &mbap_header,
                    function_code,
                    pdu,
                    read_address,
                    read_quantity,
                );
            }
            ExpectedResponseType::MaskWriteRegister {
                address,
                and_mask,
                or_mask,
            } => {
                self.handle_mask_write_register_response(
                    mbap_header,
                    function_code,
                    pdu,
                    address,
                    and_mask,
                    or_mask,
                );
            }
            ExpectedResponseType::ReadFifoQueue { .. } => {
                self.handle_read_fifo_queue_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::ReadFileRecord => {
                self.handle_read_file_record_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::WriteFileRecord => {
                self.handle_write_file_record_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::ReadDeviceIdentification {
                read_device_id_code,
            } => {
                self.handle_read_device_identification_response(
                    mbap_header,
                    function_code,
                    pdu,
                    read_device_id_code,
                );
            }
            ExpectedResponseType::EncapsulatedInterfaceTransport { mei_type } => {
                self.handle_encapsulated_interface_transport_response(
                    mbap_header,
                    function_code,
                    pdu,
                    mei_type,
                );
            }
            ExpectedResponseType::ReadExceptionStatus => {
                self.handle_read_exception_status_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::Diagnostics { sub_function } => {
                self.handle_diagnostics_response(mbap_header, function_code, pdu, sub_function);
            }
            ExpectedResponseType::GetCommEventCounter => {
                self.handle_get_comm_event_counter_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::GetCommEventLog => {
                self.handle_get_comm_event_log_response(mbap_header, function_code, pdu);
            }
            ExpectedResponseType::ReportServerId => {
                self.handle_report_server_id_response(mbap_header, function_code, pdu);
            }

            ExpectedResponseType::Undefined => {
                // Control is never expected to reach here since Undefined is only a default placeholder
                // for `ExpectedResponseType`.
                // but we handle it just in case.
            }
        }
    } // End of handle_response

    /// Handles a Read Coils response by validating it against the expected response metadata and invoking the appropriate application callback.
    ///
    /// # Parameters
    /// - `mbap_header`: The MBAP header from the received message, used to extract transaction ID and unit ID for callbacks.
    /// - `function_code`: The function code from the PDU, used to determine how to parse the response.
    /// - `pdu`: The PDU from the received message, containing the actual response data to be parsed.
    /// - `expected_quantity`: The number of coils that were expected in the response, used for validation.
    /// - `from_address`: The starting address of the coils that were requested, used for validation.
    /// - `single_read`: A boolean indicating whether this was a single coil read request, which affects how the response is processed and which callback is invoked.
    ///
    /// This method uses the coil service to parse the response PDU and validate it against the expected quantity and address. If it's a single read, it extracts the single coil value and invokes the `read_single_coil_response` callback. If it's a multiple read, it invokes the `read_coils_response` callback with the full coil response. If parsing or validation fails at any point, it simply returns without invoking callbacks (as there's no valid data to report).
    fn handle_read_coils_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_quantity: u16,
        from_address: u16,
        single_read: bool,
    ) {
        let coil_rsp = match self.coil_service.handle_read_coil_rsp(
            function_code,
            pdu,
            expected_quantity,
            from_address,
        ) {
            Ok(coil_response) => coil_response,
            Err(e) => {
                // Parsing or validation of the coil response failed. The response is dropped.
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };
        if single_read {
            // For single read, extract the value of the single coil; bail out if none.
            let coil_value = match coil_rsp.values().first() {
                Some(v) => v,
                None => return, // Err(MbusError::ParseError), // nothing to report, drop the response
            }; // If no value is found for a single coil, the response is dropped.

            self.app.read_single_coil_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                from_address,
                *coil_value != 0, // Convert to bool
            );
        } else {
            self.app.read_coils_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &coil_rsp,
                expected_quantity, // Pass the original expected quantity
            );
        }
    }

    fn handle_read_discrete_inputs_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_quantity: u16,
        from_address: u16,
        single_read: bool,
    ) {
        let inputs_rsp = match self.discrete_input_service.handle_read_discrete_inputs_rsp(
            function_code,
            pdu,
            expected_quantity,
            from_address,
        ) {
            Ok(response) => response,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        if single_read {
            let value = match inputs_rsp.value(from_address) {
                Ok(v) => v,
                Err(err) => {
                    self.app
                        .request_failed(mbap_header.transaction_id, mbap_header.unit_id, err);
                    return;
                }
            };
            self.app.read_single_discrete_input_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                from_address,
                value,
            );
        } else {
            self.app.read_discrete_inputs_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &inputs_rsp,
                expected_quantity,
            );
        }
    }

    /// Handles a Write Single Coil response by invoking the appropriate application callback.
    fn handle_write_single_coil_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        value: bool,
    ) {
        if self
            .coil_service
            .handle_write_single_coil_rsp(function_code, pdu, address, value)
            .is_ok()
        {
            // If successful
            self.app.write_single_coil_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                address,
                value,
            );
        } else {
            // If parsing or validation fails
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::ParseError,
            );
        }
    }

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback.
    fn handle_write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        quantity: u16,
    ) {
        if self
            .coil_service
            .handle_write_multiple_coils_rsp(function_code, pdu, address, quantity)
            .is_ok()
        {
            // If successful
            self.app
                .write_multiple_coils_response(txn_id, unit_id, address, quantity);
        } else {
            // If parsing or validation fails
            self.app
                .request_failed(txn_id, unit_id, MbusError::ParseError);
        }
    }

    /// Handles a Read Holding Registers response by validating it against the expected response metadata and invoking the appropriate application callback.
    fn handle_read_holding_registers_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_quantity: u16,
        from_address: u16,
        single_read: bool,
    ) {
        let register_rsp = match self.register_service.handle_read_holding_register_rsp(
            function_code,
            pdu,
            expected_quantity,
            from_address,
        ) {
            Ok(register_response) => register_response,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        if single_read {
            let value = register_rsp.values().get(0).copied().unwrap_or(0);
            self.app.read_single_holding_register_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                from_address,
                value,
            );
        } else {
            self.app.read_holding_registers_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &register_rsp,
            );
        }
    }

    /// Handles a Read Input Registers response by validating it against the expected response metadata and invoking the appropriate application callback.
    fn handle_read_input_registers_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_quantity: u16,
        from_address: u16,
        single_read: bool,
    ) {
        let register_rsp = match self.register_service.handle_read_input_register_rsp(
            function_code,
            pdu,
            expected_quantity,
            from_address,
        ) {
            Ok(register_response) => register_response,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };
        if single_read {
            let value = match register_rsp.value(from_address) {
                Ok(v) => v,
                Err(err) => {
                    self.app
                        .request_failed(mbap_header.transaction_id, mbap_header.unit_id, err);
                    return;
                }
            };
            self.app.read_single_input_register_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                from_address,
                value,
            );
        } else {
            self.app.read_input_register_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &register_rsp,
            );
        }
    }

    /// Handles a Write Single Register response by invoking the appropriate application callback.
    fn handle_write_single_register_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        value: u16,
    ) {
        if self
            .register_service
            .handle_write_single_register_rsp(function_code, pdu, address, value)
            .is_ok()
        {
            self.app.write_single_register_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                address,
                value,
            );
        } else {
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::ParseError,
            );
        }
    }

    /// Handles a Write Multiple Registers response by invoking the appropriate application callback.
    fn handle_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        quantity: u16,
    ) {
        if self
            .register_service
            .handle_write_multiple_registers_rsp(function_code, pdu, address, quantity)
            .is_ok()
        {
            self.app
                .write_multiple_registers_response(txn_id, unit_id, address, quantity);
        } else {
            self.app
                .request_failed(txn_id, unit_id, MbusError::ParseError);
        }
    }

    fn handle_read_write_multiple_registers_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        read_address: u16,
        read_quantity: u16,
    ) {
        let register_rsp = match self
            .register_service
            .handle_read_write_multiple_registers_rsp(
                function_code,
                pdu,
                read_quantity,
                read_address,
            ) {
            Ok(register_response) => register_response,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        self.app.read_write_multiple_registers_response(
            mbap_header.transaction_id,
            mbap_header.unit_id,
            &register_rsp,
        );
    }

    fn handle_mask_write_register_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) {
        if self
            .register_service
            .handle_mask_write_register_rsp(function_code, pdu, address, and_mask, or_mask)
            .is_ok()
        {
            self.app
                .mask_write_register_response(mbap_header.transaction_id, mbap_header.unit_id);
        } else {
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::ParseError,
            );
        }
    }

    fn handle_read_fifo_queue_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        let register_rsp = match self
            .fifo_queue_service
            .handle_read_fifo_queue_rsp(function_code, pdu)
        {
            Ok(register_response) => register_response,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        self.app.read_fifo_queue_response(
            mbap_header.transaction_id,
            mbap_header.unit_id,
            &register_rsp,
        );
    }

    fn handle_read_file_record_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        let data = match self
            .file_record_service
            .handle_read_file_record_rsp(function_code, pdu)
        {
            Ok(d) => d,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        self.app
            .read_file_record_response(mbap_header.transaction_id, mbap_header.unit_id, &data);
    }

    fn handle_write_file_record_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        if self
            .file_record_service
            .handle_write_file_record_rsp(function_code, pdu)
            .is_ok()
        {
            self.app
                .write_file_record_response(mbap_header.transaction_id, mbap_header.unit_id);
        } else {
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::ParseError,
            );
        }
    }

    fn handle_read_device_identification_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_code: ReadDeviceIdCode,
    ) {
        let response = match self
            .diagnostics_service
            .handle_read_device_identification_rsp(function_code, pdu)
        {
            Ok(r) => r,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        // Validate that the server echoed the correct Read Device ID Code
        if response.read_device_id_code != expected_code {
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::UnexpectedResponse,
            );
            return;
        }

        self.app.read_device_identification_response(
            mbap_header.transaction_id,
            mbap_header.unit_id,
            &response,
        );
    }

    fn handle_encapsulated_interface_transport_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_mei_type: EncapsulatedInterfaceType,
    ) {
        let (mei_type, data) = match self
            .diagnostics_service
            .handle_encapsulated_interface_transport_rsp(function_code, pdu)
        {
            Ok(r) => r,
            Err(e) => {
                self.app
                    .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e);
                return;
            }
        };

        if mei_type != expected_mei_type {
            self.app.request_failed(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                MbusError::UnexpectedResponse,
            );
            return;
        }

        self.app.encapsulated_interface_transport_response(
            mbap_header.transaction_id,
            mbap_header.unit_id,
            mei_type,
            &data,
        );
    }

    fn handle_read_exception_status_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        match self
            .diagnostics_service
            .handle_read_exception_status_rsp(function_code, pdu)
        {
            Ok(status) => self.app.read_exception_status_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                status,
            ),
            Err(e) => self
                .app
                .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e),
        }
    }

    fn handle_diagnostics_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_sub_function: DiagnosticSubFunction,
    ) {
        match self
            .diagnostics_service
            .handle_diagnostics_rsp(function_code, pdu)
        {
            Ok((sub_func, data)) => {
                if sub_func != u16::from(expected_sub_function) {
                    self.app.request_failed(
                        mbap_header.transaction_id,
                        mbap_header.unit_id,
                        MbusError::UnexpectedResponse,
                    );
                } else {
                    self.app.diagnostics_response(
                        mbap_header.transaction_id,
                        mbap_header.unit_id,
                        sub_func,
                        &data,
                    );
                }
            }
            Err(e) => self
                .app
                .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e),
        }
    }

    fn handle_get_comm_event_counter_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        match self
            .diagnostics_service
            .handle_get_comm_event_counter_rsp(function_code, pdu)
        {
            Ok((status, count)) => self.app.get_comm_event_counter_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                status,
                count,
            ),
            Err(e) => self
                .app
                .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e),
        }
    }

    fn handle_get_comm_event_log_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        match self
            .diagnostics_service
            .handle_get_comm_event_log_rsp(function_code, pdu)
        {
            Ok((status, event_count, msg_count, events)) => self.app.get_comm_event_log_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                status,
                event_count,
                msg_count,
                &events,
            ),
            Err(e) => self
                .app
                .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e),
        }
    }

    fn handle_report_server_id_response(
        &mut self,
        mbap_header: &MbapHeader,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
    ) {
        match self
            .diagnostics_service
            .handle_report_server_id_rsp(function_code, pdu)
        {
            Ok(data) => self.app.report_server_id_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &data,
            ),
            Err(e) => self
                .app
                .request_failed(mbap_header.transaction_id, mbap_header.unit_id, e),
        }
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
    use crate::client::services::coils::Coils;
    use crate::client::services::diagnostics::DeviceIdentificationResponse;
    use crate::client::services::discrete_inputs::DiscreteInputs;
    use crate::client::services::fifo::FifoQueue;
    use crate::client::services::file_record::{MAX_SUB_REQUESTS_PER_PDU, SubRequestParams};
    use crate::device_identification::{ConformityLevel, ObjectId, ReadDeviceIdCode};
    use crate::errors::MbusError;
    use crate::transport::TransportType;
    use crate::transport::{ModbusConfig, ModbusTcpConfig};
    use core::cell::RefCell; // `core::cell::RefCell` is `no_std` compatible
    use heapless::Deque;
    use heapless::Vec;
    use registers::Registers;

    const MOCK_DEQUE_CAPACITY: usize = 10; // Define a capacity for the mock deques

    // --- Mock Transport Implementation ---
    #[derive(Debug, Default)]
    struct MockTransport {
        pub sent_frames: RefCell<Deque<Vec<u8, MAX_ADU_FRAME_LEN>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
        pub recv_frames: RefCell<Deque<Vec<u8, MAX_ADU_FRAME_LEN>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
        pub connect_should_fail: bool,
        pub send_should_fail: bool,
        pub is_connected_flag: RefCell<bool>,
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
            TransportType::StdTcp
        }
    }

    // --- Mock App Implementation ---
    #[derive(Debug, Default)]
    struct MockApp {
        pub received_coil_responses: RefCell<Vec<(u16, u8, Coils, u16), 10>>, // Corrected duplicate
        pub received_write_single_coil_responses: RefCell<Vec<(u16, u8, u16, bool), 10>>,
        pub received_write_multiple_coils_responses: RefCell<Vec<(u16, u8, u16, u16), 10>>,
        pub received_discrete_input_responses: RefCell<Vec<(u16, u8, DiscreteInputs, u16), 10>>,
        pub received_holding_register_responses: RefCell<Vec<(u16, u8, Registers, u16), 10>>,
        pub received_input_register_responses: RefCell<Vec<(u16, u8, Registers, u16), 10>>,
        pub received_write_single_register_responses: RefCell<Vec<(u16, u8, u16, u16), 10>>,
        pub received_write_multiple_register_responses: RefCell<Vec<(u16, u8, u16, u16), 10>>,
        pub received_read_write_multiple_registers_responses:
            RefCell<Vec<(u16, u8, Registers), 10>>,
        pub received_mask_write_register_responses: RefCell<Vec<(u16, u8), 10>>,
        pub received_read_fifo_queue_responses: RefCell<Vec<(u16, u8, FifoQueue), 10>>,
        pub received_read_file_record_responses:
            RefCell<Vec<(u16, u8, Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>), 10>>,
        pub received_write_file_record_responses: RefCell<Vec<(u16, u8), 10>>,
        pub received_read_device_id_responses:
            RefCell<Vec<(u16, u8, DeviceIdentificationResponse), 10>>,
        pub failed_requests: RefCell<Vec<(u16, u8, MbusError), 10>>,

        pub current_time: RefCell<u64>, // For simulating time in tests
    }

    impl CoilResponse for MockApp {
        fn read_coils_response(&self, txn_id: u16, unit_id: u8, coils: &Coils, quantity: u16) {
            self.received_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id, coils.clone(), quantity))
                .unwrap();
        }

        fn read_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
            // For single coil, we create a Coils struct with quantity 1 and the single value
            let mut values_vec = Vec::new();
            values_vec.push(if value { 0x01 } else { 0x00 }).unwrap(); // Store the single bit in a byte
            let coils = Coils::new(address, 1, values_vec);
            self.received_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id, coils, 1))
                .unwrap();
        }

        fn write_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
            self.received_write_single_coil_responses
                .borrow_mut()
                .push((txn_id, unit_id, address, value))
                .unwrap();
        }

        fn write_multiple_coils_response(
            &self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            quantity: u16,
        ) {
            self.received_write_multiple_coils_responses
                .borrow_mut()
                .push((txn_id, unit_id, address, quantity))
                .unwrap();
        }
    }

    impl DiscreteInputResponse for MockApp {
        fn read_discrete_inputs_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            inputs: &DiscreteInputs,
            quantity: u16,
        ) {
            self.received_discrete_input_responses
                .borrow_mut()
                .push((txn_id, unit_id, inputs.clone(), quantity))
                .unwrap();
        }

        fn read_single_discrete_input_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            value: bool,
        ) {
            let mut values = Vec::new();
            values.push(if value { 0x01 } else { 0x00 }).unwrap();
            let inputs = DiscreteInputs::new(address, 1, values);
            self.received_discrete_input_responses
                .borrow_mut()
                .push((txn_id, unit_id, inputs, 1))
                .unwrap();
        }
    }

    impl RequestErrorNotifier for MockApp {
        fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError) {
            self.failed_requests
                .borrow_mut()
                .push((txn_id, unit_id, error))
                .unwrap();
        }
    }

    impl RegisterResponse for MockApp {
        fn read_holding_registers_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            registers: &Registers,
        ) {
            let quantity = registers.quantity();
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers.clone(), quantity))
                .unwrap();
        }

        fn read_single_input_register_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_input_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers, 1))
                .unwrap();
        }

        fn read_single_holding_register_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers, 1))
                .unwrap();
        }

        fn read_input_register_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            registers: &Registers,
        ) {
            let quantity = registers.quantity();
            self.received_input_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers.clone(), quantity))
                .unwrap();
        }

        fn write_single_register_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            value: u16,
        ) {
            self.received_write_single_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, address, value))
                .unwrap();
        }

        fn write_multiple_registers_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            quantity: u16,
        ) {
            self.received_write_multiple_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, address, quantity))
                .unwrap();
        }

        fn read_write_multiple_registers_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            registers: &Registers,
        ) {
            self.received_read_write_multiple_registers_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers.clone()))
                .unwrap();
        }

        fn mask_write_register_response(&mut self, txn_id: u16, unit_id: u8) {
            self.received_mask_write_register_responses
                .borrow_mut()
                .push((txn_id, unit_id))
                .unwrap();
        }

        fn read_single_register_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            address: u16,
            value: u16,
        ) {
            let mut values = Vec::new();
            values.push(value).unwrap();
            let registers = Registers::new(address, 1, values);
            self.received_holding_register_responses
                .borrow_mut()
                .push((txn_id, unit_id, registers, 1))
                .unwrap();
        }
    }

    impl FifoQueueResponse for MockApp {
        fn read_fifo_queue_response(&mut self, txn_id: u16, unit_id: u8, fifo_queue: &FifoQueue) {
            self.received_read_fifo_queue_responses
                .borrow_mut()
                .push((txn_id, unit_id, fifo_queue.clone()))
                .unwrap();
        }
    }

    impl FileRecordResponse for MockApp {
        fn read_file_record_response(
            &mut self,
            txn_id: u16,
            unit_id: u8,
            data: &[SubRequestParams],
        ) {
            let mut vec = Vec::new();
            vec.extend_from_slice(data).unwrap();
            self.received_read_file_record_responses
                .borrow_mut()
                .push((txn_id, unit_id, vec))
                .unwrap();
        }
        fn write_file_record_response(&mut self, txn_id: u16, unit_id: u8) {
            self.received_write_file_record_responses
                .borrow_mut()
                .push((txn_id, unit_id))
                .unwrap();
        }
    }

    impl DiagnosticsResponse for MockApp {
        fn read_device_identification_response(
            &self,
            txn_id: u16,
            unit_id: u8,
            response: &DeviceIdentificationResponse,
        ) {
            self.received_read_device_id_responses
                .borrow_mut()
                .push((txn_id, unit_id, response.clone()))
                .unwrap();
        }

        fn encapsulated_interface_transport_response(
            &self,
            _txn_id: u16,
            _unit_id: u8,
            _mei_type: EncapsulatedInterfaceType,
            _data: &[u8],
        ) {
            // For simplicity, we won't store the data in this mock response, but we could if needed.
        }

        fn diagnostics_response(
            &self,
            _txn_id: u16,
            _unit_id: u8,
            _sub_function: u16,
            _data: &[u16],
        ) {
        }

        fn get_comm_event_counter_response(
            &self,
            _txn_id: u16,
            _unit_id: u8,
            _status: u16,
            _event_count: u16,
        ) {
        }

        fn get_comm_event_log_response(
            &self,
            _txn_id: u16,
            _unit_id: u8,
            _status: u16,
            _event_count: u16,
            _message_count: u16,
            _events: &[u8],
        ) {
        }

        fn read_exception_status_response(&self, _txn_id: u16, _unit_id: u8, _status: u8) {}

        fn report_server_id_response(&self, _txn_id: u16, _unit_id: u8, _data: &[u8]) {}
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 0; // Invalid quantity

        let result = client_services.read_multiple_coils(txn_id, unit_id, address, quantity); // current_millis() is called internally
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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

        let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), 1); // Quantity should be 1
        assert_eq!(rcv_coils.values().as_slice(), &[0x01]); // Value should be 0x01 for true
        assert_eq!(*rcv_quantity, 1);

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
        let unit_id = 0x01;
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
        if let ExpectedResponseType::ReadCoils { single_read, .. } =
            client_services.expected_responses[0].response_type
        {
            assert!(single_read);
        } else {
            panic!("Expected ReadCoils response type");
        }
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
        let unit_id = 0x01;
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
        if let ExpectedResponseType::WriteSingleCoil {
            address: expected_address,
            value: expected_value,
        } = client_services.expected_responses[0].response_type
        {
            assert_eq!(expected_address, address);
            assert_eq!(expected_value, value);
        } else {
            panic!("Expected WriteSingleCoil response type");
        }
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 10;
        let values = [
            true, false, true, false, true, false, true, false, true, false,
        ]; // 0x55, 0x01

        client_services
            .write_multiple_coils(txn_id, unit_id, address, quantity, &values) // current_millis() is called internally
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
        if let ExpectedResponseType::WriteMultipleCoils {
            address: expected_address,
            quantity: expected_quantity,
        } = client_services.expected_responses[0].response_type
        {
            assert_eq!(expected_address, address);
            assert_eq!(expected_quantity, quantity);
        } else {
            panic!("Expected WriteMultipleCoils response type");
        }
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
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 10;
        let values = [
            true, false, true, false, true, false, true, false, true, false,
        ];

        // 1. Send a Write Multiple Coils request
        client_services // current_millis() is called internally
            .write_multiple_coils(txn_id, unit_id, address, quantity, &values)
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
        let unit_id = 0x01;
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
        if let ExpectedResponseType::ReadCoils {
            expected_quantity,
            from_address,
            ..
        } = client_services.expected_responses[0].response_type
        {
            assert_eq!(expected_quantity, quantity);
            assert_eq!(from_address, address);
        } else {
            panic!("Expected ReadCoils response type");
        }

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

        let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), quantity);
        assert_eq!(rcv_coils.values().as_slice(), &[0xB3]);
        assert_eq!(*rcv_quantity, quantity);

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
        let unit_id = 0x01;
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

    /// Test case: `read_multiple_coils` returns `MbusError::TooManyRequests` when the queue is full.
    #[test]
    fn test_too_many_requests_error() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());
        // Create a client with a small capacity for expected responses
        let mut client_services =
            ClientServices::<MockTransport, MockApp, 1>::new(transport, app, config).unwrap();

        // Send one request, which should fill the queue
        client_services.read_multiple_coils(1, 1, 0, 1).unwrap();
        assert_eq!(client_services.expected_responses.len(), 1);

        // Attempt to send another request, which should fail due to full queue
        let result = client_services.read_multiple_coils(2, 1, 0, 1);
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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
        let unit_id = 0x01;
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

        client_services
            .read_single_holding_register(10, 1, 100)
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
        if let ExpectedResponseType::ReadHoldingRegisters { single_read, .. } =
            client_services.expected_responses[0].response_type
        {
            assert!(single_read);
        } else {
            panic!("Expected ReadHoldingRegisters response type");
        }
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
        let unit_id = 1;
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

        client_services
            .read_single_input_register(10, 1, 100)
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
        if let ExpectedResponseType::ReadInputRegisters { single_read, .. } =
            client_services.expected_responses[0].response_type
        {
            assert!(single_read);
        } else {
            panic!("Expected ReadInputRegisters response type");
        }
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
        let unit_id = 1;
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

        let write_values = [0xAAAA, 0xBBBB];
        client_services
            .read_write_multiple_registers(11, 1, 10, 2, 20, &write_values)
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

        client_services
            .mask_write_register(12, 1, 30, 0xF0F0, 0x0F0F)
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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
        let unit_id = 1;
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

        // Request 1
        let txn_id_1 = 21;
        client_services
            .read_device_identification(txn_id_1, 1, ReadDeviceIdCode::Basic, ObjectId::from(0x00))
            .unwrap();

        // Request 2
        let txn_id_2 = 22;
        client_services
            .read_device_identification(
                txn_id_2,
                1,
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
        let unit_id = 1;
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
        assert_eq!(failed[0].2, MbusError::UnexpectedResponse);
    }
}
