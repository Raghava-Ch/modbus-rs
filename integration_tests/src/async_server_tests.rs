//! Integration tests for the async Modbus TCP server.
//!
//! These tests spin up a real `AsyncTcpServer` on a random port and use the
//! async Modbus TCP client to send requests, verifying that the full
//! encode → network → decode → app → encode → network → decode round-trip works.

use anyhow::Result;
use mbus_async::server::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};
#[cfg(feature = "diagnostics")]
use mbus_async::client::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_async::client::SubRequest;
use mbus_async::AsyncTcpClient;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use std::future::Future;
#[cfg(feature = "holding-registers")]
use std::sync::Arc;
#[cfg(feature = "holding-registers")]
use tokio::sync::Mutex;

// ── helpers ──────────────────────────────────────────────────────────────────

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

/// Connect an async TCP client to the given port on localhost.
async fn connect_client(port: u16) -> Result<AsyncTcpClient> {
    let client = AsyncTcpClient::new("127.0.0.1", port)?;
    client.connect().await?;
    Ok(client)
}

// ── test app: simple in-memory register + coil store ─────────────────────────

/// Minimal in-memory app used in non-macro tests.
#[derive(Clone, Default)]
struct TestApp {
    /// 16 holding registers, writable.
    holding: [u16; 16],
    /// 16 coils, writable.
    coils: [bool; 16],
    /// 16 input registers (read-only from the client's perspective).
    input: [u16; 16],
    /// 16 discrete inputs (read-only from the client's perspective).
    discrete_inputs: [bool; 16],
}

impl AsyncAppHandler for TestApp {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let response = self.process(req);
        std::future::ready(response)
    }
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncTrafficNotifier for TestApp {}

