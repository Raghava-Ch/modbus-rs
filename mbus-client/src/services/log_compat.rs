#[cfg(feature = "logging")]
macro_rules! client_log_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! client_log_debug {
    ($($arg:tt)*) => {{}};
}

#[cfg(feature = "logging")]
macro_rules! client_log_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! client_log_trace {
    ($($arg:tt)*) => {{}};
}

pub(crate) use client_log_debug;
pub(crate) use client_log_trace;
