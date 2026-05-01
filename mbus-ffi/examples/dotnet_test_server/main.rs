//! Tiny stand-alone Modbus TCP server used by the .NET integration tests.
//!
//! Run with `cargo run -p mbus-ffi --example dotnet_test_server --features dotnet,registers`.
//! Binds to a random port on `127.0.0.1`, prints `LISTENING <port>` to
//! stdout (so the parent process can parse it), and serves an in-memory
//! holding-register store for FC03/FC06/FC16 until killed.

#![cfg(all(feature = "registers", not(target_arch = "wasm32")))]

use std::future::Future;
use std::sync::{Arc, Mutex};

use mbus_async::server::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[derive(Clone)]
struct TestApp {
    holding: Arc<Mutex<[u16; 256]>>,
}

impl Default for TestApp {
    fn default() -> Self {
        Self {
            holding: Arc::new(Mutex::new([0u16; 256])),
        }
    }
}

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
                FunctionCode::ReadCoils,
                mbus_core::errors::ExceptionCode::IllegalFunction,
            ),
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let unit = UnitIdOrSlaveAddr::try_from(1u8).unwrap();
    let server = AsyncTcpServer::bind("127.0.0.1:0", unit).await?;
    let port = server.local_addr()?.port();

    // The parent C# test fixture parses this exact line.
    println!("LISTENING {port}");

    let app = TestApp::default();
    loop {
        let Ok((mut session, _)) = server.accept().await else {
            break;
        };
        let mut app_instance = app.clone();
        tokio::spawn(async move {
            let _ = session.run(&mut app_instance).await;
        });
    }
    Ok(())
}
