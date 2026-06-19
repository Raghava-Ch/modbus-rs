#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use heapless::Vec;
use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    BaudRate, DataBits, ModbusConfig, Parity, SerialMode, Transport, TransportError, TransportType,
};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[derive(Debug)]
struct SerialShared {
    rx_buf: VecDeque<u8>,
    tx_queue: VecDeque<std::vec::Vec<u8>>,
    connected: bool,
    opening: bool,
    reader_running: bool,
    writer_running: bool,
    rx_tx: Option<futures_channel::mpsc::UnboundedSender<std::vec::Vec<u8>>>,
}

/// Browser Web Serial transport for wasm32 targets.
///
/// The const generic `ASCII` selects the framing mode at compile time:
/// - `false` → Modbus RTU (binary + CRC)
/// - `true`  → Modbus ASCII (`:` delimited + LRC)
///
/// Prefer the type aliases [`WasmRtuTransport`] and [`WasmAsciiTransport`].
///
/// This transport expects a JS `SerialPort` object to be attached first via
/// `attach_port()`. The actual permission flow (`navigator.serial.requestPort()`)
/// must stay in a user-gesture-driven JS/WASM entry point.
#[derive(Debug)]
pub struct WasmSerialTransport<const ASCII: bool = false> {
    port: Option<JsValue>,
    shared: Rc<RefCell<SerialShared>>,
    rx_rx: Option<futures_channel::mpsc::UnboundedReceiver<std::vec::Vec<u8>>>,
    inter_frame_ms: u32,
}

/// Modbus RTU browser serial transport.
pub type WasmRtuTransport = WasmSerialTransport<false>;
/// Modbus ASCII browser serial transport.
pub type WasmAsciiTransport = WasmSerialTransport<true>;

impl<const ASCII: bool> WasmSerialTransport<ASCII> {
    /// The serial mode determined by the `ASCII` const generic.
    const MODE: SerialMode = if ASCII {
        SerialMode::Ascii
    } else {
        SerialMode::Rtu
    };

    /// Creates a new browser serial transport.
    pub fn new() -> Self {
        Self {
            port: None,
            shared: Rc::new(RefCell::new(SerialShared {
                rx_buf: VecDeque::new(),
                tx_queue: VecDeque::new(),
                connected: false,
                opening: false,
                reader_running: false,
                writer_running: false,
                rx_tx: None,
            })),
            rx_rx: None,
            inter_frame_ms: 35,
        }
    }

    /// Attaches a browser `SerialPort` object obtained from JS.
    pub fn attach_port(&mut self, port: JsValue) {
        self.port = Some(port);
        let mut shared = self.shared.borrow_mut();
        shared.connected = false;
        shared.opening = false;
        shared.rx_buf.clear();
        shared.tx_queue.clear();
    }

    /// Returns true when a browser `SerialPort` handle has been attached.
    pub fn has_port(&self) -> bool {
        self.port.is_some()
    }

    fn get_function(target: &JsValue, name: &str) -> Result<Function, TransportError> {
        Reflect::get(target, &JsValue::from_str(name))
            .map_err(|_| TransportError::Unexpected)?
            .dyn_into::<Function>()
            .map_err(|_| TransportError::Unexpected)
    }

    fn call_method0(target: &JsValue, name: &str) -> Result<JsValue, TransportError> {
        Self::get_function(target, name)?
            .call0(target)
            .map_err(|_| TransportError::Unexpected)
    }

    fn call_method1(
        target: &JsValue,
        name: &str,
        arg1: &JsValue,
    ) -> Result<JsValue, TransportError> {
        Self::get_function(target, name)?
            .call1(target, arg1)
            .map_err(|_| TransportError::Unexpected)
    }

