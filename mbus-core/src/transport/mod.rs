//! # Modbus Transport Layer
//!
//! This module defines the abstractions and configurations required for transmitting
//! Modbus Application Data Units (ADUs) over various physical and logical mediums.
//!
//! ## Core Concepts
//! - **[`Transport`]**: A unified trait that abstracts the underlying communication
//!   (TCP, Serial, or Mock) from the high-level protocol logic.
//! - **[`ModbusConfig`]**: A comprehensive configuration enum for setting up
//!   TCP/IP or Serial (RTU/ASCII) parameters.
//! - **[`BackoffStrategy`]**: Poll-driven retry scheduling strategy used after timeouts.
//! - **[`JitterStrategy`]**: Optional jitter added on top of retry backoff delays.
//! - **[`RetryRandomFn`]**: Application-supplied random callback used only when jitter is enabled.
//! - **[`UnitIdOrSlaveAddr`]**: A type-safe wrapper ensuring that Modbus addresses
//!   stay within the valid range (1-247) and handling broadcast (0) explicitly.
//!
//! ## Design Goals
//! - **`no_std` Compatibility**: Uses `heapless` data structures and `core` traits
//!   to ensure the library can run on bare-metal embedded systems.
//! - **Non-blocking I/O**: The `Transport::recv` interface is designed to be polled,
//!   allowing the client to remain responsive without requiring an OS-level thread.
//! - **Scheduled retries**: Retry backoff/jitter values are consumed by higher layers
//!   to schedule retransmissions using timestamps, never by sleeping.
//! - **Extensibility**: Users can implement the `Transport` trait to support
//!   custom hardware (e.g., specialized UART drivers or proprietary TCP stacks).
//!
//! ## Error Handling
//! Errors are categorized into [`TransportError`], which can be seamlessly converted
//! into the top-level [`crate::errors::MbusError`] used throughout the crate.

pub mod checksum;

mod config;
mod error;
mod retry;
#[allow(clippy::module_inception)]
mod transport;
mod unit_id;

pub use config::{
    BaudRate, DataBits, ModbusConfig, ModbusSerialConfig, ModbusTcpConfig, Parity, SerialMode,
};
pub use error::{TransportError, TransportType};
pub use retry::{BackoffStrategy, JitterStrategy, RetryRandomFn};
#[cfg(feature = "async")]
pub use transport::AsyncTransport;
pub use transport::{TimeKeeper, Transport};
pub use unit_id::{UidSaddrFrom, UnitIdOrSlaveAddr};
