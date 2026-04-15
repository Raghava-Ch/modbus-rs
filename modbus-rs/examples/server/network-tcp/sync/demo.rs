use anyhow::{Context, Result};
use core::cell::RefCell;
use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use mbus_server::{
    CoilsModel, ForwardingApp, HoldingRegistersModel, InputRegistersModel, ModbusAppAccess,
    ResilienceConfig, ServerServices, modbus_app,
};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

#[derive(Debug, Default, HoldingRegistersModel)]
struct HoldingRegs {
    #[reg(addr = 0)]
    setpoint: u16,
    #[reg(addr = 1)]
    mode: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputRegs {
    #[reg(addr = 0)]
    temperature_raw: u16,
    #[reg(addr = 1)]
    pressure_raw: u16,
}

#[derive(Debug, Default, CoilsModel)]
struct CoilBank {
    #[coil(addr = 0)]
    run_enable: bool,
    #[coil(addr = 1)]
    pump_enable: bool,
    #[coil(addr = 2)]
    alarm_ack: bool,
    #[coil(addr = 3)]
    remote_mode: bool,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(holding), input_registers(input), coils(coils))]
struct DemoServer {
    holding: HoldingRegs,
    input: InputRegs,
    coils: CoilBank,
}

#[derive(Debug)]
struct AcceptedTcpTransport {
    stream: TcpStream,
    connected: bool,
}

impl AcceptedTcpTransport {
    fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            connected: true,
        }
    }

    fn map_io_error(err: std::io::Error) -> TransportError {
        match err.kind() {
            ErrorKind::TimedOut | ErrorKind::WouldBlock => TransportError::Timeout,
            ErrorKind::UnexpectedEof
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NotConnected => TransportError::ConnectionClosed,
            _ => TransportError::IoError,
        }
    }
}

impl Transport for AcceptedTcpTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        let tcp_cfg = match config {
            ModbusConfig::Tcp(v) => v,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        let timeout = Duration::from_millis(tcp_cfg.response_timeout_ms as u64);

        self.stream
            .set_read_timeout(Some(timeout))
            .map_err(Self::map_io_error)?;
        self.stream
            .set_write_timeout(Some(timeout))
            .map_err(Self::map_io_error)?;
        let _ = self.stream.set_nodelay(true);

        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        let _ = self.stream.shutdown(Shutdown::Both);
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        let result = self
            .stream
            .write_all(adu)
            .and_then(|()| self.stream.flush());
        if let Err(err) = result {
            let mapped = Self::map_io_error(err);
            if mapped == TransportError::ConnectionClosed {
                self.connected = false;
            }
            return Err(mapped);
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        self.stream
            .set_nonblocking(true)
            .map_err(Self::map_io_error)?;

        let mut buffer = [0u8; MAX_ADU_FRAME_LEN];
        let read_result = self.stream.read(&mut buffer);

        let _ = self.stream.set_nonblocking(false);

        match read_result {
            Ok(0) => {
                self.connected = false;
                Err(TransportError::ConnectionClosed)
            }
            Ok(n) => Vec::from_slice(&buffer[..n]).map_err(|_| TransportError::BufferTooSmall),
            Err(err) => {
                let mapped = Self::map_io_error(err);
                if mapped == TransportError::ConnectionClosed {
                    self.connected = false;
                }
                Err(mapped)
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn seed_app() -> DemoServer {
    let mut app = DemoServer::default();

    app.input.set_temperature_raw(245);
    app.input.set_pressure_raw(1013);

    app.holding.set_setpoint(900);
    app.holding.set_mode(2);

    app.coils.run_enable = true;
    app.coils.pump_enable = true;
    app.coils.alarm_ack = false;
    app.coils.remote_mode = true;

    app
}

/// Per-worker app holder that uses interior mutability instead of OS locks.
///
/// This demonstrates that `ForwardingApp` works without `Arc<Mutex<_>>` when the
/// app is not shared across threads. Each worker owns one instance.
#[derive(Debug)]
struct OwnedWorkerApp {
    app: RefCell<DemoServer>,
}

impl OwnedWorkerApp {
    fn new(app: DemoServer) -> Self {
        Self {
            app: RefCell::new(app),
        }
    }
}

impl ModbusAppAccess for OwnedWorkerApp {
    type App = DemoServer;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        let mut app = self.app.borrow_mut();
        f(&mut app)
    }
}

fn worker_loop(
    worker_id: usize,
    stream: TcpStream,
    bind_host: &str,
    bind_port: u16,
    unit: UnitIdOrSlaveAddr,
) {
    let peer = stream
        .peer_addr()
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let transport = AcceptedTcpTransport::new(stream);

    let mut tcp_cfg = match ModbusTcpConfig::new(bind_host, bind_port) {
        Ok(cfg) => cfg,
        Err(err) => {
            eprintln!("[worker-{worker_id}] invalid tcp config: {err}");
            return;
        }
    };
    tcp_cfg.response_timeout_ms = 100;

    let config = ModbusConfig::Tcp(tcp_cfg);
    let access = OwnedWorkerApp::new(seed_app());
    let app = ForwardingApp::new(access);
    let mut server = ServerServices::new(transport, app, config, unit, ResilienceConfig::default());

    if let Err(err) = server.connect() {
        eprintln!("[worker-{worker_id}] connect failed for {peer}: {err}");
        return;
    }

    println!("[worker-{worker_id}] client connected: {peer}");

    while server.is_connected() {
        server.poll();
        thread::sleep(Duration::from_millis(1));
    }

    println!("[worker-{worker_id}] client disconnected: {peer}");
}

fn main() -> Result<()> {
    let bind = std::env::var("MBUS_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:5502".to_string());
    let unit_raw = std::env::var("MBUS_SERVER_UNIT")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);

    let unit = unit_id(unit_raw);
    let listener = TcpListener::bind(&bind).with_context(|| format!("failed to bind {bind}"))?;
    listener
        .set_nonblocking(false)
        .context("failed to set blocking listener mode")?;

    let (bind_host, bind_port) = bind
        .rsplit_once(':')
        .map(|(host, port)| (host.to_string(), port.parse::<u16>().unwrap_or(5502)))
        .unwrap_or_else(|| ("127.0.0.1".to_string(), 5502));

    println!("Modbus TCP demo server listening on {bind}");
    println!("Unit id: {}", unit.get());
    println!("Supported now: FC01, FC03, FC04, FC05, FC06, FC0F, FC10");
    println!("Try from client tool: read holding 0..1, read inputs 0..1, read coils 0..3");

    let next_worker_id = AtomicUsize::new(1);

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                println!(
                    "Accepted connection from {}",
                    stream
                        .peer_addr()
                        .unwrap_or_else(|_| "unknown".parse().unwrap())
                );
                let worker_id = next_worker_id.fetch_add(1, Ordering::Relaxed);
                let host = bind_host.clone();
                thread::spawn(move || {
                    worker_loop(worker_id, stream, &host, bind_port, unit);
                });
            }
            Err(err) => {
                let mapped = match err.kind() {
                    ErrorKind::WouldBlock | ErrorKind::TimedOut => MbusError::Timeout,
                    _ => MbusError::IoError,
                };
                eprintln!("listener accept error: {mapped}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Ok(())
}
