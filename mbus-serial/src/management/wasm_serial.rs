use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use heapless::Vec;
use js_sys::{Function, Object, Promise, Reflect, Uint8Array};
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
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
}

/// Browser Web Serial transport for wasm32 targets.
///
/// This transport expects a JS `SerialPort` object to be attached first via
/// `attach_port()`. The actual permission flow (`navigator.serial.requestPort()`)
/// must stay in a user-gesture-driven JS/WASM entry point.
#[derive(Debug)]
pub struct WasmSerialTransport {
    port: Option<JsValue>,
    mode: SerialMode,
    shared: Rc<RefCell<SerialShared>>,
}

impl WasmSerialTransport {
    /// Creates a new browser serial transport for the requested serial mode.
    pub fn new(mode: SerialMode) -> Self {
        Self {
            port: None,
            mode,
            shared: Rc::new(RefCell::new(SerialShared {
                rx_buf: VecDeque::new(),
                tx_queue: VecDeque::new(),
                connected: false,
                opening: false,
                reader_running: false,
                writer_running: false,
            })),
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

                let reader = match WasmSerialTransport::call_method0(&readable, "getReader") {
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

                    let read_result = match WasmSerialTransport::call_method0(&reader, "read")
                        .and_then(WasmSerialTransport::promise_from)
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
                    shared.borrow_mut().rx_buf.extend(bytes);
                }

                let _ = WasmSerialTransport::call_method0(&reader, "releaseLock");
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

                let writer = match WasmSerialTransport::call_method0(&writable, "getWriter") {
                    Ok(writer) => writer,
                    Err(_) => {
                        shared.borrow_mut().connected = false;
                        break;
                    }
                };

                let data = Uint8Array::from(frame.as_slice());
                let write_result =
                    match WasmSerialTransport::call_method1(&writer, "write", &data.into())
                        .and_then(WasmSerialTransport::promise_from)
                    {
                        Ok(promise) => JsFuture::from(promise).await,
                        Err(_) => {
                            shared.borrow_mut().connected = false;
                            break;
                        }
                    };

                if write_result.is_err() {
                    shared.borrow_mut().connected = false;
                    let _ = WasmSerialTransport::call_method0(&writer, "releaseLock");
                    break;
                }

                let _ = WasmSerialTransport::call_method0(&writer, "releaseLock");
            }

            shared.borrow_mut().writer_running = false;
        });
    }
}

impl Transport for WasmSerialTransport {
    type Error = TransportError;
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(SerialMode::Rtu);

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        let serial_config = match config {
            ModbusConfig::Serial(config) => config,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        if serial_config.mode != self.mode {
            return Err(TransportError::InvalidConfiguration);
        }

        let port = self.port.clone().ok_or(TransportError::ConnectionFailed)?;

        {
            let mut state = self.shared.borrow_mut();
            if state.connected || state.opening {
                return Ok(());
            }
            state.opening = true;
            state.rx_buf.clear();
            state.tx_queue.clear();
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
                WasmSerialTransport::spawn_reader_task(port_for_reader, shared.clone());
                WasmSerialTransport::spawn_writer_task(port_for_writer, shared);
            }
        });

        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.shared.borrow_mut().connected = false;
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

    fn transport_type(&self) -> TransportType {
        TransportType::CustomSerial(self.mode)
    }
}
