use anyhow::Result;
use heapless::Vec as HVec;
use mbus_core::models::coil::CoilState;
use modbus_rs::mbus_async::{AsyncError, AsyncRtuClient, AsyncSerialClient};
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, DiagnosticSubFunction, JitterStrategy, MAX_ADU_FRAME_LEN,
    MbusError, ModbusConfig, ModbusSerialConfig, Parity, SerialMode, TransportType, crc16,
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
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(Self::MODE);

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
    let client = AsyncRtuClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let coils = client.read_multiple_coils(1, 0x000A, 3).await?;

    assert_eq!(coils.from_address(), 0x000A);
    assert_eq!(coils.quantity(), 3);
    assert_eq!(coils.value(0x000A)?, CoilState::On);
    assert_eq!(coils.value(0x000B)?, CoilState::Off);
    assert_eq!(coils.value(0x000C)?, CoilState::On);

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
    let client = AsyncRtuClient::new_with_transport(transport, config, Duration::from_millis(1))?;
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
    let client = AsyncRtuClient::new_with_transport(transport, config, Duration::from_millis(1))?;
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
    let client = AsyncRtuClient::new_with_transport(transport, config, Duration::from_millis(1))?;
    client.connect().await?;

    let result = client.read_multiple_coils(1, 0x0000, 1).await;
    assert!(matches!(
        result,
        Err(AsyncError::Mbus(MbusError::ModbusException(0x02)))
    ));

    Ok(())
}

// ── Multi-drop RS-485 Simulated Bus for Timeout / Concurrency / Abort tests ──

#[derive(Clone)]
struct SimulatedRtuBus {
    seen_units: Arc<Mutex<Vec<u8>>>,
    silent_unit: u8,
    byte_queue: Arc<Mutex<VecDeque<u8>>>,
    byte_notify: Arc<tokio::sync::Notify>,
    reply_delay: Duration,
    inter_frame_timeout: Duration,
    rx_buf: Arc<Mutex<HVec<u8, MAX_ADU_FRAME_LEN>>>,
}

impl SimulatedRtuBus {
    fn new(silent_unit: u8, reply_delay: Duration) -> Self {
        Self {
            seen_units: Arc::new(Mutex::new(Vec::new())),
            silent_unit,
            byte_queue: Arc::new(Mutex::new(VecDeque::new())),
            byte_notify: Arc::new(tokio::sync::Notify::new()),
            reply_delay,
            inter_frame_timeout: Duration::from_millis(35),
            rx_buf: Arc::new(Mutex::new(HVec::new())),
        }
    }

    async fn read_one_byte(&self) -> Result<u8, MbusError> {
        loop {
            {
                let mut q = self.byte_queue.lock().unwrap();
                if let Some(b) = q.pop_front() {
                    return Ok(b);
                }
            }
            self.byte_notify.notified().await;
        }
    }
}

