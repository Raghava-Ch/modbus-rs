// When none of the std-requiring features are enabled this crate is no_std compatible.
// `std-required` is an internal umbrella feature implied by every feature that needs std
// (async, logging). Adding a new std-requiring feature only requires adding `"std-required"`
// to its entry in Cargo.toml.
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
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `async` | ✓ | Async Tokio gateway runtime (`AsyncTcpGatewayServer`) |
//! | `logging` | ✓ | `log` facade integration |
//! | `traffic` | ✗ | Raw TX/RX frame callbacks in `GatewayEventHandler` |
//!
//! ## Quick Start (sync)
//!
//! ```rust,no_run
//! use mbus_gateway::{GatewayServices, UnitRouteTable, NoopEventHandler, DownstreamChannel};
//! use mbus_core::transport::UnitIdOrSlaveAddr;
//!
//! // Build a routing table: unit 1 → channel 0
//! let mut router: UnitRouteTable<8> = UnitRouteTable::new();
//! router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
//!
//! // Instantiate the gateway with upstream and downstream transports
//! // (provide your actual transport implementations here)
//! // let upstream = StdTcpServerTransport::new(stream);
//! // let downstream = StdTcpTransport::new();
//! // let mut gw: GatewayServices<_, _, _, _, 1> =
//! //     GatewayServices::new(upstream, router, NoopEventHandler);
//! // gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
//! // loop { gw.poll().ok(); }
//! ```

pub mod dispatcher;
pub mod event;
pub mod router;
pub mod services;
pub mod txn_map;

#[cfg(feature = "async")]
pub mod async_gateway;

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
