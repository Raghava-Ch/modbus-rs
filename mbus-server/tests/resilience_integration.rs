#![cfg(feature = "holding-registers")]

mod common;
use common::{build_request, tcp_config, unit_id};

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, Pdu, compile_adu_frame};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, Transport, TransportType, UnitIdOrSlaveAddr,
};
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiagnosticsHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerInputRegisterHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{
    OverflowPolicy, ResilienceConfig, ServerExceptionHandler, ServerHoldingRegisterHandler,
    ServerServices, TimeoutConfig,
};
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct ScriptedTransport {
    recv_queue: VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>,
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    send_failures_remaining: Arc<AtomicUsize>,
    connected: bool,
}

impl Transport for ScriptedTransport {
    type Error = MbusError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let remaining = self.send_failures_remaining.load(Ordering::SeqCst);
        if remaining > 0 {
            self.send_failures_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err(MbusError::SendFailed);
        }
        self.sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .push(adu.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.recv_queue.pop_front().ok_or(MbusError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[derive(Debug)]
struct ScriptedSerialTransport {
    recv_queue: VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>,
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    send_failures_remaining: Arc<AtomicUsize>,
    connected: bool,
}

impl Transport for ScriptedSerialTransport {
    type Error = MbusError;
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::StdSerial(SerialMode::Rtu);

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let remaining = self.send_failures_remaining.load(Ordering::SeqCst);
        if remaining > 0 {
            self.send_failures_remaining.fetch_sub(1, Ordering::SeqCst);
            return Err(MbusError::SendFailed);
        }
        self.sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .push(adu.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.recv_queue.pop_front().ok_or(MbusError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[derive(Debug, Default)]
struct ProbeApp {
    call_order: Arc<Mutex<Vec<u8>>>,
    fc03_calls: Arc<AtomicUsize>,
    fc06_calls: Arc<AtomicUsize>,
    #[cfg(feature = "coils")]
    fc05_calls: Arc<AtomicUsize>,
    #[cfg(feature = "coils")]
    fc0f_calls: Arc<AtomicUsize>,
    fc10_calls: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_rx_frames: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_tx_frames: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_rx_errors: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_tx_errors: Arc<AtomicUsize>,
}

impl ServerExceptionHandler for ProbeApp {}

impl ServerDiscreteInputHandler for ProbeApp {}

impl ServerFifoHandler for ProbeApp {}

impl ServerFileRecordHandler for ProbeApp {}

impl ServerDiagnosticsHandler for ProbeApp {}

impl ServerInputRegisterHandler for ProbeApp {
    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        _out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }
}

impl ServerHoldingRegisterHandler for ProbeApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.fc03_calls.fetch_add(1, Ordering::SeqCst);
        self.call_order
            .lock()
            .expect("call_order mutex poisoned")
            .push(3);
        for i in 0..quantity as usize {
            let offset = i * 2;
            out[offset] = 0x12;
            out[offset + 1] = 0x34;
        }
        Ok((quantity * 2) as u8)
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) -> Result<(), MbusError> {
        self.fc06_calls.fetch_add(1, Ordering::SeqCst);
        self.call_order
            .lock()
            .expect("call_order mutex poisoned")
            .push(6);
        Ok(())
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _values: &[u16],
    ) -> Result<(), MbusError> {
        self.fc10_calls.fetch_add(1, Ordering::SeqCst);
        self.call_order
            .lock()
            .expect("call_order mutex poisoned")
            .push(16);
        Ok(())
    }
}

impl ServerCoilHandler for ProbeApp {
    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: bool,
    ) -> Result<(), MbusError> {
        self.fc05_calls.fetch_add(1, Ordering::SeqCst);
        self.call_order
            .lock()
            .expect("call_order mutex poisoned")
            .push(5);
        Ok(())
    }

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _quantity: u16,
        _values: &[u8],
    ) -> Result<(), MbusError> {
        self.fc0f_calls.fetch_add(1, Ordering::SeqCst);
        self.call_order
            .lock()
            .expect("call_order mutex poisoned")
            .push(15);
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for ProbeApp {
    fn on_rx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
        self.traffic_rx_frames.fetch_add(1, Ordering::SeqCst);
    }

    fn on_tx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
        self.traffic_tx_frames.fetch_add(1, Ordering::SeqCst);
    }

    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
        self.traffic_rx_errors.fetch_add(1, Ordering::SeqCst);
    }

    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
        self.traffic_tx_errors.fetch_add(1, Ordering::SeqCst);
    }
}

