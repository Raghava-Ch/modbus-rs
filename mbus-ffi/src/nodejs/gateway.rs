//! Node.js bindings for the async Modbus TCP gateway.

use std::sync::Mutex;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{AsyncTcpGatewayServer, UnitRouteTable};
use mbus_network::TokioTcpTransport;
use tokio::sync::{Mutex as TokioMutex, Notify};
use tokio::task::JoinHandle;

use crate::nodejs::errors::{to_napi_err, ERR_MODBUS_INTERNAL, ERR_MODBUS_INVALID_ARGUMENT};
use crate::nodejs::runtime;

// ── Option structs ───────────────────────────────────────────────────────────

/// Gateway bind options.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct GatewayBindOptions {
    /// Bind host address (e.g., "0.0.0.0").
    pub host: String,
    /// Bind port.
    pub port: u16,
}

/// Downstream server configuration.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DownstreamConfig {
    /// Downstream server host.
    pub host: String,
    /// Downstream server port.
    pub port: u16,
}

/// Route entry mapping unit ID to a downstream channel.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct RouteEntry {
    /// Modbus unit ID (1-247).
    pub unit_id: u8,
    /// Index into the downstreams array.
    pub channel: u32,
}

/// Gateway configuration.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct GatewayConfig {
    /// List of downstream servers.
    pub downstreams: Vec<DownstreamConfig>,
    /// Routing table mapping unit IDs to downstream channels.
    pub routes: Vec<RouteEntry>,
}

// ── AsyncTcpGateway ──────────────────────────────────────────────────────────

/// Async Modbus TCP gateway.
///
/// Forwards incoming Modbus requests to downstream servers based on unit ID routing.
#[napi]
pub struct AsyncTcpGateway {
    stop_signal: std::sync::Arc<Notify>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

#[napi]
impl AsyncTcpGateway {
    /// Creates and starts a new TCP gateway.
    ///
    /// @param opts - Gateway bind options.
    /// @param config - Gateway configuration including downstreams and routes.
    /// @returns A running gateway instance.
    #[napi(factory)]
    pub async fn bind(opts: GatewayBindOptions, config: GatewayConfig) -> Result<AsyncTcpGateway> {
        let bind_addr = format!("{}:{}", opts.host, opts.port);
        let stop_signal = std::sync::Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the route table
        let mut route_table: UnitRouteTable<16> = UnitRouteTable::new();
        for entry in &config.routes {
            let unit = UnitIdOrSlaveAddr::new(entry.unit_id)
                .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;
            route_table
                .add(unit, entry.channel as usize)
                .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;
        }

        // Connect to all downstream servers
        let mut downstream_transports = Vec::with_capacity(config.downstreams.len());
        for ds in &config.downstreams {
            let addr = format!("{}:{}", ds.host, ds.port);
            let transport = TokioTcpTransport::connect(&addr)
                .await
                .map_err(|e| to_napi_err(ERR_MODBUS_INTERNAL, e))?;
            downstream_transports.push(std::sync::Arc::new(TokioMutex::new(transport)));
        }

        // Spawn the gateway task
        let rt = runtime::get();
        let join_handle = rt.spawn(async move {
            let _ = AsyncTcpGatewayServer::serve_with_shutdown(
                &bind_addr,
                route_table,
                downstream_transports,
                stop_signal_clone.notified(),
            )
            .await;
        });

        Ok(AsyncTcpGateway {
            stop_signal,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    /// Stops the gateway.
    #[napi]
    pub async fn shutdown(&self) -> Result<()> {
        self.stop_signal.notify_one();

        let handle = {
            let mut guard = self.join_handle.lock().map_err(|_| {
                napi::Error::new(Status::GenericFailure, "Failed to acquire lock")
            })?;
            guard.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }

        Ok(())
    }
}
