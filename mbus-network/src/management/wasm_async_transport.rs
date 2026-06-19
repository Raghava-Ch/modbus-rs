#![cfg(target_arch = "wasm32")]

//! `WasmAsyncTransport` — browser WebSocket adapter implementing an async event-driven
//! model for use in WASM environments.
//!
//! Replaces the timer-polled sync model with futures channels and async/await.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_channel::oneshot;
use futures_util::future::{Either, select};
use futures_util::lock::Mutex as AsyncMutex;
use futures_util::{FutureExt, SinkExt, StreamExt};
use gloo_net::websocket::Message;
use gloo_net::websocket::futures::WebSocket;
use wasm_bindgen::prelude::*;

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{
    MAX_ADU_FRAME_LEN, MBAP_LENGTH_OFFSET_1B, MBAP_LENGTH_OFFSET_2B,
};
use mbus_core::errors::MbusError;
use std::vec::Vec as StdVec;

/// Browser WebSocket async transport for use within the modbus-rs WASM client.
pub struct WasmAsyncTransport {
    tx_out: UnboundedSender<Vec<u8>>,
    rx_in: Arc<AsyncMutex<UnboundedReceiver<Vec<u8>>>>,
    close_tx: Mutex<Option<oneshot::Sender<()>>>,
    rx_buf: Arc<AsyncMutex<StdVec<u8>>>,
    is_closed: Arc<AtomicBool>,
}

impl WasmAsyncTransport {
    /// Connect asynchronously to the WebSocket server at `url`.
    /// Resolves when the connection is open, or rejects if an error occurs during handshake.
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::open(url).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let (tx_out, mut rx_out) = unbounded::<Vec<u8>>();
        let (tx_in, rx_in) = unbounded::<Vec<u8>>();
        let (close_tx, rx_close) = oneshot::channel::<()>();
        let is_closed = Arc::new(AtomicBool::new(false));

        let is_closed_clone1 = is_closed.clone();
        let (mut ws_write, mut ws_read) = ws.split();

        // Task 1: Outgoing message loop
        wasm_bindgen_futures::spawn_local(async move {
            let mut rx_close = rx_close.fuse();
            loop {
                let next_bytes = rx_out.next();
                match select(next_bytes, rx_close).await {
                    Either::Left((bytes_opt, rx_close_next)) => {
                        rx_close = rx_close_next;
                        match bytes_opt {
                            Some(bytes) => {
                                if ws_write.send(Message::Bytes(bytes)).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    Either::Right((_, _)) => {
                        let _ = ws_write.close().await;
                        break;
                    }
                }
            }
            is_closed_clone1.store(true, Ordering::Relaxed);
        });

        // Task 2: Incoming message loop
        let is_closed_clone2 = is_closed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            while let Some(msg_res) = ws_read.next().await {
                match msg_res {
                    Ok(Message::Bytes(bytes)) => {
                        if tx_in.unbounded_send(bytes).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Text(txt)) => {
                        gloo_console::warn!(
                            "WasmAsyncTransport: unexpected text WebSocket message received and discarded. \
                             Expected binary Modbus ADU frames only. Message: ",
                            txt
                        );
                    }
                    Err(_) => break,
                }
            }
            is_closed_clone2.store(true, Ordering::Relaxed);
        });

        Ok(Self {
            tx_out,
            rx_in: Arc::new(AsyncMutex::new(rx_in)),
            close_tx: Mutex::new(Some(close_tx)),
            rx_buf: Arc::new(AsyncMutex::new(StdVec::with_capacity(
                2 * MAX_ADU_FRAME_LEN,
            ))),
            is_closed,
        })
    }

    /// Returns `true` if the transport has not yet been explicitly closed
    /// or lost its underlying connection.
    ///
    /// Note: this is an optimistic flag — it reflects the last known state,
    /// not a live probe of the WebSocket. It becomes `false` when `close()`
    /// is called, or when the background RX/TX tasks detect a socket error.
    pub fn is_open(&self) -> bool {
        !self.is_closed.load(Ordering::Relaxed)
    }

    /// Closes the underlying WebSocket.
    pub fn close(&self) {
        // Signal the TX loop to stop FIRST, so it has a chance to flush
        // any frames already queued in tx_out before we mark ourselves closed.
        if let Ok(mut guard) = self.close_tx.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
        // Only mark closed after the signal has been dispatched.
        self.is_closed.store(true, Ordering::Relaxed);
    }