fn build_fc03_read_request(txn_id: u16) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let payload = [0x00, 0x00, 0x00, 0x01];
    build_request(
        txn_id,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &payload,
    )
}

#[allow(dead_code)]
fn build_fc03_invalid_quantity_request(txn_id: u16) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let payload = [0x00, 0x00, 0x00, 0x00];
    build_request(
        txn_id,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &payload,
    )
}

fn build_fc06_write_request(txn_id: u16) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let payload = [0x00, 0x02, 0xAB, 0xCD];
    build_request(
        txn_id,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &payload,
    )
}

fn build_fc06_write_request_for_unit(txn_id: u16, wire_unit: u8) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, 0x0002, 0xABCD)
        .expect("valid FC06 payload");
    compile_adu_frame(txn_id, wire_unit, pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}

fn build_serial_fc06_write_request_for_unit(
    txn_id: u16,
    wire_unit: u8,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, 0x0002, 0xABCD)
        .expect("valid serial FC06 payload");
    compile_adu_frame(
        txn_id,
        wire_unit,
        pdu,
        TransportType::StdSerial(SerialMode::Rtu),
    )
    .expect("serial request ADU should compile")
}

#[cfg(feature = "coils")]
fn build_serial_fc05_write_request_for_unit(
    txn_id: u16,
    wire_unit: u8,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleCoil, 0x0002, 0xFF00)
        .expect("valid serial FC05 payload");
    compile_adu_frame(
        txn_id,
        wire_unit,
        pdu,
        TransportType::StdSerial(SerialMode::Rtu),
    )
    .expect("serial request ADU should compile")
}

#[cfg(feature = "coils")]
fn build_serial_fc0f_write_request_for_unit(
    txn_id: u16,
    wire_unit: u8,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    // address=0x0005, quantity=3, coil bytes=[0x05]
    let pdu = Pdu::build_write_multiple(FunctionCode::WriteMultipleCoils, 0x0005, 3, &[0x05])
        .expect("valid serial FC0F payload");
    compile_adu_frame(
        txn_id,
        wire_unit,
        pdu,
        TransportType::StdSerial(SerialMode::Rtu),
    )
    .expect("serial request ADU should compile")
}

fn build_serial_fc10_write_request_for_unit(
    txn_id: u16,
    wire_unit: u8,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    // address=0x0010, quantity=2, register bytes=[0x12,0x34,0x56,0x78]
    let pdu = Pdu::build_write_multiple(
        FunctionCode::WriteMultipleRegisters,
        0x0010,
        2,
        &[0x12, 0x34, 0x56, 0x78],
    )
    .expect("valid serial FC10 payload");
    compile_adu_frame(
        txn_id,
        wire_unit,
        pdu,
        TransportType::StdSerial(SerialMode::Rtu),
    )
    .expect("serial request ADU should compile")
}

fn serial_rtu_config() -> ModbusConfig {
    let mut port_path = heapless::String::<64>::new();
    port_path
        .push_str("/dev/mock")
        .expect("mock serial path should fit");

    ModbusConfig::Serial(ModbusSerialConfig {
        port_path,
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1_000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    })
}

fn txn_id_from_adu(frame: &[u8]) -> u16 {
    assert!(frame.len() >= 2, "ADU must include MBAP transaction id");
    ((frame[0] as u16) << 8) | (frame[1] as u16)
}

thread_local! {
    static TEST_CLOCK_MS: Cell<u64> = const { Cell::new(0) };
    static MANUAL_CLOCK_MS: Cell<u64> = const { Cell::new(0) };
}

fn reset_test_clock_ms(value: u64) {
    TEST_CLOCK_MS.with(|clock| clock.set(value));
}

