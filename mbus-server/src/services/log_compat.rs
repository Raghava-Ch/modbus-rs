#[cfg(all(feature = "logging", not(feature = "defmt")))]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(feature = "defmt")]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {
        defmt::debug!($($arg)*)
    };
}

#[cfg(not(any(feature = "logging", feature = "defmt")))]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {{
        // Evaluates to nothing.
        // We do not use `core::format_args!` here to avoid dragging in the `core::fmt` machinery,
        // which can add massive bloat (like `core::num::flt2dec`) to bare-metal builds.
    }};
}

#[cfg(all(feature = "logging", not(feature = "defmt")))]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(feature = "defmt")]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {
        defmt::trace!($($arg)*)
    };
}

#[cfg(not(any(feature = "logging", feature = "defmt")))]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {{
        // Evaluates to nothing.
        // We do not use `core::format_args!` here to avoid dragging in the `core::fmt` machinery,
        // which can add massive bloat (like `core::num::flt2dec`) to bare-metal builds.
    }};
}

pub(crate) use server_log_debug;
pub(crate) use server_log_trace;
