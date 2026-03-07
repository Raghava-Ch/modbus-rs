mod mock_app;

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use mbus_core;
    use mbus_core::client::services::ClientServices;
    use mbus_core::transport::ModbusConfig;
    use mbus_core::device_identification::{ReadDeviceIdCode, ObjectId, ConformityLevel};
    use mbus_tcp::management::std_transport::StdTcpTransport;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::mock_app;
    use mock_app::MockApp;

    #[test] // Renamed test function
    fn test_client_services_read_single_coil() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Read coils
            let mut buf = [0; 12];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x02, // Transaction ID (2)
                    0x00, 0x00, // Protocol ID (0 = Modbus)
                    0x00, 0x06, // Length (6 bytes follow)
                    0x00,       // Unit ID (0)
                    0x01,       // Function Code (1 = Read Coils)
                    0x00, 0x01, // Starting Address (1)
                    0x00, 0x01, // Quantity of Coils (1)
                ]
            );

            // Send a Read Coils response for 1 coil at address 1 with value true
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x02, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x04, // Length
                0x00,       // Unit ID
                0x01,       // Function Code (Read Coils)
                0x01,       // Byte Count
                0x01,       // Coil Status (Bit 0 = 1)
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 2;
        let unit_id = 0;
        let address = 1;
        client.read_single_coil(txn_id, unit_id, address).unwrap(); // Send read request
        client.poll(); // Process read response

        // Assert that the MockApp received the correct response
        let received_responses = client.app.received_coil_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];
        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), 1);
        assert_eq!(rcv_coils.values().as_slice(), &[0x01]); // Value should be 0x01 for true
        assert_eq!(*rcv_quantity, 1);
        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_read_coils() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Read Coils request (FC 01)
            let mut buf = [0; 12];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x05, // Transaction ID (5)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x06, // Length
                    0x01,       // Unit ID (1)
                    0x01,       // Function Code (1)
                    0x00, 0x0A, // Starting Address (10)
                    0x00, 0x03, // Quantity (3)
                ]
            );

            // Send response: 3 coils, values: [1, 0, 1] -> 0x05 (binary 101)
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x05, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x04, // Length
                0x01,       // Unit ID
                0x01,       // Function Code
                0x01,       // Byte Count
                0x05,       // Coil Status (00000101)
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 100;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 5;
        let unit_id = 1;
        let address = 10;
        let quantity = 3;

        client
            .read_multiple_coils(txn_id, unit_id, address, quantity)
            .unwrap();
        client.poll(); // Process read response

        let received_responses = client.app.received_coil_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_coils.from_address(), address);
        assert_eq!(rcv_coils.quantity(), quantity);
        assert_eq!(rcv_coils.values().as_slice(), &[0x05]);
        assert_eq!(*rcv_quantity, quantity);

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_write_single_coil() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Write Single Coil request (FC 05)
            let mut buf = [0; 12];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x03, // Transaction ID (3)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x06, // Length
                    0x01,       // Unit ID (1)
                    0x05,       // Function Code (5 = Write Single Coil)
                    0x00, 0x0A, // Address (10)
                    0xFF, 0x00, // Value (ON)
                ]
            );

            // Send response: echo back the request
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x03, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x05,       // Function Code
                0x00, 0x0A, // Address
                0xFF, 0x00, // Value
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 3;
        let unit_id = 1;
        let address = 10;
        let value = true;

        client
            .write_single_coil(txn_id, unit_id, address, value)
            .unwrap();
        client.poll(); // Process write response

        let received_responses = client.app.received_write_single_coil_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_value) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_value, value);

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_write_multiple_coils() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Write Multiple Coils request (FC 0F)
            let mut buf = [0; 15]; // 12 (MBAP + FC + Addr + Qty) + 1 (Byte Count) + 2 (Data)
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x04, // Transaction ID (4)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x09, // Length (9 bytes follow)
                    0x01,       // Unit ID (1)
                    0x0F,       // Function Code (15 = Write Multiple Coils)
                    0x00, 0x00, // Address (0)
                    0x00, 0x0A, // Quantity (10)
                    0x02,       // Byte Count (2)
                    0x55, 0x01, // Data (0x55, 0x01)
                ]
            );

            // Send response: echo back address and quantity
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x04, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x06, // Length
                0x01,       // Unit ID
                0x0F,       // Function Code
                0x00, 0x00, // Address
                0x00, 0x0A, // Quantity
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 4;
        let unit_id = 1;
        let address = 0;
        let quantity = 10;
        let values = [
            true, false, true, false, true, false, true, false, true, false,
        ];

        client
            .write_multiple_coils(txn_id, unit_id, address, quantity, &values)
            .unwrap();
        client.poll(); // Process write response

        let received_responses = client.app.received_write_multiple_coils_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_address, rcv_quantity) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(*rcv_address, address);
        assert_eq!(*rcv_quantity, quantity);

        server_handle.join().unwrap()?;
        Ok(())
    }

    /// Test case: Client handles a Modbus exception response from the server.
    #[test]
    fn test_client_services_server_exception_response() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut buf = [0; 12]; // Expect a Read Coils request
            stream.read_exact(&mut buf)?;

            // Send a Modbus exception response (e.g., Illegal Data Value, FC 0x81, Exception Code 0x03)
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x01, // Transaction ID (matches request)
                0x00, 0x00, // Protocol ID
                0x00, 0x03, // Length (Unit ID + FC + Exception Code)
                0x01,       // Unit ID
                0x81,       // Function Code (Read Coils + 0x80 for exception)
                0x03,       // Exception Code (Illegal Data Value)
            ])?;
            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 1;
        let unit_id = 1;
        let address = 10;
        let quantity = 3;

        client
            .read_multiple_coils(txn_id, unit_id, address, quantity)
            .unwrap();
        client.poll(); // Process the exception response

        // The client should receive an error, not a successful response
        assert!(client.app.received_coil_responses.borrow().is_empty());
        // In a real application, the `request_failed` callback would be checked.
        // For this mock, we just ensure no successful response was processed.

        server_handle.join().unwrap()?;
        Ok(())
    }

    /// Test case: Client handles the server closing the connection unexpectedly.
    #[test]
    fn test_client_services_server_closes_connection() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut buf = [0; 12]; // Expect a Read Coils request
            stream.read_exact(&mut buf)?;
            // Server closes connection immediately after receiving request, without sending a response
            drop(stream);
            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.response_timeout_ms = 100; // Short timeout
        config.retry_attempts = 0; // No retries for this test

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 1;
        let unit_id = 1;
        let address = 10;
        let quantity = 3;

        client
            .read_multiple_coils(txn_id, unit_id, address, quantity)
            .unwrap();
        // Poll multiple times to allow for connection closed error detection and timeout
        std::thread::sleep(std::time::Duration::from_millis(200)); // Ensure timeout
        client.poll();
        client.poll();

        // The client should eventually report a connection closed or timeout error.
        assert!(client.app.received_coil_responses.borrow().is_empty());
        // In a real application, the `request_failed` callback would be checked for MbusError::ConnectionClosed or MbusError::Timeout.

        server_handle.join().unwrap()?;
        Ok(())
    }

    /// Test case: Client times out waiting for a response from a non-responsive server.
    #[test]
    fn test_client_services_server_timeout() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (_stream, _) = listener.accept()?;
            // Server accepts connection but sends no data, causing client to timeout
            std::thread::sleep(std::time::Duration::from_secs(5)); // Ensure client times out first
            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.response_timeout_ms = 100; // Short timeout for test
        config.retry_attempts = 0; // No retries for this test

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 1;
        let unit_id = 1;
        let address = 10;
        let quantity = 3;

        client
            .read_multiple_coils(txn_id, unit_id, address, quantity)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200)); // Ensure timeout
        client.poll(); // This poll should detect the timeout

        // The client should eventually report a timeout error.
        assert!(client.app.received_coil_responses.borrow().is_empty());
        // In a real application, the `request_failed` callback would be checked for MbusError::Timeout.

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_read_discrete_inputs() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Read Discrete Inputs request (FC 02)
            let mut buf = [0; 12];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x06, // Transaction ID (6)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x06, // Length
                    0x01,       // Unit ID (1)
                    0x02,       // Function Code (2 = Read Discrete Inputs)
                    0x00, 0x0A, // Starting Address (10)
                    0x00, 0x08, // Quantity (8)
                ]
            );

            // Send response: 8 inputs, value 0xAA (10101010)
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x06, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x04, // Length
                0x01,       // Unit ID
                0x02,       // Function Code
                0x01,       // Byte Count
                0xAA,       // Input Status
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 6;
        let unit_id = 1;
        let address = 10;
        let quantity = 8;

        client
            .read_discrete_inputs(txn_id, unit_id, address, quantity)
            .unwrap();
        client.poll(); // Process read response

        let received_responses = client.app.received_discrete_input_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_inputs, rcv_quantity) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_inputs.from_address(), address);
        assert_eq!(rcv_inputs.quantity(), quantity);
        assert_eq!(rcv_inputs.values().as_slice(), &[0xAA]);
        assert_eq!(*rcv_quantity, quantity);

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_read_single_discrete_input() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Read Discrete Inputs request (FC 02) for single input
            let mut buf = [0; 12];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x07, // Transaction ID (7)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x06, // Length
                    0x01,       // Unit ID (1)
                    0x02,       // Function Code (2 = Read Discrete Inputs)
                    0x00, 0x05, // Starting Address (5)
                    0x00, 0x01, // Quantity (1)
                ]
            );

            // Send response: 1 input, value 1 (ON)
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x07, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x04, // Length
                0x01,       // Unit ID
                0x02,       // Function Code
                0x01,       // Byte Count
                0x01,       // Input Status
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 7;
        let unit_id = 1;
        let address = 5;

        client.read_single_discrete_input(txn_id, unit_id, address).unwrap();
        client.poll(); // Process read response

        let received_responses = client.app.received_discrete_input_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_inputs, rcv_quantity) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_inputs.from_address(), address);
        assert_eq!(rcv_inputs.quantity(), 1);
        assert_eq!(rcv_inputs.value(address).unwrap(), true);
        assert_eq!(*rcv_quantity, 1);

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_read_device_identification() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Expect Read Device Identification request (FC 2B, MEI 0E)
            let mut buf = [0; 11];
            stream.read_exact(&mut buf)?;
            #[rustfmt::skip]
            assert_eq!(
                buf,
                [
                    0x00, 0x08, // Transaction ID (8)
                    0x00, 0x00, // Protocol ID
                    0x00, 0x05, // Length (5 bytes follow)
                    0x01,       // Unit ID (1)
                    0x2B,       // Function Code (43)
                    0x0E,       // MEI Type (14)
                    0x01,       // Read Device ID Code (01 - Basic)
                    0x00,       // Object ID (00)
                ]
            );

            // Send response: Basic, Conformity 81, More 00, Next 00, Num 01, Obj 00, Len 03, "Foo"
            #[rustfmt::skip]
            stream.write_all(&[
                0x00, 0x08, // Transaction ID
                0x00, 0x00, // Protocol ID
                0x00, 0x0D, // Length (13 bytes follow: Unit(1)+FC(1)+MEI(1)+Read(1)+Conf(1)+More(1)+Next(1)+Num(1)+ObjId(1)+ObjLen(1)+Val(3))
                0x01,       // Unit ID
                0x2B,       // Function Code
                0x0E,       // MEI Type
                0x01,       // Read Device ID Code
                0x81,       // Conformity Level
                0x00,       // More Follows
                0x00,       // Next Object ID
                0x01,       // Number of Objects
                0x00,       // Object ID
                0x03,       // Object Length
                0x46, 0x6F, 0x6F, // Object Value "Foo"
            ])?;

            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        let txn_id = 8;
        let unit_id = 1;
        let read_code = ReadDeviceIdCode::Basic;
        let object_id = ObjectId::from(0x00);

        client.read_device_identification(txn_id, unit_id, read_code, object_id).unwrap();
        client.poll();

        let received_responses = client.app.received_read_device_id_responses.borrow();
        assert_eq!(received_responses.len(), 1);
        let (rcv_txn_id, rcv_unit_id, rcv_resp) = &received_responses[0];

        assert_eq!(*rcv_txn_id, txn_id);
        assert_eq!(*rcv_unit_id, unit_id);
        assert_eq!(rcv_resp.read_device_id_code, ReadDeviceIdCode::Basic);
        assert_eq!(rcv_resp.conformity_level, ConformityLevel::BasicStreamAndIndividual);
        assert_eq!(rcv_resp.more_follows, false);
        assert_eq!(rcv_resp.next_object_id, ObjectId::from(0x00));
        assert_eq!(rcv_resp.number_of_objects, 1);
        
        // Verify object data
        let objects: Vec<_> = rcv_resp.objects().map(|r| r.unwrap()).collect();
        assert_eq!(objects.len(), 1);
        let obj = &objects[0];
        assert_eq!(obj.object_id, ObjectId::from(0x00));
        assert_eq!(obj.value.as_slice(), b"Foo");

        server_handle.join().unwrap()?;
        Ok(())
    }

    #[test]
    fn test_client_services_read_device_identification_multi_transaction() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;

        let server_handle = thread::spawn(move || -> Result<()> {
            let (mut stream, _) = listener.accept()?;

            // Handle 2 requests
            for _ in 0..2 {
                let mut buf = [0; 11];
                stream.read_exact(&mut buf)?;
                
                // Extract TID to echo back
                let tid_hi = buf[0];
                let tid_lo = buf[1];

                // Send generic response (Basic, No objects for simplicity)
                #[rustfmt::skip]
                stream.write_all(&[
                    tid_hi, tid_lo, // Transaction ID
                    0x00, 0x00, // Protocol ID
                    0x00, 0x08, // Length (8 bytes follow)
                    0x01,       // Unit ID
                    0x2B,       // Function Code
                    0x0E,       // MEI Type
                    0x01,       // Read Device ID Code (Echo Basic)
                    0x81,       // Conformity Level
                    0x00,       // More Follows
                    0x00,       // Next Object ID
                    0x00,       // Number of Objects
                ])?;
            }
            Ok(())
        });

        let transport = StdTcpTransport::new();
        let app = MockApp::default();
        let mut config = ModbusConfig::new("127.0.0.1", addr.port()).unwrap();
        config.connection_timeout_ms = 500;

        let mut client = ClientServices::<_, 10, _>::new(transport, app, config).unwrap();

        // Send Request 1
        client.read_device_identification(10, 1, ReadDeviceIdCode::Basic, ObjectId::from(0x00)).unwrap();
        // Send Request 2
        client.read_device_identification(11, 1, ReadDeviceIdCode::Basic, ObjectId::from(0x00)).unwrap();

        // Poll twice to process both responses
        client.poll();
        client.poll();

        let received_responses = client.app.received_read_device_id_responses.borrow();
        assert_eq!(received_responses.len(), 2);
        
        assert_eq!(received_responses[0].0, 10);
        assert_eq!(received_responses[1].0, 11);

        server_handle.join().unwrap()?;
        Ok(())
    }
}
