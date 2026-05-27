//! Integration tests for the synchronous, non-blocking `GatewayServices` using mock transports.

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{
    MAX_ADU_FRAME_LEN, Pdu, compile_adu_frame, decompile_adu_frame,
};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use mbus_gateway::{
    DownstreamChannel, GatewayEventHandler, GatewayServices, NoopEventHandler, PassthroughRouter,
    PollOutcome, UnitRouteTable,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock Transport
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory transport for testing.
#[derive(Clone)]
struct MockTransport {
    /// The next frame to return from `recv()`.
    next_rx: Option<HVec<u8, MAX_ADU_FRAME_LEN>>,
    /// All frames captured by `send()`.
    sent: std::vec::Vec<std::vec::Vec<u8>>,
    connected: bool,
}

impl MockTransport {
    fn tcp() -> Self {
        Self {
            next_rx: None,
            sent: std::vec::Vec::new(),
            connected: true,
        }
    }

    fn with_rx(mut self, frame: HVec<u8, MAX_ADU_FRAME_LEN>) -> Self {
        self.next_rx = Some(frame);
        self
    }

    fn enqueue(&mut self, frame: HVec<u8, MAX_ADU_FRAME_LEN>) {
        self.next_rx = Some(frame);
    }
}

impl Transport for MockTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn connect(&mut self, _cfg: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }
    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        self.sent.push(adu.to_vec());
        Ok(())
    }
    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.next_rx.take().ok_or(TransportError::Timeout)
    }
    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn uid(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new(v).unwrap()
}

fn build_tcp_request(
    txn_id: u16,
    unit: u8,
    fc: FunctionCode,
    payload: &[u8],
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::new(fc, HVec::from_slice(payload).unwrap(), payload.len() as u8);
    compile_adu_frame(txn_id, unit, pdu, TransportType::StdTcp).unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Basic round-trip: upstream sends a request, gateway forwards to downstream on poll 1,
/// downstream replies, gateway sends response back upstream on poll 2.
#[test]
fn gateway_forwards_request_and_returns_response() {
    let request_adu = build_tcp_request(
        0x0001, // txn_id
        1,      // unit
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x08], // address=0, quantity=8
    );

    let response_adu = build_tcp_request(
        0x0000, // downstream txn (gateway assigns 0)
        1,      // unit
        FunctionCode::ReadCoils,
        &[0x01, 0xFF], // byte_count=1, coil_data=0xFF
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp().with_rx(response_adu);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _> =
        GatewayServices::new(router, NoopEventHandler, 1000);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(downstream))
        .unwrap();

    // Poll 1: drains upstream, sends request to downstream, transitions channel 0 to AwaitingResponse
    let outcome = gw.poll(0);
    assert_eq!(outcome, PollOutcome::Active);

    // Verify request was sent downstream with internal txn id = 0
    let ds_sent = gw.downstream(0).unwrap().transport().sent.clone();
    assert_eq!(ds_sent.len(), 1);
    let ds_msg = decompile_adu_frame(&ds_sent[0], TransportType::StdTcp).unwrap();
    assert_eq!(ds_msg.transaction_id(), 0);

    // Poll 2: drains downstream, receives response, forwards back to upstream, transitions channel 0 back to Idle
    let outcome = gw.poll(10);
    assert_eq!(outcome, PollOutcome::Active);

    // Verify response was returned upstream with original txn id = 0x0001
    let upstream_sent = gw.upstream(0).unwrap().transport().sent.clone();
    assert_eq!(upstream_sent.len(), 1);
    let upstream_response = decompile_adu_frame(&upstream_sent[0], TransportType::StdTcp).unwrap();
    assert_eq!(upstream_response.transaction_id(), 0x0001);
}

/// When no data is available on the upstream transport, `poll()` returns `PollOutcome::Idle`
/// without touching the downstream.
#[test]
fn gateway_poll_returns_idle_when_no_upstream_data() {
    let upstream = MockTransport::tcp();
    let downstream = MockTransport::tcp();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _> =
        GatewayServices::new(PassthroughRouter, NoopEventHandler, 1000);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(downstream))
        .unwrap();

    let outcome = gw.poll(0);
    assert_eq!(outcome, PollOutcome::Idle);
    assert!(gw.downstream(0).unwrap().transport().sent.is_empty());
}