fn stepping_clock_ms() -> u64 {
    TEST_CLOCK_MS.with(|clock| {
        let current = clock.get();
        clock.set(current + 10);
        current
    })
}

fn reset_manual_clock_ms(value: u64) {
    MANUAL_CLOCK_MS.with(|clock| clock.set(value));
}

fn manual_clock_ms() -> u64 {
    MANUAL_CLOCK_MS.with(|clock| clock.get())
}

#[test]
fn resilience_config_is_applied_at_construction() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::new(),
        sent_frames,
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        timeouts: TimeoutConfig {
            app_callback_ms: 11,
            send_ms: 22,
            response_retry_interval_ms: 44,
            request_deadline_ms: 33,
            strict_mode: true,
            overflow_policy: OverflowPolicy::DropResponse,
        },
        clock_fn: Some(stepping_clock_ms),
        max_send_retries: 4,
        enable_priority_queue: true,
        enable_broadcast_writes: false,
    };

    let server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        ProbeApp::default(),
        tcp_config(),
        unit_id(1),
        resilience,
    );

    assert_eq!(server.resilience().timeouts.app_callback_ms, 11);
    assert_eq!(server.resilience().timeouts.send_ms, 22);
    assert_eq!(server.resilience().timeouts.response_retry_interval_ms, 44);
    assert_eq!(server.resilience().timeouts.request_deadline_ms, 33);
    assert!(server.resilience().timeouts.strict_mode);
    assert_eq!(server.resilience().max_send_retries, 4);
    assert!(server.resilience().enable_priority_queue);
    assert!(!server.resilience().enable_broadcast_writes);
    assert_eq!(server.pending_request_count(), 0);
    assert_eq!(server.pending_response_count(), 0);
}

#[test]
fn priority_queue_dispatches_write_before_read() {
    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let fc03 = build_fc03_read_request(0x1001);
    let fc06 = build_fc06_write_request(0x1002);
    combined
        .extend_from_slice(fc03.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(fc06.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let call_order_ref = Arc::clone(&app.call_order);

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        enable_priority_queue: true,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), resilience);

    server.poll();

    let call_order = call_order_ref.lock().expect("call order mutex poisoned");
    assert_eq!(call_order.as_slice(), &[6, 3]);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(sent.len(), 2, "both requests should be responded to");
    assert_eq!(
        txn_id_from_adu(&sent[0]),
        0x1002,
        "FC06 should respond first"
    );
    assert_eq!(
        txn_id_from_adu(&sent[1]),
        0x1001,
        "FC03 should respond second"
    );
}

#[test]
fn queued_request_expires_when_deadline_is_exceeded() {
    reset_test_clock_ms(0);

    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let fc03_a = build_fc03_read_request(0x2001);
    let fc03_b = build_fc03_read_request(0x2002);
    combined
        .extend_from_slice(fc03_a.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(fc03_b.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc03_count = Arc::clone(&app.fc03_calls);

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        enable_priority_queue: true,
        timeouts: TimeoutConfig {
            request_deadline_ms: 5,
            ..TimeoutConfig::default()
        },
        clock_fn: Some(stepping_clock_ms),
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), resilience);

    server.poll();

    assert_eq!(fc03_count.load(Ordering::SeqCst), 0);
    assert_eq!(server.pending_request_count(), 0);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0,
        "expired queued requests should not produce responses"
    );
}

#[test]
fn failed_send_is_retried_on_next_poll() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([build_fc06_write_request(0x3001)]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(1)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        max_send_retries: 2,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        ProbeApp::default(),
        tcp_config(),
        unit_id(1),
        resilience,
    );

    // First poll: request is processed, initial send fails, response is queued.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );

    // Second poll: queued response is retried and sent successfully.
    server.poll();
    assert_eq!(server.pending_response_count(), 0);

    #[cfg(feature = "traffic")]
    {
        assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 1);
        assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 1);
        assert_eq!(server.app().traffic_tx_frames.load(Ordering::SeqCst), 1);
    }

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(sent.len(), 1, "queued response should be sent on retry");
    assert_eq!(txn_id_from_adu(&sent[0]), 0x3001);
}

