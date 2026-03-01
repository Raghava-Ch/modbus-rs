mod mock_app;

#[cfg(test)]
mod tests {
    use mbus_core;
    use anyhow::Result;
    use mbus_core::client::services::ClientServices;
    use mbus_core::transport::{ModbusTcpConfig};
    use mbus_tcp::management::std_transport::StdTcpTransport;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::mock_app;
    use mock_app::MockApp;

    #[tokio::test] // Renamed test function
    async fn test_client_services_read_single_coil() -> Result<()> {
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

        let transport = StdTcpTransport::new(None);
        let app = MockApp::default();
        let config = ModbusTcpConfig::new("127.0.0.1", addr.port());

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
}
