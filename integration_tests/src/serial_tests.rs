use anyhow::Result;
use heapless::Vec as HVec;
use modbus_rs::{
    BackoffStrategy, BaudRate, ClientServices, ConformityLevel, DataBits,
    DiagnosticSubFunction, JitterStrategy, MbusError, MAX_ADU_FRAME_LEN, ModbusConfig,
    ModbusSerialConfig, ObjectId, Parity, ReadDeviceIdCode, SerialMode, Transport,
    TransportError, TransportType, UnitIdOrSlaveAddr, crc16,
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
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let txn_id = 1;
    let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();
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
    let received_responses = client.app().received_coil_responses.borrow();
    assert_eq!(received_responses.len(), 1);
    let (rcv_txn_id, rcv_unit_id, rcv_coils) = &received_responses[0];
    let rcv_quantity = rcv_coils.quantity();

    assert_eq!(*rcv_txn_id, txn_id);
    assert_eq!(*rcv_unit_id, unit_id);
    assert_eq!(rcv_coils.from_address(), address);
    assert_eq!(rcv_coils.quantity(), quantity);
    assert_eq!(&rcv_coils.values()[..1], &[0x05]);
    assert_eq!(rcv_quantity, quantity);

    Ok(())
}

/// Test case: Broadcast a Write Single Coil request.
/// It should construct the frame with address 0, transmit it, and expect no response.
#[test]
fn test_serial_broadcast_write_single_coil_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();

    // 1. Send Broadcast Request
    client.write_single_coil(5, unit_id, 10, true)?;

    // 2. Verify Sent Frame (RTU)
    // ADU: [UnitID(0)] [FC(5)] [Addr(00 0A)] [Val(FF 00)] [CRC]
    let mut expected = vec![0x00, 0x05, 0x00, 0x0A, 0xFF, 0x00];
    let crc = crc16(&expected);
    expected.extend_from_slice(&crc.to_le_bytes());

    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, expected);
    }

    // 3. Poll and verify no expected responses are queued and no error is triggered.
    // Because it is a broadcast, the client should not wait for a reply.
    client.poll();
    assert!(client.app().failed_requests.borrow().is_empty());

    Ok(())
}

/// Test case: Broadcast a Write Multiple Registers request.
/// It should transmit correctly via serial without blocking the queue waiting for a response.
#[test]
fn test_serial_broadcast_write_multiple_registers_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
    let values = [0x1234, 0x5678];

    // 1. Send Broadcast Request
    client.write_multiple_registers(6, unit_id, 0x0001, 2, &values)?;

    // 2. Verify Sent Frame (RTU)
    let mut expected = vec![
        0x00, // Unit ID (Broadcast)
        0x10, // FC (16 = Write Multiple Registers)
        0x00, 0x01, // Address
        0x00, 0x02, // Quantity
        0x04, // Byte count
        0x12, 0x34, 0x56, 0x78, // Data
    ];
    let crc = crc16(&expected);
    expected.extend_from_slice(&crc.to_le_bytes());

    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, expected);
    }

    Ok(())
}

/// Test case: Read Operations are strictly forbidden to broadcast.
/// It should yield an immediate error and send nothing.
#[test]
fn test_serial_broadcast_read_coils_not_allowed() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();

    let res = client.read_multiple_coils(7, unit_id, 10, 3);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), MbusError::BroadcastNotAllowed);

    // Verify nothing was sent over the wire
    assert!(sent_data.borrow().is_empty());

    Ok(())
}

