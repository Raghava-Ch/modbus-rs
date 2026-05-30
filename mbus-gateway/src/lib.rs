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
//! | Feature             | Default | Description |
//! |---------------------|---------|-------------|
//! | `async`             | ✓       | Async Tokio gateway runtime (`AsyncTcpGatewayServer`) |
//! | `ws-server`         | ✗       | WebSocket gateway (`AsyncWsGatewayServer`) for WASM clients |
//! | `serial-rtu-async`  | ✗       | Async RTU serial upstream (`AsyncSerialGatewayServer<RTU>`) |
//! | `serial-ascii-async`| ✗       | Async ASCII serial upstream (`AsyncSerialGatewayServer<ASCII>`) |
//! | `logging`           | ✓       | `log` facade integration |
//! | `network`           | ✗       | Re-exports `StdTcpTransport` + `StdTcpServerTransport` from `mbus-network` for sync TCP use |
//! | `serial-rtu`        | ✗       | Re-exports `StdRtuTransport` from `mbus-serial` for sync RTU serial use |
//! | `serial-ascii`      | ✗       | Re-exports `StdAsciiTransport` from `mbus-serial` for sync ASCII serial use |
//! | `traffic`           | ✗       | Raw TX/RX frame callbacks in `GatewayEventHandler` |
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
//! use mbus_core::transport::{
//!     BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
//!     Parity, SerialMode, Transport, UnitIdOrSlaveAddr,
//! };
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
//! let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig {
//!     port_path: "/dev/ttyUSB0".try_into().unwrap(),
//!     mode: SerialMode::Rtu,
//!     baud_rate: BaudRate::Baud19200,
//!     data_bits: DataBits::Eight,
//!     stop_bits: 1,
//!     parity: Parity::None,
//!     response_timeout_ms: 1000,
//!     retry_attempts: 0,
//!     retry_backoff_strategy: BackoffStrategy::Immediate,
//!     retry_jitter_strategy: JitterStrategy::None,
//!     retry_random_fn: None,
//! });
//! downstream.connect(&serial_cfg).unwrap();
//!
//! let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _> =
//!     GatewayServices::new(router, NoopEventHandler, 1000);
//! gw.add_upstream(upstream).unwrap();
//! gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
//! loop {
//!     let now_ms = 0; // pass absolute millis from a hardware/system clock
//!     let _ = gw.poll(now_ms);
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
//! let mut gw: GatewayServices<MyTransport, MyTransport, _, _> =
//!     GatewayServices::new(PassthroughRouter, NoopEventHandler, 1000);
//! gw.add_upstream(upstream).unwrap();
//! gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
//! ```

pub mod common;

#[path = "gateway_sync/mod.rs"]
pub mod gateway_sync;

#[cfg(feature = "async")]
#[path = "gateway_async/mod.rs"]
pub mod gateway_async;

pub use common::downstream_channel::DownstreamChannel;
pub use common::event::{GatewayEventHandler, NoopEventHandler};
pub use common::router::{
    GatewayRoutingPolicy, PassthroughRouter, RangeRouteTable, UnitIdRewriteRouter, UnitRouteTable,
};

// DynRouter uses Box<dyn Trait> which requires std's allocator.
#[cfg(feature = "std-required")]
pub use common::router::DynRouter;

#[cfg(feature = "std-required")]
pub use common::router_dynamic::{DynamicRangeRouteTable, DynamicUnitRouteTable};

pub use common::txn_map::{SerialTxnMap, TxnMap};

#[cfg(feature = "std-required")]
pub use common::txn_map_dynamic::DynamicTxnMap;

pub use gateway_sync::services::{GatewayServices, PollOutcome};
#[cfg(any(
    feature = "upstream-tcp",
    feature = "upstream-serial-rtu",
    feature = "upstream-serial-ascii"
))]
pub use gateway_sync::upstream::GatewayUpstream;
pub use gateway_sync::upstream_channel::UpstreamChannel;

#[cfg(feature = "async")]
pub use gateway_async::gateway::AsyncGatewayError;

#[cfg(feature = "upstream-tcp")]
pub use gateway_async::gateway::AsyncTcpGatewayServer;

#[cfg(feature = "async")]
pub use gateway_async::raw_gateway::AsyncRawGatewayServer;