/// When the router has no route for the incoming unit ID, the gateway should
/// send a Modbus exception response upstream immediately.
#[test]
fn gateway_sends_exception_on_routing_miss() {
    let request_adu = build_tcp_request(
        0x000F,
        42, // unit 42 — no route configured
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x01],
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let router: UnitRouteTable<4> = UnitRouteTable::new();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _> =
        GatewayServices::new(router, NoopEventHandler, 1000);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp()))
        .unwrap();

    let outcome = gw.poll(0);
    assert_eq!(outcome, PollOutcome::Active);

    // Exception should have been sent upstream
    let upstream_sent = gw.upstream(0).unwrap().transport().sent.clone();
    assert_eq!(upstream_sent.len(), 1);
    let exception_adu = decompile_adu_frame(&upstream_sent[0], TransportType::StdTcp).unwrap();
    assert!(exception_adu.pdu.error_code().is_some());
}

/// When the downstream transport does not respond before the deadline,
/// `poll()` should handle the timeout, clean up, and send a GatewayPathUnavailable exception.
#[test]
fn gateway_handles_downstream_timeout() {
    let request_adu = build_tcp_request(
        0x0002,
        1,
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x01],
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp(); // no response loaded

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    #[derive(Default)]
    struct Recorder {
        timeout_count: u32,
    }
    impl GatewayEventHandler for Recorder {
        fn on_downstream_timeout(&mut self, _session_id: u8, _internal_txn: u16) {
            self.timeout_count += 1;
        }
    }

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1, 1, 4, 0> =
        GatewayServices::new(router, Recorder::default(), 500);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(downstream))
        .unwrap();

    // Poll 1: forwards request downstream (deadline = 0 + 500 = 500)
    gw.poll(0);

    // Poll 2: elapsed is 499 (timeout not reached, downstream has no data) -> Idle/no-change
    let outcome = gw.poll(499);
    assert_eq!(outcome, PollOutcome::Idle);
    assert_eq!(gw.event_handler().timeout_count, 0);

    // Poll 3: elapsed is 500 (deadline reached!) -> triggers timeout, fires exception upstream
    let outcome = gw.poll(500);
    assert_eq!(outcome, PollOutcome::Active);
    assert_eq!(gw.event_handler().timeout_count, 1);

    // Verify exception sent upstream
    let upstream_sent = gw.upstream(0).unwrap().transport().sent.clone();
    assert_eq!(upstream_sent.len(), 1);
    let exception_adu = decompile_adu_frame(&upstream_sent[0], TransportType::StdTcp).unwrap();
    assert!(exception_adu.pdu.error_code().is_some());
}

/// Verify that two upstreams can simultaneously connect and run independent sessions.
#[test]
fn gateway_supports_multi_upstream_sessions() {
    let req_1 = build_tcp_request(0x1111, 1, FunctionCode::ReadCoils, &[0, 0, 0, 8]);
    let req_2 = build_tcp_request(0x2222, 2, FunctionCode::ReadCoils, &[0, 0, 0, 8]);

    let upstream_1 = MockTransport::tcp().with_rx(req_1);
    let upstream_2 = MockTransport::tcp().with_rx(req_2);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();
    router.add(uid(2), 1).unwrap();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 2, 2, 4, 0> =
        GatewayServices::new(router, NoopEventHandler, 1000);
    gw.add_upstream(upstream_1).unwrap();
    gw.add_upstream(upstream_2).unwrap();
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp()))
        .unwrap();
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp()))
        .unwrap();

    // Poll 1: both requests received and dispatched
    gw.poll(0);

    assert_eq!(gw.downstream(0).unwrap().transport().sent.len(), 1);
    assert_eq!(gw.downstream(1).unwrap().transport().sent.len(), 1);
}

