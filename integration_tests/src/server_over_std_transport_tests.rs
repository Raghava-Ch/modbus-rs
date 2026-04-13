use heapless::Vec as HeaplessVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, Transport, TransportError, TransportType,
};
use mbus_server::{
    modbus_app, CoilsModel, HoldingRegistersModel, InputRegistersModel, ServerServices,
};
use modbus_rs::{
    ClientServices, CoilResponse, Coils, MbusError, RegisterResponse, Registers,
    RequestErrorNotifier, StdTcpTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::cell::RefCell;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::vec::Vec;

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
    const TRANSPORT_TYPE: Option<TransportType> = Some(TransportType::StdTcp);

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

    fn recv(&mut self) -> Result<HeaplessVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
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
            Ok(n) => {
                HeaplessVec::from_slice(&buffer[..n]).map_err(|_| TransportError::BufferTooSmall)
            }
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

    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }
}

#[derive(Default)]
struct TestClientApp {
    holding_reads: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Registers)>>,
    input_reads: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Registers)>>,
    coil_reads: RefCell<Vec<(u16, UnitIdOrSlaveAddr, Coils)>>,
    write_single_registers: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16)>>,
    write_multiple_registers: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16)>>,
    write_single_coils: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, bool)>>,
    write_multiple_coils: RefCell<Vec<(u16, UnitIdOrSlaveAddr, u16, u16)>>,
    failed_requests: RefCell<Vec<(u16, UnitIdOrSlaveAddr, MbusError)>>,
}

impl RequestErrorNotifier for TestClientApp {
    fn request_failed(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        self.failed_requests
            .borrow_mut()
            .push((txn_id, unit_id, error));
    }
}

impl TimeKeeper for TestClientApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_millis() as u64
    }
}

#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for TestClientApp {}

impl RegisterResponse for TestClientApp {
    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        self.holding_reads
            .borrow_mut()
            .push((txn_id, unit_id, registers.clone()));
    }

    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        self.input_reads
            .borrow_mut()
            .push((txn_id, unit_id, registers.clone()));
    }

    fn read_single_input_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
    }

    fn read_single_holding_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
    }

    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        self.write_single_registers
            .borrow_mut()
            .push((txn_id, unit_id, address, value));
    }

    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        self.write_multiple_registers
            .borrow_mut()
            .push((txn_id, unit_id, address, quantity));
    }

    fn read_write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _registers: &Registers,
    ) {
    }

    fn read_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) {
    }

    fn mask_write_register_response(&mut self, _txn_id: u16, _unit_id: UnitIdOrSlaveAddr) {}
}

impl CoilResponse for TestClientApp {
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        self.coil_reads
            .borrow_mut()
            .push((txn_id, unit_id, coils.clone()));
    }

    fn read_single_coil_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: bool,
    ) {
    }

    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        self.write_single_coils
            .borrow_mut()
            .push((txn_id, unit_id, address, value));
    }

    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        self.write_multiple_coils
            .borrow_mut()
            .push((txn_id, unit_id, address, quantity));
    }
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn seed_app() -> DemoServer {
    let mut app = DemoServer::default();
    app.holding.set_setpoint(900);
    app.holding.set_mode(2);
    app.input.set_temperature_raw(245);
    app.input.set_pressure_raw(1013);
    app.coils.run_enable = true;
    app.coils.pump_enable = true;
    app.coils.alarm_ack = false;
    app.coils.remote_mode = true;
    app
}

fn spawn_server_once() -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral listener");
    let port = listener.local_addr().expect("listener addr").port();

    let handle = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept one client");
        let transport = AcceptedTcpTransport::new(stream);

        let mut cfg = ModbusTcpConfig::new("127.0.0.1", port).expect("tcp cfg");
        cfg.response_timeout_ms = 100;

        let mut server =
            ServerServices::new(transport, seed_app(), ModbusConfig::Tcp(cfg), unit_id(1));

        server.connect().expect("server connect");

        while server.is_connected() {
            server.poll();
            thread::sleep(Duration::from_millis(1));
        }
    });

    (port, handle)
}