    fn promise_from(value: JsValue) -> Result<Promise, TransportError> {
        value
            .dyn_into::<Promise>()
            .map_err(|_| TransportError::Unexpected)
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

    fn spawn_reader_task(port: JsValue, shared: Rc<RefCell<SerialShared>>) {
        {
            let mut state = shared.borrow_mut();
            if state.reader_running {
                return;
            }
            state.reader_running = true;
        }

        spawn_local(async move {
            loop {
                if !shared.borrow().connected {
                    break;
                }

                let readable = match Reflect::get(&port, &JsValue::from_str("readable")) {
                    Ok(value) if !value.is_null() && !value.is_undefined() => value,
                    _ => {
                        shared.borrow_mut().connected = false;
                        break;
                    }
                };

                let reader =
                    match WasmSerialTransport::<ASCII>::call_method0(&readable, "getReader") {
                        Ok(reader) => reader,
                        Err(_) => {
                            shared.borrow_mut().connected = false;
                            break;
                        }
                    };

                loop {
                    if !shared.borrow().connected {
                        break;
                    }

                    let read_result =
                        match WasmSerialTransport::<ASCII>::call_method0(&reader, "read")
                            .and_then(WasmSerialTransport::<ASCII>::promise_from)
                        {
                            Ok(promise) => JsFuture::from(promise).await,
                            Err(_) => {
                                shared.borrow_mut().connected = false;
                                break;
                            }
                        };

                    let result = match read_result {
                        Ok(result) => result,
                        Err(_) => {
                            shared.borrow_mut().connected = false;
                            break;
                        }
                    };

                    let done = Reflect::get(&result, &JsValue::from_str("done"))
                        .ok()
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                    if done {
                        break;
                    }

                    let value = match Reflect::get(&result, &JsValue::from_str("value")) {
                        Ok(value) if !value.is_null() && !value.is_undefined() => value,
                        _ => continue,
                    };
                    let bytes = Uint8Array::new(&value).to_vec();
                    {
                        let mut state = shared.borrow_mut();
                        // NOTE: Split-brain buffer risk. Bytes are extended into rx_buf
                        // (used by sync Transport::recv) and unbounded_send to rx_tx
                        // (used by async recv_frame). Consuming from one path does not
                        // drain the other.
                        // TODO(async): This is solved by WasmAsyncSerialTransport which uses a single path.
                        state.rx_buf.extend(bytes.clone());
                        if let Some(tx) = &state.rx_tx {
                            let _ = tx.unbounded_send(bytes);
                        }
                    }
                }

                let _ = WasmSerialTransport::<ASCII>::call_method0(&reader, "releaseLock");
                TimeoutFuture::new(5).await;
            }

            shared.borrow_mut().reader_running = false;
        });
    }

    fn spawn_writer_task(port: JsValue, shared: Rc<RefCell<SerialShared>>) {
        {
            let mut state = shared.borrow_mut();
            if state.writer_running {
                return;
            }
            state.writer_running = true;
        }

        spawn_local(async move {
            loop {
                let next_frame = {
                    let mut state = shared.borrow_mut();
                    if !state.connected {
                        break;
                    }
                    state.tx_queue.pop_front()
                };

                let Some(frame) = next_frame else {
                    TimeoutFuture::new(5).await;
                    continue;
                };

                let writable = match Reflect::get(&port, &JsValue::from_str("writable")) {
                    Ok(value) if !value.is_null() && !value.is_undefined() => value,
                    _ => {
                        shared.borrow_mut().connected = false;
                        break;
                    }
                };

                let writer =
                    match WasmSerialTransport::<ASCII>::call_method0(&writable, "getWriter") {
                        Ok(writer) => writer,
                        Err(_) => {
                            shared.borrow_mut().connected = false;
                            break;
                        }
                    };

                let data = Uint8Array::from(frame.as_slice());
                let write_result = match WasmSerialTransport::<ASCII>::call_method1(
                    &writer,
                    "write",
                    &data.into(),
                )
                .and_then(WasmSerialTransport::<ASCII>::promise_from)
                {
                    Ok(promise) => JsFuture::from(promise).await,
                    Err(_) => {
                        shared.borrow_mut().connected = false;
                        break;
                    }
                };

                if write_result.is_err() {
                    shared.borrow_mut().connected = false;
                    let _ = WasmSerialTransport::<ASCII>::call_method0(&writer, "releaseLock");
                    break;
                }

                let _ = WasmSerialTransport::<ASCII>::call_method0(&writer, "releaseLock");
            }

            shared.borrow_mut().writer_running = false;
        });
    }
}

impl<const ASCII: bool> WasmSerialTransport<ASCII> {
    /// Sends a frame over the serial transport.
    pub fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if self.port.is_none() {
            return Err(MbusError::ConnectionClosed);
        }
        let mut state = self.shared.borrow_mut();
        if !state.connected && !state.opening {
            return Err(MbusError::ConnectionClosed);
        }
        state.tx_queue.push_back(adu.to_vec());
        Ok(())
    }

