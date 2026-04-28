//! Internal logging shims.
//!
//! When `logging` is enabled the macros forward to `log::debug!` / `log::trace!`.
//! Without the feature they become no-ops that don't even allocate a format string.

#[cfg(feature = "logging")]
macro_rules! gateway_log_debug {
    ($($t:tt)*) => { log::debug!($($t)*) };
}

#[cfg(feature = "logging")]
macro_rules! gateway_log_trace {
    ($($t:tt)*) => { log::trace!($($t)*) };
}

#[cfg(feature = "logging")]
macro_rules! gateway_log_warn {
    ($($t:tt)*) => { log::warn!($($t)*) };
}

#[cfg(not(feature = "logging"))]
macro_rules! gateway_log_debug {
    ($($t:tt)*) => {{
        let _ = core::format_args!($($t)*);
    }};
}

#[cfg(not(feature = "logging"))]
macro_rules! gateway_log_trace {
    ($($t:tt)*) => {{
        let _ = core::format_args!($($t)*);
    }};
}

#[cfg(not(feature = "logging"))]
macro_rules! gateway_log_warn {
    ($($t:tt)*) => {{
        let _ = core::format_args!($($t)*);
    }};
}

pub(crate) use gateway_log_debug;
pub(crate) use gateway_log_trace;
pub(crate) use gateway_log_warn;