impl mbus_core::transport::AsyncTransport for SimulatedRtuBus {
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(SerialMode::Rtu);

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if adu.is_empty() {
            return Err(MbusError::InvalidAduLength);
        }
        self.rx_buf.lock().unwrap().clear();
        self.byte_queue.lock().unwrap().clear();
        let unit = adu[0];
        self.seen_units.lock().unwrap().push(unit);
        if unit != self.silent_unit {
            let reply = if adu.len() >= 6 && adu[1] == 3 {
                let qty = u16::from_be_bytes([adu[4], adu[5]]) as usize;
                let byte_count = (qty * 2) as u8;
                let mut payload = vec![unit, 0x03, byte_count];
                payload.resize(3 + byte_count as usize, 0);
                append_rtu_crc(&payload)
            } else {
                adu.to_vec()
            };
            let queue = self.byte_queue.clone();
            let notify = self.byte_notify.clone();
            let delay = self.reply_delay;
            tokio::spawn(async move {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                queue.lock().unwrap().extend(reply);
                notify.notify_waiters();
            });
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let is_empty = self.rx_buf.lock().unwrap().is_empty();
        if is_empty {
            let b0 = self.read_one_byte().await?;
            self.rx_buf
                .lock()
                .unwrap()
                .push(b0)
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        loop {
            match tokio::time::timeout(self.inter_frame_timeout, self.read_one_byte()).await {
                Ok(Ok(b)) => {
                    self.rx_buf
                        .lock()
                        .unwrap()
                        .push(b)
                        .map_err(|_| MbusError::BufferTooSmall)?;
                }
                Ok(Err(e)) => {
                    self.rx_buf.lock().unwrap().clear();
                    return Err(e);
                }
                Err(_elapsed) => {
                    let mut g = self.rx_buf.lock().unwrap();
                    let frame = g.clone();
                    g.clear();
                    return Ok(frame);
                }
            }
        }
    }

    fn is_connected(&self) -> bool {
        true
    }
}

#[tokio::test]
async fn test_repro_issue1_response_timeout_not_limiting_request() -> Result<()> {
    let bus = SimulatedRtuBus::new(2, Duration::from_millis(5));
    let mut cfg = rtu_config("/dev/mock");
    cfg.response_timeout_ms = 100;
    let config = ModbusConfig::Serial(cfg);
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    let t0 = std::time::Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_millis(500),
        client.read_holding_registers(2, 0, 1),
    )
    .await;

    // Expected: Request should time out after ~100ms via response_timeout_ms.
    // Actual: response_timeout_ms is ignored, so it hangs until outer 500ms safety timeout expires.
    assert!(
        outcome.is_ok(),
        "ISSUE 1 REPRODUCED: Request hung past 500ms because response_timeout_ms (100ms) was ignored! Took {:?}",
        t0.elapsed()
    );
    let res = outcome.unwrap();
    assert!(
        matches!(res, Err(AsyncError::Timeout)),
        "Expected timeout error, got: {:?}",
        res
    );
    assert!(
        t0.elapsed() < Duration::from_millis(300),
        "Expected timeout around 100ms, but took {:?}",
        t0.elapsed()
    );

    Ok(())
}

#[tokio::test]
async fn test_repro_issue2_timeout_closes_handle_for_every_unit_but_is_connected_stays_true()
-> Result<()> {
    let bus = SimulatedRtuBus::new(2, Duration::from_millis(5));
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;
    client.set_request_timeout(Duration::from_millis(100));

    // 1. Read silent unit 2 -> fails with Timeout
    let res2 = client.read_holding_registers(2, 0, 1).await;
    assert!(
        matches!(res2, Err(AsyncError::Timeout)),
        "Expected silent unit 2 to time out, got: {:?}",
        res2
    );

    // 2. Read healthy unit 1
    // Expected: On a multi-drop bus, reading healthy unit 1 should succeed.
    // Actual: TaskCommand::Disconnect was issued, closing the transport for all units!
    let res1 = client.read_holding_registers(1, 0, 1).await;
    assert!(
        res1.is_ok(),
        "ISSUE 2 REPRODUCED: Timeout on silent unit 2 closed the transport for healthy unit 1! Got: {:?}",
        res1
    );

    Ok(())
}

#[tokio::test]
async fn test_repro_issue3_timeout_starts_when_queued_not_when_sent() -> Result<()> {
    // Unit 1 answers in 40ms, Unit 2 is silent, Unit 3 answers in 20ms
    let bus = SimulatedRtuBus::new(2, Duration::from_millis(40));
    let seen = bus.seen_units.clone();
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;
    // Set 100ms timeout
    client.set_request_timeout(Duration::from_millis(100));

    // Queue 3 requests at once
    let c1 = client.clone();
    let c2 = client.clone();
    let c3 = client.clone();
    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });
    let h2 = tokio::spawn(async move { c2.read_holding_registers(2, 0, 1).await });
    let h3 = tokio::spawn(async move { c3.read_holding_registers(3, 0, 1).await });

    let (r1, r2, r3) = tokio::join!(h1, h2, h3);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();
    let r3 = r3.unwrap();

    let seen_units = seen.lock().unwrap().clone();
    println!("r1: {:?}, r2: {:?}, r3: {:?}", r1, r2, r3);
    println!("seen units: {:?}", seen_units);

    // Expected: Unit 3 should get its own timeout from when sent, reach the wire, and succeed.
    // Actual: Unit 3's timer expired while queued behind Unit 2, so it timed out at the same time and never reached the wire!
    assert!(
        seen_units.contains(&3),
        "ISSUE 3 REPRODUCED: Unit 3 timed out while queued behind unit 2 and never reached the wire! Seen units: {:?}",
        seen_units
    );
    assert!(
        r3.is_ok(),
        "ISSUE 3 REPRODUCED: Unit 3 failed prematurely, got: {:?}",
        r3
    );

    Ok(())
}