/// N_PENDING = 0: when all downstreams are busy, incoming request is dropped
/// and `on_downstream_busy(queued=false)` is fired.
#[test]
fn gateway_n_pending_0_drops_on_busy() {
    let req_first = build_tcp_request(0x0001, 1, FunctionCode::ReadCoils, &[0, 0, 0, 8]);
    let req_second = build_tcp_request(0x0002, 1, FunctionCode::ReadCoils, &[0, 0, 0, 8]);

    // Upstream has two requests lined up
    let upstream = MockTransport::tcp().with_rx(req_first);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    #[derive(Default)]
    struct Recorder {
        busy_calls: std::vec::Vec<(u8, bool)>,
    }
    impl GatewayEventHandler for Recorder {
        fn on_downstream_busy(&mut self, session_id: u8, _unit: UnitIdOrSlaveAddr, queued: bool) {
            self.busy_calls.push((session_id, queued));
        }
    }

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1, 1, 4, 0> =
        GatewayServices::new(router, Recorder::default(), 1000);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp()))
        .unwrap();

    // First request processed and dispatched to downstream, channel becomes Busy (AwaitingResponse)
    gw.poll(0);

    // Queue up the second request on the upstream transport
    gw.upstream_mut(0)
        .unwrap()
        .transport_mut()
        .enqueue(req_second);

    // Poll again: downstream is busy, N_PENDING = 0, so second request is dropped
    gw.poll(0);

    assert_eq!(gw.event_handler().busy_calls.len(), 1);
    assert_eq!(gw.event_handler().busy_calls[0], (0, false)); // queued=false
}

/// N_PENDING > 0: when all downstreams are busy, incoming request is queued
/// and then dispatched once the downstream becomes free.
#[test]
fn gateway_n_pending_gt0_queues_on_busy() {
    let req_first = build_tcp_request(0x0001, 1, FunctionCode::ReadCoils, &[0, 0, 0, 8]);
    let req_second = build_tcp_request(0x0002, 1, FunctionCode::ReadCoils, &[0, 0, 0, 8]);

    let resp_first = build_tcp_request(0, 1, FunctionCode::ReadCoils, &[1, 0xFF]);
    let resp_second = build_tcp_request(1, 1, FunctionCode::ReadCoils, &[1, 0xEE]);

    let upstream = MockTransport::tcp().with_rx(req_first);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    #[derive(Default)]
    struct Recorder {
        busy_calls: std::vec::Vec<(u8, bool)>,
    }
    impl GatewayEventHandler for Recorder {
        fn on_downstream_busy(&mut self, session_id: u8, _unit: UnitIdOrSlaveAddr, queued: bool) {
            self.busy_calls.push((session_id, queued));
        }
    }

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1, 1, 4, 2> =
        GatewayServices::new(router, Recorder::default(), 1000);
    gw.add_upstream(upstream).unwrap();
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp()))
        .unwrap();

    // 1. First request processed and dispatched to downstream, channel becomes busy
    gw.poll(0);

    // 2. Queue second request upstream
    gw.upstream_mut(0)
        .unwrap()
        .transport_mut()
        .enqueue(req_second);

    // 3. Poll: downstream busy, request queued in pending queue, busy_calls should show queued=true
    gw.poll(0);
    assert_eq!(gw.event_handler().busy_calls.len(), 1);
    assert_eq!(gw.event_handler().busy_calls[0], (0, true)); // queued=true
    assert_eq!(gw.downstream(0).unwrap().transport().sent.len(), 1); // only 1 sent so far

    // 4. Load the first response onto downstream
    gw.downstream_mut(0)
        .unwrap()
        .transport_mut()
        .enqueue(resp_first);

    // 5. Poll: downstream drains, processes response, completes request 1, goes Idle.
    // Immediately after going Idle, it drains pending queue and dispatches request 2!
    gw.poll(10);
    assert_eq!(gw.downstream(0).unwrap().transport().sent.len(), 2); // request 2 dispatched!

    // 6. Load second response onto downstream
    gw.downstream_mut(0)
        .unwrap()
        .transport_mut()
        .enqueue(resp_second);

    // 7. Poll: completes request 2
    gw.poll(20);

    // Both responses should have been sent upstream
    assert_eq!(gw.upstream(0).unwrap().transport().sent.len(), 2);
}
