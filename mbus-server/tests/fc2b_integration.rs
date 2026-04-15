mod common;
use common::{
    MockSerialTransport, MockTransport, build_request, build_serial_request, serial_rtu_config,
    tcp_config, unit_id,
};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{ModbusAppHandler, ResilienceConfig, ServerServices};

// ---------------------------------------------------------------------------
// App stub
// ---------------------------------------------------------------------------

struct DeviceIdApp;

impl ModbusAppHandler for DeviceIdApp {
    #[cfg(feature = "diagnostics")]
    fn read_device_identification_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        // All objects this device supports
        const OBJECTS: &[(u8, &[u8])] = &[
            (0x00, b"ACME Corp"),
            (0x01, b"Widget-9000"),
            (0x02, b"1.4.2"),
        ];

        // Conformity: BasicStreamAndIndividual = 0x81
        let conformity = 0x81u8;

        if read_device_id_code == 0x04 {
            // Individual access: return exactly the requested object
            for &(id, val) in OBJECTS {
                if id == start_object_id {
                    let needed = 2 + val.len();
                    if needed > out.len() {
                        return Err(MbusError::BufferTooSmall);
                    }
                    out[0] = id;
                    out[1] = val.len() as u8;
                    out[2..needed].copy_from_slice(val);
                    return Ok((needed as u8, conformity, false, 0x00));
                }
            }
            // Requested object not found — app signals failure
            return Err(MbusError::InvalidAddress);
        }

        // Stream access: return all objects with id >= start_object_id
        let mut written = 0usize;
        for &(id, val) in OBJECTS.iter().filter(|&&(id, _)| id >= start_object_id) {
            let needed = 2 + val.len();
            if written + needed > out.len() {
                break;
            }
            out[written] = id;
            out[written + 1] = val.len() as u8;
            out[written + 2..written + needed].copy_from_slice(val);
            written += needed;
        }
        Ok((written as u8, conformity, false, 0x00))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fc2b_pdu_bytes(read_device_id_code: u8, start_object_id: u8) -> [u8; 3] {
    // PDU data for FC 0x2B: [MEI_type=0x0E, code, object_id]
    [0x0E, read_device_id_code, start_object_id]
}

fn run_once_serial(request: HVec<u8, MAX_ADU_FRAME_LEN>) -> Vec<u8> {
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transport = MockSerialTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent),
        connected: true,
    };
    let mut server = ServerServices::new(
        transport,
        DeviceIdApp,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server.poll();
    sent.lock()
        .expect("mutex")
        .first()
        .cloned()
        .expect("server must send a response")
}

fn run_once_tcp(request: HVec<u8, MAX_ADU_FRAME_LEN>) -> Vec<u8> {
    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent),
        connected: true,
    };
    let mut server = ServerServices::new(
        transport,
        DeviceIdApp,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server.poll();
    sent.lock()
        .expect("mutex")
        .first()
        .cloned()
        .expect("server must send a response")
}

fn decode_exception(value: u8) -> ExceptionCode {
    match value {
        0x01 => ExceptionCode::IllegalFunction,
        0x02 => ExceptionCode::IllegalDataAddress,
        0x03 => ExceptionCode::IllegalDataValue,
        0x04 => ExceptionCode::ServerDeviceFailure,
        _ => panic!("unexpected exception code: {value:#04x}"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// FC 0x2B Basic read (code=1) over serial returns all three basic objects.
#[test]
fn fc2b_basic_read_serial_returns_all_basic_objects() {
    let pdu_data = fc2b_pdu_bytes(0x01, 0x00);
    let request = build_serial_request(1, unit_id(1), FunctionCode::EncapsulatedInterfaceTransport, &pdu_data);

    let response = run_once_serial(request);

    // Serial RTU: [slave_addr(1), FC(1), PDU_data..., CRC(2)]
    assert_eq!(response[1], 0x2B, "FC byte");
    assert_eq!(response[2], 0x0E, "MEI type");
    assert_eq!(response[3], 0x01, "read device id code echoed");
    assert_eq!(response[4], 0x81, "conformity level BasicStreamAndIndividual");
    assert_eq!(response[5], 0x00, "more_follows = false");
    assert_eq!(response[6], 0x00, "next_object_id");
    assert_eq!(response[7], 3, "number of objects");

    // Object 0x00: VendorName = "ACME Corp"
    let mut offset = 8usize;
    assert_eq!(response[offset], 0x00, "object id");
    let len0 = response[offset + 1] as usize;
    assert_eq!(&response[offset + 2..offset + 2 + len0], b"ACME Corp");
    offset += 2 + len0;

    // Object 0x01: ProductCode = "Widget-9000"
    assert_eq!(response[offset], 0x01, "object id");
    let len1 = response[offset + 1] as usize;
    assert_eq!(&response[offset + 2..offset + 2 + len1], b"Widget-9000");
    offset += 2 + len1;

    // Object 0x02: MajorMinorRevision = "1.4.2"
    assert_eq!(response[offset], 0x02, "object id");
    let len2 = response[offset + 1] as usize;
    assert_eq!(&response[offset + 2..offset + 2 + len2], b"1.4.2");
}

/// FC 0x2B works over TCP (no serial-only restriction).
#[test]
fn fc2b_basic_read_tcp_works() {
    let pdu_data = fc2b_pdu_bytes(0x01, 0x00);
    let request = build_request(2, unit_id(1), FunctionCode::EncapsulatedInterfaceTransport, &pdu_data);

    let response = run_once_tcp(request);

    // TCP MBAP: [TxnId(2), ProtoId(2), Len(2), UnitId(1), FC(1), MEI(1), ...]
    assert_eq!(response[7], 0x2B, "FC byte");
    assert_eq!(response[8], 0x0E, "MEI type");
    assert_eq!(response[9], 0x01, "read device id code echoed");
    assert_eq!(response[10], 0x81, "conformity level");
    assert_eq!(response[11], 0x00, "more_follows = false");
    assert_eq!(response[12], 0x00, "next_object_id");
    assert_eq!(response[13], 3, "number of objects");
}

/// FC 0x2B Specific access (code=4) returns exactly the requested object.
#[test]
fn fc2b_specific_access_returns_single_object() {
    // Request object 0x01 (ProductCode) specifically
    let pdu_data = fc2b_pdu_bytes(0x04, 0x01);
    let request = build_serial_request(3, unit_id(1), FunctionCode::EncapsulatedInterfaceTransport, &pdu_data);

    let response = run_once_serial(request);

    assert_eq!(response[1], 0x2B, "FC byte");
    assert_eq!(response[2], 0x0E, "MEI type");
    assert_eq!(response[3], 0x04, "read device id code echoed");
    assert_eq!(response[7], 1, "exactly one object returned");
    assert_eq!(response[8], 0x01, "object id = ProductCode");
    let len = response[9] as usize;
    assert_eq!(&response[10..10 + len], b"Widget-9000");
}

/// FC 0x2B with unknown MEI type returns IllegalFunction exception.
#[test]
fn fc2b_unknown_mei_type_returns_exception() {
    // Use MEI type 0x0D (CANopen) which we don't handle
    let pdu_data = [0x0D, 0x01, 0x00];
    let request = build_serial_request(4, unit_id(1), FunctionCode::EncapsulatedInterfaceTransport, &pdu_data);

    let response = run_once_serial(request);

    // Serial exception: [slave_addr(1), FC|0x80(1), exception_code(1), CRC(2)]
    assert_eq!(response[1], 0xAB, "exception FC = 0x2B | 0x80");
    assert_eq!(
        decode_exception(response[2]),
        ExceptionCode::IllegalFunction
    );
}