#[test]
fn queued_response_retry_waits_for_configured_interval() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([build_fc06_write_request(0x3002)]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(1)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        max_send_retries: 2,
        timeouts: TimeoutConfig {
            response_retry_interval_ms: 50,
            ..TimeoutConfig::default()
        },
        clock_fn: Some(manual_clock_ms),
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        ProbeApp::default(),
        tcp_config(),
        unit_id(1),
        resilience,
    );

    // Poll #1: request is processed, immediate send fails, response is queued.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );

    // Poll #2 at t=0: retry is not due yet.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );

    // Poll #3 at t=49ms: still not due.
    reset_manual_clock_ms(49);
    server.poll();
    assert_eq!(server.pending_response_count(), 1);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );

    // Poll #4 at t=50ms: retry becomes due and should succeed.
    reset_manual_clock_ms(50);
    server.poll();
    assert_eq!(server.pending_response_count(), 0);
    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(txn_id_from_adu(&sent[0]), 0x3002);
}

#[test]
fn retry_budget_zero_drops_queued_response_without_retry() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([build_fc06_write_request(0x3101)]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(1)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        max_send_retries: 0,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        ProbeApp::default(),
        tcp_config(),
        unit_id(1),
        resilience,
    );

    // Poll #1 queues failed response.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);

    // Poll #2 drops queued response immediately because retry budget is zero.
    server.poll();
    assert_eq!(server.pending_response_count(), 0);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0,
        "no send attempt should succeed when retry budget is zero"
    );
}

#[test]
fn queued_response_is_dropped_after_retry_budget_is_exhausted() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([build_fc06_write_request(0x3201)]),
        sent_frames: Arc::clone(&sent_frames),
        // First send during request handling fails, then first retry fails.
        send_failures_remaining: Arc::new(AtomicUsize::new(2)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        max_send_retries: 1,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        ProbeApp::default(),
        tcp_config(),
        unit_id(1),
        resilience,
    );

    // Poll #1 queues failed response.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);

    // Poll #2 attempts one retry and fails again, response remains queued.
    server.poll();
    assert_eq!(server.pending_response_count(), 1);

    // Poll #3 sees retry_count >= max_send_retries and drops it.
    server.poll();
    assert_eq!(server.pending_response_count(), 0);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );
}