impl TestApp {
    fn process(&mut self, req: ModbusRequest) -> ModbusResponse {
        match req {
            // FC01 — Read Coils
            ModbusRequest::ReadCoils { address, count, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.coils.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadCoils,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                // Pack bits, LSB of first byte = coil at `address`
                let byte_count = (cnt + 7) / 8;
                let mut bytes = vec![0u8; byte_count];
                for i in 0..cnt {
                    if self.coils[addr + i] {
                        bytes[i / 8] |= 1 << (i % 8);
                    }
                }
                ModbusResponse::packed_bits(FunctionCode::ReadCoils, &bytes)
            }
            // FC05 — Write Single Coil
            ModbusRequest::WriteSingleCoil { address, value, .. } => {
                let addr = address as usize;
                if addr >= self.coils.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteSingleCoil,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                self.coils[addr] = value;
                ModbusResponse::echo_coil(address, value)
            }
            // FC0F — Write Multiple Coils
            ModbusRequest::WriteMultipleCoils { address, count, data, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.coils.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteMultipleCoils,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                for i in 0..cnt {
                    let byte_idx = i / 8;
                    let bit_idx = i % 8;
                    if byte_idx < data.len() {
                        self.coils[addr + i] = (data[byte_idx] >> bit_idx) & 1 == 1;
                    }
                }
                ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleCoils, address, count)
            }
            // FC03 — Read Holding Registers
            ModbusRequest::ReadHoldingRegisters { address, count, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.holding.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadHoldingRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                ModbusResponse::registers(FunctionCode::ReadHoldingRegisters, &self.holding[addr..addr + cnt])
            }
            // FC06 — Write Single Register
            ModbusRequest::WriteSingleRegister { address, value, .. } => {
                let addr = address as usize;
                if addr >= self.holding.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteSingleRegister,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                self.holding[addr] = value;
                ModbusResponse::echo_register(address, value)
            }
            // FC10 — Write Multiple Registers
            ModbusRequest::WriteMultipleRegisters { address, count, data, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.holding.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteMultipleRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                for i in 0..cnt {
                    let hi = data.get(i * 2).copied().unwrap_or(0);
                    let lo = data.get(i * 2 + 1).copied().unwrap_or(0);
                    self.holding[addr + i] = u16::from_be_bytes([hi, lo]);
                }
                ModbusResponse::echo_multi_write(
                    FunctionCode::WriteMultipleRegisters,
                    address,
                    count,
                )
            }
            // FC04 — Read Input Registers
            ModbusRequest::ReadInputRegisters { address, count, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.input.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadInputRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                ModbusResponse::registers(FunctionCode::ReadInputRegisters, &self.input[addr..addr + cnt])
            }
            // FC02 — Read Discrete Inputs
            ModbusRequest::ReadDiscreteInputs { address, count, .. } => {
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > self.discrete_inputs.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadDiscreteInputs,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                let byte_count = (cnt + 7) / 8;
                let mut bytes = vec![0u8; byte_count];
                for i in 0..cnt {
                    if self.discrete_inputs[addr + i] {
                        bytes[i / 8] |= 1 << (i % 8);
                    }
                }
                ModbusResponse::packed_bits(FunctionCode::ReadDiscreteInputs, &bytes)
            }
            // FC16 — Mask Write Register
            ModbusRequest::MaskWriteRegister { address, and_mask, or_mask, .. } => {
                let addr = address as usize;
                if addr >= self.holding.len() {
                    return ModbusResponse::exception(
                        FunctionCode::MaskWriteRegister,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                let old = self.holding[addr];
                self.holding[addr] = (old & and_mask) | (or_mask & !and_mask);
                ModbusResponse::echo_mask_write(address, and_mask, or_mask)
            }
            // FC19 — Read/Write Multiple Registers
            ModbusRequest::ReadWriteMultipleRegisters {
                read_address, read_count, write_address, write_count, ref data, ..
            } => {
                let r_addr = read_address as usize;
                let r_cnt = read_count as usize;
                let w_addr = write_address as usize;
                let w_cnt = write_count as usize;
                if r_addr + r_cnt > self.holding.len() || w_addr + w_cnt > self.holding.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadWriteMultipleRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                // Write first (per Modbus spec).
                for i in 0..w_cnt {
                    let hi = data.get(i * 2).copied().unwrap_or(0);
                    let lo = data.get(i * 2 + 1).copied().unwrap_or(0);
                    self.holding[w_addr + i] = u16::from_be_bytes([hi, lo]);
                }
                // Then read.
                ModbusResponse::registers(
                    FunctionCode::ReadWriteMultipleRegisters,
                    &self.holding[r_addr..r_addr + r_cnt],
                )
            }
            // FC18 — Read FIFO Queue
            #[cfg(feature = "fifo")]
            ModbusRequest::ReadFifoQueue { pointer_address, .. } => {
                if pointer_address == 0x0005 {
                    // Return 2-element FIFO: count=2, values [0x001A, 0x002B]
                    let payload: &[u8] = &[0x00, 0x02, 0x00, 0x1A, 0x00, 0x2B];
                    ModbusResponse::fifo_response(payload)
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadFifoQueue,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    )
                }
            }
            // FC14 — Read File Record
            #[cfg(feature = "file-record")]
            ModbusRequest::ReadFileRecord { sub_requests, .. } => {
                // Build a sub-response block per sub-request: [sub_len(1), ref_type=0x06(1), data(sub_len-1)...]
                // Each request gets 2 dummy u16 words: [0xAA, 0xBB] (4 bytes data), sub_len = 1+4 = 5
                let word_a = 0xAABBu16;
                let word_b = 0xCCDDu16;
                let mut payload: Vec<u8> = Vec::new();
                for _ in 0..sub_requests.len() {
                    // sub_len = ref_type(1) + data(4) = 5
                    payload.push(5);       // sub_len
                    payload.push(0x06);    // ref_type (always 6 per spec)
                    payload.extend_from_slice(&word_a.to_be_bytes());
                    payload.extend_from_slice(&word_b.to_be_bytes());
                }
                ModbusResponse::read_file_record_response(&payload)
            }
            // FC15 — Write File Record (echo back PDU data)
            #[cfg(feature = "file-record")]
            ModbusRequest::WriteFileRecord { raw_pdu_data, .. } => {
                ModbusResponse::echo_write_file_record(raw_pdu_data)
            }
            // FC2B — Encapsulated Interface Transport (MEI 0x0E Read Device ID)
            #[cfg(feature = "diagnostics")]
            ModbusRequest::EncapsulatedInterfaceTransport { mei_type, .. } => {
                if mei_type == 0x0E {
                    // Two objects: VendorName (0x00) = "Test", ProductCode (0x01) = "V1"
                    let mut objects: Vec<u8> = Vec::new();
                    let vendor = b"Test";
                    objects.push(0x00);                        // Object ID: VendorName
                    objects.push(vendor.len() as u8);
                    objects.extend_from_slice(vendor);
                    let product = b"V1";
                    objects.push(0x01);                        // Object ID: ProductCode
                    objects.push(product.len() as u8);
                    objects.extend_from_slice(product);
                    ModbusResponse::read_device_id(
                        0x01,  // read_device_id_code: Basic
                        0x01,  // conformity_level
                        false, // more_follows
                        0x00,  // next_object_id
                        &objects,
                    )
                } else {
                    ModbusResponse::exception(
                        FunctionCode::EncapsulatedInterfaceTransport,
                        mbus_core::errors::ExceptionCode::IllegalFunction,
                    )
                }
            }
            // Serial-only diagnostics FCs — parsed and dispatched by the server;
            // reachable here via raw TCP socket tests that bypass client-side check_serial().
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReadExceptionStatus { .. } => {
                // Reports 8 coil-like exception status bits; return a fixed byte for testing.
                ModbusResponse::read_exception_status(0xA5)
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::Diagnostics { sub_function, data, .. } => {
                // Echo the sub-function and data back to the client.
                ModbusResponse::diagnostics_echo(sub_function, data)
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventCounter { .. } => {
                // status = 0x0000 (ready), event_count = 5
                ModbusResponse::comm_event_counter(0x0000, 0x0005)
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventLog { .. } => {
                // payload: status(2) + event_count(2) + msg_count(2) + events...
                let payload = &[0x00u8, 0x00, 0x00, 0x05, 0x00, 0x02, 0xAB, 0xCD];
                ModbusResponse::comm_event_log(payload)
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReportServerId { .. } => {
                // server_id_bytes + run_indicator (0xFF = running)
                let payload = &[0x01u8, 0x02, 0xFF];
                ModbusResponse::report_server_id(payload)
            }
            // Catch-all
            _ => ModbusResponse::NoResponse,
        }
    }
}

// ── helpers to start a server in a background task ───────────────────────────

/// Bind an `AsyncTcpServer` to a random port and spawn the serve loop.
/// Returns the bound port.
async fn start_server(app: TestApp) -> Result<u16> {
    start_server_custom(app).await
}

/// Generic variant of `start_server` that accepts any `AsyncAppHandler + Clone`.
async fn start_server_custom<APP>(app: APP) -> Result<u16>
where
    APP: AsyncAppHandler + Clone + Send + 'static,
{
    let server = AsyncTcpServer::bind("127.0.0.1:0", unit_id(1)).await?;
    let port = server.local_addr()?.port();
    tokio::spawn(async move {
        let server = server;
        loop {
            let Ok((mut session, _)) = server.accept().await else { break };
            let mut app_instance = app.clone();
            tokio::spawn(async move {
                let _ = session.run(&mut app_instance).await;
            });
        }
    });
    Ok(port)
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// FC03 — read holding registers that were pre-seeded.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc03_read_holding_registers() -> Result<()> {
    let mut app = TestApp::default();
    app.holding[0] = 0x1234;
    app.holding[1] = 0x5678;
    app.holding[2] = 0x9ABC;

    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let regs = client.read_holding_registers(1, 0, 3).await?;
    let qty = regs.quantity() as usize;
    assert_eq!(qty, 3);
    assert_eq!(regs.values()[0], 0x1234);
    assert_eq!(regs.values()[1], 0x5678);
    assert_eq!(regs.values()[2], 0x9ABC);
    Ok(())
}

/// FC06 — write then read a single holding register.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc06_write_single_register_echo() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // The server echoes back address + value for FC06.
    client.write_single_register(1, 5, 0xBEEF).await?;

    // A subsequent read should return the written value (same connection, same task).
    let regs = client.read_holding_registers(1, 5, 1).await?;
    assert_eq!(regs.values()[0], 0xBEEF);
    Ok(())
}

/// FC06 — write beyond the address space returns an exception.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc06_write_out_of_range_returns_exception() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // Address 100 is outside the 16-element test array.
    let err = client.write_single_register(1, 100, 1).await;
    assert!(
        err.is_err(),
        "expected error for out-of-range write, got Ok"
    );
    Ok(())
}

/// FC10 — write multiple registers, then read them back.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc10_write_multiple_registers() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    client
        .write_multiple_registers(1, 2, &[0x0001, 0x0002, 0x0003])
        .await?;

    let regs = client.read_holding_registers(1, 2, 3).await?;
    assert_eq!(regs.quantity(), 3);
    assert_eq!(regs.values()[0], 0x0001);
    assert_eq!(regs.values()[1], 0x0002);
    assert_eq!(regs.values()[2], 0x0003);
    Ok(())
}

/// FC04 — read input registers.
#[cfg(feature = "input-registers")]
#[tokio::test]
async fn fc04_read_input_registers() -> Result<()> {
    let mut app = TestApp::default();
    app.input[0] = 0xCAFE;
    app.input[1] = 0xBABE;

    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let regs = client.read_input_registers(1, 0, 2).await?;
    assert_eq!(regs.values()[0], 0xCAFE);
    assert_eq!(regs.values()[1], 0xBABE);
    Ok(())
}

/// FC01 — read coils.
#[cfg(feature = "coils")]
#[tokio::test]
async fn fc01_read_coils() -> Result<()> {
    let mut app = TestApp::default();
    // Set coil pattern: T F T F T F T F (address 0..7)
    for i in (0..8usize).step_by(2) {
        app.coils[i] = true;
    }

    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    assert_eq!(coils.quantity(), 8);
    // LSB of packed byte → coil 0 = ON → 0x55 = 0101_0101
    assert_eq!(coils.values()[0], 0x55);
    Ok(())
}

/// FC05 — write single coil and read it back.
#[cfg(feature = "coils")]
#[tokio::test]
async fn fc05_write_single_coil() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    client.write_single_coil(1, 3, true).await?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    // Coil 3 (zero-indexed) is ON: bit 3 = 0b0000_1000 = 0x08
    assert_eq!(coils.values()[0] & 0x08, 0x08, "coil 3 should be ON");
    // All other low bits should be off
    assert_eq!(coils.values()[0] & !0x08, 0x00, "no other coils should be set");
    Ok(())
}

