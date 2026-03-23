use anyhow::Result;
use heapless::Vec;
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, TimeKeeper, UnitIdOrSlaveAddr};
use mbus_tcp::StdTcpTransport;
use modbus_client::app::{CoilResponse, RequestErrorNotifier};
use modbus_client::services::ClientServices;
use modbus_client::services::coil::Coils;
use std::cell::RefCell;

// --- MockApp for Client ---
// This struct implements the CoilResponse trait and is used by the client
// to receive and store responses from the Modbus server.
#[derive(Debug, Default)]
struct ClientMockApp {
    pub received_coil_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Coils), 10>>,
    pub received_write_single_coil_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, bool), 10>>,
    pub received_write_multiple_coils_responses:
        RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16), 10>>,
}

impl CoilResponse for ClientMockApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils.clone()))
            .unwrap();
    }
    fn read_single_coil_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        let val_byte = if value { 1 } else { 0 };
        let coils = Coils::new(address, 1)
            .unwrap()
            .with_values(&[val_byte], 1)
            .unwrap();

        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils))
            .unwrap();
    }
    fn write_single_coil_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        self.received_write_single_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, address, value))
            .unwrap();
    }
    fn write_multiple_coils_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        self.received_write_multiple_coils_responses
            .borrow_mut()
            .push((txn_id, unit_id, address, quantity))
            .unwrap();
    }
}

impl RequestErrorNotifier for ClientMockApp {
    fn request_failed(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "Client: Request failed - txn_id: {}, unit_id: {}, error: {}",
            txn_id,
            unit_id.get(),
            error
        );
    }
}