#[test]
fn response_queue_full_drops_additional_failed_responses() {
    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let a = build_fc06_write_request(0x3301);
    let b = build_fc06_write_request(0x3302);
    combined
        .extend_from_slice(a.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(b.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        // Both immediate sends fail.
        send_failures_remaining: Arc::new(AtomicUsize::new(2)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        max_send_retries: 3,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp, 1> =
        ServerServices::with_queue_depth(
            transport,
            ProbeApp::default(),
            tcp_config(),
            unit_id(1),
            resilience,
        );

    server.poll();

    // First failed response is queued; second failed response is dropped due to full queue.
    assert_eq!(server.pending_response_count(), 1);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        0
    );

    #[cfg(feature = "traffic")]
    {
        assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 2);
        assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 2);
        assert_eq!(server.app().traffic_tx_frames.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn request_queue_full_falls_back_to_immediate_dispatch() {
    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let low_priority_fc03 = build_fc03_read_request(0x3401);
    let high_priority_fc06 = build_fc06_write_request(0x3402);
    combined
        .extend_from_slice(low_priority_fc03.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(high_priority_fc06.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let order_ref = Arc::clone(&app.call_order);

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        enable_priority_queue: true,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp, 1> =
        ServerServices::with_queue_depth(transport, app, tcp_config(), unit_id(1), resilience);

    server.poll();

    // FC03 occupies the only queue slot; FC06 is dispatched immediately on queue-full fallback.
    let order = order_ref.lock().expect("call_order mutex poisoned");
    assert_eq!(order.as_slice(), &[6, 3]);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(sent.len(), 2);
    assert_eq!(txn_id_from_adu(&sent[0]), 0x3402);
    assert_eq!(txn_id_from_adu(&sent[1]), 0x3401);
}

#[test]
fn strict_mode_expiry_sends_exception_responses() {
    reset_test_clock_ms(0);

    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let fc03_a = build_fc03_read_request(0x3501);
    let fc03_b = build_fc03_read_request(0x3502);
    combined
        .extend_from_slice(fc03_a.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(fc03_b.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        enable_priority_queue: true,
        timeouts: TimeoutConfig {
            request_deadline_ms: 5,
            strict_mode: true,
            ..TimeoutConfig::default()
        },
        clock_fn: Some(stepping_clock_ms),
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), resilience);

    server.poll();

    assert_eq!(server.pending_request_count(), 0);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(
        sent.len(),
        2,
        "strict mode should emit exception for each stale request"
    );

    let mut txn_ids = vec![txn_id_from_adu(&sent[0]), txn_id_from_adu(&sent[1])];
    txn_ids.sort_unstable();
    assert_eq!(txn_ids, vec![0x3501, 0x3502]);

    for frame in sent.iter() {
        assert!(
            frame.len() >= 9,
            "exception ADU must contain MBAP + FC + EX"
        );
        assert_eq!(
            frame[7], 0x83,
            "expired FC03 should respond with FC03 exception code"
        );
        assert_eq!(
            frame[8],
            ExceptionCode::ServerDeviceFailure as u8,
            "timeout expiry should map to ServerDeviceFailure exception code"
        );
    }

    #[cfg(feature = "traffic")]
    {
        assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 0);
        assert_eq!(server.app().traffic_rx_errors.load(Ordering::SeqCst), 2);
        assert_eq!(server.app().traffic_tx_frames.load(Ordering::SeqCst), 2);
    }
}

#[cfg(feature = "traffic")]
#[test]
fn traffic_callbacks_emit_for_successful_request_and_response() {
    let request = build_fc03_read_request(0x4401);
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([request]),
        sent_frames: Arc::new(Mutex::new(Vec::new())),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let app = ProbeApp::default();
    let server_resilience = ResilienceConfig::default();

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), server_resilience);

    server.poll();

    assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 1);
    assert_eq!(server.app().traffic_tx_frames.load(Ordering::SeqCst), 1);
    assert_eq!(server.app().traffic_rx_errors.load(Ordering::SeqCst), 0);
    assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 0);
}

#[cfg(feature = "traffic")]
#[test]
fn traffic_callbacks_emit_for_exception_and_send_failure() {
    let request = build_fc03_invalid_quantity_request(0x4402);
    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([request]),
        sent_frames: Arc::new(Mutex::new(Vec::new())),
        send_failures_remaining: Arc::new(AtomicUsize::new(1)),
        connected: true,
    };

    let app = ProbeApp::default();
    let server_resilience = ResilienceConfig::default();

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), server_resilience);

    server.poll();

    assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 1);
    assert_eq!(server.app().traffic_rx_errors.load(Ordering::SeqCst), 1);
    assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 1);
}

