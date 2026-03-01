use heapless::Vec;

use crate::{
    client::services::coils::{CoilService},
    data_unit::{
        common::{AdditionalAddress, MbapHeader, ModbusMessage},
        tcp::ModbusTcpMessage,
    },
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{ModbusTcpConfig, Transport, TransportType},
};

pub mod coils;
pub mod registers;

#[derive(Debug, Default)]
struct ExpectedResponse {
    txn_id: u16,
    unit_id: u8,
    expected_quantity: u16,
    from_address: u16,
    single_read: bool, // Indicates if this is a single read (e.g., Read Single Coil) or multiple read (e.g., Read Multiple Coils)
}

#[derive(Debug)]
pub struct ClientServices<TRANSPORT, const N: usize, APP> {
    pub app: APP,
    transport: TRANSPORT,

    expected_responses: Vec<ExpectedResponse, N>,
}

impl<TRANSPORT: Transport, const N: usize, APP: crate::app::CoilResponse>
    ClientServices<TRANSPORT, N, APP>
{
    pub fn new(
        mut transport: TRANSPORT,
        app: APP,
        config: ModbusTcpConfig,
    ) -> Result<Self, MbusError> {
        transport
            .connect(&config)
            .map_err(|_e| MbusError::ConnectionFailed)?;
        Ok(Self {
            app,
            transport,
            expected_responses: Vec::new(),
        })
    }

    pub fn poll(&mut self) {
        match self.transport.recv() {
            Ok(frame) => {
                self.ingest_frame(&frame);
            }
            Err(_e) => {
                // Handle transport errors (e.g., log, disconnect, retry connection)
            }
        }
    }

    pub fn read_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        self.read_coils(txn_id, unit_id, address, quantity, false)
    }

    pub fn read_single_coil(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
    ) -> Result<(), MbusError> {
        self.read_coils(txn_id, unit_id, address, 1, true)
    }

    fn read_coils(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        single_read: bool,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let pdu = CoilService::read_coils_request(address, quantity);
        let pdu_request = pdu?; // Unwrap the PDU request here
        let adu = match transport_type {
            // adu is a ModbusMessage, not raw bytes yet
            TransportType::StdTcp | TransportType::CustomTcp => {
                // Construct Modbus TCP ADU
                // The length field is PDU length (FC + Data) + 1 (Unit ID)
                let pdu_bytes_len = pdu_request.to_bytes()?.len() as u16;
                let mbap_header = MbapHeader::new(txn_id, pdu_bytes_len + 1, unit_id);
                ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu_request)
            }
            TransportType::StdSerial | TransportType::CustomSerial => {
                // For Modbus RTU/ASCII, the PDU is the ADU (with framing handled by transport)
                todo!()
            }
        };

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id,
                expected_quantity: quantity, // Store the requested quantity
                from_address: address,       // Store the requested address
                single_read: single_read,    // This is a multiple read request
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&adu.to_bytes()?)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Ingests received Modbus frames from the transport layer.
    fn ingest_frame(&mut self, frame: &[u8]) {
        // Changed to &mut self, removed transport param
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let message = match decode_transport_frame(frame, transport_type) {
            Some(value) => value,
            None => return,
        };

        let mbap_header = message.mbap_header();
        let pdu = message.pdu();
        match pdu.function_code() {
            FunctionCode::ReadCoils => {
                self.handle_coil_response(&message, mbap_header);
            }
            _ => {
                // Handle other function codes or ignore
            }
        }
    }

    fn handle_coil_response(&mut self, message: &ModbusTcpMessage, mbap_header: &MbapHeader) {
        // Find the matching expected response and its index
        let index = self.expected_responses.iter().position(|r| {
            r.txn_id == mbap_header.transaction_id && r.unit_id == mbap_header.unit_id
        });

        let expected_response = match index {
            Some(idx) => self.expected_responses.swap_remove(idx),
            None => return, // No matching request found, ignore response
        };

        // Extract original request parameters from the matched expected_response
        let expected_quantity = expected_response.expected_quantity; // Use stored quantity
        let from_address = expected_response.from_address; // Use stored address
        let pdu = message.pdu();
        let coil_response =
            match CoilService::handle_coil_response(pdu, expected_quantity, from_address) {
                Some(response) => response,
                None => {
                    // Handle parsing error (e.g., log it)
                    return;
                }
            };
        if expected_response.single_read {
            // For single read, extract the value of the single coil; bail out if none.
            let coil_value = match coil_response.values().first().cloned() {
                Some(v) => v,
                None => return, // nothing to report, drop the response
            };
            self.app.read_single_coil_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                from_address,
                coil_value != 0, // Convert to bool
            );
        } else {
            self.app.read_coils_response(
                mbap_header.transaction_id,
                mbap_header.unit_id,
                &coil_response,
                expected_quantity, // Pass the original expected quantity
            );
        }
    }
}

