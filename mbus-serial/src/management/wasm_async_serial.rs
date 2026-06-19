#![cfg(target_arch = "wasm32")]

//! `WasmAsyncSerialTransport` — browser Web Serial adapter implementing an async event-driven
//! model for use in WASM environments.
//!
//! Replaces the timer-polled sync model with futures channels and async/await, providing
//! thread-safety without any unsafe code.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_channel::oneshot;
use futures_util::future::{Either, select};
use futures_util::lock::Mutex as AsyncMutex;
use futures_util::{FutureExt, StreamExt};
use gloo_timers::future::TimeoutFuture;
use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    AsyncTransport, BaudRate, DataBits, ModbusConfig, Parity, SerialMode, TransportType,
};
use std::vec::Vec as StdVec;

/// Browser Web Serial async transport.
///
/// The const generic `ASCII` selects the framing mode at compile time:
/// - `false` → Modbus RTU (binary + CRC)
/// - `true`  → Modbus ASCII (`:` delimited + LRC)
///
/// Prefer the type aliases [`WasmAsyncRtuTransport`] and [`WasmAsyncAsciiTransport`].
pub struct WasmAsyncSerialTransport<const ASCII: bool = false> {
    /// JS SerialPort object.
    port: Option<JsValue>,
    /// Outbound frame queue: send to TX task.
    tx_sender: Option<UnboundedSender<StdVec<u8>>>,
    /// Inbound byte stream: receive from RX task.
    rx_receiver: Arc<AsyncMutex<Option<UnboundedReceiver<StdVec<u8>>>>>,
    /// Signal to close background tasks.
    close_tx: Arc<Mutex<Option<oneshot::Sender<()>>>>,
    /// Baud-rate-derived inter-frame silence in milliseconds (RTU mode).
    inter_frame_ms: u32,
    /// Shared connected/closing flag.
    is_connected: Arc<AtomicBool>,
}

/// Modbus RTU browser async serial transport.
pub type WasmAsyncRtuTransport = WasmAsyncSerialTransport<false>;
/// Modbus ASCII browser async serial transport.
pub type WasmAsyncAsciiTransport = WasmAsyncSerialTransport<true>;

impl<const ASCII: bool> WasmAsyncSerialTransport<ASCII> {
    /// The serial mode determined by the `ASCII` const generic.
    const MODE: SerialMode = if ASCII {
        SerialMode::Ascii
    } else {
        SerialMode::Rtu
    };