fn spawn_server_for_clients(client_count: usize) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral listener");
    let port = listener.local_addr().expect("listener addr").port();

    let handle = thread::spawn(move || {
        for _ in 0..client_count {
            let (stream, _) = listener.accept().expect("accept client");
            let transport = AcceptedTcpTransport::new(stream);

            let mut cfg = ModbusTcpConfig::new("127.0.0.1", port).expect("tcp cfg");
            cfg.response_timeout_ms = 100;

            let mut server =
                ServerServices::new(transport, seed_app(), ModbusConfig::Tcp(cfg), unit_id(1));

            server.connect().expect("server connect");

            while server.is_connected() {
                server.poll();
                thread::sleep(Duration::from_millis(1));
            }
        }
    });

    (port, handle)
}

fn spawn_server_concurrent(client_count: usize) -> (u16, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral listener");
    let port = listener.local_addr().expect("listener addr").port();

    let handle = thread::spawn(move || {
        let mut workers = Vec::with_capacity(client_count);

        for _ in 0..client_count {
            let (stream, _) = listener.accept().expect("accept client");
            let worker = thread::spawn(move || {
                let transport = AcceptedTcpTransport::new(stream);

                let mut cfg = ModbusTcpConfig::new("127.0.0.1", port).expect("tcp cfg");
                cfg.response_timeout_ms = 100;

                let mut server =
                    ServerServices::new(transport, seed_app(), ModbusConfig::Tcp(cfg), unit_id(1));

                server.connect().expect("server connect");

                while server.is_connected() {
                    server.poll();
                    thread::sleep(Duration::from_millis(1));
                }
            });

            workers.push(worker);
        }

        for worker in workers {
            worker.join().expect("worker join");
        }
    });

    (port, handle)
}

fn new_client(port: u16) -> ClientServices<StdTcpTransport, TestClientApp, 16> {
    let mut cfg = ModbusTcpConfig::new("127.0.0.1", port).expect("client cfg");
    cfg.connection_timeout_ms = 500;
    cfg.response_timeout_ms = 200;

    let mut client = ClientServices::new(
        StdTcpTransport::new(),
        TestClientApp::default(),
        ModbusConfig::Tcp(cfg),
    )
    .expect("create client services");

    client.connect().expect("connect client");
    client
}

