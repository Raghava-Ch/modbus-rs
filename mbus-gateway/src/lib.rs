// When none of the std-requiring features are enabled this crate is no_std compatible.
// `std-required` is an internal umbrella feature implied by every feature that needs std
// (async, logging, network, serial-*). Adding a new std-requiring feature only requires
// adding `"std-required"` to its entry in Cargo.toml.
#![cfg_attr(not(any(doc, feature = "std-required")), no_std)]

//! # mbus-gateway
//!
//! A Modbus gateway runtime that bridges two Modbus networks.
//!
//! The gateway acts as a **server** to upstream clients (e.g., SCADA over TCP) and as a
//! **client** to downstream devices (e.g., RTU slaves on a serial bus). It accepts upstream
//! requests, routes them by unit ID to the correct downstream channel, translates ADU framing
//! (TCP MBAP ↔ RTU CRC ↔ ASCII LRC), forwards the PDU, and returns the response.
//!
//! ## Feature Flags
//!
//! | Feature        | Default | Description |
//! |----------------|---------|-------------|
//! | `async`        | ✓       | Async Tokio gateway runtime (`AsyncTcpGatewayServer`) |
//! | `ws-server`    | ✗       | WebSocket gateway (`AsyncWsGatewayServer`) for WASM clients |
//! | `logging`      | ✓       | `log` facade integration |
//! | `network`      | ✗       | Re-exports `StdTcpTransport` + `StdTcpServerTransport` from `mbus-network` for sync TCP use |
//! | `serial-rtu`   | ✗       | Re-exports `StdRtuTransport` from `mbus-serial` for sync RTU serial use |
//! | `serial-ascii` | ✗       | Re-exports `StdAsciiTransport` from `mbus-serial` for sync ASCII serial use |
//! | `traffic`      | ✗       | Raw TX/RX frame callbacks in `GatewayEventHandler` |
//!
//! ## Quick Start (sync, TCP → RTU)
//!
//! Enable `network` + `serial-rtu` features, then:
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "network", feature = "serial-rtu"))]
//! # {
//! use std::net::TcpListener;
//! use mbus_gateway::{
//!     GatewayServices, UnitRouteTable, NoopEventHandler, DownstreamChannel,
//!     StdTcpServerTransport, StdRtuTransport,
//! };
//! use mbus_core::transport::{ModbusConfig, SerialConfig, UnitIdOrSlaveAddr};
//!
//! // Route unit 1 → channel 0
//! let mut router: UnitRouteTable<8> = UnitRouteTable::new();
//! router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
//!
//! let listener = TcpListener::bind("0.0.0.0:502").unwrap();
//! let (stream, _peer) = listener.accept().unwrap();
//! let upstream = StdTcpServerTransport::new(stream);
//!
//! let mut downstream = StdRtuTransport::new();
//! let serial_cfg = ModbusConfig::Serial(SerialConfig::default());
//! downstream.connect(&serial_cfg).unwrap();
//!
//! let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _, 1> =
//!     GatewayServices::new(upstream, router, NoopEventHandler);
//! gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
//! loop {
//!     let _ = gw.poll();
//! }
//! # }
//! ```
//!
//! ## Custom Transport
//!
//! `GatewayServices` is fully generic over any type that implements [`mbus_core::transport::Transport`].
//! You are **not** limited to the built-in TCP or serial transports — any communication
//! medium (shared-memory ring buffer, USB, CAN, UART with a custom framing, …) can be
//! used by implementing the five-method `Transport` trait:
//!
//! ```rust
//! use heapless::Vec;
//! use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
//! use mbus_core::errors::MbusError;
//! use mbus_core::transport::{ModbusConfig, Transport, TransportType};
//! use mbus_gateway::{GatewayServices, PassthroughRouter, NoopEventHandler, DownstreamChannel};
//!
//! /// A minimal custom transport (loopback / in-memory for illustration).
//! struct MyTransport {
//!     connected: bool,
//!     pending: Option<Vec<u8, MAX_ADU_FRAME_LEN>>,
//! }
//!
//! impl MyTransport {
//!     fn new() -> Self { Self { connected: false, pending: None } }
//! }
//!
//! impl Transport for MyTransport {
//!     type Error = MbusError;
//!     const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;
//!
//!     fn connect(&mut self, _cfg: &ModbusConfig) -> Result<(), MbusError> {
//!         self.connected = true;
//!         Ok(())
//!     }
//!     fn disconnect(&mut self) -> Result<(), MbusError> {
//!         self.connected = false;
//!         Ok(())
//!     }
//!     fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
//!         let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
//!         buf.extend_from_slice(adu).map_err(|_| MbusError::BufferTooSmall)?;
//!         self.pending = Some(buf);
//!         Ok(())
//!     }
//!     fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
//!         self.pending.take().ok_or(MbusError::Timeout)
//!     }
//!     fn is_connected(&self) -> bool { self.connected }
//! }
//!
//! // Plug the custom transport into GatewayServices just like any built-in transport.
//! let upstream = MyTransport::new();
//! let downstream = MyTransport::new();
//! let mut gw: GatewayServices<MyTransport, MyTransport, _, _, 1> =
//!     GatewayServices::new(upstream, PassthroughRouter, NoopEventHandler);
//! gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
//! ```

pub mod dispatcher;
pub mod event;
pub mod router;
pub mod services;
pub mod txn_map;

#[cfg(feature = "async")]
pub mod async_gateway;

#[cfg(feature = "ws-server")]
pub mod ws_gateway;

pub(crate) mod log_compat;

pub use dispatcher::DownstreamChannel;
pub use event::{GatewayEventHandler, NoopEventHandler};
pub use router::{
    GatewayRoutingPolicy, PassthroughRouter, RangeRouteTable, UnitIdRewriteRouter, UnitRouteTable,
};
pub use services::GatewayServices;
pub use txn_map::{SerialTxnMap, TxnMap};

#[cfg(feature = "async")]
pub use async_gateway::{AsyncGatewayError, AsyncTcpGatewayServer};

#[cfg(feature = "ws-server")]
pub use ws_gateway::{AsyncWsGatewayServer, WsGatewayConfig};

// ── Concrete transport re-exports ─────────────────────────────────────────────

/// Sync TCP transports from `mbus-network` (enabled by the `network` feature).
///
/// - [`StdTcpServerTransport`] — wraps an accepted `TcpStream`; use on the upstream side.
/// - [`StdTcpTransport`] — outbound TCP client; use on the downstream side.
#[cfg(feature = "network")]
pub use mbus_network::{StdTcpServerTransport, StdTcpTransport};

/// Sync RTU serial transport from `mbus-serial` (enabled by the `serial-rtu` feature).
///
/// [`StdRtuTransport`] implements Modbus RTU framing (binary + CRC16) over a
/// `serialport`-backed port.  Use it on either the upstream or downstream side.
#[cfg(feature = "serial-rtu")]
pub use mbus_serial::StdRtuTransport;

/// Sync ASCII serial transport from `mbus-serial` (enabled by the `serial-ascii` feature).
///
/// [`StdAsciiTransport`] implements Modbus ASCII framing (`:` delimited + LRC) over a
/// `serialport`-backed port.  Use it on either the upstream or downstream side.
#[cfg(feature = "serial-ascii")]
pub use mbus_serial::StdAsciiTransport;