/// Test case: Broadcasting Diagnostic requests.
/// Only certain diagnostic sub-functions (like ForceListenOnlyMode) are permitted for broadcasting.
#[test]
fn test_serial_broadcast_diagnostics_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();

    // 0x04 Force Listen Only Mode is allowed for broadcast
    let res = client.diagnostics(
        8,
        unit_id,
        DiagnosticSubFunction::ForceListenOnlyMode,
        &[0x0000],
    );
    assert!(res.is_ok());

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
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.write_single_coil(2, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, true)?;

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

    let received = client.app().received_write_single_coil_responses.borrow();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0],
        (2, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, true)
    );

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
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.read_device_identification(
        3,
        UnitIdOrSlaveAddr::try_from(1).unwrap(),
        ReadDeviceIdCode::Basic,
        ObjectId::from(0x00),
    )?;

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

    let received = client.app().received_read_device_id_responses.borrow();
    assert_eq!(received.len(), 1);
    let (txn_id, unit_id, resp) = &received[0];
    assert_eq!(*txn_id, 3);
    assert_eq!(*unit_id, UnitIdOrSlaveAddr::try_from(1).unwrap());
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
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let txn_id = 4;
    let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();
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

    let received_responses = client.app().received_coil_responses.borrow();
    assert_eq!(received_responses.len(), 1);
    let (rcv_txn_id, rcv_unit_id, rcv_coils) = &received_responses[0];
    let rcv_quantity = rcv_coils.quantity();

    assert_eq!(*rcv_txn_id, txn_id);
    assert_eq!(*rcv_unit_id, unit_id);
    assert_eq!(rcv_coils.from_address(), address);
    assert_eq!(rcv_coils.quantity(), quantity);
    assert_eq!(&rcv_coils.values()[..1], &[0x05]);
    assert_eq!(rcv_quantity, quantity);

    Ok(())
}

#[test]
fn test_serial_write_single_coil_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.write_single_coil(20, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, true)?;

    // Binary payload: 01 05 00 0A FF 00, LRC = F1
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, b":0105000AFF00F1\r\n");
    }

    // Echo response in ASCII mode.
    recv_data
        .borrow_mut()
        .extend_from_slice(b":0105000AFF00F1\r\n");

    client.poll();

    let received = client.app().received_write_single_coil_responses.borrow();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].0, 20);
    assert_eq!(received[0].1, UnitIdOrSlaveAddr::try_from(1).unwrap());
    assert_eq!(received[0].2, 10);
    assert!(received[0].3);

    Ok(())
}

#[test]
fn test_serial_read_holding_registers_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.read_holding_registers(21, UnitIdOrSlaveAddr::try_from(1).unwrap(), 1, 2)?;

    // Binary payload: 01 03 00 01 00 02, LRC = F9
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, b":010300010002F9\r\n");
    }

    // Response payload: 01 03 04 12 34 56 78, LRC = E4
    recv_data
        .borrow_mut()
        .extend_from_slice(b":01030412345678E4\r\n");

    client.poll();

    // Register callbacks in MockApp are no-op; assert parse path did not report failure.
    assert!(client.app().failed_requests.borrow().is_empty());

    Ok(())
}

#[test]
fn test_serial_read_device_id_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    client.read_device_identification(
        22,
        UnitIdOrSlaveAddr::try_from(1).unwrap(),
        ReadDeviceIdCode::Basic,
        ObjectId::from(0x00),
    )?;

    // Binary payload: 01 2B 0E 01 00, LRC = C5
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, b":012B0E0100C5\r\n");
    }

    // Response payload: 01 2B 0E 01 81 00 00 01 00 03 46 6F 6F, LRC = 1C
    recv_data
        .borrow_mut()
        .extend_from_slice(b":012B0E01810000010003466F6F1C\r\n");

    client.poll();

    let received = client.app().received_read_device_id_responses.borrow();
    assert_eq!(received.len(), 1);
    let (txn_id, unit_id, resp) = &received[0];
    assert_eq!(*txn_id, 22);
    assert_eq!(*unit_id, UnitIdOrSlaveAddr::try_from(1).unwrap());
    assert_eq!(
        resp.conformity_level,
        ConformityLevel::BasicStreamAndIndividual
    );
    let objects: Vec<_> = resp.objects().map(|r| r.unwrap()).collect();
    assert_eq!(objects[0].value.as_slice(), b"Foo");

    Ok(())
}

