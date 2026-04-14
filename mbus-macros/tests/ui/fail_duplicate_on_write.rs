#![allow(unexpected_cfgs)]

use mbus_macros::modbus_app;

#[modbus_app(coils(coils, on_write_0 = first, on_write_0 = second))]
struct App {
    coils: u8,
}

fn main() {
    let _ = 0u8;
}
