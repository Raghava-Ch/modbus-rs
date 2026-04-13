//! # mbus-client
//!
//! `mbus-client` is a high-level, `no_std` compatible Modbus client implementation.
//! It provides a structured way to interact with Modbus servers (slaves) over various
//! transport layers like TCP, RTU, or ASCII.
//!
//! ## Core Concepts
//!
//! - **`ClientServices`**: The central coordinator. It manages the lifecycle of a request,
//!   including ADU construction, transmission, response tracking, timeouts, and retries.
//! - **`Transport`**: An abstraction over the physical or link layer. This allows the client
//!   to work seamlessly over hardware-specific UARTs, TCP sockets, or custom implementations.
//! - **`App` Traits**: A set of traits (e.g., `CoilResponse`, `RegisterResponse`) that the
//!   user implements to receive asynchronous-style callbacks when a response is parsed.
//!
//! ## Features
//!
//! - **Pipelining**: Supports multiple concurrent outstanding requests (configurable via const generics).
//! - **Reliability**: Built-in support for automatic retries and configurable response timeouts.
//! - **Memory Safety**: Uses `heapless` for all internal buffering, ensuring zero dynamic
//!   allocation and suitability for hard real-time or embedded systems.
//! - **Protocol Coverage**: Implements standard function codes for coils, discrete inputs,
//!   holding/input registers, FIFO queues, and file records.
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! // These core imports are always available
//! use mbus_core::transport::{UnitIdOrSlaveAddr, ModbusConfig, ModbusTcpConfig, Transport, TransportType, TimeKeeper};
//! use mbus_core::errors::MbusError;
//!
//! # use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
//! # use mbus_client::app::{CoilResponse, RequestErrorNotifier};
//! # use mbus_client::services::coil::Coils;
//! # use mbus_client::services::ClientServices;
//! # use heapless::Vec;
//! #
//! # struct YourTransport;
//! # impl YourTransport { fn new() -> Self { Self } }
//! # impl Transport for YourTransport {
//! #     type Error = MbusError;
//! #     const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;
//! #     fn connect(&mut self, _: &ModbusConfig) -> Result<(), Self::Error> { Ok(()) }
//! #     fn disconnect(&mut self) -> Result<(), Self::Error> { Ok(()) }
//! #     fn send(&mut self, _: &[u8]) -> Result<(), Self::Error> { Ok(()) }
//! #     fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> { Ok(Vec::new()) }
//! #     fn is_connected(&self) -> bool { true }
//! # }
//! // 1. Define your application state and implement response traits
//! // Application traits and service modules are feature-gated.
//! // To use Coil services, enable the "coils" feature in Cargo.toml.
//! struct MyDevice;
//! #[cfg(feature = "coils")]
//! impl CoilResponse for MyDevice {
//!     fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
//!         // Handle the data here
//!     }
//!     // Implement other CoilResponse methods or use default empty implementations if not needed
//!     fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
//!     fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
//!     fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
//! }
//! # impl RequestErrorNotifier for MyDevice {
//! #     fn request_failed(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: MbusError) {}
//! # }
//! # impl TimeKeeper for MyDevice {
//! #     fn current_millis(&self) -> u64 { 0 }
//! # }
//!
//! # fn main() -> Result<(), MbusError> {
//! // 2. Initialize transport and config
//! let transport = YourTransport::new();
//! let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
//!
//! // 3. Create the service (N=5 allows 5 concurrent requests)
//! let mut client = ClientServices::<_, _, 5>::new(transport, MyDevice, config)?;
//! client.connect()?;
//!
//! // 4. Send a request (only available if the "coils" feature is enabled)
//! #[cfg(feature = "coils")]
//! {
//!     client.coils().read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;
//! }
//!
//! // 5. Periodically poll to process incoming bytes and handle timeouts
//! loop {
//!     client.poll();
//! #   break;
//! }
//! # Ok(())
//! # }
//! ```

#![cfg_attr(not(doc), no_std)]
#![warn(missing_docs)]

/// Application-level traits and callback definitions. Users implement these to handle data
/// returned by the Modbus server. These modules are conditionally compiled based on features.
pub mod app;

/// Internal Modbus services.
/// Contains logic for specific function codes (Coils, Registers, etc.) and
/// the core `ClientServices` orchestration logic.
pub mod services;