/// FC0F — write multiple coils and read them back.
#[cfg(feature = "coils")]
#[tokio::test]
async fn fc0f_write_multiple_coils() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // Write 8 coils starting at address 0: alternating ON/OFF = 0xAA (bits 1,3,5,7 ON)
    let mut coils_to_write = mbus_core::models::coil::Coils::new(0, 8)?;
    for i in [1u16, 3, 5, 7] {
        coils_to_write.set_value(i, true)?;
    }
    client.write_multiple_coils(1, 0, &coils_to_write).await?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    assert_eq!(coils.values()[0], 0xAA, "coil pattern should be 0xAA");
    Ok(())
}

/// Shared-state server: two sequential clients share the same `Arc<Mutex<TestApp>>`.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn shared_state_server_arc_mutex() -> Result<()> {
    let app = Arc::new(Mutex::new(TestApp::default()));

    let server = AsyncTcpServer::bind("127.0.0.1:0", unit_id(1)).await?;
    let port = server.local_addr()?.port();

    // Spawn the server accept loop with Arc<Mutex<TestApp>>
    let shared = Arc::clone(&app);
    tokio::spawn(async move {
        loop {
            let Ok((mut session, _)) = server.accept().await else { break };
            let app_clone = Arc::clone(&shared);
            tokio::spawn(async move {
                let mut handler = app_clone;
                let _ = session.run(&mut handler).await;
            });
        }
    });

    // Client 1: write register 0 = 0x1111
    let c1 = connect_client(port).await?;
    c1.write_single_register(1, 0, 0x1111).await?;
    drop(c1);

    // Client 2: write register 1 = 0x2222
    let c2 = connect_client(port).await?;
    c2.write_single_register(1, 1, 0x2222).await?;
    drop(c2);

    // Verify both writes ended up in the shared app
    let guard = app.lock().await;
    assert_eq!(guard.holding[0], 0x1111, "client 1 write should be visible");
    assert_eq!(guard.holding[1], 0x2222, "client 2 write should be visible");
    Ok(())
}

/// Verify a second client on a different connection also gets served.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn two_concurrent_clients() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    let c1 = connect_client(port).await?;
    let c2 = connect_client(port).await?;

    // Two independent writes from two independent clients
    c1.write_single_register(1, 0, 0xAAAA).await?;
    c2.write_single_register(1, 0, 0xBBBB).await?;

    // Each client's last write is the one it sees on its own connection
    // (TestApp is cloned per session so changes aren't shared — that is expected here).
    let r1 = c1.read_holding_registers(1, 0, 1).await?;
    let r2 = c2.read_holding_registers(1, 0, 1).await?;

    assert_eq!(r1.values()[0], 0xAAAA);
    assert_eq!(r2.values()[0], 0xBBBB);
    Ok(())
}