    /// Sends a frame over the WebSocket.
    pub fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if !self.is_open() {
            return Err(MbusError::ConnectionClosed);
        }
        self.tx_out.unbounded_send(adu.to_vec()).map_err(|_| {
            self.is_closed.store(true, Ordering::Relaxed);
            MbusError::ConnectionClosed
        })
    }

    /// Receives a single frame, awaiting asynchronously.
    pub async fn recv_frame(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        // MBAP header is always 6 bytes.
        const MBAP_PREFIX_LEN: usize = 6;

        if !self.is_open() {
            return Err(MbusError::ConnectionClosed);
        }

        let mut rx_in_guard = self.rx_in.lock().await;
        let mut rx_buf_guard = self.rx_buf.lock().await;

        loop {
            // ── Step 1: Try to extract a complete ADU from rx_buf ──────────────
            if rx_buf_guard.len() >= MBAP_PREFIX_LEN {
                // Parse the "length" field at bytes 4–5 (big-endian u16).
                // This tells us how many bytes follow the 6-byte prefix.
                let remaining_len = u16::from_be_bytes([
                    rx_buf_guard[MBAP_LENGTH_OFFSET_1B],
                    rx_buf_guard[MBAP_LENGTH_OFFSET_2B],
                ]) as usize;

                if remaining_len == 0 {
                    // A length of 0 is invalid per the Modbus TCP spec.
                    rx_buf_guard.clear();
                    return Err(MbusError::InvalidDataLen);
                }

                let total_len = MBAP_PREFIX_LEN + remaining_len;

                if total_len > MAX_ADU_FRAME_LEN {
                    // Frame claims to be larger than the maximum ADU size.
                    rx_buf_guard.clear();
                    return Err(MbusError::BufferTooSmall);
                }

                if rx_buf_guard.len() >= total_len {
                    // A complete frame is available. Copy it out.
                    let mut hvec = HVec::new();
                    // extend_from_slice fails only if total_len > MAX_ADU_FRAME_LEN,
                    // which we already checked above, so .unwrap() is safe here.
                    hvec.extend_from_slice(&rx_buf_guard[..total_len]).unwrap();

                    // Remove the consumed bytes from the front of rx_buf.
                    let leftover = rx_buf_guard.len() - total_len;
                    if leftover > 0 {
                        rx_buf_guard.copy_within(total_len.., 0);
                    }
                    rx_buf_guard.truncate(leftover);

                    return Ok(hvec);
                }
                // Not enough bytes yet — fall through to read more.
            }

            // ── Step 2: Read the next WebSocket message and append to rx_buf ───
            match rx_in_guard.next().await {
                Some(bytes) => {
                    // Overflow guard: if the buffer would exceed 2× max ADU size,
                    // something is very wrong (corrupted stream). Clear and error.
                    if rx_buf_guard.len() + bytes.len() > 2 * MAX_ADU_FRAME_LEN {
                        rx_buf_guard.clear();
                        return Err(MbusError::BufferTooSmall);
                    }
                    rx_buf_guard.extend_from_slice(&bytes);
                    // Loop back to Step 1 to check if we now have a complete frame.
                }
                None => {
                    // Channel closed — the RX background task has exited.
                    self.is_closed.store(true, Ordering::Relaxed);
                    return Err(MbusError::ConnectionClosed);
                }
            }
        }
    }
}

impl mbus_core::transport::AsyncTransport for WasmAsyncTransport {
    const SUPPORTS_BROADCAST_WRITES: bool = false;
    const TRANSPORT_TYPE: mbus_core::transport::TransportType =
        mbus_core::transport::TransportType::CustomTcp;

    fn send<'a>(
        &'a mut self,
        adu: &'a [u8],
    ) -> impl std::future::Future<Output = Result<(), MbusError>> + Send + 'a {
        async move { self.send_frame(adu) }
    }

    fn recv(
        &mut self,
    ) -> impl std::future::Future<Output = Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError>> + Send + '_
    {
        async move { self.recv_frame().await }
    }

    fn is_connected(&self) -> bool {
        self.is_open()
    }
}
