//! Web Serial server binding (phase 1).
//!
//! This type wires lifecycle and JS callback bridging. Serial frame I/O loop
//! wiring is intentionally deferred to the next phase.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use js_sys::Function;
use wasm_bindgen::prelude::*;

use super::adapters::SerialServerAdapter;
use super::bridge::JsServerHandler;
use super::binding_types::{WasmSerialServerConfig, WasmServerTransportKind};

#[wasm_bindgen]
/// Browser-facing Modbus server endpoint for Web Serial traffic.
pub struct WasmSerialServer {
    adapter: Rc<RefCell<SerialServerAdapter>>,
    running: Rc<Cell<bool>>,
    bridge: JsServerHandler,
    dispatched_requests: Cell<u32>,
    sent_frames: Cell<u32>,
    received_frames: Cell<u32>,
    last_error: RefCell<Option<String>>,
}

impl WasmSerialServer {
    fn capture_error(&self, e: JsValue) -> JsValue {
        let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
        *self.last_error.borrow_mut() = Some(msg);
        e
    }
}

#[wasm_bindgen]
impl WasmSerialServer {
    /// Create a serial server with a JS request handler callback.
    ///
    /// `on_request` receives one request object and may return a direct value or Promise.
    #[wasm_bindgen(constructor)]
    pub fn new(config: WasmSerialServerConfig, on_request: Function) -> Result<Self, JsValue> {
        let adapter = SerialServerAdapter::new(&config)?;
        Ok(Self {
            adapter: Rc::new(RefCell::new(adapter)),
            running: Rc::new(Cell::new(false)),
            bridge: JsServerHandler::new(on_request),
            dispatched_requests: Cell::new(0),
            sent_frames: Cell::new(0),
            received_frames: Cell::new(0),
            last_error: RefCell::new(None),
        })
    }

    /// Attach browser SerialPort object delegated to mbus-serial transport.
    pub fn attach_serial_port(&self, port: JsValue) {
        self.adapter.borrow_mut().attach_port(port);
    }

    /// Start server lifecycle.
    pub fn start(&self) -> Result<(), JsValue> {
        self.adapter
            .borrow_mut()
            .connect()
            .map_err(|e| self.capture_error(e))?;
        self.running.set(true);
        Ok(())
    }

    /// Stop server lifecycle.
    pub fn stop(&self) -> Result<(), JsValue> {
        self.adapter
            .borrow_mut()
            .disconnect()
            .map_err(|e| self.capture_error(e))?;
        self.running.set(false);
        Ok(())
    }

    /// Whether server lifecycle is currently active.
    pub fn is_running(&self) -> bool {
        self.running.get()
    }

    /// Whether delegated serial transport currently reports connected.
    pub fn transport_connected(&self) -> bool {
        self.adapter.borrow().is_connected()
    }

    /// Selected serial mode as numeric enum.
    pub fn mode(&self) -> WasmServerTransportKind {
        self.adapter.borrow().kind()
    }

    /// Send one encoded frame through delegated serial transport.
    pub fn send_frame(&self, frame: &[u8]) -> Result<(), JsValue> {
        self.adapter
            .borrow_mut()
            .send_frame(frame)
            .map_err(|e| self.capture_error(e))?;
        self.sent_frames.set(self.sent_frames.get() + 1);
        Ok(())
    }

    /// Try receiving one frame from delegated serial transport.
    pub fn recv_frame(&self) -> Result<Vec<u8>, JsValue> {
        let frame = self
            .adapter
            .borrow_mut()
            .recv_frame()
            .map_err(|e| self.capture_error(e))?;

        match frame {
            Some(frame) => {
                self.received_frames.set(self.received_frames.get() + 1);
                Ok(frame.as_slice().to_vec())
            }
            None => Ok(Vec::new()),
        }
    }

    /// Dispatch a request object into JS app handler.
    pub async fn dispatch_request(&self, request: JsValue) -> Result<JsValue, JsValue> {
        if !self.is_running() {
            return Err(self.capture_error(JsValue::from_str("server is not running")));
        }
        let out = self
            .bridge
            .dispatch(request)
            .await
            .map_err(|e| self.capture_error(e))?;
        self.dispatched_requests
            .set(self.dispatched_requests.get() + 1);
        Ok(out)
    }

    /// Returns a point-in-time status snapshot for diagnostics/observability.
    pub fn status_snapshot(&self) -> super::binding_types::WasmServerStatusSnapshot {
        super::binding_types::WasmServerStatusSnapshot::new(
            self.mode(),
            self.is_running(),
            self.transport_connected(),
            self.dispatched_requests.get(),
            self.sent_frames.get(),
            self.received_frames.get(),
            self.last_error.borrow().is_some(),
        )
    }

    /// Returns the last captured binding-layer error message, if any.
    pub fn last_error_message(&self) -> Option<String> {
        self.last_error.borrow().clone()
    }

    /// Clears the stored last-error message.
    pub fn clear_last_error(&self) {
        self.last_error.borrow_mut().take();
    }
}
