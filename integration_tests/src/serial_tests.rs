use anyhow::Result;
use heapless::Vec as HVec;
use mbus_core::{
    client::services::ClientServices,
    data_unit::common::{MAX_ADU_FRAME_LEN},
    device_identification::{ConformityLevel, ObjectId, ReadDeviceIdCode},
    transport::{
        BaudRate, ModbusConfig, ModbusSerialConfig, Parity, SerialMode, Transport, TransportError,
        TransportType,
    },
};
use std::{cell::RefCell, rc::Rc, str::FromStr};

use crate::mock_app::MockApp;

/// A custom mock transport that simulates a serial connection.
/// It captures sent frames for verification and allows injecting response frames.
#[derive(Debug, Clone)]
struct MockSerialTransport {
    /// Shared buffer to store data sent by the client.
    sent_data: Rc<RefCell<Vec<u8>>>,
    /// Shared buffer to stage data to be received by the client.
    recv_data: Rc<RefCell<Vec<u8>>>,
    mode: SerialMode,
}

impl MockSerialTransport {
    fn new(mode: SerialMode) -> Self {
        Self {
            sent_data: Rc::new(RefCell::new(Vec::new())),
            recv_data: Rc::new(RefCell::new(Vec::new())),
            mode,
        }
    }
}

impl Transport for MockSerialTransport {
    type Error = TransportError;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        self.sent_data.borrow_mut().extend_from_slice(adu);
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        let mut data = self.recv_data.borrow_mut();
        if data.is_empty() {
            return Err(TransportError::Timeout);
        }

        // For this mock, we assume the test setup injects exactly one full frame
        // into recv_data, so we consume it all.
        let mut res = HVec::new();
        res.extend_from_slice(&data)
            .map_err(|_| TransportError::BufferTooSmall)?;
        data.clear();
        Ok(res)
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn transport_type(&self) -> TransportType {
        TransportType::CustomSerial(self.mode)
    }
}

#[test]
fn test_serial_read_coils_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let txn_id = 1;
    let unit_id = 1;
    let address = 10;
    let quantity = 3;

    // 1. Send Request
    client.read_multiple_coils(txn_id, unit_id, address, quantity)?;

    // 2. Verify Sent Frame (RTU)
    // ADU: [UnitID(1)] [FC(1)] [Addr(00 0A)] [Qty(00 03)] [CRC(5c 09)]
    // CRC calculation: 01 01 00 0A 00 03 -> CRC 0x33E1 (Little Endian: E1 33)
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, vec![0x01, 0x01, 0x00, 0x0A, 0x00, 0x03, 0x5c, 0x09]);
    }
    sent_data.borrow_mut().clear();

    // 3. Inject Response
    // Response: 3 coils, values: [1, 0, 1] -> 0x05
    // ADU: [UnitID(1)] [FC(1)] [ByteCount(1)] [Data(05)] [CRC(91 8B)]
    // CRC: 01 01 01 05 -> 0x9174 (LE: 74 91)
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(&[0x01, 0x01, 0x01, 0x05, 0x91, 0x8B]);
    }

    // 4. Poll to process response
    client.poll();

    // 5. Verify App Callback
    let received_responses = client.app.received_coil_responses.borrow();
    assert_eq!(received_responses.len(), 1);
    let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];

    assert_eq!(*rcv_txn_id, txn_id);
    assert_eq!(*rcv_unit_id, unit_id);
    assert_eq!(rcv_coils.from_address(), address);
    assert_eq!(rcv_coils.quantity(), quantity);
    assert_eq!(rcv_coils.values().as_slice(), &[0x05]);
    assert_eq!(*rcv_quantity, quantity);

    Ok(())
}

#[test]
fn test_serial_write_single_coil_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.write_single_coil(2, 1, 10, true)?;

    // Verify Sent Frame (RTU)
    // ADU: [UnitID(1)] [FC(5)] [Addr(00 0A)] [Val(FF 00)] [CRC(AC 38)]
    // CRC: 01 05 00 0A FF 00 -> 0xDDFA (LE: FA DD)
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, vec![0x01, 0x05, 0x00, 0x0A, 0xFF, 0x00, 0xAC, 0x38]);
    }

    // Inject Response (Echo)
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(&[0x01, 0x05, 0x00, 0x0A, 0xFF, 0x00, 0xAC, 0x38]);
    }

    client.poll();

    let received = client.app.received_write_single_coil_responses.borrow();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0], (2, 1, 10, true));

    Ok(())
}

#[test]
fn test_serial_read_device_id_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: 8,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.read_device_identification(3, 1, ReadDeviceIdCode::Basic, ObjectId::from(0x00))?;

    // Verify Sent Frame (RTU)
    // ADU: [Unit(1)] [FC(2B)] [MEI(0E)] [Code(01)] [Obj(00)] [CRC(70 77)]
    // CRC: 01 2B 0E 01 00 -> 0xD241 (LE: 41 D2)
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, vec![0x01, 0x2B, 0x0E, 0x01, 0x00, 0x70, 0x77]);
    }

    // Inject Response
    // ADU: [Unit(1)] [PDU] [CRC(E1 E9)]
    // PDU: [FC(2B)] [MEI(0E)] [Code(01)] [Conf(81)] [More(00)] [Next(00)] [Num(01)] [Obj(00)] [Len(03)] [Val("Foo")]
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(&[
            0x01, 0x2B, 0x0E, 0x01, 0x81, 0x00, 0x00, 0x01, 0x00, 0x03, 0x46, 0x6F, 0x6F, 0xE1,
            0xE9,
        ]);
    }

    client.poll();

    let received = client.app.received_read_device_id_responses.borrow();
    assert_eq!(received.len(), 1);
    let (txn_id, unit_id, resp) = &received[0];
    assert_eq!(*txn_id, 3);
    assert_eq!(*unit_id, 1);
    assert_eq!(
        resp.conformity_level,
        ConformityLevel::BasicStreamAndIndividual
    );
    let objects: Vec<_> = resp.objects().map(|r| r.unwrap()).collect();
    assert_eq!(objects[0].value.as_slice(), b"Foo");

    Ok(())
}

#[test]
fn test_serial_read_coils_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    // Standard Modbus ASCII configuration
    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: 7,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let txn_id = 4;
    let unit_id = 1;
    let address = 10;
    let quantity = 3;

    client.read_multiple_coils(txn_id, unit_id, address, quantity)?;

    // Verify Sent Frame (ASCII)
    // Request: Read Coils (FC 01) for 3 coils at address 10.
    // Binary: 01 01 00 0A 00 03
    // LRC: -(1+1+10+3) = -15 = F1
    // Frame: :0101000A0003F1\r\n
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, b":0101000A0003F1\r\n");
    }

    // Inject Response
    // Response: 3 coils, values: [1, 0, 1] -> 0x05
    // Binary: 01 01 01 05
    // LRC: -(1+1+1+5) = -8 = F8
    // Frame: :01010105F8\r\n
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(b":01010105F8\r\n");
    }

    client.poll();

    let received_responses = client.app.received_coil_responses.borrow();
    assert_eq!(received_responses.len(), 1);
    let (rcv_txn_id, rcv_unit_id, rcv_coils, rcv_quantity) = &received_responses[0];

    assert_eq!(*rcv_txn_id, txn_id);
    assert_eq!(*rcv_unit_id, unit_id);
    assert_eq!(rcv_coils.from_address(), address);
    assert_eq!(rcv_coils.quantity(), quantity);
    assert_eq!(rcv_coils.values().as_slice(), &[0x05]);
    assert_eq!(*rcv_quantity, quantity);

    Ok(())
}
