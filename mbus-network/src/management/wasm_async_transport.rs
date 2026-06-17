//! `WasmAsyncTransport` — browser WebSocket adapter implementing an async event-driven
//! model for use in WASM environments.
//!
//! Replaces the timer-polled sync model with futures channels and async/await.

use std::cell::RefCell;
use std::rc::Rc;

use futures_channel::mpsc::{unbounded, UnboundedReceiver};
use futures_channel::oneshot;
use futures_util::stream::StreamExt;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, ErrorEvent, MessageEvent, WebSocket};

use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;

/// Browser WebSocket async transport for use within the modbus-rs WASM client.
pub struct WasmAsyncTransport {
    ws: WebSocket,
    rx_rx: UnboundedReceiver<Vec<u8>>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(web_sys::CloseEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
}

impl WasmAsyncTransport {
    /// Connect asynchronously to the WebSocket server at `url`.
    /// Resolves when the connection is open, or rejects if an error occurs during handshake.
    pub async fn connect(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let (tx_open, rx_open) = oneshot::channel::<Result<(), JsValue>>();
        let (rx_tx, rx_rx) = unbounded::<Vec<u8>>();

        let tx_open_cell = Rc::new(RefCell::new(Some(tx_open)));

        // Setup on_open callback
        let tx_open_clone = tx_open_cell.clone();
        let on_open = Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_evt| {
            if let Some(tx) = tx_open_clone.borrow_mut().take() {
                let _ = tx.send(Ok(()));
            }
        }));
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        // Setup on_error callback
        let tx_open_clone2 = tx_open_cell.clone();
        let on_error = Closure::<dyn FnMut(ErrorEvent)>::wrap(Box::new(move |evt: ErrorEvent| {
            if let Some(tx) = tx_open_clone2.borrow_mut().take() {
                let _ = tx.send(Err(evt.into()));
            }
        }));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        // Setup on_message callback
        let rx_tx_clone = rx_tx.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |evt: MessageEvent| {
            if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                let array = js_sys::Uint8Array::new(&buf);
                let bytes = array.to_vec();
                let _ = rx_tx_clone.unbounded_send(bytes);
            }
        }));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        // Setup on_close callback
        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::wrap(Box::new(move |_evt| {
            // WebSocket has closed. Channel receivers will yield None.
        }));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        // Await connection opening
        match rx_open.await {
            Ok(Ok(())) => {
                Ok(Self {
                    ws,
                    rx_rx,
                    _on_message: on_message,
                    _on_close: on_close,
                    _on_error: on_error,
                })
            }
            Ok(Err(err)) => Err(err),
            Err(_) => Err(JsValue::from_str("WebSocket connection channel dropped before open")),
        }
    }

    /// Returns `true` only when the underlying websocket is fully OPEN.
    pub fn is_open(&self) -> bool {
        self.ws.ready_state() == WebSocket::OPEN
    }

    /// Closes the underlying WebSocket.
    pub fn close(&self) {
        let _ = self.ws.close();
    }

    /// Sends a frame over the WebSocket.
    pub fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if !self.is_open() {
            return Err(MbusError::ConnectionClosed);
        }
        self.ws.send_with_u8_array(adu).map_err(|_| MbusError::IoError)
    }

    /// Receives a single frame, awaiting asynchronously.
    pub async fn recv_frame(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        match self.rx_rx.next().await {
            Some(bytes) => {
                if bytes.len() > MAX_ADU_FRAME_LEN {
                    return Err(MbusError::BufferTooSmall);
                }
                let mut hvec = HVec::new();
                if hvec.extend_from_slice(&bytes).is_err() {
                    return Err(MbusError::BufferTooSmall);
                }
                Ok(hvec)
            }
            None => Err(MbusError::ConnectionClosed),
        }
    }
}
