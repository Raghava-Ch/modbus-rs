//! `WsUpstreamTransport` — server-side WebSocket upstream transport.
//!
//! Wraps an accepted [`WebSocketStream<TcpStream>`] from `tokio-tungstenite` and
//! implements [`AsyncTransport`] so that `AsyncWsGatewayServer` can feed it
//! directly into the generic `run_async_session` loop shared with
//! [`AsyncTcpGatewayServer`].
//!
//! ## Framing contract
//!
//! The browser-side [`WasmModbusClient`] (mbus-ffi) already constructs complete
//! Modbus TCP ADUs (MBAP header + PDU) and ships each one as a single binary
//! WebSocket message.  `recv()` therefore just unwraps the binary payload — no
//! reassembly is needed.  Ping/Pong/Text frames are silently skipped.
//!
//! [`WasmModbusClient`]: https://docs.rs/mbus-ffi

use futures_util::{SinkExt, StreamExt};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{AsyncTransport, TransportType};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

/// Async gateway upstream transport that communicates via WebSocket.
///
/// Created by `AsyncWsGatewayServer` after a successful WebSocket handshake;
/// callers rarely construct this type directly.
///
/// Each binary WebSocket message is expected to carry exactly one complete
/// Modbus TCP ADU — the same wire format produced by `WasmModbusClient` in
/// the browser.  Non-binary messages (Ping, Pong, Text) are silently skipped.
pub struct WsUpstreamTransport {
    ws: WebSocketStream<TcpStream>,
    connected: bool,
}

impl WsUpstreamTransport {
    /// Wrap an already-accepted WebSocket stream.
    pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
        Self { ws, connected: true }
    }
}

impl AsyncTransport for WsUpstreamTransport {
    const SUPPORTS_BROADCAST_WRITES: bool = false;
    /// WebSocket carries Modbus TCP ADUs (MBAP framing), same as raw TCP.
    const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if !self.connected {
            return Err(MbusError::ConnectionClosed);
        }
        // Copy the ADU bytes into a binary WebSocket message and flush.
        let bytes = bytes::Bytes::copy_from_slice(adu);
        self.ws
            .send(Message::Binary(bytes))
            .await
            .map_err(|_| {
                self.connected = false;
                MbusError::IoError
            })
    }

    async fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        if !self.connected {
            return Err(MbusError::ConnectionClosed);
        }
        loop {
            match self.ws.next().await {
                // A complete ADU arrives as a binary message.
                Some(Ok(Message::Binary(data))) => {
                    if data.len() > MAX_ADU_FRAME_LEN {
                        return Err(MbusError::BufferTooSmall);
                    }
                    let mut frame: HVec<u8, MAX_ADU_FRAME_LEN> = HVec::new();
                    frame
                        .extend_from_slice(&data)
                        .map_err(|_| MbusError::BufferTooSmall)?;
                    return Ok(frame);
                }
                // Ignore control and text frames — they are not Modbus ADUs.
                Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_)))
                | Some(Ok(Message::Text(_)))
                | Some(Ok(Message::Frame(_))) => continue,
                // Graceful close or stream exhausted.
                Some(Ok(Message::Close(_))) | None => {
                    self.connected = false;
                    return Err(MbusError::ConnectionClosed);
                }
                // Transport-level error.
                Some(Err(_)) => {
                    self.connected = false;
                    return Err(MbusError::ConnectionLost);
                }
            }
        }
    }
}