#[test]
fn test_serial_broadcast_write_single_coil_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
    client.write_single_coil(23, unit_id, 10, true)?;

    // Binary payload: 00 05 00 0A FF 00, LRC = F2
    {
        let sent = sent_data.borrow();
        assert_eq!(*sent, b":0005000AFF00F2\r\n");
    }

    // Broadcast write should not wait for reply or trigger error.
    client.poll();
    assert!(client.app().failed_requests.borrow().is_empty());

    Ok(())
}

#[test]
fn test_serial_broadcast_read_coils_not_allowed_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let sent_data = transport.sent_data.clone();
    let app = MockApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let unit_id = UnitIdOrSlaveAddr::new_broadcast_address();
    let res = client.read_multiple_coils(24, unit_id, 10, 3);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), MbusError::BroadcastNotAllowed);
    assert!(sent_data.borrow().is_empty());

    Ok(())
}

/// Test case: Simulates a fragmented stream receiving a complete RTU frame attached to a half-frame.
/// It verifies the client processes the complete frame and queues the remainder seamlessly.
#[test]
fn test_serial_fragmented_frames_rtu() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Rtu);
    let sent_data = transport.sent_data.clone();
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();
    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);
    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    // 1. Send Request 1 (Read Coils)
    client.read_multiple_coils(1, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, 3)?;
    sent_data.borrow_mut().clear();

    // Prepared mock responses
    let frame1 = [0x01, 0x01, 0x01, 0x05, 0x91, 0x8B]; // Complete Response 1
    let frame2 = [0x01, 0x05, 0x00, 0x0A, 0xFF, 0x00, 0xAC, 0x38]; // Response 2

    // 2. Inject Frame 1 entirely, and ONLY the first 4 bytes of Frame 2 (fragmentation boundary)
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(&frame1);
        recv.extend_from_slice(&frame2[..4]);
    }

    // 3. Poll: This should successfully process Request 1, and securely keep the 4 bytes in the buffer
    client.poll();
    {
        let received = client.app().received_coil_responses.borrow();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].0, 1);
    }

    // 4. Send Request 2 (Write Single Coil) - Only allowed to pipiline after Req 1 completes for serial
    client.write_single_coil(2, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, true)?;
    sent_data.borrow_mut().clear();

    // 5. Inject the remaining 4 bytes of Frame 2
    {
        let mut recv = recv_data.borrow_mut();
        recv.extend_from_slice(&frame2[4..]);
    }

    // 6. Poll: This should stitch the remaining bytes to the buffer and successfully process Request 2
    client.poll();
    {
        let received_writes = client.app().received_write_single_coil_responses.borrow();
        assert_eq!(received_writes.len(), 1);
        assert_eq!(received_writes[0].0, 2);
    }

    Ok(())
}

/// Test case: Simulates fragmented frames over Modbus ASCII where delimiting characters determine boundaries.
#[test]
fn test_serial_fragmented_frames_ascii() -> Result<()> {
    let transport = MockSerialTransport::new(SerialMode::Ascii);
    let recv_data = transport.recv_data.clone();

    let app = MockApp::default();
    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/mock").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        parity: Parity::Even,
        stop_bits: 1,
        response_timeout_ms: 1000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);
    let mut client = ClientServices::<_, _, 1>::new(transport, app, config)?;

    let frame1 = b":01010105F8\r\n";
    let frame2 = b":0105000AFF00F1\r\n";

    client.read_multiple_coils(1, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, 3)?;
    recv_data.borrow_mut().extend_from_slice(frame1);
    recv_data.borrow_mut().extend_from_slice(&frame2[..8]);
    client.poll();
    assert_eq!(client.app().received_coil_responses.borrow()[0].0, 1);

    client.write_single_coil(2, UnitIdOrSlaveAddr::try_from(1).unwrap(), 10, true)?;
    recv_data.borrow_mut().extend_from_slice(&frame2[8..]);
    client.poll();
    assert_eq!(
        client.app().received_write_single_coil_responses.borrow()[0].0,
        2
    );

    Ok(())
}
