//! Integration tests for the synchronous `GatewayServices` using mock transports.

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{compile_adu_frame, decompile_adu_frame, MAX_ADU_FRAME_LEN, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use mbus_gateway::{
    DownstreamChannel, GatewayEventHandler, GatewayServices, NoopEventHandler, PassthroughRouter,
    UnitRouteTable,
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock Transport
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory transport for testing.
struct MockTransport {
    /// The next frame to return from `recv()`.
    next_rx: Option<HVec<u8, MAX_ADU_FRAME_LEN>>,
    /// All frames captured by `send()`.
    sent: std::vec::Vec<std::vec::Vec<u8>>,
    transport_type: TransportType,
    connected: bool,
}

impl MockTransport {
    fn tcp() -> Self {
        Self {
            next_rx: None,
            sent: std::vec::Vec::new(),
            transport_type: TransportType::StdTcp,
            connected: true,
        }
    }

    fn with_rx(mut self, frame: HVec<u8, MAX_ADU_FRAME_LEN>) -> Self {
        self.next_rx = Some(frame);
        self
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
// Helper: build a Modbus TCP ADU
// ─────────────────────────────────────────────────────────────────────────────

fn uid(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new(v).unwrap()
}

fn build_tcp_request(txn_id: u16, unit: u8, fc: FunctionCode, payload: &[u8]) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::new(
        fc,
        HVec::from_slice(payload).unwrap(),
        payload.len() as u8,
    );
    compile_adu_frame(txn_id, unit, pdu, TransportType::StdTcp).unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Basic round-trip: upstream sends a request, gateway forwards to downstream,
/// downstream replies, gateway sends response back upstream.
#[test]
fn gateway_forwards_request_and_returns_response() {
    // Build a read-coils request (FC01) from an upstream client.
    let request_adu = build_tcp_request(
        0x0001,        // txn_id
        1,             // unit
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x08], // address=0, quantity=8
    );

    // Build a response that the downstream device would return.
    let response_adu = build_tcp_request(
        0x0000,              // downstream txn (gateway assigns 0)
        1,                   // unit
        FunctionCode::ReadCoils,
        &[0x01, 0xFF],       // byte_count=1, coil_data=0xFF
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp().with_rx(response_adu);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, router, NoopEventHandler);
    gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();

    gw.poll().expect("poll should succeed");

    // The upstream transport should have received one response frame.
    let upstream_sent = gw.upstream().sent.clone();
    assert_eq!(upstream_sent.len(), 1, "gateway should have sent one response upstream");

    // Parse the upstream response and verify the txn_id was restored.
    let upstream_response =
        decompile_adu_frame(&upstream_sent[0], TransportType::StdTcp).unwrap();
    assert_eq!(upstream_response.transaction_id(), 0x0001,
        "original upstream txn_id should be restored in the response");
    assert_eq!(upstream_response.unit_id_or_slave_addr().get(), 1);
}

/// When no data is available on the upstream transport, `poll()` returns `Ok(())`
/// without touching the downstream.
#[test]
fn gateway_poll_returns_ok_when_no_upstream_data() {
    let upstream = MockTransport::tcp(); // no next_rx → Timeout
    let downstream = MockTransport::tcp();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, PassthroughRouter, NoopEventHandler);
    gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();

    let result = gw.poll();
    assert!(result.is_ok(), "poll with no upstream data should return Ok(())");
    // No frames should have been sent to downstream.
    assert!(gw.downstream(0).unwrap().transport().sent.is_empty());
}

/// When the router has no route for the incoming unit ID, the gateway should
/// send a Modbus exception response upstream and return `Ok(())`.
#[test]
fn gateway_sends_exception_on_routing_miss() {
    let request_adu = build_tcp_request(
        0x000F,
        42,  // unit 42 — no route configured
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x01],
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);

    // Empty routing table → any unit will miss.
    let router: UnitRouteTable<4> = UnitRouteTable::new();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, router, NoopEventHandler);
    // Add a downstream just in case (shouldn't be touched).
    gw.add_downstream(DownstreamChannel::new(MockTransport::tcp())).unwrap();

    gw.poll().expect("routing miss should return Ok(())");

    // An exception should have been sent upstream.
    let upstream_sent = gw.upstream().sent.clone();
    assert_eq!(upstream_sent.len(), 1, "gateway should have sent an exception upstream");

    // The parsed PDU should carry an error_code (Modbus exception response).
    let exception_adu =
        decompile_adu_frame(&upstream_sent[0], TransportType::StdTcp).unwrap();
    assert!(
        exception_adu.pdu.error_code().is_some(),
        "exception PDU should carry an error code"
    );
}