impl TimeKeeper for ClientMockApp {
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

fn main() -> Result<()> {
    // --- Modbus Client Operations ---
    let transport = StdTcpTransport::new();
    let app = ClientMockApp::default();
    let mut tcp_config = ModbusTcpConfig::new("192.168.55.200", 502)
        .map_err(|e| anyhow::anyhow!(MbusError::from(e)))?;
    tcp_config.connection_timeout_ms = 500;
    let config = ModbusConfig::Tcp(tcp_config);

    let mut client =
        ClientServices::<_, _, 10>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();

    println!("\n--- Testing Read Single Coil ---");
    let read_single_address = 1;
    let txn_id_read_single = 100;
    client
        .read_single_coil(txn_id_read_single, unit_id, read_single_address)
        .map_err(|e| anyhow::anyhow!("Failed to send read single coil: {:?}", e))?;

    // In a real-world scenario, you would call poll() in a loop.
    // For this example, we poll a few times to allow for network latency.
    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    {
        let received_read_single = client.app().received_coil_responses.borrow();
        assert_eq!(received_read_single.len(), 1);
        let (_, _, coils) = &received_read_single[0];
        println!(
            "Client: Read single coil at address {}: {}",
            read_single_address,
            coils.value(read_single_address)?
        );
        assert_eq!(coils.value(read_single_address)?, true); // Initialized to true
    }
    println!("\n--- Testing Write Single Coil ---");
    let write_single_address = 0;
    let write_single_value = true;
    let txn_id_write_single = 101; // This line is fine
    client
        .write_single_coil(
            txn_id_write_single,
            unit_id,
            write_single_address,
            write_single_value,
        )
        .map_err(|e| anyhow::anyhow!("Failed to send write single coil: {:?}", e))?;

    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    {
        let received_write_single = client.app().received_write_single_coil_responses.borrow();
        assert_eq!(received_write_single.len(), 1);
        let (_, _, addr, val) = &received_write_single[0];
        println!("Client: Wrote single coil at address {}: {}", addr, val);
        assert_eq!(*addr, write_single_address);
        assert_eq!(*val, write_single_value);
    }
    // Verify write by reading back
    println!("\n--- Verifying Write Single Coil by Reading Back ---");
    client
        .read_single_coil(txn_id_read_single + 1, unit_id, write_single_address)
        .map_err(|e| anyhow::anyhow!("Failed to send verification read: {:?}", e))?;

    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    {
        let received_read_back = client.app().received_coil_responses.borrow();
        assert_eq!(received_read_back.len(), 2); // One for initial read, one for read back
        let (_, _, coils_read_back) = &received_read_back[1];
        println!(
            "Client: Read back coil at address {}: {}",
            write_single_address,
            coils_read_back.value(write_single_address)?
        );
        assert_eq!(
            coils_read_back.value(write_single_address)?,
            write_single_value
        );
    }

    println!("\n--- Testing Read Multiple Coils ---");
    let read_multi_address = 10;
    let read_multi_quantity = 3;
    let txn_id_read_multi = 102; // This line is fine
    client
        .read_multiple_coils(
            txn_id_read_multi,
            unit_id,
            read_multi_address,
            read_multi_quantity,
        )
        .map_err(|e| anyhow::anyhow!("Failed to send read multiple coils: {:?}", e))?;

    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    {
        let received_read_multi = client.app().received_coil_responses.borrow();
        assert_eq!(received_read_multi.len(), 3); // Initial read, read back, multi read
        let (_, _, coils_multi) = &received_read_multi[2];
        println!(
            "Client: Read multiple coils from address {} quantity {}:",
            read_multi_address, read_multi_quantity
        );
        for i in 0..read_multi_quantity {
            let current_address = read_multi_address + i;
            println!(
                "  Coil {}: {}",
                current_address,
                coils_multi.value(current_address)?
            );
        }
        assert_eq!(coils_multi.value(10)?, true);
        assert_eq!(coils_multi.value(11)?, false);
        assert_eq!(coils_multi.value(12)?, true);
    }

    println!("\n--- Testing Write Multiple Coils ---");
    let write_multi_address = 0;
    let write_multi_quantity = 3;

    let mut write_multi_coils = Coils::new(write_multi_address, write_multi_quantity).unwrap();
    write_multi_coils
        .set_value(write_multi_address + 0, false)
        .unwrap();
    write_multi_coils
        .set_value(write_multi_address + 1, true)
        .unwrap();
    write_multi_coils
        .set_value(write_multi_address + 2, true)
        .unwrap();

    let txn_id_write_multi = 103; // This line is fine
    client
        .write_multiple_coils(
            txn_id_write_multi,
            unit_id,
            write_multi_address,
            &write_multi_coils,
        )
        .map_err(|e| anyhow::anyhow!("Failed to send write multiple coils: {:?}", e))?;

    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    {
        let received_write_multi = client.app().received_write_multiple_coils_responses.borrow();
        assert_eq!(received_write_multi.len(), 1);
        let (_, _, addr_multi, qty_multi) = &received_write_multi[0];
        println!(
            "Client: Wrote multiple coils from address {} quantity {}",
            addr_multi, qty_multi
        );
        assert_eq!(*addr_multi, write_multi_address);
        assert_eq!(*qty_multi, write_multi_quantity);
    }
    // Verify write by reading back
    println!("\n--- Verifying Write Multiple Coils by Reading Back ---");
    client
        .read_multiple_coils(
            txn_id_read_multi + 1,
            unit_id,
            write_multi_address,
            write_multi_quantity,
        )
        .map_err(|e| anyhow::anyhow!("Failed to send verification read multi: {:?}", e))?;

    for _ in 0..5 {
        client.poll();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let received_read_back_multi = client.app().received_coil_responses.borrow();
    assert_eq!(received_read_back_multi.len(), 4);
    let (_, _, coils_read_back_multi) = &received_read_back_multi[3];
    println!(
        "Client: Read back multiple coils from address {} quantity {}:",
        write_multi_address, write_multi_quantity
    );

    let expected_values = [false, true, true];
    for i in 0..write_multi_quantity {
        let current_address = write_multi_address + i;
        println!(
            "  Coil {}: {}",
            current_address,
            coils_read_back_multi.value(current_address)?
        );
        assert_eq!(
            coils_read_back_multi.value(current_address)?,
            expected_values[i as usize]
        );
    }

    // In a real application, you'd need a mechanism to gracefully shut down the server thread.
    // For this example, the server thread will continue to listen until the main process exits.
    // server_handle.join().unwrap()?; // This would block indefinitely.

    println!("\nModbus coil operations example completed successfully!");
    Ok(())
}
