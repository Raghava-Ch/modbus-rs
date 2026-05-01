//! End-to-end integration test for the `dotnet` C-ABI surface.
//!
//! Spins up an in-process [`AsyncTcpServer`] backed by a tiny in-memory app,
//! drives every public `mbus_dn_*` entry point through the same pointer/buffer
//! discipline a P/Invoke caller would use from C#, and asserts on round-trip
//! values.  No managed runtime is involved here — the equivalent C# test
//! lives under `mbus-ffi/dotnet/ModbusRs.Tests/`.

#![cfg(all(feature = "dotnet", feature = "registers", not(target_arch = "wasm32")))]

use std::ffi::CString;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use mbus_async::server::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;

use mbus_ffi::dotnet::client::{
    MbusDnTcpClient, mbus_dn_tcp_client_connect, mbus_dn_tcp_client_disconnect,
    mbus_dn_tcp_client_free, mbus_dn_tcp_client_has_pending_requests, mbus_dn_tcp_client_new,
    mbus_dn_tcp_client_read_holding_registers, mbus_dn_tcp_client_set_request_timeout_ms,
    mbus_dn_tcp_client_write_multiple_registers, mbus_dn_tcp_client_write_single_register,
};
use mbus_ffi::dotnet::status::MbusDnStatus;

// ── Test fixture: a tiny in-memory holding register app ──────────────────────

#[derive(Clone, Default)]
struct TestApp {
    holding: Arc<Mutex<[u16; 16]>>,
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncTrafficNotifier for TestApp {}

impl AsyncAppHandler for TestApp {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let response = self.process(req);
        std::future::ready(response)
    }
}

impl TestApp {
    fn process(&self, req: ModbusRequest) -> ModbusResponse {
        match req {
            ModbusRequest::ReadHoldingRegisters { address, count, .. } => {
                let regs = self.holding.lock().unwrap();
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > regs.len() {
                    return ModbusResponse::exception(
                        FunctionCode::ReadHoldingRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                ModbusResponse::registers(
                    FunctionCode::ReadHoldingRegisters,
                    &regs[addr..addr + cnt],
                )
            }
            ModbusRequest::WriteSingleRegister { address, value, .. } => {
                let mut regs = self.holding.lock().unwrap();
                let addr = address as usize;
                if addr >= regs.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteSingleRegister,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                regs[addr] = value;
                ModbusResponse::echo_register(address, value)
            }
            ModbusRequest::WriteMultipleRegisters {
                address,
                count,
                data,
                ..
            } => {
                let mut regs = self.holding.lock().unwrap();
                let addr = address as usize;
                let cnt = count as usize;
                if addr + cnt > regs.len() {
                    return ModbusResponse::exception(
                        FunctionCode::WriteMultipleRegisters,
                        mbus_core::errors::ExceptionCode::IllegalDataAddress,
                    );
                }
                for i in 0..cnt {
                    let hi = data.get(i * 2).copied().unwrap_or(0);
                    let lo = data.get(i * 2 + 1).copied().unwrap_or(0);
                    regs[addr + i] = u16::from_be_bytes([hi, lo]);
                }
                ModbusResponse::echo_multi_write(
                    FunctionCode::WriteMultipleRegisters,
                    address,
                    count,
                )
            }
            _ => ModbusResponse::exception(
                FunctionCode::ReadCoils, // arbitrary; unused features
                mbus_core::errors::ExceptionCode::IllegalFunction,
            ),
        }
    }
}

fn unit(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

/// Bind a server on its own runtime thread so the .NET-binding's shared
/// runtime (used by the FFI calls below) is fully isolated from it — this
/// mirrors how a real out-of-process C# host would interact with it.
fn start_server(app: TestApp) -> Result<u16> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<u16, String>>();
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("server runtime");
        rt.block_on(async move {
            let server = match AsyncTcpServer::bind("127.0.0.1:0", unit(1)).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(format!("bind failed: {e:?}")));
                    return;
                }
            };
            let port = server.local_addr().unwrap().port();
            let _ = tx.send(Ok(port));
            loop {
                let Ok((mut session, _)) = server.accept().await else {
                    break;
                };
                let mut app_instance = app.clone();
                tokio::spawn(async move {
                    let _ = session.run(&mut app_instance).await;
                });
            }
        });
    });
    rx.recv()?.map_err(|e| anyhow::anyhow!(e))
}

// ── helpers ──────────────────────────────────────────────────────────────────

struct ClientGuard {
    handle: *mut MbusDnTcpClient,
}

