use mbus_core::{
    app::{CoilResponse, Coils},
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

    fn request_failed(&self, _txn_id: u16, _unit_id: u8, _error: MbusError) {
        // In a real application, this would log the error or update some state.
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
