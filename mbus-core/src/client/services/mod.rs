use heapless::Vec;

use crate::{
    data_unit::{
        common::{MAX_ADU_FRAME_LEN, MbapHeader},
        tcp::ModbusTcpMessage,
    },
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{ModbusConfig, TimeKeeper, Transport, TransportType},
};

pub mod coils;
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
        expected_quantity: u16,
        from_address: u16,
        single_read: bool,
    },
    /// Expected response for a Write Single Coil request, includes metadata to validate the response.
    WriteSingleCoil { address: u16, value: bool },
    /// Expected response for a Write Multiple Coils request, includes metadata to validate the response.
    WriteMultipleCoils { address: u16, quantity: u16 },
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
/// * `N` - The maximum number of concurrent outstanding requests (capacity of the expected responses queue).
/// * `APP` - The application layer that handles processed Modbus responses.
#[derive(Debug)]
pub struct ClientServices<TRANSPORT, const N: usize, APP> {
    /// Application layer that implements the CoilResponse trait, used to handle responses and invoke callbacks.
    pub app: APP,
    /// Transport layer used for sending and receiving Modbus frames. Must implement the Transport trait.
    transport: TRANSPORT,

    /// Service struct for constructing coil-related requests and parsing responses.
    coil_service: coils::CoilService,

    config: ModbusConfig,

    /// Queue of expected responses for sent requests, used to match incoming responses with their corresponding requests.
    expected_responses: Vec<ExpectedResponse, N>,
}

/// Implementation of core client services, including methods for sending requests and processing responses.
impl<TRANSPORT: Transport, const N: usize, APP: crate::app::CoilResponse + TimeKeeper>
    ClientServices<TRANSPORT, N, APP>
{
    /// Creates a new instance of ClientServices, connecting to the transport layer with the provided configuration.
    pub fn new(
        mut transport: TRANSPORT,
        app: APP,
        config: ModbusConfig,
    ) -> Result<Self, MbusError> {
        transport
            .connect(&config)
            .map_err(|_e| MbusError::ConnectionFailed)?;

        Ok(Self {
            app,
            transport,
            coil_service: coils::CoilService::new(),
            config,
            expected_responses: Vec::new(),
        })
    }

    /// Polls the transport layer for incoming Modbus frames and processes them.
    /// It also handles timeouts and retries for outstanding requests, using the application's `TimeKeeper` for current time.
    ///
    /// # Arguments
    /// * `current_millis` - The current monotonic time in milliseconds. This is provided
    ///   as an argument for `no_std` compatibility, as `std::time::Instant` is not available.
    pub fn poll(&mut self) {
        // 1. Attempt to receive a frame
        match self.transport.recv() {
            Ok(frame) => {
                // If a frame is received, ingest it
                self.ingest_frame(&frame);
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
        let mut i = 0;
        while i < self.expected_responses.len() {
            let expected_response = &mut self.expected_responses[i];
            if current_millis
                .checked_sub(expected_response.sent_timestamp)
                .unwrap_or(0)
                > self.config.response_timeout_ms.into()
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
                        let response = self.expected_responses.swap_remove(i);
                        self.app.request_failed(
                            response.txn_id,
                            response.unit_id,
                            MbusError::SendFailed,
                        );
                        continue; // Move to the next item in the (now shorter) vec
                    }
                } else {
                    // No retries left, report timeout to application
                    let response = self.expected_responses.swap_remove(i); // Remove the timed-out request
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
                retries_left: self.config.retry_attempts,
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
                retries_left: self.config.retry_attempts,
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
                retries_left: self.config.retry_attempts,
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
                retries_left: self.config.retry_attempts,
                response_type: ExpectedResponseType::WriteMultipleCoils { address, quantity },
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Ingests received Modbus frames from the transport layer.
    fn ingest_frame(&mut self, frame: &[u8]) {
        // Changed to &mut self, removed transport param
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let message = match decode_transport_frame(frame, transport_type) {
            Some(value) => value,
            None => {
                return; // Malformed frame or parsing error, frame is dropped.
            }
        };

        let mbap_header = message.mbap_header();
        let function_code = message.function_code();
        self.handle_response(&message, mbap_header, function_code);
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
}

/// Decodes a raw transport frame into a ModbusTcpMessage based on the transport type.
fn decode_transport_frame(frame: &[u8], transport_type: TransportType) -> Option<ModbusTcpMessage> {
    let message = match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // Parse MBAP header and PDU
            match ModbusTcpMessage::from_adu_bytes(frame) {
                Ok(msg) => msg, // Successfully decoded the frame.
                // If decoding fails, the frame is dropped.
                Err(_e) => {
                    // FUTURE: Handle parsing error.
                    return None;
                }
            }
        }
        TransportType::StdSerial | TransportType::CustomSerial => {
            todo!("Serial transport is not yet implemented for frame decoding.")
        }
    };
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CoilResponse;
    use crate::client::services::coils::Coils;
    use crate::errors::MbusError;
    use crate::transport::ModbusConfig;
    use core::cell::RefCell; // `core::cell::RefCell` is `no_std` compatible
    use heapless::Deque;
    use heapless::Vec;

    const MOCK_DEQUE_CAPACITY: usize = 10; // Define a capacity for the mock deques

    // --- Mock Transport Implementation ---
    #[derive(Debug, Default)]
    struct MockTransport {
        pub sent_frames: RefCell<Deque<Vec<u8, 260>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
        pub recv_frames: RefCell<Deque<Vec<u8, 260>, MOCK_DEQUE_CAPACITY>>, // Changed to heapless::Deque
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

        fn recv(&mut self) -> Result<Vec<u8, 260>, Self::Error> {
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

        fn request_failed(&self, _txn_id: u16, _unit_id: u8, _error: MbusError) {
            // For now, just ignore in mock. In a real app, this would log or handle the error.
        }
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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()

        let client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config);
        assert!(client_services.is_ok());
        assert!(client_services.unwrap().transport.is_connected());
    }

    /// Test case: `ClientServices::new` returns an error if transport connection fails.
    #[test]
    fn test_client_services_new_connection_failure() {
        let mut transport = MockTransport::default();
        transport.connect_should_fail = true;
        let app = MockApp::default();
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()

        let client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config);
        assert!(client_services.is_err());
        assert_eq!(client_services.unwrap_err(), MbusError::ConnectionFailed);
    }

    /// Test case: `read_multiple_coils` sends a valid ADU over the transport.
    #[test]
    fn test_read_multiple_coils_sends_valid_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap(); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let mut config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        config.response_timeout_ms = 100; // Short timeout for testing
        config.retry_attempts = 1; // One retry

        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

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
        let config = ModbusConfig::new("127.0.0.1", 502).unwrap();
        // Create a client with a small capacity for expected responses
        let mut client_services =
            ClientServices::<MockTransport, 1, MockApp>::new(transport, app, config).unwrap();

        // Send one request, which should fill the queue
        client_services.read_multiple_coils(1, 1, 0, 1).unwrap();
        assert_eq!(client_services.expected_responses.len(), 1);

        // Attempt to send another request, which should fail due to full queue
        let result = client_services.read_multiple_coils(2, 1, 0, 1);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), MbusError::TooManyRequests);
        assert_eq!(client_services.expected_responses.len(), 1); // Queue size remains 1
    }
}
