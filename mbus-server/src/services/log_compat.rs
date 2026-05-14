#[cfg(feature = "logging")]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {{
        // Evaluates to nothing.
        // We do not use `core::format_args!` here to avoid dragging in the `core::fmt` machinery,
        // which can add massive bloat (like `core::num::flt2dec`) to bare-metal builds.
    }};
}

#[cfg(feature = "logging")]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {{
        // Evaluates to nothing.
        // We do not use `core::format_args!` here to avoid dragging in the `core::fmt` machinery,
        // which can add massive bloat (like `core::num::flt2dec`) to bare-metal builds.
    }};
}

pub(crate) use server_log_debug;
pub(crate) use server_log_trace;