#[test]
fn deadline_checks_are_inert_without_clock_function() {
    let mut combined = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    let fc03_a = build_fc03_read_request(0x3601);
    let fc03_b = build_fc03_read_request(0x3602);
    combined
        .extend_from_slice(fc03_a.as_slice())
        .expect("first frame should fit");
    combined
        .extend_from_slice(fc03_b.as_slice())
        .expect("second frame should fit");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc03_count = Arc::clone(&app.fc03_calls);

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([combined]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let resilience = ResilienceConfig {
        enable_priority_queue: true,
        timeouts: TimeoutConfig {
            request_deadline_ms: 1,
            ..TimeoutConfig::default()
        },
        clock_fn: None,
        ..ResilienceConfig::default()
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> =
        ServerServices::new(transport, app, tcp_config(), unit_id(1), resilience);

    server.poll();

    assert_eq!(fc03_count.load(Ordering::SeqCst), 2);
    assert_eq!(server.pending_request_count(), 0);
    assert_eq!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .len(),
        2
    );
}

#[test]
fn parser_resync_recovers_from_garbage_prefix() {
    let mut prefixed = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    // Invalid MBAP prefix with oversized length to force parse/resync.
    let garbage_prefix = [0xAA, 0xBB, 0xCC, 0xDD, 0xFF, 0xFF, 0x99];
    prefixed
        .extend_from_slice(&garbage_prefix)
        .expect("garbage prefix should fit");
    let valid = build_fc06_write_request(0x3701);
    prefixed
        .extend_from_slice(valid.as_slice())
        .expect("valid frame should fit after prefix");

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc06_count = Arc::clone(&app.fc06_calls);

    let transport = ScriptedTransport {
        recv_queue: VecDeque::from([prefixed]),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    server.poll();

    assert_eq!(fc06_count.load(Ordering::SeqCst), 1);
    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(sent.len(), 1);
    assert_eq!(txn_id_from_adu(&sent[0]), 0x3701);
}

#[test]
fn metrics_track_dropped_responses_on_queue_overflow() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();

    // Create 3 requests with send failures to test queue overflow
    let mut recv_queue = VecDeque::new();
    for i in 0..3 {
        recv_queue.push_back(build_fc06_write_request(0x0001 + i as u16));
    }

    let transport = ScriptedTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(3)), // Fail all 3 sends
        connected: true,
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp, 1> = // Queue depth of 1
        ServerServices::with_queue_depth(
            transport,
            app,
            tcp_config(),
            unit_id(1),
            ResilienceConfig {
                timeouts: TimeoutConfig {
                    app_callback_ms: 0,
                    send_ms: 0,
                    response_retry_interval_ms: 0,
                    request_deadline_ms: 0,
                    strict_mode: false,
                    overflow_policy: OverflowPolicy::DropResponse,
                },
                clock_fn: None,
                max_send_retries: 3,
                enable_priority_queue: false, // Direct dispatch, not queued
                enable_broadcast_writes: false,
            },
        );

    // Each poll processes one request
    // Req 1: success, response sent ok, nothing queued
    // But requests will have send failures, so:
    // Req 1: FC06 succeeds, send the response -> fails, gets queued (1/1)
    // Req 2: FC06 succeeds, send the response -> fails, DROPPED (queue full)
    // Req 3: FC06 succeeds, send the response -> fails, DROPPED (queue full)
    for _ in 0..3 {
        server.poll();
    }

    // Verify metrics: 1 response dropped (req 2 and/or 3)
    assert_eq!(
        server.dropped_response_count(),
        1,
        "Expected 1 dropped response"
    );
    assert_eq!(
        server.peak_response_queue_size(),
        1,
        "Queue never exceeds 1"
    );
}

#[test]
fn back_pressure_metrics_initialized() {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();

    let transport = ScriptedTransport {
        recv_queue: VecDeque::new(),
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(0)),
        connected: true,
    };

    let server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    // Verify metrics start at zero
    assert_eq!(server.dropped_response_count(), 0);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(server.peak_response_queue_size(), 0);
}

#[test]
fn addressed_unicast_request_is_rejected_with_exception_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_fc06_write_request(txn_id));
    }
    recv_queue.push_back(build_fc06_write_request(8));

    let transport = ScriptedTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: false,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 1);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(
        sent.len(),
        1,
        "rejected addressed request should emit one exception response"
    );
    assert_eq!(txn_id_from_adu(&sent[0]), 8);
    assert!(
        sent[0][7] & 0x80 != 0,
        "rejected request should use exception function code"
    );
    assert_eq!(
        sent[0][7], 0x86,
        "FC06 rejection should emit FC06 exception response"
    );
    assert_eq!(
        sent[0][8],
        ExceptionCode::ServerDeviceFailure as u8,
        "TooManyRequests currently maps to ServerDeviceFailure"
    );
}

#[test]
fn misaddressed_frame_is_silently_dropped_even_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_fc06_write_request(txn_id));
    }
    recv_queue.push_back(build_fc06_write_request_for_unit(8, 2));

    let transport = ScriptedTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: false,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert!(
        sent.is_empty(),
        "misaddressed frames must not generate a response under back-pressure"
    );

    #[cfg(feature = "traffic")]
    {
        assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 7);
        assert_eq!(server.app().traffic_rx_errors.load(Ordering::SeqCst), 0);
        assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 7);
    }
}

