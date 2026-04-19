//! Async Modbus server — zero-boilerplate and explicit-loop APIs.
//!
//! # Quick start
//!
//! ```rust,ignore
//! use mbus_async::server::{AsyncTcpServer, ModbusRequest, ModbusResponse, AsyncAppHandler};
//! use mbus_core::transport::UnitIdOrSlaveAddr;
//!
//! struct MyApp;
//!
//! impl AsyncAppHandler for MyApp {
//!     async fn handle(&mut self, req: ModbusRequest) -> ModbusResponse {
//!         ModbusResponse::NoResponse
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let unit = UnitIdOrSlaveAddr::try_from(1u8).unwrap();
//!     AsyncTcpServer::serve("0.0.0.0:502", MyApp, unit).await.unwrap();
//! }
//! ```

pub mod app_handler;
pub mod session;
#[cfg(feature = "diagnostics-stats")]
pub mod statistics;

#[cfg(feature = "server-tcp")]
pub mod tcp_server;

#[cfg(feature = "server-serial")]
pub mod serial_server;

// Flat re-exports for ergonomic use.
pub use app_handler::{
    AsyncAppHandler, AsyncAppRequirements, AsyncServerError, ModbusRequest, ModbusResponse,
};
#[cfg(feature = "traffic")]
pub use app_handler::{AsyncTrafficDirection, AsyncTrafficNotifier};
pub use session::AsyncServerSession;
#[cfg(feature = "diagnostics-stats")]
pub use statistics::AsyncServerStatistics;

#[cfg(feature = "server-tcp")]
pub use tcp_server::AsyncTcpServer;

#[cfg(feature = "server-serial")]
pub use serial_server::{AsyncAsciiServer, AsyncRtuServer, AsyncSerialServer};
