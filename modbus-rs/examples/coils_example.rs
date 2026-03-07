use anyhow::Result;
use heapless::Vec as HeaplessVec;
use mbus_core::app::{
    CoilResponse, Coils, DiscreteInputResponse, FifoQueueResponse, FileRecordResponse, RegisterResponse, RequestErrorNotifier
};
use mbus_core::client::services::discrete_inputs::DiscreteInputs;
use mbus_core::client::services::{ClientServices, discrete_inputs};
use mbus_core::client::services::coils::MAX_COIL_BYTES;
use mbus_core::client::services::fifo::FifoQueue;
use mbus_core::client::services::file_record::SubRequestParams;
use mbus_core::client::services::registers::Registers;
// Import MAX_COIL_BYTES
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, TimeKeeper};
use mbus_tcp::management::std_transport::StdTcpTransport;
use std::cell::RefCell;

// --- MockApp for Client ---
// This struct implements the CoilResponse trait and is used by the client
// to receive and store responses from the Modbus server.
#[derive(Debug, Default)]
struct ClientMockApp {
    pub received_coil_responses: RefCell<HeaplessVec<(u16, u8, Coils, u16), 10>>,
    pub received_write_single_coil_responses: RefCell<HeaplessVec<(u16, u8, u16, bool), 10>>,
    pub received_write_multiple_coils_responses: RefCell<HeaplessVec<(u16, u8, u16, u16), 10>>,
}

impl CoilResponse for ClientMockApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: u8, coils: &Coils, quantity: u16) {
        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils.clone(), quantity))
            .unwrap();
    }
    fn read_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
        let mut values_vec = heapless::Vec::<u8, MAX_COIL_BYTES>::new();
        values_vec.push(if value { 0x01 } else { 0x00 }).unwrap();
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
    fn write_multiple_coils_response(&self, txn_id: u16, unit_id: u8, address: u16, quantity: u16) {
        self.received_write_multiple_coils_responses
            .borrow_mut()
            .push((txn_id, unit_id, address, quantity))
            .unwrap();
    }
}

impl RegisterResponse for ClientMockApp {
    fn read_input_register_response(&mut self, _txn_id: u16, _unit_id: u8, _registers: &Registers) {
    }
    fn read_holding_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
    }
    fn write_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }

    fn read_single_input_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _starting_address: u16,
        _quantity: u16,
    ) {
    }
    fn read_write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
    }
    fn read_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn read_single_holding_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn mask_write_register_response(&mut self, _txn_id: u16, _unit_id: u8) {}
}

impl RequestErrorNotifier for ClientMockApp {
    fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError) {
        println!(
            "Client: Request failed - txn_id: {}, unit_id: {}, error: {}",
            txn_id, unit_id, error
        );
    }
}

impl FifoQueueResponse for ClientMockApp {
    fn read_fifo_queue_response(&mut self, _txn_id: u16, _unit_id: u8, _values: &FifoQueue) {
        // Not used in this example
    }
}

impl DiscreteInputResponse for ClientMockApp {
    fn read_discrete_inputs_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _inputs: &DiscreteInputs,
        _quantity: u16
    ) {
    }

    fn read_single_discrete_input_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: bool,
    ) {
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

impl FileRecordResponse for ClientMockApp {
    fn read_file_record_response(&mut self, _txn_id: u16, _unit_id: u8, _data: &[SubRequestParams]) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
    fn write_file_record_response(&mut self, _txn_id: u16, _unit_id: u8) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

fn main() -> Result<()> {
    // --- Modbus Client Operations ---
    let transport = StdTcpTransport::new();
    let app = ClientMockApp::default();
    let mut config =
        ModbusConfig::default("192.168.55.101").map_err(|e| anyhow::anyhow!(MbusError::from(e)))?;
    config.connection_timeout_ms = 500;

    let mut client =
        ClientServices::<_, 10, _>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = 1;

    println!("\n--- Testing Read Single Coil ---");
    let read_single_address = 1;
    let txn_id_read_single = 100;
    client
        .read_single_coil(txn_id_read_single, unit_id, read_single_address)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    {
        let received_read_single = client.app.received_coil_responses.borrow();
        assert_eq!(received_read_single.len(), 1);
        let (_, _, coils, _) = &received_read_single[0];
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
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    {
        let received_write_single = client.app.received_write_single_coil_responses.borrow();
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
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    {
        let received_read_back = client.app.received_coil_responses.borrow();
        assert_eq!(received_read_back.len(), 2); // One for initial read, one for read back
        let (_, _, coils_read_back, _) = &received_read_back[1];
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
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    {
        let received_read_multi = client.app.received_coil_responses.borrow();
        assert_eq!(received_read_multi.len(), 3); // Initial read, read back, multi read
        let (_, _, coils_multi, _) = &received_read_multi[2];
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
    let write_multi_values = [false, true, true]; // Address 0, 1, 2
    let txn_id_write_multi = 103; // This line is fine
    client
        .write_multiple_coils(
            txn_id_write_multi,
            unit_id,
            write_multi_address,
            write_multi_quantity,
            &write_multi_values,
        )
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    {
        let received_write_multi = client.app.received_write_multiple_coils_responses.borrow();
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
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process the response
    let received_read_back_multi = client.app.received_coil_responses.borrow();
    assert_eq!(received_read_back_multi.len(), 4);
    let (_, _, coils_read_back_multi, _) = &received_read_back_multi[3];
    println!(
        "Client: Read back multiple coils from address {} quantity {}:",
        write_multi_address, write_multi_quantity
    );
    for i in 0..write_multi_quantity {
        let current_address = write_multi_address + i;
        println!(
            "  Coil {}: {}",
            current_address,
            coils_read_back_multi.value(current_address)?
        );
        assert_eq!(
            coils_read_back_multi.value(current_address)?,
            write_multi_values[i as usize]
        );
    }

    // In a real application, you'd need a mechanism to gracefully shut down the server thread.
    // For this example, the server thread will continue to listen until the main process exits.
    // server_handle.join().unwrap()?; // This would block indefinitely.

    println!("\nModbus coil operations example completed successfully!");
    Ok(())
}