    /// Creates a new browser async serial transport.
    pub fn new() -> Self {
        Self {
            port: None,
            tx_sender: None,
            rx_receiver: Arc::new(AsyncMutex::new(None)),
            close_tx: Arc::new(Mutex::new(None)),
            inter_frame_ms: 35,
            is_connected: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attaches a browser `SerialPort` object obtained from JS.
    pub fn attach_port(&mut self, port: JsValue) {
        self.port = Some(port);
        self.is_connected.store(false, Ordering::Relaxed);
    }

    /// Returns true when a browser `SerialPort` handle has been attached.
    pub fn has_port(&self) -> bool {
        self.port.is_some()
    }

    fn get_function(target: &JsValue, name: &str) -> Result<Function, MbusError> {
        Reflect::get(target, &JsValue::from_str(name))
            .map_err(|_| MbusError::Unexpected)?
            .dyn_into::<Function>()
            .map_err(|_| MbusError::Unexpected)
    }

    fn call_method0(target: &JsValue, name: &str) -> Result<JsValue, MbusError> {
        Self::get_function(target, name)?
            .call0(target)
            .map_err(|_| MbusError::Unexpected)
    }

    fn call_method1(target: &JsValue, name: &str, arg1: &JsValue) -> Result<JsValue, MbusError> {
        Self::get_function(target, name)?
            .call1(target, arg1)
            .map_err(|_| MbusError::Unexpected)
    }

    fn promise_from(value: JsValue) -> Result<Promise, MbusError> {
        value
            .dyn_into::<Promise>()
            .map_err(|_| MbusError::Unexpected)
    }

    fn baud_rate_value(baud_rate: BaudRate) -> f64 {
        match baud_rate {
            BaudRate::Baud9600 => 9600.0,
            BaudRate::Baud19200 => 19200.0,
            BaudRate::Custom(rate) => rate as f64,
        }
    }

    fn parity_value(parity: Parity) -> &'static str {
        match parity {
            Parity::None => "none",
            Parity::Even => "even",
            Parity::Odd => "odd",
        }
    }

    fn data_bits_value(data_bits: DataBits) -> f64 {
        match data_bits {
            DataBits::Five => 5.0,
            DataBits::Six => 6.0,
            DataBits::Seven => 7.0,
            DataBits::Eight => 8.0,
        }
    }

    fn serial_open_options(config: &mbus_core::transport::ModbusSerialConfig) -> JsValue {
        let options = Object::new();
        let _ = Reflect::set(
            &options,
            &JsValue::from_str("baudRate"),
            &JsValue::from_f64(Self::baud_rate_value(config.baud_rate)),
        );
        let _ = Reflect::set(
            &options,
            &JsValue::from_str("dataBits"),
            &JsValue::from_f64(Self::data_bits_value(config.data_bits)),
        );
        let _ = Reflect::set(
            &options,
            &JsValue::from_str("stopBits"),
            &JsValue::from_f64(config.stop_bits as f64),
        );
        let _ = Reflect::set(
            &options,
            &JsValue::from_str("parity"),
            &JsValue::from_str(Self::parity_value(config.parity)),
        );
        options.into()
    }

    /// Connects to the attached serial port asynchronously.
    pub async fn connect(&mut self, config: &ModbusConfig) -> Result<(), MbusError> {
        let serial_cfg = match config {
            ModbusConfig::Serial(c) => c,
            _ => return Err(MbusError::InvalidConfiguration),
        };

        if serial_cfg.mode != Self::MODE {
            return Err(MbusError::InvalidConfiguration);
        }

        let port = self.port.clone().ok_or(MbusError::ConnectionFailed)?;

        if self.is_connected.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Compute baud-rate-based inter-frame timeout
        let baud = match serial_cfg.baud_rate {
            BaudRate::Baud9600 => 9600,
            BaudRate::Baud19200 => 19200,
            BaudRate::Custom(rate) => rate,
        }
        .max(1) as u64;
        let char_time_us = (11 * 1_000_000) / baud;
        let silence_us = ((char_time_us * 7) / 2).max(1750).max(100_000);
        self.inter_frame_ms = (silence_us / 1000) as u32;

        let (tx_out, mut rx_out) = unbounded::<StdVec<u8>>();
        let (tx_in, rx_in) = unbounded::<StdVec<u8>>();
        let (close_tx, rx_close) = oneshot::channel::<()>();

        let options = Self::serial_open_options(serial_cfg);
        let open_result =
            Self::call_method1(&port, "open", &options).map_err(|_| MbusError::ConnectionFailed)?;
        let open_promise =
            Self::promise_from(open_result).map_err(|_| MbusError::ConnectionFailed)?;

        JsFuture::from(open_promise)
            .await
            .map_err(|_| MbusError::ConnectionFailed)?;

        self.is_connected.store(true, Ordering::Relaxed);
        self.tx_sender = Some(tx_out);
        *self.rx_receiver.lock().await = Some(rx_in);
        *self.close_tx.lock().unwrap() = Some(close_tx);

        let is_connected_clone1 = self.is_connected.clone();
        let port_clone1 = port.clone();

        // Spawn TX loop
        spawn_local(async move {
            let mut rx_close = rx_close.fuse();
            loop {
                let next_frame = rx_out.next();
                match select(next_frame, rx_close).await {
                    Either::Left((frame_opt, rx_close_next)) => {
                        rx_close = rx_close_next;
                        match frame_opt {
                            Some(frame) => {
                                let writable = match Reflect::get(
                                    &port_clone1,
                                    &JsValue::from_str("writable"),
                                ) {
                                    Ok(value) if !value.is_null() && !value.is_undefined() => value,
                                    _ => break,
                                };
                                let writer = match Self::call_method0(&writable, "getWriter") {
                                    Ok(w) => w,
                                    Err(_) => break,
                                };
                                let data = Uint8Array::from(frame.as_slice());
                                let write_promise =
                                    match Self::call_method1(&writer, "write", &data.into())
                                        .and_then(Self::promise_from)
                                    {
                                        Ok(p) => p,
                                        Err(_) => {
                                            let _ = Self::call_method0(&writer, "releaseLock");
                                            break;
                                        }
                                    };
                                let write_result = JsFuture::from(write_promise).await;
                                let _ = Self::call_method0(&writer, "releaseLock");
                                if write_result.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    Either::Right((_, _)) => {
                        break;
                    }
                }
            }
            is_connected_clone1.store(false, Ordering::Relaxed);
        });

        // Spawn RX loop
        let is_connected_clone2 = self.is_connected.clone();
        let port_clone2 = port;
        spawn_local(async move {
            loop {
                if !is_connected_clone2.load(Ordering::Relaxed) {
                    break;
                }
                let readable = match Reflect::get(&port_clone2, &JsValue::from_str("readable")) {
                    Ok(value) if !value.is_null() && !value.is_undefined() => value,
                    _ => break,
                };
                let reader = match Self::call_method0(&readable, "getReader") {
                    Ok(r) => r,
                    Err(_) => break,
                };
                loop {
                    if !is_connected_clone2.load(Ordering::Relaxed) {
                        break;
                    }
                    let read_promise =
                        match Self::call_method0(&reader, "read").and_then(Self::promise_from) {
                            Ok(p) => p,
                            Err(_) => break,
                        };
                    let read_result = match JsFuture::from(read_promise).await {
                        Ok(res) => res,
                        Err(_) => break,
                    };
                    let done = Reflect::get(&read_result, &JsValue::from_str("done"))
                        .ok()
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if done {
                        break;
                    }
                    let value = match Reflect::get(&read_result, &JsValue::from_str("value")) {
                        Ok(v) if !v.is_null() && !v.is_undefined() => v,
                        _ => continue,
                    };
                    let bytes = Uint8Array::new(&value).to_vec();
                    if tx_in.unbounded_send(bytes).is_err() {
                        break;
                    }
                }
                let _ = Self::call_method0(&reader, "releaseLock");
                // Wait briefly before trying to acquire lock again
                TimeoutFuture::new(5).await;
            }
            is_connected_clone2.store(false, Ordering::Relaxed);
        });

        Ok(())
    }

    /// Disconnects from the serial port asynchronously, awaiting the browser's close promise.
    pub async fn disconnect(&mut self) -> Result<(), MbusError> {
        if let Some(close_tx) = self.close_tx.lock().unwrap().take() {
            let _ = close_tx.send(());
        }
        self.is_connected.store(false, Ordering::Relaxed);

        if let Some(port) = self.port.take() {
            if let Ok(close_result) = Self::call_method0(&port, "close") {
                if let Ok(close_promise) = Self::promise_from(close_result) {
                    let _ = JsFuture::from(close_promise).await;
                }
            }
        }
        Ok(())
    }

    /// Receives a frame asynchronously using RTU or ASCII framing.
    pub async fn recv_frame(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        if ASCII {
            self.recv_ascii().await
        } else {
            self.recv_rtu().await
        }
    }

    async fn recv_rtu(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let mut rx_guard = self.rx_receiver.lock().await;
        let rx = rx_guard.as_mut().ok_or(MbusError::ConnectionClosed)?;

        let mut buf: HVec<u8, MAX_ADU_FRAME_LEN> = HVec::new();

        // Wait for the first chunk (indefinite block)
        let chunk = match rx.next().await {
            Some(c) => c,
            None => return Err(MbusError::ConnectionClosed),
        };
        if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
            return Err(MbusError::BufferTooSmall);
        }
        buf.extend_from_slice(&chunk)
            .map_err(|_| MbusError::BufferTooSmall)?;

        if let Some(expected) =
            mbus_core::data_unit::common::derive_length_from_bytes(&buf, Self::TRANSPORT_TYPE)
        {
            if buf.len() >= expected {
                return Ok(buf);
            }
        }

        let inter_frame_ms = self.inter_frame_ms;
        loop {
            let mut timeout_fut = SendTimeoutFuture {
                inner: TimeoutFuture::new(inter_frame_ms),
            }
            .fuse();
            let mut next_fut = rx.next().fuse();

            futures_util::select! {
                res = next_fut => {
                    match res {
                        Some(chunk) => {
                            if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
                                return Err(MbusError::BufferTooSmall);
                            }
                            buf.extend_from_slice(&chunk).map_err(|_| MbusError::BufferTooSmall)?;
                            if let Some(expected) = mbus_core::data_unit::common::derive_length_from_bytes(&buf, Self::TRANSPORT_TYPE) {
                                if buf.len() >= expected {
                                    return Ok(buf);
                                }
                            }
                        }
                        None => return Err(MbusError::ConnectionClosed),
                    }
                }
                _ = timeout_fut => {
                    return Ok(buf);
                }
            }
        }
    }

    async fn recv_ascii(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let mut rx_guard = self.rx_receiver.lock().await;
        let rx = rx_guard.as_mut().ok_or(MbusError::ConnectionClosed)?;

        let mut buf: HVec<u8, MAX_ADU_FRAME_LEN> = HVec::new();
        loop {
            let chunk = match rx.next().await {
                Some(c) => c,
                None => return Err(MbusError::ConnectionClosed),
            };
            if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
                return Err(MbusError::BufferTooSmall);
            }
            buf.extend_from_slice(&chunk)
                .map_err(|_| MbusError::BufferTooSmall)?;
            let len = buf.len();
            if len >= 2 && buf[len - 2] == b'\r' && buf[len - 1] == b'\n' {
                return Ok(buf);
            }
        }
    }
}

impl<const ASCII: bool> AsyncTransport for WasmAsyncSerialTransport<ASCII> {
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(Self::MODE);

    fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }

    fn send<'a>(
        &'a mut self,
        adu: &'a [u8],
    ) -> impl std::future::Future<Output = Result<(), MbusError>> + Send + 'a {
        async move {
            if !self.is_connected() {
                return Err(MbusError::ConnectionClosed);
            }
            if let Some(tx) = &self.tx_sender {
                tx.unbounded_send(adu.to_vec()).map_err(|_| {
                    self.is_connected.store(false, Ordering::Relaxed);
                    MbusError::ConnectionClosed
                })
            } else {
                Err(MbusError::ConnectionClosed)
            }
        }
    }

    fn recv(
        &mut self,
    ) -> impl std::future::Future<Output = Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError>> + Send + '_
    {
        async move { self.recv_frame().await }
    }
}

/// A workaround wrapper to make `TimeoutFuture` `Send`.
///
/// SAFETY: In single-threaded WASM environments, JS timers are thread-bound but only executed
/// on a single thread. This wrapper is needed to satisfy the `Send` bound on `AsyncTransport::recv`
/// futures.
/// TODO(wasm-threads): Remove when native thread-safe timers exist for WASM.
struct SendTimeoutFuture {
    inner: TimeoutFuture,
}

unsafe impl Send for SendTimeoutFuture {}
unsafe impl Sync for SendTimeoutFuture {}

impl std::future::Future for SendTimeoutFuture {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();
            std::pin::Pin::new_unchecked(&mut this.inner).poll(cx)
        }
    }
}
