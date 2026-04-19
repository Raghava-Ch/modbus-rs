//! Unit-ID / slave-address filtering tests.
//!
//! Verifies that [`ServerServices`] correctly:
//! - Responds to frames addressed to its configured unit ID.
//! - Silently discards frames addressed to a different unit ID (no response sent).
//! - Silently discards broadcast frames (address `0`) without sending a response.
//!
//! Per the Modbus specification a server must **never** respond to a frame that is
//! not addressed to it — doing so would corrupt bus communication.

mod common;
use common::{MockTransport, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, Pdu, compile_adu_frame};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};
use mbus_server::ResilienceConfig;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiagnosticsHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerServices;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// Records how many times the app callback was invoked.
#[derive(Debug)]
struct CountingApp {
    calls: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_rx_frames: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_tx_frames: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_rx_errors: Arc<AtomicUsize>,
    #[cfg(feature = "traffic")]
    traffic_tx_errors: Arc<AtomicUsize>,
}

impl ServerExceptionHandler for CountingApp {}

impl ServerCoilHandler for CountingApp {}

impl ServerDiscreteInputHandler for CountingApp {}

impl ServerFifoHandler for CountingApp {}

impl ServerFileRecordHandler for CountingApp {}

impl ServerDiagnosticsHandler for CountingApp {}

impl ServerInputRegisterHandler for CountingApp {
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

impl ServerHoldingRegisterHandler for CountingApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Write valid big-endian register words so the server can send a success response.
        let byte_count = (quantity * 2) as usize;
        for b in out[..byte_count].iter_mut() {
            *b = 0x00;
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
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _values: &[u16],
    ) -> Result<(), MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for CountingApp {
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

/// Build a TCP FC03 request frame addressed to the given `wire_unit`.
fn build_fc03_request(
    txn_id: u16,
    wire_unit: u8,
    address: u16,
    quantity: u16,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_read_window(FunctionCode::ReadHoldingRegisters, address, quantity)
        .expect("valid FC03 payload");
    compile_adu_frame(txn_id, wire_unit, pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}

/// Build a TCP FC06 write request addressed to the given `wire_unit`.
fn build_fc06_request(
    txn_id: u16,
    wire_unit: u8,
    address: u16,
    value: u16,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, address, value)
        .expect("valid FC06 payload");
    compile_adu_frame(txn_id, wire_unit, pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}

/// Run one request through `ServerServices` configured for `server_unit`.
/// Returns `(app_call_count, sent_frame_count)`.
fn run_request(
    frame: HVec<u8, MAX_ADU_FRAME_LEN>,
    server_unit: UnitIdOrSlaveAddr,
) -> (usize, usize) {
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let calls = Arc::new(AtomicUsize::new(0));

    let transport = MockTransport {
        next_rx: Some(frame),
        sent_frames: sent_frames.clone(),
        connected: true,
    };
    let app = CountingApp {
        calls: calls.clone(),
        #[cfg(feature = "traffic")]
        traffic_rx_frames: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "traffic")]
        traffic_tx_frames: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "traffic")]
        traffic_rx_errors: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "traffic")]
        traffic_tx_errors: Arc::new(AtomicUsize::new(0)),
    };

    let mut server = ServerServices::new(
        transport,
        app,
        tcp_config(),
        server_unit,
        ResilienceConfig::default(),
    );
    server.connect().expect("connect should succeed");
    server.poll();

    let frame_count = sent_frames.lock().expect("mutex poisoned").len();
    (calls.load(Ordering::SeqCst), frame_count)
}

