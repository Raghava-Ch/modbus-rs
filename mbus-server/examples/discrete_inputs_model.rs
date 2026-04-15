//! Minimal `DiscreteInputsModel` usage example.
//!
//! Run:
//! cargo run -p mbus-server --example discrete_inputs_model

use mbus_core::errors::MbusError;
use mbus_server::DiscreteInputsModel;
use mbus_server::prelude::*;

#[derive(Debug, Clone, Default, DiscreteInputsModel)]
struct PanelInputs {
    #[discrete_input(addr = 0)]
    ready: bool,
    #[discrete_input(addr = 1)]
    alarm: bool,
    #[discrete_input(addr = 2)]
    maintenance_required: bool,
    #[discrete_input(addr = 3)]
    door_open: bool,
}

fn main() -> Result<(), MbusError> {
    let inputs = PanelInputs {
        ready: true,
        alarm: false,
        maintenance_required: true,
        door_open: false,
    };

    let mut out = [0u8; 1];
    let byte_count = inputs.encode(0, 4, &mut out)?;

    println!("encoded {} byte: {:08b}", byte_count, out[0]);
    Ok(())
}