#[cfg(feature = "async")]
pub use gateway_async::shutdown::{GatewayShutdown, GatewayShutdownToken};

#[cfg(feature = "upstream-ws")]
pub use gateway_async::ws_gateway::{AsyncWsGatewayServer, WsGatewayConfig};

#[cfg(all(
    feature = "async",
    any(feature = "upstream-serial-rtu", feature = "upstream-serial-ascii")
))]
pub use gateway_async::serial_gateway::{AsyncSerialGatewayServer, SerialGatewayConfig, GatewayError};

// ── Concrete transport re-exports ─────────────────────────────────────────────

/// Sync TCP server transport from `mbus-network` (enabled by the `upstream-tcp` feature).
#[cfg(feature = "upstream-tcp")]
pub use mbus_network::StdTcpServerTransport;

/// Sync TCP client transport from `mbus-network` (enabled by the `downstream-tcp` feature).
#[cfg(feature = "downstream-tcp")]
pub use mbus_network::StdTcpTransport;

/// Sync RTU serial transport from `mbus-serial` (enabled by `upstream-serial-rtu` or `downstream-serial-rtu`).
#[cfg(any(feature = "upstream-serial-rtu", feature = "downstream-serial-rtu"))]
pub use mbus_serial::StdRtuTransport;

/// Sync ASCII serial transport from `mbus-serial` (enabled by `upstream-serial-ascii` or `downstream-serial-ascii`).
#[cfg(any(feature = "upstream-serial-ascii", feature = "downstream-serial-ascii"))]
pub use mbus_serial::StdAsciiTransport;

/// Async RTU serial transport from `mbus-serial` (enabled by `upstream-serial-rtu` or `downstream-serial-rtu` with `async`).
#[cfg(all(
    feature = "async",
    any(feature = "upstream-serial-rtu", feature = "downstream-serial-rtu")
))]
pub use mbus_serial::TokioRtuTransport;

/// Async ASCII serial transport from `mbus-serial` (enabled by `upstream-serial-ascii` or `downstream-serial-ascii` with `async`).
#[cfg(all(
    feature = "async",
    any(feature = "upstream-serial-ascii", feature = "downstream-serial-ascii")
))]
pub use mbus_serial::TokioAsciiTransport;

// ── Heterogeneous downstream & config re-exports ─────────────────────────────

/// Heterogeneous async downstream transport (TCP / RTU / ASCII) with built-in framing
/// translation.  Use [`DownstreamConfig`] to build instances from config-file settings.
#[cfg(all(
    feature = "async",
    any(
        feature = "downstream-tcp",
        feature = "downstream-serial-rtu",
        feature = "downstream-serial-ascii"
    )
))]
pub use gateway_async::downstream::{DownstreamConfig, DownstreamConnectError, GatewayTransport};

#[cfg(all(
    feature = "async",
    any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii")
))]
pub use gateway_async::downstream::SerialDownstreamConfig;

/// Async TCP transport (re-exported from `mbus-network`).
#[cfg(all(
    feature = "async",
    any(feature = "upstream-tcp", feature = "downstream-tcp")
))]
pub use mbus_network::TokioTcpTransport;

/// Transport configuration types re-exported for use in config builders.
///
/// Includes serial config enums (`BaudRate`, `DataBits`, `Parity`, `SerialMode`) as
/// well as `UnitIdOrSlaveAddr` — lets callers avoid importing `mbus-core` directly.
#[cfg(any(
    feature = "upstream-serial-rtu",
    feature = "upstream-serial-ascii",
    feature = "downstream-serial-rtu",
    feature = "downstream-serial-ascii"
))]
pub mod transport_types {
    pub use mbus_core::transport::{
        BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
        Parity, SerialMode, UnitIdOrSlaveAddr,
    };
}

/// `UnitIdOrSlaveAddr` for TCP-only/WebSocket setups without serial features.
///
/// When serial features are active the richer `transport_types` module is preferred.
#[cfg(not(any(
    feature = "upstream-serial-rtu",
    feature = "upstream-serial-ascii",
    feature = "downstream-serial-rtu",
    feature = "downstream-serial-ascii"
)))]
pub mod transport_types {
    pub use mbus_core::transport::UnitIdOrSlaveAddr;
}
