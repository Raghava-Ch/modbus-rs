#![cfg(feature = "diagnostics-stats")]

mod common;

use common::{build_serial_request, serial_rtu_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{ModbusAppHandler, ResilienceConfig, ServerServices};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct StatsApp;

impl ModbusAppHandler for StatsApp {
    #[cfg(feature = "diagnostics")]
    fn diagnostics_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _sub_function: mbus_core::function_codes::public::DiagnosticSubFunction,
        _data: u16,
    ) -> Result<u16, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for StatsApp {}

#[derive(Debug)]
struct QueueSerialTransport {
    rx_queue: Arc<Mutex<VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>>>,
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    connected: bool,
}

impl Transport for QueueSerialTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType =
        TransportType::StdSerial(mbus_core::transport::SerialMode::Rtu);

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
        self.rx_queue
            .lock()
            .expect("rx_queue mutex poisoned")
            .pop_front()
            .ok_or(TransportError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

type TestServer = ServerServices<QueueSerialTransport, StatsApp, 8>;

type SharedRxQueue = Arc<Mutex<VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>>>;
type SharedSentFrames = Arc<Mutex<Vec<Vec<u8>>>>;

fn make_server() -> (TestServer, SharedRxQueue, SharedSentFrames) {
    let rx_queue = Arc::new(Mutex::new(VecDeque::new()));
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = QueueSerialTransport {
        rx_queue: Arc::clone(&rx_queue),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };

    let server = ServerServices::new(
        transport,
        StatsApp,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    (server, rx_queue, sent_frames)
}

fn build_diagnostics_request(sub_function: u16, data: u16) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let payload = [
        (sub_function >> 8) as u8,
        sub_function as u8,
        (data >> 8) as u8,
        data as u8,
    ];
    build_serial_request(1, unit_id(1), FunctionCode::Diagnostics, &payload)
}

fn send_request(
    server: &mut TestServer,
    rx_queue: &SharedRxQueue,
    sent_frames: &SharedSentFrames,
    request: HVec<u8, MAX_ADU_FRAME_LEN>,
) -> Option<Vec<u8>> {
    let before = sent_frames
        .lock()
        .expect("sent_frames mutex poisoned")
        .len();
    rx_queue
        .lock()
        .expect("rx_queue mutex poisoned")
        .push_back(request);

    server.poll();

    let sent_frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    if sent_frames.len() > before {
        sent_frames.last().cloned()
    } else {
        None
    }
}

fn decode_diagnostics_value(response: &[u8]) -> u16 {
    u16::from_be_bytes([response[4], response[5]])
}

#[test]
fn diagnostics_stats_track_message_and_server_response_counts() {
    let (mut server, rx_queue, sent_frames) = make_server();

    let loopback_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x0000, 0x1234),
    )
    .expect("loopback should produce a response");
    assert_eq!(decode_diagnostics_value(&loopback_response), 0x1234);

    let message_count_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x000B, 0x0000),
    )
    .expect("message count query should produce a response");
    assert_eq!(decode_diagnostics_value(&message_count_response), 2);

    let server_message_count_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x000E, 0x0000),
    )
    .expect("server message count query should produce a response");
    assert_eq!(decode_diagnostics_value(&server_message_count_response), 2);

    assert_eq!(server.stats.message_count, 3);
    assert_eq!(server.stats.server_message_count, 3);
}

#[test]
fn diagnostics_stats_track_parse_errors_and_clear_counters() {
    let (mut server, rx_queue, sent_frames) = make_server();

    let mut bad_request = build_diagnostics_request(0x0000, 0xABCD);
    let last_index = bad_request.len() - 1;
    bad_request[last_index] ^= 0xFF;

    let parse_error_response = send_request(&mut server, &rx_queue, &sent_frames, bad_request);
    assert!(
        parse_error_response.is_none(),
        "invalid CRC should not produce a response"
    );
    assert_eq!(server.stats.comm_error_count, 1);

    let (mut query_server, query_rx_queue, query_sent_frames) = make_server();
    query_server.stats.increment_comm_error_count();

    let comm_error_count_response = send_request(
        &mut query_server,
        &query_rx_queue,
        &query_sent_frames,
        build_diagnostics_request(0x000C, 0x0000),
    )
    .expect("communication error count query should produce a response");
    assert_eq!(decode_diagnostics_value(&comm_error_count_response), 1);

    let clear_response = send_request(
        &mut query_server,
        &query_rx_queue,
        &query_sent_frames,
        build_diagnostics_request(0x000A, 0x0000),
    )
    .expect("clear counters should produce a response");
    assert_eq!(decode_diagnostics_value(&clear_response), 0x0000);
    assert_eq!(query_server.stats.comm_error_count, 0);

    let cleared_comm_error_count_response = send_request(
        &mut query_server,
        &query_rx_queue,
        &query_sent_frames,
        build_diagnostics_request(0x000C, 0x0000),
    )
    .expect("communication error count query should produce a response after clear");
    assert_eq!(
        decode_diagnostics_value(&cleared_comm_error_count_response),
        0
    );
}

#[test]
fn diagnostics_stats_track_no_response_paths() {
    let (mut server, rx_queue, sent_frames) = make_server();

    let no_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x0004, 0x0000),
    );
    assert!(
        no_response.is_none(),
        "force listen-only mode must not send a response"
    );
    assert_eq!(server.stats.no_response_count, 1);

    let no_response_count_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x000F, 0x0000),
    )
    .expect("no response count query should produce a response");
    assert_eq!(decode_diagnostics_value(&no_response_count_response), 1);
}

#[test]
fn diagnostics_stats_track_exception_responses() {
    let (mut server, rx_queue, sent_frames) = make_server();

    let invalid_sub_function_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x0013, 0x0000),
    )
    .expect("invalid sub-function should produce an exception response");
    assert_eq!(invalid_sub_function_response[1], 0x88);
    assert_eq!(server.stats.exception_error_count, 1);

    let exception_count_response = send_request(
        &mut server,
        &rx_queue,
        &sent_frames,
        build_diagnostics_request(0x000D, 0x0000),
    )
    .expect("exception count query should produce a response");
    assert_eq!(decode_diagnostics_value(&exception_count_response), 1);
}