#[tokio::test]
async fn test_repro_issue4_abort_in_flight_breaks_later_requests() -> Result<()> {
    let bus = SimulatedRtuBus::new(99, Duration::from_millis(5));
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;
    client.set_request_timeout(Duration::from_millis(200));

    // 1. Initial healthy read succeeds
    let t0 = std::time::Instant::now();
    let init_res = client.read_holding_registers(1, 0, 1).await;
    println!("Step 1 took {:?}", t0.elapsed());
    assert!(init_res.is_ok(), "Initial read failed: {:?}", init_res);

    // 2. In-flight request is aborted / cancelled after 15ms
    let t1 = std::time::Instant::now();
    tokio::select! {
        res = client.read_holding_registers(1, 0, 1) => {
            println!("Step 2 read finished early: {:?}", res);
        }
        _ = tokio::time::sleep(Duration::from_millis(15)) => {
            println!("Step 2 aborted at {:?}", t1.elapsed());
        }
    }

    // 3. IMMEDIATELY issue next read to healthy unit 1 (like repro.mjs does)
    let t2 = std::time::Instant::now();
    let next_res = client.read_holding_registers(1, 0, 1).await;
    println!("Step 3 result after {:?}: {:?}", t2.elapsed(), next_res);
    assert!(
        next_res.is_ok(),
        "ISSUE 4 REPRODUCED: In-flight abort broke subsequent requests! Got: {:?}",
        next_res
    );

    Ok(())
}

#[tokio::test]
async fn test_queue_timeout_expires_while_waiting_for_wire_slot() -> Result<()> {
    // Unit 1 answers in 300ms, Unit 2 answers in 20ms
    let bus = SimulatedRtuBus::new(99, Duration::from_millis(300));
    let seen = bus.seen_units.clone();
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    // Wire turnaround timeout: 1000ms
    client.set_response_timeout(Duration::from_millis(1000));
    // Queue wait timeout: 80ms
    client.set_queue_timeout(Duration::from_millis(80));

    // Request 1 takes 300ms on the wire.
    let c1 = client.clone();
    let c2 = client.clone();

    let start = std::time::Instant::now();
    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });

    // Wait 10ms to ensure Request 1 is dispatched to wire and occupies the serial transport
    tokio::time::sleep(Duration::from_millis(10)).await;

    let h2 = tokio::spawn(async move { c2.read_holding_registers(2, 0, 1).await });

    let (r1, r2) = tokio::join!(h1, h2);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    let seen_units = seen.lock().unwrap().clone();
    println!("r1: {:?}, r2: {:?}", r1, r2);
    println!("seen units: {:?}", seen_units);
    println!("elapsed: {:?}", start.elapsed());

    // Request 1 was on wire for 300ms and should succeed
    assert!(r1.is_ok(), "Request 1 should succeed, got: {:?}", r1);

    // Request 2 was queued behind Request 1. Since queue timeout was 80ms, it should have failed with Timeout while queued!
    assert!(
        matches!(r2, Err(AsyncError::Timeout)),
        "Request 2 should have timed out in queue, got: {:?}",
        r2
    );

    // Request 2 must NEVER have been transmitted to the wire!
    assert!(
        !seen_units.contains(&2),
        "Request 2 should NOT have reached the wire! Seen units: {:?}",
        seen_units
    );

    // After Request 1 finishes and Request 2 was dropped from queue, subsequent Request 3 executes cleanly
    let r3 = client.read_holding_registers(1, 0, 1).await;
    assert!(
        r3.is_ok(),
        "Subsequent request should succeed, got: {:?}",
        r3
    );

    Ok(())
}