fn poll_until(
    client: &mut ClientServices<StdTcpTransport, TestClientApp, 16>,
    ready: impl Fn(&ClientServices<StdTcpTransport, TestClientApp, 16>) -> bool,
) {
    for _ in 0..80 {
        client.poll();
        if ready(client) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for response callback");
}

#[test]
fn server_fc03_fc04_reads_work_via_std_tcp_transport() {
    let (port, server) = spawn_server_once();
    let mut client = new_client(port);

    client
        .read_holding_registers(11, unit_id(1), 0, 2)
        .expect("queue FC03 request");

    poll_until(&mut client, |c| !c.app().holding_reads.borrow().is_empty());

    let holding = client.app().holding_reads.borrow();
    let (txn, uid, regs) = &holding[0];
    assert_eq!(*txn, 11);
    assert_eq!(*uid, unit_id(1));
    assert_eq!(regs.from_address(), 0);
    assert_eq!(regs.quantity(), 2);
    assert_eq!(regs.value(0).expect("reg0"), 900);
    assert_eq!(regs.value(1).expect("reg1"), 2);
    drop(holding);

    client
        .read_input_registers(12, unit_id(1), 0, 2)
        .expect("queue FC04 request");

    poll_until(&mut client, |c| !c.app().input_reads.borrow().is_empty());

    let inputs = client.app().input_reads.borrow();
    let (txn, uid, regs) = &inputs[0];
    assert_eq!(*txn, 12);
    assert_eq!(*uid, unit_id(1));
    assert_eq!(regs.from_address(), 0);
    assert_eq!(regs.quantity(), 2);
    assert_eq!(regs.value(0).expect("in0"), 245);
    assert_eq!(regs.value(1).expect("in1"), 1013);
    drop(inputs);

    drop(client);
    server.join().expect("server join");
}

#[test]
fn server_fc06_fc10_writes_roundtrip_via_std_tcp_transport() {
    let (port, server) = spawn_server_once();
    let mut client = new_client(port);

    client
        .write_single_register(21, unit_id(1), 0, 4321)
        .expect("queue FC06 request");

    poll_until(&mut client, |c| {
        !c.app().write_single_registers.borrow().is_empty()
    });

    client
        .write_multiple_registers(22, unit_id(1), 0, 2, &[123, 456])
        .expect("queue FC10 request");

    poll_until(&mut client, |c| {
        !c.app().write_multiple_registers.borrow().is_empty()
    });

    client
        .read_holding_registers(23, unit_id(1), 0, 2)
        .expect("queue FC03 request");

    poll_until(&mut client, |c| c.app().holding_reads.borrow().len() == 1);

    let holding = client.app().holding_reads.borrow();
    let (_, _, regs) = &holding[0];
    assert_eq!(regs.value(0).expect("reg0"), 123);
    assert_eq!(regs.value(1).expect("reg1"), 456);
    drop(holding);

    assert!(client.app().failed_requests.borrow().is_empty());

    drop(client);
    server.join().expect("server join");
}

#[test]
fn server_fc01_fc05_fc0f_coils_roundtrip_via_std_tcp_transport() {
    let (port, server) = spawn_server_once();
    let mut client = new_client(port);

    client
        .read_multiple_coils(31, unit_id(1), 0, 4)
        .expect("queue FC01 request");

    poll_until(&mut client, |c| !c.app().coil_reads.borrow().is_empty());

    {
        let coils = client.app().coil_reads.borrow();
        let (_, _, bits) = &coils[0];
        assert_eq!(bits.values()[0] & 0x0F, 0b0000_1011);
    }

    client
        .write_single_coil(32, unit_id(1), 2, true)
        .expect("queue FC05 request");

    poll_until(&mut client, |c| {
        !c.app().write_single_coils.borrow().is_empty()
    });

    let mut desired = Coils::new(0, 4).expect("create coil payload");
    desired.set_value(0, true).expect("set c0");
    desired.set_value(1, false).expect("set c1");
    desired.set_value(2, true).expect("set c2");
    desired.set_value(3, false).expect("set c3");

    client
        .write_multiple_coils(33, unit_id(1), 0, &desired)
        .expect("queue FC0F request");

    poll_until(&mut client, |c| {
        !c.app().write_multiple_coils.borrow().is_empty()
    });

    client
        .read_multiple_coils(34, unit_id(1), 0, 4)
        .expect("queue FC01 request");

    poll_until(&mut client, |c| c.app().coil_reads.borrow().len() >= 2);

    let coils = client.app().coil_reads.borrow();
    let (_, _, bits) = &coils[1];
    assert_eq!(bits.values()[0] & 0x0F, 0b0000_0101);
    drop(coils);

    assert!(client.app().failed_requests.borrow().is_empty());

    drop(client);
    server.join().expect("server join");
}

#[test]
fn server_handles_reconnect_churn_via_std_tcp_transport() {
    let session_count = 3;
    let (port, server) = spawn_server_for_clients(session_count);

    for txn in 0..session_count {
        let mut client = new_client(port);
        client
            .read_holding_registers(40 + txn as u16, unit_id(1), 0, 2)
            .expect("queue FC03 request");

        poll_until(&mut client, |c| !c.app().holding_reads.borrow().is_empty());

        let holding = client.app().holding_reads.borrow();
        let (_, _, regs) = &holding[0];
        assert_eq!(regs.value(0).expect("reg0"), 900);
        assert_eq!(regs.value(1).expect("reg1"), 2);
        drop(holding);
        drop(client);
    }

    server.join().expect("server join");
}

#[test]
fn server_invalid_address_surfaces_modbus_exception_via_std_tcp_transport() {
    let (port, server) = spawn_server_once();
    let mut client = new_client(port);

    client
        .read_holding_registers(51, unit_id(1), 50, 1)
        .expect("queue FC03 invalid-address request");

    poll_until(&mut client, |c| {
        !c.app().failed_requests.borrow().is_empty()
    });

    let failures = client.app().failed_requests.borrow();
    let (_, _, err) = failures[0];
    assert_eq!(err, MbusError::ModbusException(0x02));
    drop(failures);

    drop(client);
    server.join().expect("server join");
}

#[test]
fn server_unknown_function_returns_illegal_function_exception() {
    let (port, server) = spawn_server_once();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect raw client");
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("set raw client read timeout");

    // Use FC02 (Read Discrete Inputs): valid frame shape, but this server runtime does not
    // implement FC02 dispatch yet, so it must return Illegal Function.
    // MBAP: txn=0x1234, proto=0, len=0x0006 (unit + fc + addr_hi + addr_lo + qty_hi + qty_lo)
    let req = [
        0x12, 0x34, 0x00, 0x00, 0x00, 0x06, 0x01, 0x02, 0x00, 0x00, 0x00, 0x01,
    ];
    stream.write_all(&req).expect("write FC02 request");

    let mut rsp = [0u8; 9];
    let mut read = 0usize;
    for _ in 0..20 {
        match stream.read(&mut rsp[read..]) {
            Ok(0) => break,
            Ok(n) => {
                read += n;
                if read == rsp.len() {
                    break;
                }
            }
            Err(err)
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => panic!("unexpected socket read error: {err}"),
        }
    }
    assert_eq!(read, rsp.len(), "did not receive full exception ADU");

    assert_eq!(rsp[0], 0x12);
    assert_eq!(rsp[1], 0x34);
    assert_eq!(rsp[2], 0x00);
    assert_eq!(rsp[3], 0x00);
    assert_eq!(rsp[6], 0x01);
    assert_eq!(rsp[7], 0x82); // FC02 + exception bit
    assert_eq!(rsp[8], 0x01); // Illegal Function

    drop(stream);
    server.join().expect("server join");
}

#[test]
fn server_survives_peer_drop_mid_request_and_serves_next_client() {
    let (port, server) = spawn_server_for_clients(2);

    {
        let mut raw = TcpStream::connect(("127.0.0.1", port)).expect("connect raw client");
        // Send partial MBAP/request then drop connection abruptly.
        let partial = [0x00, 0x11, 0x00, 0x00, 0x00];
        raw.write_all(&partial).expect("write partial request");
    }

    let mut client = new_client(port);
    client
        .read_holding_registers(71, unit_id(1), 0, 2)
        .expect("queue FC03 request");

    poll_until(&mut client, |c| !c.app().holding_reads.borrow().is_empty());

    let holding = client.app().holding_reads.borrow();
    let (_, _, regs) = &holding[0];
    assert_eq!(regs.value(0).expect("reg0"), 900);
    assert_eq!(regs.value(1).expect("reg1"), 2);
    drop(holding);

    drop(client);
    server.join().expect("server join");
}

#[test]
fn server_reassembles_fragmented_request_frames() {
    let (port, server) = spawn_server_once();
    let mut raw = TcpStream::connect(("127.0.0.1", port)).expect("connect raw client");
    raw.set_read_timeout(Some(Duration::from_millis(500)))
        .expect("set read timeout");

    // FC03: read 2 holding registers from address 0, txn=0x002A
    let req = [
        0x00, 0x2A, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x02,
    ];

    raw.write_all(&req[..3]).expect("write chunk1");
    thread::sleep(Duration::from_millis(20));
    raw.write_all(&req[3..7]).expect("write chunk2");
    thread::sleep(Duration::from_millis(20));
    raw.write_all(&req[7..]).expect("write chunk3");

    let mut rsp = [0u8; 13];
    raw.read_exact(&mut rsp).expect("read FC03 response");

    assert_eq!(rsp[0], 0x00);
    assert_eq!(rsp[1], 0x2A);
    assert_eq!(rsp[2], 0x00);
    assert_eq!(rsp[3], 0x00);
    assert_eq!(rsp[4], 0x00);
    assert_eq!(rsp[5], 0x07);
    assert_eq!(rsp[6], 0x01);
    assert_eq!(rsp[7], 0x03);
    assert_eq!(rsp[8], 0x04);
    assert_eq!(&rsp[9..13], &[0x03, 0x84, 0x00, 0x02]);

    drop(raw);
    server.join().expect("server join");
}

#[test]
fn server_handles_concurrent_clients_without_deadlock() {
    let client_count = 4usize;
    let (port, server) = spawn_server_concurrent(client_count);

    let barrier = Arc::new(Barrier::new(client_count));
    let mut clients = Vec::with_capacity(client_count);

    for i in 0..client_count {
        let gate = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            let mut client = new_client(port);
            gate.wait();

            let base = 80 + (i as u16) * 10;

            client
                .read_holding_registers(base, unit_id(1), 0, 2)
                .expect("queue FC03 request");
            poll_until(&mut client, |c| !c.app().holding_reads.borrow().is_empty());

            client
                .write_single_coil(base + 1, unit_id(1), 0, i % 2 == 0)
                .expect("queue FC05 request");
            poll_until(&mut client, |c| {
                !c.app().write_single_coils.borrow().is_empty()
            });

            client
                .read_multiple_coils(base + 2, unit_id(1), 0, 4)
                .expect("queue FC01 request");
            poll_until(&mut client, |c| !c.app().coil_reads.borrow().is_empty());

            assert!(client.app().failed_requests.borrow().is_empty());
        });

        clients.push(handle);
    }

    for client in clients {
        client.join().expect("client thread join");
    }

    server.join().expect("server join");
}
