#[cfg(feature = "holding-registers")]
use mbus_core::models::register::HoldingRegisters;
#[cfg(feature = "input-registers")]
use mbus_core::models::register::InputRegisters;

/// Opaque wrapper around a `HoldingRegisters` value passed through a C callback.
///
/// C callers receive a `*const MbusHoldingRegisters` inside the relevant context struct.
/// Use the corresponding accessor functions (e.g. `mbus_holding_registers_get_value`) to
/// extract data. The pointer is **only valid during the callback invocation**.
#[cfg(feature = "holding-registers")]
#[repr(transparent)]
pub struct MbusHoldingRegisters(pub(crate) HoldingRegisters);

/// Opaque wrapper around an `InputRegisters` value passed through a C callback.
///
/// C callers receive a `*const MbusInputRegisters` inside the relevant context struct.
/// Use the corresponding accessor functions (e.g. `mbus_input_registers_get_value`) to
/// extract data. The pointer is **only valid during the callback invocation**.
#[cfg(feature = "input-registers")]
#[repr(transparent)]
pub struct MbusInputRegisters(pub(crate) InputRegisters);

/// C-facing opaque register handle used in callback context structs.
///
/// All three register callback context structs (`MbusReadHoldingRegistersCtx`,
/// `MbusReadInputRegistersCtx`, `MbusReadWriteMultipleRegistersCtx`) expose a
/// `*const MbusRegisters` field so that a single family of C accessor functions
/// can cover all register reads. On the Rust side, the pointer always points to
/// either a `MbusHoldingRegisters` or `MbusInputRegisters` value whose layout is
/// identical (both are `#[repr(transparent)]` wrappers over the same struct).
#[cfg(feature = "holding-registers")]
pub use MbusHoldingRegisters as MbusRegisters;
#[cfg(all(feature = "input-registers", not(feature = "holding-registers")))]
pub use MbusInputRegisters as MbusRegisters;