/// FC02 — read discrete inputs.
#[cfg(feature = "discrete-inputs")]
#[tokio::test]
async fn fc02_read_discrete_inputs() -> Result<()> {
    let mut app = TestApp::default();
    // Set inputs 0, 2, 4, 6 ON → packed bits = 0x55 (0101_0101)
    for i in [0usize, 2, 4, 6] {
        app.discrete_inputs[i] = true;
    }

    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let inputs = client.read_discrete_inputs(1, 0, 8).await?;
    assert_eq!(inputs.quantity(), 8);
    assert_eq!(inputs.values()[0], 0x55, "expected alternating ON/OFF pattern 0x55");
    Ok(())
}

/// FC02 — read discrete inputs beyond range returns exception.
#[cfg(feature = "discrete-inputs")]
#[tokio::test]
async fn fc02_read_discrete_inputs_out_of_range() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // TestApp has 16 discrete inputs — reading 5 from address 14 goes out of range.
    let err = client.read_discrete_inputs(1, 14, 5).await;
    assert!(
        err.is_err(),
        "expected error for out-of-range discrete input read"
    );
    Ok(())
}

/// FC03 — read holding registers beyond range returns exception.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc03_read_out_of_range_returns_exception() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // TestApp has 16 registers — reading 5 from address 14 goes out of range.
    let err = client.read_holding_registers(1, 14, 5).await;
    assert!(
        err.is_err(),
        "expected error for out-of-range holding register read"
    );
    Ok(())
}

/// FC01 — read coils beyond range returns exception.
#[cfg(feature = "coils")]
#[tokio::test]
async fn fc01_read_coils_out_of_range_returns_exception() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // TestApp has 16 coils — reading 5 starting at address 14 goes out of range.
    let err = client.read_multiple_coils(1, 14, 5).await;
    assert!(
        err.is_err(),
        "expected error for out-of-range coil read"
    );
    Ok(())
}

/// FC16 — mask-write a register and verify the result.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc16_mask_write_register() -> Result<()> {
    let mut app = TestApp::default();
    app.holding[2] = 0b1111_0000_1111_0000u16; // initial value

    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // and_mask = 0xFF00 keeps only the high byte, or_mask = 0x00AA sets specific low bits.
    // result = (0xF0F0 & 0xFF00) | (0x00AA & 0x00FF) = 0xF000 | 0x00AA = 0xF0AA
    client.mask_write_register(1, 2, 0xFF00, 0x00AA).await?;

    let regs = client.read_holding_registers(1, 2, 1).await?;
    assert_eq!(regs.values()[0], 0xF0AA, "mask-write result mismatch");
    Ok(())
}

/// FC16 — mask-write to an out-of-range address returns exception.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc16_mask_write_register_out_of_range() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let err = client.mask_write_register(1, 100, 0xFF00, 0x00FF).await;
    assert!(err.is_err(), "expected error for out-of-range mask-write");
    Ok(())
}

/// FC17 — write registers then read a different (overlapping) window back.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc17_read_write_multiple_registers() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    // Write registers 4, 5, 6 and read back registers 5, 6, 7 (7 stays 0).
    let read_regs = client
        .read_write_multiple_registers(1, 5, 3, 4, &[0xAAAA, 0xBBBB, 0xCCCC])
        .await?;

    assert_eq!(read_regs.quantity(), 3);
    assert_eq!(read_regs.values()[0], 0xBBBB, "reg 5 after fc17");
    assert_eq!(read_regs.values()[1], 0xCCCC, "reg 6 after fc17");
    assert_eq!(read_regs.values()[2], 0x0000, "reg 7 should still be zero");
    Ok(())
}

/// FC17 — write and read to out-of-range addresses return exceptions.
#[cfg(feature = "holding-registers")]
#[tokio::test]
async fn fc17_read_write_multiple_registers_out_of_range() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let err = client
        .read_write_multiple_registers(1, 14, 5, 0, &[0x0001])
        .await;
    assert!(err.is_err(), "expected error for out-of-range FC17 read");
    Ok(())
}

/// FC18 — read a 2-element FIFO queue from pointer address 0x0005.
#[cfg(feature = "fifo")]
#[tokio::test]
async fn fc18_read_fifo_queue() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let queue = client.read_fifo_queue(1, 0x0005).await?;
    assert_eq!(queue.length(), 2, "FIFO should report 2 elements");
    let values = queue.queue();
    assert_eq!(values[0], 0x001A, "first FIFO value");
    assert_eq!(values[1], 0x002B, "second FIFO value");
    Ok(())
}

/// FC14 — read file record: 1 sub-request, expect 2 dummy words back.
#[cfg(feature = "file-record")]
#[tokio::test]
async fn fc14_read_file_record() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let mut sub_request = SubRequest::new();
    sub_request.add_read_sub_request(1, 0, 2)?;
    let result = client.read_file_record(1, &sub_request).await?;
    assert_eq!(result.len(), 1, "one sub-response expected");
    let entry = &result[0];
    let data = entry.record_data.as_ref().expect("read sub_request should have data");
    assert_eq!(data[0], 0xAABB, "first word");
    assert_eq!(data[1], 0xCCDD, "second word");
    Ok(())
}

/// FC15 — write file record: success (echo response matches request).
#[cfg(feature = "file-record")]
#[tokio::test]
async fn fc15_write_file_record() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let mut data_buf: heapless::Vec<u16, 252> = heapless::Vec::new();
    data_buf.extend_from_slice(&[0x0001u16, 0x0002, 0x0003]).ok();
    let mut sub_request = SubRequest::new();
    sub_request.add_write_sub_request(1, 0, 3, data_buf)?;
    // write_file_record returns Ok(()) on a matching echo response
    client.write_file_record(1, &sub_request).await?;
    Ok(())
}