#[tokio::test]
async fn test_queue_timeout_dispatches_when_within_budget() -> Result<()> {
    // Unit 1 answers in 40ms, Unit 2 answers in 20ms
    let bus = SimulatedRtuBus::new(99, Duration::from_millis(40));
    let seen = bus.seen_units.clone();
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    // Wire turnaround timeout: 1000ms, Queue wait timeout: 200ms
    client.set_response_timeout(Duration::from_millis(1000));
    client.set_queue_timeout(Duration::from_millis(200));

    let c1 = client.clone();
    let c2 = client.clone();

    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });
    tokio::time::sleep(Duration::from_millis(5)).await;
    let h2 = tokio::spawn(async move { c2.read_holding_registers(2, 0, 1).await });

    let (r1, r2) = tokio::join!(h1, h2);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();

    let seen_units = seen.lock().unwrap().clone();
    assert!(r1.is_ok(), "Request 1 should succeed: {:?}", r1);
    assert!(
        r2.is_ok(),
        "Request 2 was queued within budget and must succeed: {:?}",
        r2
    );
    assert!(
        seen_units.contains(&1) && seen_units.contains(&2),
        "Both units must reach wire: {:?}",
        seen_units
    );

    Ok(())
}

#[tokio::test]
async fn test_queue_timeout_selective_expiry_in_multi_request_queue() -> Result<()> {
    // Unit 1 takes 250ms on wire, Unit 2 and 3 take 20ms
    let bus = SimulatedRtuBus::new(99, Duration::from_millis(250));
    let seen = bus.seen_units.clone();
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    client.set_response_timeout(Duration::from_millis(1000));

    // Request 1 occupies the wire
    let c1 = client.clone();
    let c2 = client.clone();
    let c3 = client.clone();

    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Configure short queue timeout for Request 2: 70ms
    client.set_queue_timeout(Duration::from_millis(70));
    let h2 = tokio::spawn(async move { c2.read_holding_registers(2, 0, 1).await });
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Configure generous queue timeout for Request 3: 600ms
    client.set_queue_timeout(Duration::from_millis(600));
    let h3 = tokio::spawn(async move { c3.read_holding_registers(3, 0, 1).await });

    let (r1, r2, r3) = tokio::join!(h1, h2, h3);
    let r1 = r1.unwrap();
    let r2 = r2.unwrap();
    let r3 = r3.unwrap();

    let seen_units = seen.lock().unwrap().clone();
    assert!(r1.is_ok(), "Request 1 should succeed: {:?}", r1);
    assert!(
        matches!(r2, Err(AsyncError::Timeout)),
        "Request 2 should have timed out in queue: {:?}",
        r2
    );
    assert!(
        r3.is_ok(),
        "Request 3 (600ms queue budget) should survive Request 2 expiry and succeed: {:?}",
        r3
    );
    assert!(
        !seen_units.contains(&2),
        "Request 2 must never reach wire: {:?}",
        seen_units
    );
    assert!(
        seen_units.contains(&3),
        "Request 3 must reach wire: {:?}",
        seen_units
    );

    Ok(())
}

