//! Server-side transport adapter wrappers.
//!
//! Important boundary:
//! - Transport logic is implemented in `mbus-network` and `mbus-serial`.
//! - This module only adapts those transports for wasm server binding lifecycle.

use wasm_bindgen::JsValue;

use heapless::{String as HString, Vec as HVec};
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    ModbusTcpConfig, Parity, SerialMode, Transport, TransportError,
};
use mbus_network::WasmWsTransport;
use mbus_serial::{WasmAsciiTransport, WasmRtuTransport};

use super::binding_types::{WasmSerialServerConfig, WasmServerTransportKind, WasmTcpGatewayConfig};

pub(super) fn to_js_error(err: TransportError) -> JsValue {
    JsValue::from_str(&format!("transport error: {err:?}"))
}

pub(super) struct TcpGatewayServerAdapter {
    inner: WasmWsTransport,
    config: ModbusConfig,
}

impl TcpGatewayServerAdapter {
    pub(super) fn new(cfg: &WasmTcpGatewayConfig) -> Result<Self, JsValue> {
        let host = HString::try_from("wasm-server")
            .map_err(|_| JsValue::from_str("failed to build tcp host string"))?;
        let config = ModbusConfig::Tcp(ModbusTcpConfig {
            host,
            port: 0,
            connection_timeout_ms: 5000,
            response_timeout_ms: 2000,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        Ok(Self {
            inner: WasmWsTransport::new(&cfg.ws_url()),
            config,
        })
    }

    pub(super) fn connect(&mut self) -> Result<(), JsValue> {
        self.inner.connect(&self.config).map_err(to_js_error)
    }

    pub(super) fn disconnect(&mut self) -> Result<(), JsValue> {
        self.inner.disconnect().map_err(to_js_error)
    }

    pub(super) fn is_connected(&self) -> bool {
        self.inner.is_open()
    }

    pub(super) fn is_connecting(&self) -> bool {
        self.inner.is_connecting()
    }

    pub(super) fn send_frame(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        self.inner.send(frame).map_err(to_js_error)
    }

    pub(super) fn recv_frame(&mut self) -> Result<Option<HVec<u8, MAX_ADU_FRAME_LEN>>, JsValue> {
        match self.inner.recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(TransportError::Timeout) => Ok(None),
            Err(err) => Err(to_js_error(err)),
        }
    }
}

enum RuntimeSerialAdapter {
    Rtu(WasmRtuTransport),
    Ascii(WasmAsciiTransport),
}

impl RuntimeSerialAdapter {
    fn connect(&mut self, cfg: &ModbusConfig) -> Result<(), TransportError> {
        match self {
            Self::Rtu(t) => t.connect(cfg),
            Self::Ascii(t) => t.connect(cfg),
        }
    }

    fn disconnect(&mut self) -> Result<(), TransportError> {
        match self {
            Self::Rtu(t) => t.disconnect(),
            Self::Ascii(t) => t.disconnect(),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Self::Rtu(t) => t.is_connected(),
            Self::Ascii(t) => t.is_connected(),
        }
    }

    fn send_frame(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        match self {
            Self::Rtu(t) => t.send(frame),
            Self::Ascii(t) => t.send(frame),
        }
    }

    fn recv_frame(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, TransportError> {
        match self {
            Self::Rtu(t) => t.recv(),
            Self::Ascii(t) => t.recv(),
        }
    }

    fn attach_port(&mut self, port: JsValue) {
        match self {
            Self::Rtu(t) => t.attach_port(port),
            Self::Ascii(t) => t.attach_port(port),
        }
    }
}

pub(super) struct SerialServerAdapter {
    inner: RuntimeSerialAdapter,
    config: ModbusConfig,
    kind: WasmServerTransportKind,
}

impl SerialServerAdapter {
    pub(super) fn new(cfg: &WasmSerialServerConfig) -> Result<Self, JsValue> {
        let kind = cfg.mode();
        let serial_mode = match kind {
            WasmServerTransportKind::SerialRtu => SerialMode::Rtu,
            WasmServerTransportKind::SerialAscii => SerialMode::Ascii,
            WasmServerTransportKind::TcpGateway => {
                return Err(JsValue::from_str("tcp gateway is not a serial mode"));
            }
        };

        let mut port_path = HString::new();
        port_path
            .push_str("web-serial-server")
            .map_err(|_| JsValue::from_str("failed to build serial port path"))?;

        let config = ModbusConfig::Serial(ModbusSerialConfig {
            port_path,
            mode: serial_mode,
            baud_rate: BaudRate::Baud19200,
            data_bits: DataBits::Eight,
            stop_bits: 1,
            parity: Parity::None,
            response_timeout_ms: 2000,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let inner = match kind {
            WasmServerTransportKind::SerialRtu => RuntimeSerialAdapter::Rtu(WasmRtuTransport::new()),
            WasmServerTransportKind::SerialAscii => {
                RuntimeSerialAdapter::Ascii(WasmAsciiTransport::new())
            }
            WasmServerTransportKind::TcpGateway => {
                return Err(JsValue::from_str("tcp gateway is not a serial mode"));
            }
        };

        Ok(Self {
            inner,
            config,
            kind,
        })
    }

    pub(super) fn kind(&self) -> WasmServerTransportKind {
        self.kind
    }

    pub(super) fn attach_port(&mut self, port: JsValue) {
        self.inner.attach_port(port);
    }

    pub(super) fn connect(&mut self) -> Result<(), JsValue> {
        self.inner.connect(&self.config).map_err(to_js_error)
    }

    pub(super) fn disconnect(&mut self) -> Result<(), JsValue> {
        self.inner.disconnect().map_err(to_js_error)
    }

    pub(super) fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    pub(super) fn send_frame(&mut self, frame: &[u8]) -> Result<(), JsValue> {
        self.inner.send_frame(frame).map_err(to_js_error)
    }

    pub(super) fn recv_frame(&mut self) -> Result<Option<HVec<u8, MAX_ADU_FRAME_LEN>>, JsValue> {
        match self.inner.recv_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(TransportError::Timeout) => Ok(None),
            Err(err) => Err(to_js_error(err)),
        }
    }
}