#[cfg(feature = "traffic")]
fn run_request_with_traffic(
    frame: HVec<u8, MAX_ADU_FRAME_LEN>,
    server_unit: UnitIdOrSlaveAddr,
) -> (usize, usize, usize, usize, usize, usize) {
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let traffic_rx_frames = Arc::new(AtomicUsize::new(0));
    let traffic_tx_frames = Arc::new(AtomicUsize::new(0));
    let traffic_rx_errors = Arc::new(AtomicUsize::new(0));
    let traffic_tx_errors = Arc::new(AtomicUsize::new(0));

    let transport = MockTransport {
        next_rx: Some(frame),
        sent_frames: sent_frames.clone(),
        connected: true,
    };
    let app = CountingApp {
        calls: calls.clone(),
        traffic_rx_frames: traffic_rx_frames.clone(),
        traffic_tx_frames: traffic_tx_frames.clone(),
        traffic_rx_errors: traffic_rx_errors.clone(),
        traffic_tx_errors: traffic_tx_errors.clone(),
    };

    let mut server = ServerServices::new(
        transport,
        app,
        tcp_config(),
        server_unit,
        ResilienceConfig::default(),
    );
    server.connect().expect("connect should succeed");
    server.poll();

    let frame_count = sent_frames.lock().expect("mutex poisoned").len();
    (
        calls.load(Ordering::SeqCst),
        frame_count,
        traffic_rx_frames.load(Ordering::SeqCst),
        traffic_tx_frames.load(Ordering::SeqCst),
        traffic_rx_errors.load(Ordering::SeqCst),
        traffic_tx_errors.load(Ordering::SeqCst),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A frame addressed to the server's own unit ID must be processed and generate
/// exactly one response frame.
#[test]
fn matching_unit_id_is_processed_and_responded_to() {
    let server_unit = unit_id(1);
    let frame = build_fc03_request(1, 1, 0, 1); // wire unit = 1, server unit = 1

    let (app_calls, response_count) = run_request(frame, server_unit);

    assert_eq!(app_calls, 1, "app callback must be invoked exactly once");
    assert_eq!(
        response_count, 1,
        "server must send exactly one response frame"
    );
}

/// A frame addressed to a different unicast unit ID must be silently discarded.
/// The Modbus spec forbids a server from responding to frames not addressed to it.
#[test]
fn mismatched_unit_id_is_silently_dropped_with_no_response() {
    let server_unit = unit_id(1);
    let frame = build_fc03_request(2, 5, 0, 1); // wire unit = 5, server unit = 1

    let (app_calls, response_count) = run_request(frame, server_unit);

    assert_eq!(
        app_calls, 0,
        "app callback must NOT be invoked for misaddressed frame"
    );
    assert_eq!(
        response_count, 0,
        "server must NOT send any response for a misaddressed frame"
    );
}

/// A variety of mismatched unit IDs must all be silently dropped.
#[test]
fn various_mismatched_unit_ids_are_all_silently_dropped() {
    let server_unit = unit_id(10);

    for wire_unit in [1u8, 2, 9, 11, 50, 100, 247] {
        let frame = build_fc03_request(3, wire_unit, 0, 1);
        let (app_calls, response_count) = run_request(frame, server_unit);

        assert_eq!(
            app_calls, 0,
            "app must not be called for wire_unit={wire_unit} (server is unit 10)"
        );
        assert_eq!(
            response_count, 0,
            "no response must be sent for wire_unit={wire_unit} (server is unit 10)"
        );
    }
}

/// A broadcast frame (address 0) must be silently discarded — no app callback,
/// no response frame. Full broadcast write forwarding for Serial is not yet
/// implemented; broadcast frames are discarded to avoid accidental responses.
#[test]
fn broadcast_frame_is_silently_dropped_with_no_response() {
    let server_unit = unit_id(1);
    // Wire unit = 0 (broadcast). FC06 is a write FC — it would be the candidate for
    // broadcast forwarding in a Serial context, but must still be dropped until
    // the feature is implemented.
    let frame = build_fc06_request(4, 0, 0, 0xABCD);

    let (app_calls, response_count) = run_request(frame, server_unit);

    assert_eq!(app_calls, 0, "broadcast must not invoke the app callback");
    assert_eq!(
        response_count, 0,
        "broadcast must never generate a response frame"
    );
}

/// Back-to-back polling: a misaddressed frame followed by a correctly addressed
/// frame. The second frame must be answered normally.
#[test]
fn misaddressed_frame_does_not_corrupt_server_state_for_next_request() {
    let server_unit = unit_id(3);
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let calls = Arc::new(AtomicUsize::new(0));

    // Use a two-poll sequence by injecting frames manually.
    let wrong_frame = build_fc03_request(10, 7, 0, 1); // wrong unit
    let right_frame = build_fc03_request(11, 3, 0, 1); // correct unit

    // First poll — misaddressed frame.
    {
        let transport = MockTransport {
            next_rx: Some(wrong_frame),
            sent_frames: sent_frames.clone(),
            connected: true,
        };
        let app = CountingApp {
            calls: calls.clone(),
            #[cfg(feature = "traffic")]
            traffic_rx_frames: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_tx_frames: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_rx_errors: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_tx_errors: Arc::new(AtomicUsize::new(0)),
        };
        let mut server = ServerServices::new(
            transport,
            app,
            tcp_config(),
            server_unit,
            ResilienceConfig::default(),
        );
        server.connect().expect("connect should succeed");
        server.poll();
    }

    let frames_after_wrong = sent_frames.lock().expect("mutex poisoned").len();
    assert_eq!(
        frames_after_wrong, 0,
        "misaddressed frame must produce no response"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app must not be called for wrong frame"
    );

    // Second poll — correctly addressed frame.
    {
        let transport = MockTransport {
            next_rx: Some(right_frame),
            sent_frames: sent_frames.clone(),
            connected: true,
        };
        let app = CountingApp {
            calls: calls.clone(),
            #[cfg(feature = "traffic")]
            traffic_rx_frames: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_tx_frames: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_rx_errors: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "traffic")]
            traffic_tx_errors: Arc::new(AtomicUsize::new(0)),
        };
        let mut server = ServerServices::new(
            transport,
            app,
            tcp_config(),
            server_unit,
            ResilienceConfig::default(),
        );
        server.connect().expect("connect should succeed");
        server.poll();
    }

    let frames_after_right = sent_frames.lock().expect("mutex poisoned").len();
    assert_eq!(
        frames_after_right, 1,
        "correctly addressed frame must produce one response"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "app must be called exactly once for correct frame"
    );
}

