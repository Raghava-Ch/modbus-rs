use mbus_core::{
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    transport::{TimeKeeper, UnitIdOrSlaveAddr},
};
use modbus_client::{
    app::{
        CoilResponse, DiagnosticsResponse, DiscreteInputResponse, FifoQueueResponse,
        FileRecordResponse, RegisterResponse, RequestErrorNotifier,
    },
    services::{
        coil::Coils, diagnostic::DeviceIdentificationResponse, discrete_input::DiscreteInputs,
        fifo_queue::FifoQueue, file_record::SubRequestParams, register::Registers,
    },
};
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use std::vec::Vec; // Import standard Vec for the type alias

/// Type alias for the complex tuple representing a received encapsulated interface transport response.
type ReceivedEncapsulatedInterfaceTransportResponse = (u16, UnitIdOrSlaveAddr, EncapsulatedInterfaceType, Vec<u8>);


#[allow(dead_code)]
#[derive(Default)]
pub struct MockApp {
    pub received_coil_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Coils)>>, // Corrected duplicate
    pub received_write_single_coil_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, bool)>>,
    pub received_write_multiple_coils_responses: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16)>>,
    pub received_discrete_input_responses:
        RefCell<Vec<(u16, UnitIdOrSlaveAddr, DiscreteInputs, u16)>>,
    pub received_read_device_id_responses:
        RefCell<Vec<(u16, UnitIdOrSlaveAddr, DeviceIdentificationResponse)>>,
    pub received_encapsulated_interface_transport_responses: RefCell<Vec<ReceivedEncapsulatedInterfaceTransportResponse>>,
    pub failed_requests: RefCell<Vec<(u16, UnitIdOrSlaveAddr, MbusError)>>,
}

impl CoilResponse for MockApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils.clone()));
    }
    fn read_single_coil_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        let mut coils = Coils::new(address, 1).unwrap();
        coils.set_value(address, value).unwrap();
        self.received_coil_responses
            .borrow_mut()
            .push((txn_id, unit_id, coils));
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
            .push((txn_id, unit_id, address, value));
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
            .push((txn_id, unit_id, address, quantity));
    }
}

impl DiscreteInputResponse for MockApp {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        inputs: &DiscreteInputs,
    ) {
        self.received_discrete_input_responses.borrow_mut().push((
            txn_id,
            unit_id,
            inputs.clone(),
            inputs.quantity(),
        ));
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        // Create a DiscreteInputs container for a single bit.
        // The value is packed into the first byte of the heapless Vec.
        let mut values = [0u8; mbus_core::models::discrete_input::MAX_DISCRETE_INPUT_BYTES];
        values[0] = if value { 0x01 } else { 0x00 };

        let inputs = DiscreteInputs::new(address, 1)
            .expect("Failed to create DiscreteInputs")
            .with_values(&values, 1)
            .expect("Failed to set discrete input values");

        self.received_discrete_input_responses
            .borrow_mut()
            .push((txn_id, unit_id, inputs, 1));
    }
}

impl RequestErrorNotifier for MockApp {
    fn request_failed(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        self.failed_requests
            .borrow_mut()
            .push((txn_id, unit_id, error));
        println!("Request failed: {:?}", error);
    }
}

impl RegisterResponse for MockApp {
    fn read_multiple_holding_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _registers: &Registers,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_multiple_input_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _registers: &Registers,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_single_input_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
    }

    fn read_single_holding_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
    }

    fn write_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _registers: &Registers,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn read_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }

    fn mask_write_register_response(&mut self, _txn_id: u16, _unit_id: UnitIdOrSlaveAddr) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

impl FifoQueueResponse for MockApp {
    fn read_fifo_queue_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _fifo_queue: &FifoQueue,
    ) {
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
    fn read_file_record_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _data: &[SubRequestParams],
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
    fn write_file_record_response(&mut self, _txn_id: u16, _unit_id: UnitIdOrSlaveAddr) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

impl DiagnosticsResponse for MockApp {
    fn read_device_identification_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        response: &DeviceIdentificationResponse,
    ) {
        self.received_read_device_id_responses.borrow_mut().push((
            txn_id,
            unit_id,
            response.clone(),
        ));
    }

    fn encapsulated_interface_transport_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) {
        self.received_encapsulated_interface_transport_responses
            .borrow_mut()
            .push((txn_id, unit_id, mei_type, data.to_vec()));
    }

    fn read_exception_status_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u8,
    ) {
    }

    fn diagnostics_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _sub_function: DiagnosticSubFunction,
        _data: &[u16],
    ) {
    }

    fn get_comm_event_counter_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u16,
        _event_count: u16,
    ) {
    }

    fn get_comm_event_log_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u16,
        _event_count: u16,
        _message_count: u16,
        _events: &[u8],
    ) {
    }

    fn report_server_id_response(&self, _txn_id: u16, _unit_id: UnitIdOrSlaveAddr, _data: &[u8]) {}
}
