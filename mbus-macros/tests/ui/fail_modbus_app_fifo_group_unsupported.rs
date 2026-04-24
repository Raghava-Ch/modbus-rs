use mbus_macros::modbus_app;

// Referencing a field name that does not exist in the struct must be rejected.
#[modbus_app(fifo(nonexistent_fifo))]
struct App {
    some_field: u8,
}

fn main() {}
