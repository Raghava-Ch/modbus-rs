#[cfg(feature = "logging")]
macro_rules! client_log_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! client_log_debug {
    ($($arg:tt)*) => {{
        let _ = core::format_args!($($arg)*);
    }};
}

#[cfg(feature = "logging")]
macro_rules! client_log_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! client_log_trace {
    ($($arg:tt)*) => {{
        let _ = core::format_args!($($arg)*);
    }};
}

pub(crate) use client_log_debug;
pub(crate) use client_log_trace;