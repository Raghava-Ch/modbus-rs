use mbus_core::{
    app::{CoilResponse, Coils, DiscreteInputResponse, FifoQueueResponse, FileRecordResponse, RegisterResponse, RequestErrorNotifier},
    client::services::{discrete_inputs::DiscreteInputs, fifo::FifoQueue, file_record::SubRequestParams, registers::Registers},
    errors::MbusError,
    transport::TimeKeeper,
};
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

#[allow(dead_code)]
#[derive(Default)]
pub struct MockApp {
    pub received_coil_responses: RefCell<Vec<(u16, u8, Coils, u16)>>, // Corrected duplicate
    pub received_write_single_coil_responses: RefCell<Vec<(u16, u8, u16, bool)>>,
    pub received_write_multiple_coils_responses: RefCell<Vec<(u16, u8, u16, u16)>>,
    pub received_discrete_input_responses: RefCell<Vec<(u16, u8, DiscreteInputs, u16)>>,
}

impl CoilResponse for MockApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: u8, coils: &Coils, quantity: u16) {
        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils.clone(), quantity));
    }
    fn read_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
        self.received_coil_responses.borrow_mut().push((
            txn_id,
            unit_id,
            Coils::new(
                address,
                1,
                heapless::Vec::from_slice(&[if value { 1 } else { 0 }]).unwrap(),
            ),
            1,
        ));
    }

    fn write_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
        self.received_write_single_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, address, value));
    }

    fn write_multiple_coils_response(&self, txn_id: u16, unit_id: u8, address: u16, quantity: u16) {
        self.received_write_multiple_coils_responses
            .borrow_mut()
            .push((txn_id, unit_id, address, quantity));
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
            .push((txn_id, unit_id, inputs.clone(), quantity));
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: bool,
    ) {
        let mut values = heapless::Vec::new();
        values.push(if value { 1 } else { 0 }).unwrap();
        let inputs = DiscreteInputs::new(address, 1, values);
        self.received_discrete_input_responses
            .borrow_mut()
            .push((txn_id, unit_id, inputs, 1));
    }
}

impl RequestErrorNotifier for MockApp {
    fn request_failed(&self, _txn_id: u16, _unit_id: u8, _error: MbusError) {
        // In a real application, this would log the error or update some state.
    }
}

impl RegisterResponse for MockApp {
    fn read_holding_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_input_register_response(&mut self, _txn_id: u16, _unit_id: u8, _registers: &Registers) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_single_input_register_response(
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

    fn write_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _quantity: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn mask_write_register_response(&mut self, _txn_id: u16, _unit_id: u8) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

impl FifoQueueResponse for MockApp {
    fn read_fifo_queue_response(&mut self, _txn_id: u16, _unit_id: u8, _fifo_queue: &FifoQueue) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

impl TimeKeeper for MockApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64
    }
}


impl FileRecordResponse for MockApp {
    fn read_file_record_response(&mut self, _txn_id: u16, _unit_id: u8, _data: &[SubRequestParams]) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
    fn write_file_record_response(&mut self, _txn_id: u16, _unit_id: u8) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}