/// FC2B/MEI 0x0E — read device identification: basic stream of objects.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc2b_read_device_identification() -> Result<()> {
    let app = TestApp::default();
    let port = start_server(app).await?;
    let client = connect_client(port).await?;

    let resp = client
        .read_device_identification(1, ReadDeviceIdCode::Basic, ObjectId::from(0x00))
        .await?;

    // VendorName (0x00) = "Test"
    let mut found_vendor = false;
    let mut found_product = false;
    for obj in resp.objects() {
        let obj = obj?;
        let id = u8::from(obj.object_id);
        let val = core::str::from_utf8(&obj.value).unwrap_or("");
        if id == 0x00 {
            assert_eq!(val, "Test", "VendorName mismatch");
            found_vendor = true;
        } else if id == 0x01 {
            assert_eq!(val, "V1", "ProductCode mismatch");
            found_product = true;
        }
    }
    assert!(found_vendor, "VendorName object missing");
    assert!(found_product, "ProductCode object missing");
    Ok(())
}

// ── Raw TCP helpers for serial-only FCs ──────────────────────────────────────
//
// FC07 / FC08 / FC0B / FC0C / FC11 are serial-only per the Modbus spec, so the
// async client enforces check_serial() and refuses to send them over TCP.
// The *server* has no such restriction — it parses whatever bytes arrive.
// These helpers bypass the client by writing hand-built MBAP+PDU frames directly
// to a raw TcpStream, letting us exercise the full server dispatch path.

/// Build a 6-byte Modbus TCP MBAP header.
///
/// `pdu_len` = number of bytes in the PDU (FC byte + payload). The MBAP `length`
/// field encodes `1 (unit_id) + pdu_len`.
fn mbap(txn_id: u16, unit_id: u8, pdu: &[u8]) -> Vec<u8> {
    let length = (1 + pdu.len()) as u16;
    let mut frame = Vec::with_capacity(6 + pdu.len());
    frame.extend_from_slice(&txn_id.to_be_bytes()); // Transaction ID
    frame.extend_from_slice(&[0x00, 0x00]);          // Protocol ID
    frame.extend_from_slice(&length.to_be_bytes());  // Length
    frame.push(unit_id);                             // Unit ID
    frame.extend_from_slice(pdu);                    // PDU
    frame
}

/// Send `request` to the server at `port`, read the complete MBAP + PDU response.
async fn raw_tcp_roundtrip(port: u16, request: &[u8]) -> Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await?;
    stream.write_all(request).await?;

    // Read the 6-byte MBAP header first to learn the PDU length.
    let mut header = [0u8; 6];
    stream.read_exact(&mut header).await?;
    let body_len = u16::from_be_bytes([header[4], header[5]]) as usize; // unit(1) + PDU
    let mut body = vec![0u8; body_len];
    stream.read_exact(&mut body).await?;

    let mut full = header.to_vec();
    full.extend_from_slice(&body);
    Ok(full)
}

/// FC07 — Read Exception Status (serial-only): send raw ADU, verify 1-byte status response.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc07_read_exception_status_raw() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // PDU = [FC=0x07] (no payload; FC07 request is function-code only)
    let request = mbap(0x0001, 1, &[0x07]);
    let response = raw_tcp_roundtrip(port, &request).await?;

    // MBAP(6) + unit(1) + fc(1) + status(1) = 9 bytes total
    assert_eq!(response.len(), 9, "FC07 response length");
    assert_eq!(response[6], 1,    "unit id echoed");
    assert_eq!(response[7], 0x07, "FC byte");
    assert_eq!(response[8], 0xA5, "exception status byte");
    Ok(())
}

/// FC08 — Diagnostics (serial-only): echo Return Query Data (sub_fn=0x0000).
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc08_diagnostics_echo_raw() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // PDU = [FC=0x08, sub_fn=0x0000(2), data=0x1234(2)]
    let request = mbap(0x0002, 1, &[0x08, 0x00, 0x00, 0x12, 0x34]);
    let response = raw_tcp_roundtrip(port, &request).await?;

    // MBAP(6) + unit(1) + fc(1) + sub_fn(2) + data(2) = 12 bytes
    assert_eq!(response.len(), 12, "FC08 response length");
    assert_eq!(response[7], 0x08, "FC byte");
    assert_eq!(&response[8..10],  &[0x00, 0x00], "sub-function echoed");
    assert_eq!(&response[10..12], &[0x12, 0x34], "data echoed");
    Ok(())
}

/// FC0B — Get Comm Event Counter (serial-only): verify 2-word response.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc0b_get_comm_event_counter_raw() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // PDU = [FC=0x0B] (no payload)
    let request = mbap(0x0003, 1, &[0x0B]);
    let response = raw_tcp_roundtrip(port, &request).await?;

    // MBAP(6) + unit(1) + fc(1) + status_word(2) + event_count(2) = 12 bytes
    assert_eq!(response.len(), 12, "FC0B response length");
    assert_eq!(response[7], 0x0B, "FC byte");
    let status = u16::from_be_bytes([response[8],  response[9]]);
    let count  = u16::from_be_bytes([response[10], response[11]]);
    assert_eq!(status, 0x0000, "status word");
    assert_eq!(count,  0x0005, "event count");
    Ok(())
}

