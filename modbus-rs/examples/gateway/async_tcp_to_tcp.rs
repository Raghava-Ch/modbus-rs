//! Async TCP-to-TCP gateway with unit-ID remapping.
//!
//! Accepts upstream Modbus TCP connections on port 5502 and forwards requests
//! to a downstream Modbus TCP server on port 502.  Unit IDs 1–10 are rewritten
//! by an additive offset of +100 before hitting the downstream (i.e. upstream
//! unit 1 → downstream unit 101).
//!
//! # Usage
//!
//! ```text
//! MBUS_GATEWAY_UPSTREAM=0.0.0.0:5502 \
//! MBUS_GATEWAY_DOWNSTREAM=192.168.1.10:502 \
//!   cargo run --example modbus_rs_gateway_async_tcp_to_tcp \
//!     --features gateway,network-tcp,async
//! ```

use std::env;
use std::sync::Arc;

use tokio::sync::Mutex;

use mbus_gateway::{
    AsyncTcpGatewayServer, UnitIdRewriteRouter, UnitRouteTable,
};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let upstream_addr =
        env::var("MBUS_GATEWAY_UPSTREAM").unwrap_or_else(|_| "0.0.0.0:5502".into());
    let downstream_addr =
        env::var("MBUS_GATEWAY_DOWNSTREAM").unwrap_or_else(|_| "127.0.0.1:502".into());

    println!("Connecting to downstream {downstream_addr}");
    let ds_transport = TokioTcpTransport::connect(&downstream_addr).await?;
    let shared_downstream = Arc::new(Mutex::new(ds_transport));

    // Route units 1–10 to channel 0, rewriting the unit ID by +100.
    let mut inner_router: UnitRouteTable<16> = UnitRouteTable::new();
    for unit_id in 1u8..=10 {
        if let Ok(uid) = UnitIdOrSlaveAddr::new(unit_id) {
            inner_router.add(uid, 0).ok();
        }
    }
    let router = UnitIdRewriteRouter::new(inner_router, 100);

    println!("Starting async gateway on {upstream_addr}");
    AsyncTcpGatewayServer::serve(
        upstream_addr,
        router,
        vec![shared_downstream],
    )
    .await?;

    Ok(())
}
