use mbus_core::app::{CoilResponse, Coils};
use std::cell::RefCell;

#[allow(dead_code)]
#[derive(Default)]
pub struct MockApp {
    pub received_coil_responses: RefCell<Vec<(u16, u8, Coils, u16)>>,
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
}