#[test]
fn broadcast_frame_is_silently_dropped_even_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_fc06_write_request(txn_id));
    }
    recv_queue.push_back(build_fc06_write_request_for_unit(8, 0));

    let transport = ScriptedTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: true,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert!(
        sent.is_empty(),
        "broadcast must not generate a response under back-pressure"
    );

    #[cfg(feature = "traffic")]
    {
        assert_eq!(server.app().traffic_rx_frames.load(Ordering::SeqCst), 7);
        assert_eq!(server.app().traffic_rx_errors.load(Ordering::SeqCst), 0);
        assert_eq!(server.app().traffic_tx_errors.load(Ordering::SeqCst), 7);
    }
}

#[test]
fn serial_broadcast_write_is_applied_without_response_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_serial_fc06_write_request_for_unit(txn_id, 1));
    }
    recv_queue.push_back(build_serial_fc06_write_request_for_unit(8, 0));

    let transport = ScriptedSerialTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedSerialTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: true,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 8);

    let sent = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert!(
        sent.is_empty(),
        "serial broadcast write must not generate any response under back-pressure"
    );
}

#[cfg(feature = "coils")]
#[test]
fn serial_broadcast_write_single_coil_is_applied_without_response_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc05_count = Arc::clone(&app.fc05_calls);
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_serial_fc06_write_request_for_unit(txn_id, 1));
    }
    recv_queue.push_back(build_serial_fc05_write_request_for_unit(8, 0));

    let transport = ScriptedSerialTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedSerialTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: true,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);
    assert_eq!(fc05_count.load(Ordering::SeqCst), 1);
    assert!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .is_empty(),
        "serial broadcast FC05 must not generate a response"
    );
}

#[cfg(feature = "coils")]
#[test]
fn serial_broadcast_write_multiple_coils_is_applied_without_response_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc0f_count = Arc::clone(&app.fc0f_calls);
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_serial_fc06_write_request_for_unit(txn_id, 1));
    }
    recv_queue.push_back(build_serial_fc0f_write_request_for_unit(8, 0));

    let transport = ScriptedSerialTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedSerialTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: true,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);
    assert_eq!(fc0f_count.load(Ordering::SeqCst), 1);
    assert!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .is_empty(),
        "serial broadcast FC0F must not generate a response"
    );
}

#[test]
fn serial_broadcast_write_multiple_registers_is_applied_without_response_under_back_pressure() {
    reset_manual_clock_ms(0);

    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app = ProbeApp::default();
    let fc10_count = Arc::clone(&app.fc10_calls);
    let fc06_count = Arc::clone(&app.fc06_calls);

    let mut recv_queue = VecDeque::new();
    for txn_id in 1..=7u16 {
        recv_queue.push_back(build_serial_fc06_write_request_for_unit(txn_id, 1));
    }
    recv_queue.push_back(build_serial_fc10_write_request_for_unit(8, 0));

    let transport = ScriptedSerialTransport {
        recv_queue,
        sent_frames: Arc::clone(&sent_frames),
        send_failures_remaining: Arc::new(AtomicUsize::new(7)),
        connected: true,
    };

    let mut server: ServerServices<ScriptedSerialTransport, ProbeApp> = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig {
            timeouts: TimeoutConfig {
                app_callback_ms: 0,
                send_ms: 0,
                response_retry_interval_ms: 1_000,
                request_deadline_ms: 0,
                strict_mode: false,
                overflow_policy: OverflowPolicy::RejectRequest,
            },
            clock_fn: Some(manual_clock_ms),
            max_send_retries: 3,
            enable_priority_queue: true,
            enable_broadcast_writes: true,
        },
    );

    for _ in 0..7 {
        server.poll();
    }

    server.poll();

    assert_eq!(server.pending_response_count(), 7);
    assert_eq!(server.rejected_request_count(), 0);
    assert_eq!(fc06_count.load(Ordering::SeqCst), 7);
    assert_eq!(fc10_count.load(Ordering::SeqCst), 1);
    assert!(
        sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .is_empty(),
        "serial broadcast FC10 must not generate a response"
    );
}
