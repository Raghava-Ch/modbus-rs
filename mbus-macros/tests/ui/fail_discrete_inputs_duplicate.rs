extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidQuantity,
        InvalidByteCount,
        BufferTooSmall,
        InvalidValue,
    }
}

pub mod models {
    pub mod discrete_input {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub enum DiscreteInputState {
            On,
            #[default]
            Off,
        }
    }
}

pub trait DiscreteInputMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
}

use mbus_macros::DiscreteInputsModel;
use models::discrete_input::DiscreteInputState;

#[derive(DiscreteInputsModel)]
struct BadDiscreteInputs {
    #[discrete_input(addr = 0)]
    first: DiscreteInputState,
    #[discrete_input(addr = 0)]
    second: DiscreteInputState,
}

fn main() {
    let _ = BadDiscreteInputs {
        first: DiscreteInputState::Off,
        second: DiscreteInputState::On,
    };
}
