//! Async client module.
//!
//! Public entry points:
//! - [`AsyncTcpClient`] (TCP)
//! - [`AsyncSerialClient`] (RTU/ASCII)
//!
//! # Module layout
//!
//! | Module | Contents |
//! |---|---|
//! | `command` | [`ClientRequest`] and [`TaskCommand`] channel envelopes |
//! | `response` | [`ClientResponse`] typed result enum |
//! | `notifier` | [`AsyncClientNotifier`] traffic hook trait (`traffic` feature) |
//! | `client_core` | [`AsyncClientCore`] — public request API |
//! | `network_client` | [`AsyncTcpClient`] — TCP constructor |
//! | `serial_client` | [`AsyncSerialClient`] — serial constructor |

pub(crate) mod command;
pub(crate) mod response;
pub(crate) mod encode;
pub(crate) mod decode;
pub(crate) mod task;
#[cfg(feature = "traffic")]
pub mod notifier;

mod client_core;
mod network_client;
mod serial_client;

pub use client_core::AsyncClientCore;
pub use network_client::AsyncTcpClient;
pub use serial_client::AsyncSerialClient;
#[cfg(feature = "traffic")]
pub use notifier::AsyncClientNotifier;

use mbus_core::errors::MbusError;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "file-record")]
pub use mbus_core::models::file_record::{SubRequest, SubRequestParams};
#[cfg(feature = "diagnostics")]
pub use mbus_core::models::diagnostic::{DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode};

#[cfg(feature = "diagnostics")]
/// Diagnostics response payload returned by FC 08.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsDataResponse {
    /// Echoed diagnostic sub-function code.
    pub sub_function: DiagnosticSubFunction,
    /// Echoed diagnostic data words.
    pub data: Vec<u16>,
}
#[cfg(feature = "diagnostics")]
/// Communication event log payload `(status, event_count, message_count, events)` returned by FC 12.
pub type CommEventLogResponse = (u16, u16, u16, Vec<u8>);

/// Async facade error type.
#[derive(Debug, PartialEq, Eq)]
pub enum AsyncError {
    /// Error propagated from the underlying Modbus client stack.
    Mbus(MbusError),
    /// Background worker channel is closed or worker thread has stopped.
    WorkerClosed,
    /// Internal response routing mismatch between request and callback payload type.
    UnexpectedResponseType,
}

impl From<MbusError> for AsyncError {
    fn from(value: MbusError) -> Self {
        Self::Mbus(value)
    }
}

impl std::fmt::Display for AsyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mbus(err) => write!(f, "Modbus error: {err}"),
            Self::WorkerClosed => write!(f, "async worker channel closed"),
            Self::UnexpectedResponseType => write!(f, "unexpected response type from worker"),
        }
    }
}

impl std::error::Error for AsyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mbus(err) => Some(err),
            _ => None,
        }
    }
}