/// FC0C — Get Comm Event Log (serial-only): verify byte-count-prefixed payload.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc0c_get_comm_event_log_raw() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // PDU = [FC=0x0C] (no payload)
    let request = mbap(0x0004, 1, &[0x0C]);
    let response = raw_tcp_roundtrip(port, &request).await?;

    // MBAP(6) + unit(1) + fc(1) + byte_count(1) + payload(8) = 17 bytes
    assert_eq!(response[7], 0x0C, "FC byte");
    let byte_count = response[8] as usize;
    assert_eq!(response.len(), 6 + 1 + 1 + 1 + byte_count, "frame length");
    // payload: status(0x0000) + event_count(0x0005) + msg_count(0x0002) + events[0xAB,0xCD]
    assert_eq!(&response[9..11],  &[0x00, 0x00], "status");
    assert_eq!(&response[11..13], &[0x00, 0x05], "event count");
    assert_eq!(&response[13..15], &[0x00, 0x02], "msg count");
    assert_eq!(response[15], 0xAB, "first event byte");
    assert_eq!(response[16], 0xCD, "second event byte");
    Ok(())
}

/// FC11 — Report Server ID (serial-only): verify byte-count-prefixed payload.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc11_report_server_id_raw() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // PDU = [FC=0x11] (no payload)
    let request = mbap(0x0005, 1, &[0x11]);
    let response = raw_tcp_roundtrip(port, &request).await?;

    // MBAP(6) + unit(1) + fc(1) + byte_count(1) + payload(3) = 12 bytes
    assert_eq!(response[7], 0x11, "FC byte");
    let byte_count = response[8] as usize;
    assert_eq!(response.len(), 6 + 1 + 1 + 1 + byte_count, "frame length");
    // payload: [0x01, 0x02, 0xFF]
    assert_eq!(response[9],  0x01, "server id byte 0");
    assert_eq!(response[10], 0x02, "server id byte 1");
    assert_eq!(response[11], 0xFF, "run indicator (0xFF = running)");
    Ok(())
}

// ── Session behavior tests ────────────────────────────────────────────────────

/// Helpers for persistent raw-TCP multi-request tests.
async fn raw_tcp_connect(port: u16) -> Result<tokio::net::TcpStream> {
    Ok(tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await?)
}

async fn raw_tcp_send(stream: &mut tokio::net::TcpStream, data: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    stream.write_all(data).await?;
    Ok(())
}

/// Read exactly one MBAP+PDU frame from `stream`, with a 250 ms timeout.
/// Returns `None` if the timeout expires (i.e. the server sent no response).
async fn raw_tcp_recv_timeout(
    stream: &mut tokio::net::TcpStream,
) -> Option<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut header = [0u8; 6];
    tokio::time::timeout(
        std::time::Duration::from_millis(250),
        stream.read_exact(&mut header),
    )
    .await
    .ok()?
    .ok()?;
    let body_len = u16::from_be_bytes([header[4], header[5]]) as usize;
    let mut body = vec![0u8; body_len];
    tokio::time::timeout(
        std::time::Duration::from_millis(250),
        stream.read_exact(&mut body),
    )
    .await
    .ok()?
    .ok()?;
    let mut full = header.to_vec();
    full.extend_from_slice(&body);
    Some(full)
}

/// FC08/0x0004 enables Listen Only Mode; FC08/0x0001 clears it.
///
/// Verifies:
/// - ForceListenOnlyMode (0x0004) produces no response.
/// - Non-FC08 requests while in listen-only are silently dropped (no response).
/// - RestartCommunicationsOption (0x0001) echoes back and clears the mode.
/// - Normal requests are handled again after restart.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc08_listen_only_mode_and_restart() -> Result<()> {
    let port = start_server(TestApp::default()).await?;
    let mut stream = raw_tcp_connect(port).await?;

    // 1. Baseline: FC03 read holding registers works normally.
    raw_tcp_send(&mut stream, &mbap(0x0001, 1, &[0x03, 0x00, 0x00, 0x00, 0x01])).await?;
    let resp = raw_tcp_recv_timeout(&mut stream).await.expect("FC03 before listen-only");
    assert_eq!(resp[7], 0x03, "FC03 FC byte before listen-only");

    // 2. FC08/0x0004 — Force Listen Only Mode → no response (per Modbus spec).
    raw_tcp_send(&mut stream, &mbap(0x0002, 1, &[0x08, 0x00, 0x04, 0x00, 0x00])).await?;
    assert!(
        raw_tcp_recv_timeout(&mut stream).await.is_none(),
        "ForceListenOnlyMode must produce no response"
    );

    // 3. FC03 while in listen-only → silently dropped, no response.
    raw_tcp_send(&mut stream, &mbap(0x0003, 1, &[0x03, 0x00, 0x00, 0x00, 0x01])).await?;
    assert!(
        raw_tcp_recv_timeout(&mut stream).await.is_none(),
        "FC03 in listen-only must produce no response"
    );

    // 4. FC08/0x0001 — Restart Communications Option → echo + clears listen-only.
    raw_tcp_send(&mut stream, &mbap(0x0004, 1, &[0x08, 0x00, 0x01, 0x00, 0x00])).await?;
    let resp = raw_tcp_recv_timeout(&mut stream).await.expect("RestartComms echo");
    assert_eq!(resp[7], 0x08, "FC08 byte in restart response");
    assert_eq!(
        u16::from_be_bytes([resp[8], resp[9]]),
        0x0001,
        "sub-function 0x0001 echoed"
    );

    // 5. FC03 should work again now that listen-only is cleared.
    raw_tcp_send(&mut stream, &mbap(0x0005, 1, &[0x03, 0x00, 0x00, 0x00, 0x01])).await?;
    let resp = raw_tcp_recv_timeout(&mut stream).await.expect("FC03 after restart");
    assert_eq!(resp[7], 0x03, "FC03 FC byte after listen-only cleared");

    Ok(())
}

