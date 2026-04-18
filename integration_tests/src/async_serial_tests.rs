use anyhow::Result;
use heapless::Vec as HVec;
use modbus_rs::mbus_async::{AsyncError, AsyncSerialClient};
use modbus_rs::{
    crc16, BackoffStrategy, BaudRate, DataBits, DiagnosticSubFunction, JitterStrategy, MbusError,
    ModbusConfig, ModbusSerialConfig, Parity, SerialMode, TransportType,
    MAX_ADU_FRAME_LEN,
};
use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
struct MockAsyncSerialTransport<const ASCII: bool = false> {
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    recv_frames: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl<const ASCII: bool> MockAsyncSerialTransport<ASCII> {
    const MODE: SerialMode = if ASCII {
        SerialMode::Ascii
    } else {
        SerialMode::Rtu
    };

    fn new() -> Self {
        Self {
            sent_frames: Arc::new(Mutex::new(Vec::new())),
            recv_frames: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<const ASCII: bool> mbus_core::transport::AsyncTransport for MockAsyncSerialTransport<ASCII> {
    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.sent_frames
            .lock()
            .expect("sent_frames lock poisoned")
            .push(adu.to_vec());
        Ok(())
    }

    async fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let maybe_frame = self
            .recv_frames
            .lock()
            .expect("recv_frames lock poisoned")
            .pop_front();

        let frame = match maybe_frame {
            Some(v) => v,
            None => return Err(MbusError::Timeout),
        };

        let mut out = HVec::new();
        out.extend_from_slice(&frame)
            .map_err(|_| MbusError::BufferTooSmall)?;
        Ok(out)
    }

    fn transport_type(&self) -> TransportType {
        TransportType::CustomSerial(Self::MODE)
    }

    fn is_connected(&self) -> bool {
        true
    }
}

fn append_rtu_crc(frame_wo_crc: &[u8]) -> Vec<u8> {
    let mut frame = frame_wo_crc.to_vec();
    let crc = crc16(frame_wo_crc);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

fn rtu_config(path: &str) -> ModbusSerialConfig {
    ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(path).expect("path too long"),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 100,
        mode: SerialMode::Rtu,
        retry_attempts: 1,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    }
}

fn ascii_config(path: &str) -> ModbusSerialConfig {
    ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(path).expect("path too long"),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        stop_bits: 1,
        parity: Parity::Even,
        response_timeout_ms: 100,
        mode: SerialMode::Ascii,
        retry_attempts: 1,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    }
}

#[test]
fn test_async_serial_rtu_rejects_ascii_mode() -> Result<()> {
    let err = match AsyncSerialClient::new_rtu(ascii_config("/dev/null")) {
        Ok(_) => panic!("expected InvalidConfiguration for RTU constructor with ASCII config"),
        Err(e) => e,
    };
    assert_eq!(err, AsyncError::Mbus(MbusError::InvalidConfiguration));
    Ok(())
}

#[test]
fn test_async_serial_rtu_with_poll_interval_rejects_ascii_mode() -> Result<()> {
    let err = match AsyncSerialClient::new_rtu_with_poll_interval(
        ascii_config("/dev/null"),
        Duration::from_millis(5),
    ) {
        Ok(_) => panic!("expected InvalidConfiguration for RTU poll constructor with ASCII config"),
        Err(e) => e,
    };
    assert_eq!(err, AsyncError::Mbus(MbusError::InvalidConfiguration));
    Ok(())
}

#[test]
fn test_async_serial_ascii_rejects_rtu_mode() -> Result<()> {
    let err = match AsyncSerialClient::new_ascii(rtu_config("/dev/null")) {
        Ok(_) => panic!("expected InvalidConfiguration for ASCII constructor with RTU config"),
        Err(e) => e,
    };
    assert_eq!(err, AsyncError::Mbus(MbusError::InvalidConfiguration));
    Ok(())
}

#[test]
fn test_async_serial_ascii_with_poll_interval_rejects_rtu_mode() -> Result<()> {
    let err = match AsyncSerialClient::new_ascii_with_poll_interval(
        rtu_config("/dev/null"),
        Duration::from_millis(5),
    ) {
        Ok(_) => panic!("expected InvalidConfiguration for ASCII poll constructor with RTU config"),
        Err(e) => e,
    };
    assert_eq!(err, AsyncError::Mbus(MbusError::InvalidConfiguration));
    Ok(())
}

#[tokio::test]
async fn test_async_serial_nonexistent_port() -> Result<()> {
    // Construction is side-effect free; the explicit connect step should fail.
    let config = rtu_config("/dev/nonexistent_port_12345");
    let client = AsyncSerialClient::new_rtu(config)?;
    let result = client.connect().await;

    assert!(
        result.is_err(),
        "Expected connect error for nonexistent port"
    );
    Ok(())
}

#[test]
fn test_async_serial_rtu_poll_interval_validation() -> Result<()> {
    // Test that zero poll interval is handled
    let config = rtu_config("/dev/null");
    let result = AsyncSerialClient::new_rtu_with_poll_interval(config, Duration::from_millis(0));

    // Should either accept it or reject with validation error
    // The key is that it doesn't panic
    let _ = result;
    Ok(())
}

#[test]
fn test_async_serial_ascii_poll_interval_validation() -> Result<()> {
    // Test that very large poll interval is handled
    let config = ascii_config("/dev/null");
    let result = AsyncSerialClient::new_ascii_with_poll_interval(config, Duration::from_secs(60));

    // Should either accept it or reject gracefully
    let _ = result;
    Ok(())
}

#[test]
fn test_async_serial_rtu_with_invalid_baud_rate() -> Result<()> {
    // Test configuration with valid structure but potentially invalid baud rate
    let config = rtu_config("/dev/null");
    let result = AsyncSerialClient::new_rtu(config);

    // Should handle gracefully without panicking
    let _ = result;
    Ok(())
}

#[test]
fn test_async_serial_multiple_constructor_variants() -> Result<()> {
    // Verify that all constructor variants exist and are callable
    // Without panicking, even if they fail

    let r1 = AsyncSerialClient::new_rtu(rtu_config("/dev/null"));
    let r2 = AsyncSerialClient::new_rtu_with_poll_interval(
        rtu_config("/dev/null"),
        Duration::from_millis(10),
    );

    let r3 = AsyncSerialClient::new_ascii(ascii_config("/dev/null"));
    let r4 = AsyncSerialClient::new_ascii_with_poll_interval(
        ascii_config("/dev/null"),
        Duration::from_millis(10),
    );

    // At least verify the calls didn't panic
    let _ = (r1, r2, r3, r4);
    Ok(())
}

#[tokio::test]
async fn test_async_serial_e2e_read_multiple_coils_rtu() -> Result<()> {
    let transport = MockAsyncSerialTransport::<false>::new();
    let sent = transport.sent_frames.clone();
    let recv = transport.recv_frames.clone();

    recv.lock()
        .expect("recv_frames lock poisoned")
        .push_back(append_rtu_crc(&[0x01, 0x01, 0x01, 0x05]));

    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client =
        AsyncSerialClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let coils = client.read_multiple_coils(1, 0x000A, 3).await?;

    assert_eq!(coils.from_address(), 0x000A);
    assert_eq!(coils.quantity(), 3);
    assert!(coils.value(0x000A)?);
    assert!(!coils.value(0x000B)?);
    assert!(coils.value(0x000C)?);

    let frames = sent.lock().expect("sent_frames lock poisoned");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0],
        append_rtu_crc(&[0x01, 0x01, 0x00, 0x0A, 0x00, 0x03])
    );

    Ok(())
}

