#![cfg(feature = "discrete-inputs")]

mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{ModbusAppHandler, ResilienceConfig, ServerServices};

#[derive(Debug, Default)]
struct DiscreteInputApp;

impl ModbusAppHandler for DiscreteInputApp {
    fn read_discrete_inputs_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if address != 0 {
            return Err(MbusError::InvalidAddress);
        }
        if quantity > 8 {
            return Err(MbusError::InvalidQuantity);
        }
        // Return a deterministic pattern: inputs 0 and 3 are ON.
        out[0] = 0b0000_1001;
        Ok(1)
    }

    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        _out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        _out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) -> Result<(), MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _values: &[u16],
    ) -> Result<(), MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for DiscreteInputApp {}

fn run_once(payload: &[u8]) -> Vec<u8> {
    let request = build_request(1, unit_id(1), FunctionCode::ReadDiscreteInputs, payload);
    let sent_frames = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent_frames),
        connected: true,
    };

    let mut server: ServerServices<MockTransport, DiscreteInputApp> = ServerServices::new(
        transport,
        DiscreteInputApp,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    server.poll();

    let frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(frames.len(), 1);
    frames[0].clone()
}

#[test]
fn fc02_success_returns_packed_discrete_input_bytes() {
    let response = run_once(&[0x00, 0x00, 0x00, 0x04]);

    assert_eq!(response[7], 0x02, "FC02 response function code");
    assert_eq!(response[8], 1, "byte count");
    assert_eq!(response[9], 0b0000_1001, "packed discrete inputs");
}

#[test]
fn fc02_invalid_quantity_returns_exception_before_callback() {
    let response = run_once(&[0x00, 0x00, 0x00, 0x00]);

    assert_eq!(response[7], 0x82, "FC02 exception function code");
    assert_eq!(
        response[8],
        ExceptionCode::IllegalDataValue as u8,
        "InvalidQuantity should map to IllegalDataValue"
    );
}

#[test]
fn fc02_address_overflow_returns_exception_before_callback() {
    let response = run_once(&[0xFF, 0xFF, 0x00, 0x02]);

    assert_eq!(response[7], 0x82, "FC02 exception function code");
    assert_eq!(
        response[8],
        ExceptionCode::IllegalDataAddress as u8,
        "InvalidAddress should map to IllegalDataAddress"
    );
}