/// When the downstream transport returns `Timeout`, `poll()` should propagate
/// `Err(MbusError::Timeout)` and fire the `on_downstream_timeout` callback.
#[test]
fn gateway_propagates_downstream_timeout() {
    let request_adu = build_tcp_request(
        0x0002,
        1,
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x01],
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp(); // no next_rx → will always Timeout

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(1), 0).unwrap();

    // Use a custom event handler to verify the callback fires.
    struct Recorder { timeout_count: u32 }
    impl GatewayEventHandler for Recorder {
        fn on_downstream_timeout(&mut self, _session_id: u8, _internal_txn: u16) {
            self.timeout_count += 1;
        }
    }

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, router, Recorder { timeout_count: 0 });
    gw.set_max_downstream_recv_attempts(3); // speed up the test
    gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();

    let result = gw.poll();
    assert!(
        matches!(result, Err(MbusError::Timeout)),
        "should return Err(Timeout)"
    );
    assert_eq!(gw.event_handler().timeout_count, 1, "timeout callback should fire once");
}

/// Verify that the downstream transport receives the forwarded frame with the
/// internal (gateway-assigned) transaction ID, not the original upstream one.
#[test]
fn gateway_rewrites_txn_id_for_downstream() {
    let request_adu = build_tcp_request(
        0xABCD,  // upstream txn
        5,
        FunctionCode::ReadHoldingRegisters,
        &[0x00, 0x10, 0x00, 0x02],
    );

    let response_adu = build_tcp_request(
        0x0000,  // downstream internal txn 0
        5,
        FunctionCode::ReadHoldingRegisters,
        &[0x04, 0x00, 0x01, 0x00, 0x02], // byte_count=4, regs=[1, 2]
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp().with_rx(response_adu);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(5), 0).unwrap();

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, router, NoopEventHandler);
    gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();

    gw.poll().unwrap();

    // The downstream should have received a frame with internal txn id 0 (not 0xABCD).
    let ds_sent = gw.downstream(0).unwrap().transport().sent.clone();
    assert_eq!(ds_sent.len(), 1);
    let ds_msg = decompile_adu_frame(&ds_sent[0], TransportType::StdTcp).unwrap();
    assert_eq!(ds_msg.transaction_id(), 0, "downstream should use internal txn_id 0");

    // The upstream response should restore the original txn_id.
    let us_sent = gw.upstream().sent.clone();
    assert_eq!(us_sent.len(), 1);
    let us_msg = decompile_adu_frame(&us_sent[0], TransportType::StdTcp).unwrap();
    assert_eq!(us_msg.transaction_id(), 0xABCD, "upstream response should use original txn_id");
}

/// Verify that `on_forward` and `on_response_returned` event callbacks fire
/// correctly during a successful request-response cycle.
#[test]
fn gateway_event_callbacks_fire_on_success() {
    let request_adu = build_tcp_request(
        0x0010, 3,
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x04],
    );
    let response_adu = build_tcp_request(
        0x0000, 3,
        FunctionCode::ReadCoils,
        &[0x01, 0x0F],
    );

    let upstream = MockTransport::tcp().with_rx(request_adu);
    let downstream = MockTransport::tcp().with_rx(response_adu);

    let mut router: UnitRouteTable<4> = UnitRouteTable::new();
    router.add(uid(3), 0).unwrap();

    struct Recorder {
        forwards: u32,
        responses: u32,
    }
    impl GatewayEventHandler for Recorder {
        fn on_forward(&mut self, _sid: u8, _unit: UnitIdOrSlaveAddr, _ch: usize) {
            self.forwards += 1;
        }
        fn on_response_returned(&mut self, _sid: u8, _txn: u16) {
            self.responses += 1;
        }
    }

    let mut gw: GatewayServices<MockTransport, MockTransport, _, _, 1> =
        GatewayServices::new(upstream, router, Recorder { forwards: 0, responses: 0 });
    gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
    gw.poll().unwrap();

    assert_eq!(gw.event_handler().forwards, 1);
    assert_eq!(gw.event_handler().responses, 1);
}