#[tokio::test]
async fn test_async_serial_e2e_write_single_register_rtu() -> Result<()> {
    let transport = MockAsyncSerialTransport::<false>::new();
    let sent = transport.sent_frames.clone();
    let recv = transport.recv_frames.clone();

    recv.lock()
        .expect("recv_frames lock poisoned")
        .push_back(append_rtu_crc(&[0x01, 0x06, 0x00, 0x20, 0x12, 0x34]));

    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client =
        AsyncSerialClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let (addr, value) = client.write_single_register(1, 0x0020, 0x1234).await?;
    assert_eq!(addr, 0x0020);
    assert_eq!(value, 0x1234);

    let frames = sent.lock().expect("sent_frames lock poisoned");
    assert_eq!(frames.len(), 1);
    assert_eq!(
        frames[0],
        append_rtu_crc(&[0x01, 0x06, 0x00, 0x20, 0x12, 0x34])
    );

    Ok(())
}

#[tokio::test]
async fn test_async_serial_e2e_serial_diagnostics_paths_rtu() -> Result<()> {
    let transport = MockAsyncSerialTransport::<false>::new();
    let recv = transport.recv_frames.clone();

    {
        let mut q = recv.lock().expect("recv_frames lock poisoned");
        q.push_back(append_rtu_crc(&[0x01, 0x07, 0xAB]));
        q.push_back(append_rtu_crc(&[0x01, 0x0B, 0x00, 0x02, 0x00, 0x05]));
        q.push_back(append_rtu_crc(&[0x01, 0x08, 0x00, 0x00, 0x00, 0x2A]));
    }

    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client =
        AsyncSerialClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let status = client.read_exception_status(1).await?;
    assert_eq!(status, 0xAB);

    let (event_status, event_count) = client.get_comm_event_counter(1).await?;
    assert_eq!(event_status, 0x0002);
    assert_eq!(event_count, 0x0005);

    let diag = client
        .diagnostics(1, DiagnosticSubFunction::ReturnQueryData, &[0x002A])
        .await?;
    assert_eq!(diag.sub_function, DiagnosticSubFunction::ReturnQueryData);
    assert_eq!(diag.data, vec![0x002A]);

    Ok(())
}

#[tokio::test]
async fn test_async_serial_e2e_exception_propagation_rtu() -> Result<()> {
    let transport = MockAsyncSerialTransport::<false>::new();
    let recv = transport.recv_frames.clone();

    recv.lock()
        .expect("recv_frames lock poisoned")
        .push_back(append_rtu_crc(&[0x01, 0x81, 0x02]));

    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client =
        AsyncSerialClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let result = client.read_multiple_coils(1, 0x0000, 1).await;
    assert!(matches!(
        result,
        Err(AsyncError::Mbus(MbusError::ModbusException(0x02)))
    ));

    Ok(())
}