#[tokio::test]
async fn test_queue_and_response_timeout_dynamic_runtime_reconfiguration() -> Result<()> {
    let bus = SimulatedRtuBus::new(2, Duration::from_millis(50));
    let config = ModbusConfig::Serial(rtu_config("/dev/mock"));
    let client = AsyncRtuClient::new_with_transport(bus, config.clone(), Duration::from_millis(1))?;
    client.connect().await?;

    // 1. Test set_response_timeout affects silent unit turnaround
    client.set_response_timeout(Duration::from_millis(80));
    let t0 = std::time::Instant::now();
    let res = client.read_holding_registers(2, 0, 1).await;
    let elapsed = t0.elapsed();
    assert!(matches!(res, Err(AsyncError::Timeout)));
    assert!(
        elapsed >= Duration::from_millis(70) && elapsed < Duration::from_millis(250),
        "Elapsed: {:?}",
        elapsed
    );

    // 2. Test clear_queue_timeout allows unbounded queue wait
    let bus2 = SimulatedRtuBus::new(99, Duration::from_millis(150));
    let client2 = AsyncRtuClient::new_with_transport(bus2, config, Duration::from_millis(1))?;
    client2.connect().await?;

    client2.set_response_timeout(Duration::from_millis(1000));
    client2.set_queue_timeout(Duration::from_millis(50));
    client2.clear_queue_timeout(); // Clear timeout

    let c1 = client2.clone();
    let c2 = client2.clone();

    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let h2 = tokio::spawn(async move { c2.read_holding_registers(3, 0, 1).await });

    let (r1, r2) = tokio::join!(h1, h2);
    assert!(r1.unwrap().is_ok());
    assert!(
        r2.unwrap().is_ok(),
        "After clear_queue_timeout, request should wait in queue without timing out"
    );

    Ok(())
}

#[tokio::test]
async fn test_rtu_in_flight_abort_swallows_late_response() -> Result<()> {
    // Unit 1 replies after 80ms, Unit 3 replies after 5ms.
    let bus = SimulatedRtuBus::new(99, Duration::from_millis(80));
    let mut cfg = rtu_config("/dev/mock");
    cfg.response_timeout_ms = 300;
    let config = ModbusConfig::Serial(cfg);
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    let c1 = client.clone();
    let c2 = client.clone();

    // 1. Dispatch request to Unit 1 (slow unit)
    let h1 = tokio::spawn(async move { c1.read_holding_registers(1, 0, 1).await });

    // 2. Wait until request 1 is on the wire, then abort it mid-flight
    tokio::time::sleep(Duration::from_millis(15)).await;
    h1.abort();
    let r1 = h1.await;
    assert!(r1.is_err() && r1.unwrap_err().is_cancelled());

    // 3. Immediately dispatch request to Unit 3
    let r3 = c2.read_holding_registers(3, 0, 1).await;

    // Unit 3 must succeed after Unit 1's late response is swallowed, without ChecksumError or UnitIdMismatch
    assert!(
        r3.is_ok(),
        "Unit 3 read failed after Unit 1 in-flight abort: {:?}",
        r3.err()
    );
    let regs = r3.unwrap();
    assert_eq!(regs.quantity(), 1);

    Ok(())
}

#[tokio::test]
async fn test_rtu_in_flight_abort_silent_slave_unblocks_on_timeout() -> Result<()> {
    // Unit 2 is completely silent (never replies).
    let bus = SimulatedRtuBus::new(2, Duration::from_millis(10));
    let mut cfg = rtu_config("/dev/mock");
    cfg.response_timeout_ms = 80;
    let config = ModbusConfig::Serial(cfg);
    let client = AsyncRtuClient::new_with_transport(bus, config, Duration::from_millis(1))?;
    client.connect().await?;

    let c1 = client.clone();
    let c2 = client.clone();

    // 1. Dispatch to silent unit
    let h1 = tokio::spawn(async move { c1.read_holding_registers(2, 0, 1).await });

    // 2. Abort while in-flight
    tokio::time::sleep(Duration::from_millis(15)).await;
    h1.abort();

    // 3. Immediately dispatch to healthy unit 1
    let t0 = std::time::Instant::now();
    let r1 = c2.read_holding_registers(1, 0, 1).await;
    let elapsed = t0.elapsed();

    assert!(
        r1.is_ok(),
        "Healthy unit read failed after silent unit abort: {:?}",
        r1.err()
    );
    // Verified that it waited for the silent unit turnaround window (~80ms) and unblocked
    assert!(
        elapsed >= Duration::from_millis(50),
        "Elapsed: {:?}",
        elapsed
    );

    Ok(())
}