/// FC08/0x0000 — ReturnQueryData: session handles the loopback echo without forwarding
/// to the application handler.  Uses a TestApp that panics if Diagnostics 0x0000 reaches it.
#[cfg(feature = "diagnostics")]
#[tokio::test]
async fn fc08_loopback_handled_by_session_not_app() -> Result<()> {
    // App that panics on Diagnostics 0x0000 — proves the session intercepted it.
    #[derive(Clone, Default)]
    struct PanicOnLoopback;
    impl AsyncAppHandler for PanicOnLoopback {
        fn handle(
            &mut self,
            req: ModbusRequest,
        ) -> impl Future<Output = ModbusResponse> + Send {
            let resp = match req {
                ModbusRequest::Diagnostics { sub_function: 0x0000, .. } => {
                    panic!("session should have intercepted sub-function 0x0000")
                }
                _ => ModbusResponse::NoResponse,
            };
            std::future::ready(resp)
        }
    }
    #[cfg(feature = "traffic")]
    impl mbus_async::server::AsyncTrafficNotifier for PanicOnLoopback {}

    let port = start_server_custom(PanicOnLoopback).await?;

    // PDU = [FC=0x08, sub=0x0000, data=0xABCD]
    let response = raw_tcp_roundtrip(port, &mbap(0x0001, 1, &[0x08, 0x00, 0x00, 0xAB, 0xCD])).await?;
    assert_eq!(response[7], 0x08, "FC byte in echo response");
    assert_eq!(u16::from_be_bytes([response[8], response[9]]), 0x0000, "sub-function echoed");
    assert_eq!(u16::from_be_bytes([response[10], response[11]]), 0xABCD, "data echoed");

    Ok(())
}

/// FC08 statistics counter sub-functions (diagnostics-stats feature).
///
/// Verifies that ReturnServerMessageCount (0x000E) and ReturnBusExceptionErrorCount (0x000D)
/// return accurate counts after dispatching requests.
#[cfg(feature = "diagnostics-stats")]
#[tokio::test]
async fn fc08_diagnostics_stats_counters() -> Result<()> {
    let port = start_server(TestApp::default()).await?;
    let mut stream = raw_tcp_connect(port).await?;

    // Send three FC03 requests (valid, address 0, count 1).
    for txn in 1u16..=3 {
        raw_tcp_send(&mut stream, &mbap(txn, 1, &[0x03, 0x00, 0x00, 0x00, 0x01])).await?;
        raw_tcp_recv_timeout(&mut stream).await.expect("FC03 response");
    }

    // Send one FC03 with an out-of-range address to trigger an exception.
    raw_tcp_send(&mut stream, &mbap(0x0004, 1, &[0x03, 0xFF, 0xFF, 0x00, 0x01])).await?;
    let exc = raw_tcp_recv_timeout(&mut stream).await.expect("exception response");
    assert_eq!(exc[7], 0x03 | 0x80, "exception FC byte");

    // FC08/0x000E — ReturnServerMessageCount → expect 4 (3 normal + 1 exception).
    raw_tcp_send(&mut stream, &mbap(0x0005, 1, &[0x08, 0x00, 0x0E, 0x00, 0x00])).await?;
    let stat = raw_tcp_recv_timeout(&mut stream).await.expect("server_message_count response");
    let count = u16::from_be_bytes([stat[10], stat[11]]);
    assert_eq!(count, 4, "server_message_count should be 4");

    // FC08/0x000D — ReturnBusExceptionErrorCount → expect 1.
    raw_tcp_send(&mut stream, &mbap(0x0006, 1, &[0x08, 0x00, 0x0D, 0x00, 0x00])).await?;
    let stat = raw_tcp_recv_timeout(&mut stream).await.expect("exception_error_count response");
    let exc_count = u16::from_be_bytes([stat[10], stat[11]]);
    assert_eq!(exc_count, 1, "exception_error_count should be 1");

    // FC08/0x000A — ClearCounters → all counters reset to 0.
    raw_tcp_send(&mut stream, &mbap(0x0007, 1, &[0x08, 0x00, 0x0A, 0x00, 0x00])).await?;
    raw_tcp_recv_timeout(&mut stream).await.expect("clear counters echo");

    // After clear: server_message_count should be 0.
    raw_tcp_send(&mut stream, &mbap(0x0008, 1, &[0x08, 0x00, 0x0E, 0x00, 0x00])).await?;
    let stat = raw_tcp_recv_timeout(&mut stream).await.expect("server_message_count after clear");
    let count_after = u16::from_be_bytes([stat[10], stat[11]]);
    assert_eq!(count_after, 0, "server_message_count should be 0 after clear");

    Ok(())
}

/// Framing error (truly unknown FC byte) — server silently discards, no response.
///
/// A byte value not in the `FunctionCode` enum (e.g. 0x20) fails `FunctionCode::try_from`
/// inside `Pdu::from_bytes`, so `parse_adu` returns a `FramingError`.  The session
/// increments `comm_error_count` and continues the loop — no response is sent.
///
/// Note: feature-disabled-but-valid FCs (e.g. FC03 when `registers` is off) would reach
/// `ModbusRequest::Unknown` and receive an `IllegalFunction` exception.  That path can
/// only be exercised in a partial-feature build and is verified by code review and the
/// `Unknown` intercept in `session.run()`.
#[tokio::test]
async fn framing_error_fc_gets_no_response() -> Result<()> {
    let port = start_server(TestApp::default()).await?;

    // 0x20 is not in the FunctionCode enum: try_from returns UnsupportedFunction,
    // and parse_adu yields a FramingError → no response from the server.
    let mut stream = raw_tcp_connect(port).await?;
    raw_tcp_send(&mut stream, &mbap(0x0001, 1, &[0x20, 0x00, 0x00])).await?;
    let response = raw_tcp_recv_timeout(&mut stream).await;
    assert!(response.is_none(), "server must not respond to an unrecognised FC byte");

    // Verify the session is still alive: a valid request succeeds afterwards.
    raw_tcp_send(&mut stream, &mbap(0x0002, 1, &[0x07])).await?;
    let alive = raw_tcp_recv_timeout(&mut stream).await.expect("FC07 succeeds after framing error");
    assert_eq!(alive[7], 0x07, "FC07 response proves session survived");

    Ok(())
}

