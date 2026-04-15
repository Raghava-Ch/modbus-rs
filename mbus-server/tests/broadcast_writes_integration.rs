//! Integration tests for Serial broadcast write handling.
//!
//! Verifies that [`ServerServices`] with `enable_broadcast_writes: true`:
//! - Dispatches FC05/FC0F/FC06/FC10 addressed to slave address 0 with **no response**.
//! - Invokes the app callback for each supported function code.
//! - Silently drops broadcast frames when `enable_broadcast_writes: false`.
//! - Silently drops broadcast frames on TCP (not supported by that transport).

mod common;
use common::{
    MockTransport, build_request, build_serial_request, serial_rtu_config, tcp_config, unit_id,
};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, SerialMode, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{ModbusAppHandler, ResilienceConfig, ServerServices};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Broadcast-capable serial mock transport
// ---------------------------------------------------------------------------

/// A serial RTU mock that declares `SUPPORTS_BROADCAST_WRITES = true`, required
/// for the server to route broadcast frames to app callbacks.
struct MockSerialBroadcastTransport {
    next_rx: Option<HVec<u8, MAX_ADU_FRAME_LEN>>,
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    connected: bool,
}

impl Transport for MockSerialBroadcastTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdSerial(SerialMode::Rtu);
    const SUPPORTS_BROADCAST_WRITES: bool = true;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        self.sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .push(adu.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.next_rx.take().ok_or(TransportError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ---------------------------------------------------------------------------
// App with atomic call counter
// ---------------------------------------------------------------------------

struct BroadcastApp {
    calls: Arc<AtomicUsize>,
}

impl ModbusAppHandler for BroadcastApp {
    #[cfg(feature = "coils")]
    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: bool,
    ) -> Result<(), MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[cfg(feature = "coils")]
    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _quantity: u16,
        _values: &[u8],
    ) -> Result<(), MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[cfg(feature = "holding-registers")]
    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) -> Result<(), MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[cfg(feature = "holding-registers")]
    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _values: &[u16],
    ) -> Result<(), MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for BroadcastApp {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn broadcast_addr() -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new_broadcast_address()
}

/// Run one poll on a broadcast-capable serial transport with broadcast enabled.
/// Returns `(app_call_count, sent_frame_count)`.
fn run_broadcast_serial(request: HVec<u8, MAX_ADU_FRAME_LEN>) -> (usize, usize) {
    let calls = Arc::new(AtomicUsize::new(0));
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let transport = MockSerialBroadcastTransport {
        next_rx: Some(request),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };
    let app = BroadcastApp {
        calls: Arc::clone(&calls),
    };
    let mut server = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig {
            enable_broadcast_writes: true,
            ..Default::default()
        },
    );
    server.poll();
    (
        calls.load(Ordering::SeqCst),
        sent_frames.lock().expect("poisoned").len(),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// FC05 broadcast on serial: no response, app callback invoked.
#[cfg(feature = "coils")]
#[test]
fn fc05_broadcast_serial_no_response_callback_invoked() {
    // address=10 (0x000A), coil ON (0xFF00)
    let request = build_serial_request(
        1,
        broadcast_addr(),
        FunctionCode::WriteSingleCoil,
        &[0x00, 0x0A, 0xFF, 0x00],
    );
    let (call_count, sent_count) = run_broadcast_serial(request);
    assert_eq!(sent_count, 0, "broadcast must not generate a response");
    assert_eq!(call_count, 1, "app callback must be invoked exactly once");
}

/// FC0F broadcast on serial: no response, app callback invoked.
#[cfg(feature = "coils")]
#[test]
fn fc0f_broadcast_serial_no_response_callback_invoked() {
    // address=0, quantity=3 coils, byte_count=1, packed=0b00000111
    let request = build_serial_request(
        2,
        broadcast_addr(),
        FunctionCode::WriteMultipleCoils,
        &[0x00, 0x00, 0x00, 0x03, 0x01, 0x07],
    );
    let (call_count, sent_count) = run_broadcast_serial(request);
    assert_eq!(sent_count, 0, "broadcast must not generate a response");
    assert_eq!(call_count, 1, "app callback must be invoked exactly once");
}

/// FC06 broadcast on serial: no response, app callback invoked.
#[cfg(feature = "holding-registers")]
#[test]
fn fc06_broadcast_serial_no_response_callback_invoked() {
    // address=5 (0x0005), value=0x1234
    let request = build_serial_request(
        3,
        broadcast_addr(),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x05, 0x12, 0x34],
    );
    let (call_count, sent_count) = run_broadcast_serial(request);
    assert_eq!(sent_count, 0, "broadcast must not generate a response");
    assert_eq!(call_count, 1, "app callback must be invoked exactly once");
}

/// FC10 broadcast on serial: no response, app callback invoked.
#[cfg(feature = "holding-registers")]
#[test]
fn fc10_broadcast_serial_no_response_callback_invoked() {
    // address=0, quantity=2, byte_count=4, values=[0x0064, 0x00C8]
    let request = build_serial_request(
        4,
        broadcast_addr(),
        FunctionCode::WriteMultipleRegisters,
        &[0x00, 0x00, 0x00, 0x02, 0x04, 0x00, 0x64, 0x00, 0xC8],
    );
    let (call_count, sent_count) = run_broadcast_serial(request);
    assert_eq!(sent_count, 0, "broadcast must not generate a response");
    assert_eq!(call_count, 1, "app callback must be invoked exactly once");
}

/// With `enable_broadcast_writes: false` (the default) the frame is silently dropped
/// and the app callback is never invoked.
#[cfg(feature = "holding-registers")]
#[test]
fn broadcast_disabled_silent_drop_no_callback() {
    let calls = Arc::new(AtomicUsize::new(0));
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let transport = MockSerialBroadcastTransport {
        next_rx: Some(build_serial_request(
            5,
            broadcast_addr(),
            FunctionCode::WriteSingleRegister,
            &[0x00, 0x05, 0x00, 0x01],
        )),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };
    let app = BroadcastApp {
        calls: Arc::clone(&calls),
    };
    let mut server = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(), // enable_broadcast_writes: false
    );
    server.poll();
    assert_eq!(
        sent_frames.lock().unwrap().len(),
        0,
        "no response when broadcast is disabled"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "callback must not fire when broadcast is disabled"
    );
}

/// TCP transport does not support broadcast writes: the frame is silently dropped
/// even with `enable_broadcast_writes: true`.
#[cfg(feature = "holding-registers")]
#[test]
fn broadcast_on_tcp_silently_dropped() {
    let calls = Arc::new(AtomicUsize::new(0));
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    // address=0 (broadcast), FC06
    let request = build_request(
        6,
        broadcast_addr(),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x05, 0x00, 0x01],
    );
    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };
    let app = BroadcastApp {
        calls: Arc::clone(&calls),
    };
    let mut server = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig {
            enable_broadcast_writes: true,
            ..Default::default()
        },
    );
    server.poll();
    assert_eq!(
        sent_frames.lock().unwrap().len(),
        0,
        "TCP transport must silently drop broadcast frames"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app callback must not fire for TCP broadcast"
    );
}