impl ClientGuard {
    fn new(host: &str, port: u16) -> Self {
        let host_c = CString::new(host).unwrap();
        let handle = unsafe { mbus_dn_tcp_client_new(host_c.as_ptr(), port) };
        assert!(!handle.is_null(), "mbus_dn_tcp_client_new returned null");
        Self { handle }
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        unsafe { mbus_dn_tcp_client_free(self.handle) };
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[test]
fn lifecycle_null_handles_are_handled_gracefully() {
    // Free of null is a documented no-op.
    unsafe { mbus_dn_tcp_client_free(std::ptr::null_mut()) };

    // Calls against null handle return MbusErrNullPointer rather than crashing.
    let s = unsafe { mbus_dn_tcp_client_connect(std::ptr::null_mut()) };
    assert_eq!(s, MbusDnStatus::MbusErrNullPointer);

    let s = unsafe { mbus_dn_tcp_client_disconnect(std::ptr::null_mut()) };
    assert_eq!(s, MbusDnStatus::MbusErrNullPointer);

    let s = unsafe { mbus_dn_tcp_client_set_request_timeout_ms(std::ptr::null_mut(), 100) };
    assert_eq!(s, MbusDnStatus::MbusErrNullPointer);

    assert_eq!(
        unsafe { mbus_dn_tcp_client_has_pending_requests(std::ptr::null_mut()) },
        0
    );
}

#[test]
fn new_with_invalid_host_returns_null() {
    // null host pointer
    let h = unsafe { mbus_dn_tcp_client_new(std::ptr::null(), 502) };
    assert!(h.is_null());

    // non-UTF-8 host bytes (0xFF is not valid UTF-8)
    let bad = [0xFFu8, 0u8];
    let h = unsafe { mbus_dn_tcp_client_new(bad.as_ptr() as *const _, 502) };
    assert!(h.is_null());
}

#[test]
fn round_trip_holding_register_read_write() -> Result<()> {
    let mut initial = [0u16; 16];
    initial[0] = 0x1111;
    initial[1] = 0x2222;
    initial[2] = 0x3333;
    let app = TestApp {
        holding: Arc::new(Mutex::new(initial)),
    };
    let port = start_server(app.clone())?;

    let client = ClientGuard::new("127.0.0.1", port);

    // Connect.
    let s = unsafe { mbus_dn_tcp_client_connect(client.handle) };
    assert_eq!(s, MbusDnStatus::MbusOk, "connect failed: {s:?}");

    // Set a generous 5-second per-request timeout.
    let s = unsafe { mbus_dn_tcp_client_set_request_timeout_ms(client.handle, 5_000) };
    assert_eq!(s, MbusDnStatus::MbusOk);

    // ── FC03 — read 3 holding registers ─────────────────────────────────────
    let mut buf = [0u16; 8];
    let mut count: u16 = 0;
    let s = unsafe {
        mbus_dn_tcp_client_read_holding_registers(
            client.handle,
            1,
            0,
            3,
            buf.as_mut_ptr(),
            buf.len() as u16,
            &mut count,
        )
    };
    assert_eq!(s, MbusDnStatus::MbusOk, "read FC03 failed: {s:?}");
    assert_eq!(count, 3);
    assert_eq!(&buf[..3], &[0x1111, 0x2222, 0x3333]);

    // ── FC03 — buffer-too-small is reported, not a crash ────────────────────
    let mut tiny = [0u16; 1];
    let s = unsafe {
        mbus_dn_tcp_client_read_holding_registers(
            client.handle,
            1,
            0,
            3,
            tiny.as_mut_ptr(),
            tiny.len() as u16,
            &mut count,
        )
    };
    assert_eq!(s, MbusDnStatus::MbusErrBufferTooSmall);

    // ── FC06 — write a single holding register ──────────────────────────────
    let mut echo_addr: u16 = 0;
    let mut echo_val: u16 = 0;
    let s = unsafe {
        mbus_dn_tcp_client_write_single_register(
            client.handle,
            1,
            5,
            0xBEEF,
            &mut echo_addr,
            &mut echo_val,
        )
    };
    assert_eq!(s, MbusDnStatus::MbusOk, "write FC06 failed: {s:?}");
    assert_eq!((echo_addr, echo_val), (5, 0xBEEF));

    // FC06 also accepts null out pointers.
    let s = unsafe {
        mbus_dn_tcp_client_write_single_register(
            client.handle,
            1,
            6,
            0xCAFE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(s, MbusDnStatus::MbusOk);

    // ── FC16 — write 3 holding registers ────────────────────────────────────
    let payload = [0xAAAAu16, 0xBBBB, 0xCCCC];
    let mut echo_addr: u16 = 0;
    let mut echo_qty: u16 = 0;
    let s = unsafe {
        mbus_dn_tcp_client_write_multiple_registers(
            client.handle,
            1,
            8,
            payload.as_ptr(),
            payload.len() as u16,
            &mut echo_addr,
            &mut echo_qty,
        )
    };
    assert_eq!(s, MbusDnStatus::MbusOk, "write FC16 failed: {s:?}");
    assert_eq!((echo_addr, echo_qty), (8, 3));

    // FC16 with quantity=0 is rejected before dispatch.
    let s = unsafe {
        mbus_dn_tcp_client_write_multiple_registers(
            client.handle,
            1,
            8,
            payload.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(s, MbusDnStatus::MbusErrInvalidQuantity);

    // ── verify the writes actually landed in the app store ──────────────────
    {
        let regs = app.holding.lock().unwrap();
        assert_eq!(regs[5], 0xBEEF);
        assert_eq!(regs[6], 0xCAFE);
        assert_eq!(&regs[8..11], &[0xAAAA, 0xBBBB, 0xCCCC]);
    }

    // ── readback via FC03 confirms the same ─────────────────────────────────
    let mut buf = [0u16; 8];
    let mut count: u16 = 0;
    let s = unsafe {
        mbus_dn_tcp_client_read_holding_registers(
            client.handle,
            1,
            5,
            6,
            buf.as_mut_ptr(),
            buf.len() as u16,
            &mut count,
        )
    };
    assert_eq!(s, MbusDnStatus::MbusOk);
    assert_eq!(count, 6);
    assert_eq!(&buf[..6], &[0xBEEF, 0xCAFE, 0, 0xAAAA, 0xBBBB, 0xCCCC]);

    // ── disconnect cleanly ──────────────────────────────────────────────────
    let s = unsafe { mbus_dn_tcp_client_disconnect(client.handle) };
    assert_eq!(s, MbusDnStatus::MbusOk);

    // Give the worker a moment to settle the disconnect notification.
    thread::sleep(Duration::from_millis(50));
    Ok(())
}