/// Verify the broadcast-writes configuration API.
///
/// The actual serial-specific broadcast discard (when `enable_broadcast_writes = false`)
/// requires a loopback serial transport to exercise the `is_serial_type()` guard.
/// Here we verify the API surface: default is disabled, setting it works, and a session
/// with it enabled starts up and processes normal requests correctly.
#[tokio::test]
async fn broadcast_write_suppression_config_api() -> Result<()> {
    let server = AsyncTcpServer::bind("127.0.0.1:0", unit_id(1)).await?;
    let port = server.local_addr()?.port();
    tokio::spawn(async move {
        loop {
            let Ok((mut session, _)) = server.accept().await else { break };
            assert!(!session.broadcast_writes_enabled(), "default must be false");
            session.set_broadcast_writes(true);
            assert!(session.broadcast_writes_enabled());
            session.set_broadcast_writes(false);
            assert!(!session.broadcast_writes_enabled());

            let mut app_instance = TestApp::default();
            tokio::spawn(async move {
                let _ = session.run(&mut app_instance).await;
            });
        }
    });

    // A normal roundtrip confirms the session started and the assertions above passed.
    let response = raw_tcp_roundtrip(port, &mbap(0x0001, 1, &[0x07])).await?;
    assert_eq!(response[7], 0x07, "FC07 proves session started correctly");

    Ok(())
}

// ── #[async_modbus_app] macro tests ──────────────────────────────────────────

/// Verify `#[async_modbus_app]` generates a working `AsyncAppHandler` impl.
///
/// Uses the macro to derive dispatch for holding registers and coils, then
/// runs a real TCP server against the standard async client to confirm all
/// generated read/write paths work end-to-end.
#[cfg(feature = "async-server")]
#[tokio::test]
async fn async_modbus_app_macro_read_write_roundtrip() -> Result<()> {
    use mbus_macros::{async_modbus_app, CoilsModel, HoldingRegistersModel};

    #[derive(Debug, Default, Clone, HoldingRegistersModel)]
    struct Regs {
        #[reg(addr = 0)]
        val0: u16,
        #[reg(addr = 1)]
        val1: u16,
    }

    #[derive(Debug, Default, Clone, CoilsModel)]
    struct Coils {
        #[coil(addr = 0)]
        c0: bool,
        #[coil(addr = 1)]
        c1: bool,
    }

    #[async_modbus_app(holding_registers(regs), coils(coils))]
    #[derive(Debug, Default, Clone)]
    struct MacroApp {
        regs: Regs,
        coils: Coils,
    }

    #[cfg(feature = "traffic")]
    impl mbus_async::server::AsyncTrafficNotifier for MacroApp {}

    let port = start_server_custom(MacroApp::default()).await?;
    let mut client = connect_client(port).await?;

    // Write then read holding register.
    client.write_single_register(1u8, 0, 42).await?;
    let regs = client.read_holding_registers(1u8, 0, 1).await?;
    assert_eq!(regs.value(0).unwrap(), 42);

    // Write then read coil.
    client.write_single_coil(1u8, 0, true).await?;
    let coils = client.read_multiple_coils(1u8, 0, 1).await?;
    assert!(coils.value(0).unwrap());

    Ok(())
}

// ── AsyncTrafficNotifier tests ────────────────────────────────────────────────

/// Verify that `AsyncTrafficNotifier::on_rx_frame` and `on_tx_frame` are called
/// for each successful request/response cycle.
#[cfg(all(feature = "async-server", feature = "async-traffic"))]
#[tokio::test]
async fn traffic_notifier_rx_tx_counts() -> Result<()> {
    use mbus_async::server::AsyncTrafficNotifier;
    use mbus_core::errors::MbusError;
    use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

    #[derive(Clone, Default)]
    struct Counters {
        rx: Arc<AtomicU32>,
        tx: Arc<AtomicU32>,
    }

    #[derive(Clone)]
    struct TrafficApp {
        counters: Counters,
    }
    impl AsyncAppHandler for TrafficApp {
        fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
            let resp = match req {
                #[cfg(feature = "diagnostics")]
                ModbusRequest::ReadExceptionStatus { .. } => {
                    ModbusResponse::read_exception_status(0x00)
                }
                _ => ModbusResponse::NoResponse,
            };
            std::future::ready(resp)
        }
    }
    impl AsyncTrafficNotifier for TrafficApp {
        fn on_rx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {
            self.counters.rx.fetch_add(1, Ordering::SeqCst);
        }
        fn on_tx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {
            self.counters.tx.fetch_add(1, Ordering::SeqCst);
        }
        fn on_rx_error(
            &mut self,
            _txn_id: u16,
            _unit: UnitIdOrSlaveAddr,
            _error: MbusError,
            _frame: &[u8],
        ) {
        }
    }

    let counters = Counters::default();
    let app = TrafficApp { counters: counters.clone() };
    let port = start_server_custom(app).await?;

    // Send 3 FC07 (ReadExceptionStatus) requests — each fully dispatched to app
    // and responded to. Expect on_rx_frame and on_tx_frame once per round-trip.
    let _r1 = raw_tcp_roundtrip(port, &mbap(0x0001, 1, &[0x07])).await?;
    let _r2 = raw_tcp_roundtrip(port, &mbap(0x0002, 1, &[0x07])).await?;
    let _r3 = raw_tcp_roundtrip(port, &mbap(0x0003, 1, &[0x07])).await?;

    assert_eq!(counters.rx.load(Ordering::SeqCst), 3, "on_rx_frame called for each dispatched request");
    assert_eq!(counters.tx.load(Ordering::SeqCst), 3, "on_tx_frame called for each sent response");

    Ok(())
}