#[cfg(feature = "traffic")]
#[test]
fn traffic_callbacks_for_address_filtering_behave_as_expected() {
    let matched = build_fc03_request(100, 1, 0, 1);
    let (calls, responses, rx_frames, tx_frames, rx_errors, tx_errors) =
        run_request_with_traffic(matched, unit_id(1));
    assert_eq!(calls, 1);
    assert_eq!(responses, 1);
    assert_eq!(rx_frames, 1);
    assert_eq!(tx_frames, 1);
    assert_eq!(rx_errors, 0);
    assert_eq!(tx_errors, 0);

    let misaddressed = build_fc03_request(101, 9, 0, 1);
    let (calls, responses, rx_frames, tx_frames, rx_errors, tx_errors) =
        run_request_with_traffic(misaddressed, unit_id(1));
    assert_eq!(calls, 0);
    assert_eq!(responses, 0);
    assert_eq!(rx_frames, 0);
    assert_eq!(tx_frames, 0);
    assert_eq!(rx_errors, 0);
    assert_eq!(tx_errors, 0);

    let broadcast = build_fc06_request(102, 0, 0, 0xABCD);
    let (calls, responses, rx_frames, tx_frames, rx_errors, tx_errors) =
        run_request_with_traffic(broadcast, unit_id(1));
    assert_eq!(calls, 0);
    assert_eq!(responses, 0);
    assert_eq!(rx_frames, 0);
    assert_eq!(tx_frames, 0);
    assert_eq!(rx_errors, 0);
    assert_eq!(tx_errors, 0);
}
