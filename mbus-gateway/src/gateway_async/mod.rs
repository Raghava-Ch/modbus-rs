pub mod gateway;
pub mod raw_gateway;
pub mod shutdown;

#[cfg(any(
    feature = "downstream-tcp",
    feature = "downstream-serial-rtu",
    feature = "downstream-serial-ascii"
))]
pub mod downstream;

#[cfg(feature = "upstream-ws")]
pub mod ws_gateway;

#[cfg(any(feature = "upstream-serial-rtu", feature = "upstream-serial-ascii"))]
pub mod serial_gateway;
