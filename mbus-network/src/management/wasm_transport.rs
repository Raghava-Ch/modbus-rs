//! `WasmWsTransport` — browser WebSocket adapter implementing
//! `mbus_core::transport::Transport` for use in WASM environments.
//!
//! Design rules:
//! - `connect()` registers WS event listeners; returns immediately (WS handshake is async).
//! - `recv()`    is non-blocking; drains the shared `rx_buf` populated by `onmessage`.
//!              Returns `Err(TransportError::Timeout)` when no bytes are ready — this is
//!              exactly what `ClientServices::poll()` expects.
//! - `send()`    calls `ws.send_with_u8_array()` synchronously.
//! - Single-threaded WASM: `Rc<RefCell<...>>` is safe and avoids `Mutex` overhead.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::{ModbusConfig, Transport, TransportError, TransportType};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{BinaryType, ErrorEvent, MessageEvent, WebSocket};

use heapless::Vec as HVec;

struct WsShared {
    rx_buf: VecDeque<u8>,
    connected: bool,
}

/// Browser WebSocket transport for use within the modbus-rs WASM client.
pub struct WasmWsTransport {
    url: String,
    ws: Option<WebSocket>,
    shared: Rc<RefCell<WsShared>>,
    _on_message: Option<Closure<dyn FnMut(MessageEvent)>>,
    _on_open: Option<Closure<dyn FnMut(web_sys::Event)>>,
    _on_close: Option<Closure<dyn FnMut(web_sys::CloseEvent)>>,
    _on_error: Option<Closure<dyn FnMut(ErrorEvent)>>,
}

impl WasmWsTransport {
    /// Create a new transport that will connect to `url` on the first `connect()` call.
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            ws: None,
            shared: Rc::new(RefCell::new(WsShared {
                rx_buf: VecDeque::new(),
                connected: false,
            })),
            _on_message: None,
            _on_open: None,
            _on_close: None,
            _on_error: None,
        }
    }
}

impl Transport for WasmWsTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: Option<TransportType> = Some(TransportType::CustomTcp);

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        let ws = WebSocket::new(&self.url).map_err(|_| TransportError::ConnectionFailed)?;
        ws.set_binary_type(BinaryType::Arraybuffer);

        let shared_msg = self.shared.clone();
        let on_message =
            Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |evt: MessageEvent| {
                if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buf);
                    let bytes = array.to_vec();
                    shared_msg.borrow_mut().rx_buf.extend(bytes);
                }
            }));
        ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

        let shared_open = self.shared.clone();
        let on_open = Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_evt| {
            shared_open.borrow_mut().connected = true;
        }));
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

        let shared_close = self.shared.clone();
        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::wrap(Box::new(move |_evt| {
            shared_close.borrow_mut().connected = false;
        }));
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

        let shared_err = self.shared.clone();
        let on_error = Closure::<dyn FnMut(ErrorEvent)>::wrap(Box::new(move |_evt| {
            shared_err.borrow_mut().connected = false;
        }));
        ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

        self._on_message = Some(on_message);
        self._on_open = Some(on_open);
        self._on_close = Some(on_close);
        self._on_error = Some(on_error);
        self.ws = Some(ws);
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        if let Some(ws) = self.ws.take() {
            let _ = ws.close();
        }
        self.shared.borrow_mut().connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let ws = self.ws.as_ref().ok_or(TransportError::ConnectionFailed)?;
        ws.send_with_u8_array(adu)
            .map_err(|_| TransportError::IoError)
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        let mut shared = self.shared.borrow_mut();
        if shared.rx_buf.is_empty() {
            return Err(TransportError::Timeout);
        }
        let drain_len = shared.rx_buf.len().min(MAX_ADU_FRAME_LEN);
        let mut out: HVec<u8, MAX_ADU_FRAME_LEN> = HVec::new();
        for byte in shared.rx_buf.drain(..drain_len) {
            let _ = out.push(byte);
        }
        Ok(out)
    }

    fn is_connected(&self) -> bool {
        match &self.ws {
            None => false,
            Some(ws) => {
                let state = ws.ready_state();
                // Treat CONNECTING (0) as connected so the tick loop does not
                // falsely interpret the pre-handshake window as a disconnect.
                state == WebSocket::CONNECTING || state == WebSocket::OPEN
            }
        }
    }

    fn transport_type(&self) -> TransportType {
        TransportType::CustomTcp
    }
}