    /// Receives a frame asynchronously using RTU or ASCII framing.
    pub async fn recv_frame(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        if ASCII {
            self.recv_ascii().await
        } else {
            self.recv_rtu().await
        }
    }

    async fn recv_rtu(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let rx = self.rx_rx.as_mut().ok_or(MbusError::ConnectionClosed)?;
        let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();

        use futures_util::stream::StreamExt;
        let chunk = match rx.next().await {
            Some(c) => c,
            None => return Err(MbusError::ConnectionClosed),
        };
        if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
            return Err(MbusError::BufferTooSmall);
        }
        buf.extend(chunk);

        if let Some(expected) =
            mbus_core::data_unit::common::derive_length_from_bytes(&buf, Self::TRANSPORT_TYPE)
        {
            if buf.len() >= expected {
                return Ok(buf);
            }
        }

        let inter_frame_ms = self.inter_frame_ms;
        loop {
            use futures_util::FutureExt;
            let mut timeout_fut = gloo_timers::future::TimeoutFuture::new(inter_frame_ms).fuse();
            let mut next_fut = rx.next().fuse();

            futures_util::select! {
                res = next_fut => {
                    match res {
                        Some(chunk) => {
                            if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
                                return Err(MbusError::BufferTooSmall);
                            }
                            buf.extend(chunk);
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

    async fn recv_ascii(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let rx = self.rx_rx.as_mut().ok_or(MbusError::ConnectionClosed)?;
        let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();

        use futures_util::stream::StreamExt;
        loop {
            let chunk = match rx.next().await {
                Some(c) => c,
                None => return Err(MbusError::ConnectionClosed),
            };
            if buf.len() + chunk.len() > MAX_ADU_FRAME_LEN {
                return Err(MbusError::BufferTooSmall);
            }
            buf.extend(chunk);
            let len = buf.len();
            if len >= 2 && buf[len - 2] == b'\r' && buf[len - 1] == b'\n' {
                return Ok(buf);
            }
        }
    }
}

impl<const ASCII: bool> Transport for WasmSerialTransport<ASCII> {
    type Error = TransportError;
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(Self::MODE);

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        let serial_config = match config {
            ModbusConfig::Serial(config) => config,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        if serial_config.mode != Self::MODE {
            return Err(TransportError::InvalidConfiguration);
        }

        let port = self.port.clone().ok_or(TransportError::ConnectionFailed)?;

        let (tx, rx) = futures_channel::mpsc::unbounded::<std::vec::Vec<u8>>();
        self.rx_rx = Some(rx);

        let baud = match serial_config.baud_rate {
            BaudRate::Baud9600 => 9600,
            BaudRate::Baud19200 => 19200,
            BaudRate::Custom(rate) => rate,
        }
        .max(1) as u64;
        let char_time_us = (11 * 1_000_000) / baud;
        let silence_us = ((char_time_us * 7) / 2).max(1750).max(100_000);
        self.inter_frame_ms = (silence_us / 1000) as u32;

        {
            let mut state = self.shared.borrow_mut();
            // NOTE: Reconnecting an already connected or opening transport is a silent no-op.
            // This does not update configuration options or recreate the reader/writer tasks.
            if state.connected || state.opening {
                return Ok(());
            }
            state.opening = true;
            state.rx_buf.clear();
            state.tx_queue.clear();
            state.rx_tx = Some(tx);
        }

        let shared = self.shared.clone();
        let options = Self::serial_open_options(serial_config);
        let open_result = Self::call_method1(&port, "open", &options)?;
        let open_promise = Self::promise_from(open_result)?;
        let port_for_reader = port.clone();
        let port_for_writer = port.clone();

        spawn_local(async move {
            let opened = JsFuture::from(open_promise).await.is_ok();
            {
                let mut state = shared.borrow_mut();
                state.opening = false;
                state.connected = opened;
            }
            if opened {
                WasmSerialTransport::<ASCII>::spawn_reader_task(port_for_reader, shared.clone());
                WasmSerialTransport::<ASCII>::spawn_writer_task(port_for_writer, shared);
            }
        });

        Ok(())
    }

    /// Disconnects the serial port.
    ///
    /// NOTE: This function is fire-and-forget; it returns success before the
    /// underlying promise for `SerialPort.close()` resolves. A subsequent connect call
    /// immediately after this may race with the closing operation.
    /// TODO(async): Use WasmAsyncSerialTransport for a fully awaited async disconnect.
    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.rx_rx = None;
        let mut state = self.shared.borrow_mut();
        state.connected = false;
        state.rx_tx = None;
        if let Some(port) = self.port.clone() {
            if let Ok(close_result) = Self::call_method0(&port, "close") {
                if let Ok(close_promise) = Self::promise_from(close_result) {
                    spawn_local(async move {
                        let _ = JsFuture::from(close_promise).await;
                    });
                }
            }
        }
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        if self.port.is_none() {
            return Err(TransportError::ConnectionClosed);
        }
        let mut state = self.shared.borrow_mut();
        if !state.connected && !state.opening {
            return Err(TransportError::ConnectionClosed);
        }
        state.tx_queue.push_back(adu.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        let mut shared = self.shared.borrow_mut();
        if shared.rx_buf.is_empty() {
            return Err(TransportError::Timeout);
        }

        let drain_len = shared.rx_buf.len().min(MAX_ADU_FRAME_LEN);
        let mut out: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        for byte in shared.rx_buf.drain(..drain_len) {
            let _ = out.push(byte);
        }
        Ok(out)
    }

    fn is_connected(&self) -> bool {
        let state = self.shared.borrow();
        state.connected || state.opening
    }
}

// SAFETY: WasmSerialTransport is compiled for the WASM target which is single-threaded.
// TODO(wasm-threads): Switch WasmSerialTransport to Arc/Mutex or remove these unsafe impls
// when multi-threaded WASM is used, as Rc/RefCell are not thread-safe.
#[cfg(target_arch = "wasm32")]
unsafe impl<const ASCII: bool> Send for WasmSerialTransport<ASCII> {}
#[cfg(target_arch = "wasm32")]
unsafe impl<const ASCII: bool> Sync for WasmSerialTransport<ASCII> {}

impl<const ASCII: bool> mbus_core::transport::AsyncTransport for WasmSerialTransport<ASCII> {
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(Self::MODE);

    fn send<'a>(
        &'a mut self,
        adu: &'a [u8],
    ) -> impl std::future::Future<Output = Result<(), MbusError>> + Send + 'a {
        async move { self.send_frame(adu) }
    }

    fn recv(
        &mut self,
    ) -> impl std::future::Future<Output = Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError>> + Send + '_
    {
        UnsafeSendFuture::new(async move { self.recv_frame().await })
    }

    fn is_connected(&self) -> bool {
        Transport::is_connected(self)
    }
}

// TODO(wasm-threads): Remove UnsafeSendFuture once WasmAsyncSerialTransport is wired up.
#[cfg(target_arch = "wasm32")]
struct UnsafeSendFuture<F> {
    inner: F,
}

#[cfg(target_arch = "wasm32")]
impl<F> UnsafeSendFuture<F> {
    fn new(inner: F) -> Self {
        Self { inner }
    }
}

// Safety: This is compiled for the WASM target which is single-threaded.
#[cfg(target_arch = "wasm32")]
unsafe impl<F> Send for UnsafeSendFuture<F> {}

#[cfg(target_arch = "wasm32")]
impl<F: std::future::Future> std::future::Future for UnsafeSendFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let unsafe_self = unsafe { self.get_unchecked_mut() };
        let inner_pin = unsafe { std::pin::Pin::new_unchecked(&mut unsafe_self.inner) };
        inner_pin.poll(cx)
    }
}