fn decode_transport_frame(frame: &[u8], transport_type: TransportType) -> Option<ModbusTcpMessage> {
    let message = match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // Parse MBAP header and PDU
            match ModbusTcpMessage::from_adu_bytes(frame) {
                Ok(msg) => msg,
                Err(_e) => {
                    // Handle parsing error (e.g., log it)
                    return None;
                }
            }
        }
        TransportType::StdSerial | TransportType::CustomSerial => {
            // For Modbus RTU/ASCII, parse the frame directly as PDU
            todo!()
        }
    };
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::CoilResponse;
    use crate::client::services::coils::{Coils};
    use crate::errors::MbusError;
    use crate::transport::ModbusTcpConfig;
    use heapless::Vec;
    use core::cell::RefCell; // `core::cell::RefCell` is `no_std` compatible
    use heapless::Deque;

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

        fn connect(&mut self, _config: &ModbusTcpConfig) -> Result<(), Self::Error> {
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
            self.received_coil_responses.borrow_mut().push((txn_id, unit_id, coils, 1)).unwrap();
        }
    }

    // --- ClientServices Tests ---

    /// Test case: `ClientServices::new` successfully connects to the transport.
    #[test]
    fn test_client_services_new_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()

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
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()

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
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 8;
        client_services.read_multiple_coils(txn_id, unit_id, address, quantity).unwrap();

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
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 0; // Invalid quantity

        let result = client_services.read_multiple_coils(txn_id, unit_id, address, quantity);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `read_multiple_coils` returns an error if sending fails.
    #[test]
    fn test_read_multiple_coils_send_failure() {
        let mut transport = MockTransport::default();
        transport.send_should_fail = true;
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502);
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 8;

        let result = client_services.read_multiple_coils(txn_id, unit_id, address, quantity);
        assert_eq!(result.unwrap_err(), MbusError::SendFailed);
    }

    /// Test case: `ClientServices` successfully sends a Read Coils request and processes a valid response.
    #[test]
    fn test_client_services_read_coils_e2e_success() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 8;
        client_services.read_multiple_coils(txn_id, unit_id, address, quantity).unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services.transport.sent_frames.borrow_mut().pop_front().unwrap(); // Corrected: Removed duplicate pop_front()
        // Expected ADU: TID(0x0001), PID(0x0000), Length(0x0006 = Unit ID + FC + Addr + Qty), UnitID(0x01), FC(0x01), Addr(0x0000), Qty(0x0008)
        assert_eq!(sent_adu.as_slice(),
            &[
                0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x01, 0x00, 0x00, 0x00, 0x08
            ]
        );

        // Verify that the expected response was recorded
        assert_eq!(client_services.expected_responses.len(), 1);
        let expected_req = &client_services.expected_responses[0];
        assert_eq!(expected_req.txn_id, txn_id);
        assert_eq!(expected_req.unit_id, unit_id);
        assert_eq!(expected_req.expected_quantity, quantity);
        assert_eq!(expected_req.from_address, address);

        // 2. Manually construct a valid Read Coils response ADU
        // Response for reading 8 coils, values: 10110011 (0xB3)
        // ADU: TID(0x0001), PID(0x0000), Length(0x0004 = Unit ID + FC + Byte Count + Coil Data), UnitID(0x01), FC(0x01), Byte Count(0x01), Coil Data(0xB3)
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0xB3];

        // Simulate receiving the frame
        client_services.ingest_frame(&response_adu);

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

    /// Test case: `ingest_frame` ignores responses with wrong function code.
    #[test]
    fn test_ingest_frame_wrong_fc() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        // ADU with FC 0x03 (Read Holding Registers) instead of 0x01 (Read Coils)
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x03, 0x01, 0xB3];

        client_services.ingest_frame(&response_adu);

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores malformed ADUs.
    #[test]
    fn test_ingest_frame_malformed_adu() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        // Malformed ADU (too short)
        let malformed_adu = [0x01, 0x02, 0x03];

        client_services.ingest_frame(&malformed_adu);

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores responses for unknown transaction IDs.
    #[test]
    fn test_ingest_frame_unknown_txn_id() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        // No request was sent, so no expected response is in the queue.
        let response_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0xB3];

        client_services.ingest_frame(&response_adu);

        let received_responses = client_services.app.received_coil_responses.borrow();
        assert!(received_responses.is_empty());
    }

    /// Test case: `ingest_frame` ignores responses that fail PDU parsing.
    #[test]
    fn test_ingest_frame_pdu_parse_failure() {
        let transport = MockTransport::default();
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", 502); // Removed .to_string()
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0001;
        let unit_id = 0x01;
        let address = 0x0000;
        let quantity = 8;
        client_services.read_multiple_coils(txn_id, unit_id, address, quantity).unwrap();

        // Craft a PDU that will cause `parse_read_coils_response` to fail.
        // For example, byte count mismatch: PDU indicates 1 byte of data, but provides 2.
        // ADU: TID(0x0001), PID(0x0000), Length(0x0005), UnitID(0x01), FC(0x01), Byte Count(0x01), Data(0xB3, 0x00)
        let response_adu = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x01, 0x01, 0xB3, 0x00,
        ]; // Corrected duplicate
        
        client_services.ingest_frame(&response_adu);

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
        let config = ModbusTcpConfig::new("127.0.0.1", 502);
        let mut client_services = ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0002;
        let unit_id = 0x01;
        let address = 0x0005;

        // 1. Send a Read Single Coil request
        client_services.read_single_coil(txn_id, unit_id, address).unwrap();

        // Verify that the request was sent via the mock transport
        let sent_adu = client_services.transport.sent_frames.borrow_mut().pop_front().unwrap();
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
        client_services.ingest_frame(&response_adu);

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
        let config = ModbusTcpConfig::new("127.0.0.1", 502);
        let mut client_services =
            ClientServices::<MockTransport, 10, MockApp>::new(transport, app, config).unwrap();

        let txn_id = 0x0002;
        let unit_id = 0x01;
        let address = 0x0005;

        client_services
            .read_single_coil(txn_id, unit_id, address)
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
        assert_eq!(client_services.expected_responses.len(), 1);
        assert!(client_services.expected_responses[0].single_read);
    }
}
