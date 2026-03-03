mod mock_app;

#[cfg(test)]
mod tests {
    use mbus_core;
    use anyhow::Result;
    use mbus_core::client::services::ClientServices;
    use mbus_core::transport::{ModbusConfig};
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

        client.read_multiple_coils(txn_id, unit_id, address, quantity).unwrap();
        client.poll();

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
        client.poll();

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
        let values = [true, false, true, false, true, false, true, false, true, false];

        client.write_multiple_coils(txn_id, unit_id, address, quantity, &values).unwrap();
        client.poll();

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
}
