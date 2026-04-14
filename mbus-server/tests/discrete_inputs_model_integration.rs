#![cfg(feature = "discrete-inputs")]

mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use mbus_core::function_codes::public::FunctionCode;
use mbus_server::{DiscreteInputsModel, ResilienceConfig, ServerServices, modbus_app};

#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

#[derive(Debug, Default, DiscreteInputsModel)]
struct StatusInputs {
    #[discrete_input(addr = 0)]
    power_ok: bool,
    #[discrete_input(addr = 1)]
    estop: bool,
    #[discrete_input(addr = 2)]
    guard_closed: bool,
    #[discrete_input(addr = 3)]
    remote_enabled: bool,
}

#[derive(Debug, Default, DiscreteInputsModel)]
struct AlertInputs {
    #[discrete_input(addr = 100)]
    over_temp: bool,
    #[discrete_input(addr = 101)]
    low_pressure: bool,
    #[discrete_input(addr = 102)]
    io_fault: bool,
    #[discrete_input(addr = 103)]
    sensor_mismatch: bool,
}

#[derive(Debug, Default)]
#[modbus_app(discrete_inputs(status, alerts))]
struct App {
    status: StatusInputs,
    alerts: AlertInputs,
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for App {}

fn run_once(payload: &[u8], app: App) -> Vec<u8> {
    let request = build_request(1, unit_id(1), FunctionCode::ReadDiscreteInputs, payload);
    let sent_frames = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent_frames),
        connected: true,
    };

    let mut server: ServerServices<MockTransport, App> = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    server.poll();

    let frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(frames.len(), 1);
    frames[0].clone()
}

#[test]
fn derived_discrete_inputs_route_first_map() {
    let app = App {
        status: StatusInputs {
            power_ok: true,
            estop: false,
            guard_closed: true,
            remote_enabled: true,
        },
        alerts: AlertInputs::default(),
    };

    let response = run_once(&[0x00, 0x00, 0x00, 0x04], app);

    assert_eq!(response[7], 0x02);
    assert_eq!(response[8], 1);
    assert_eq!(response[9], 0b0000_1101);
}

#[test]
fn derived_discrete_inputs_route_second_map() {
    let app = App {
        status: StatusInputs::default(),
        alerts: AlertInputs {
            over_temp: false,
            low_pressure: true,
            io_fault: false,
            sensor_mismatch: true,
        },
    };

    let response = run_once(&[0x00, 0x64, 0x00, 0x04], app);

    assert_eq!(response[7], 0x02);
    assert_eq!(response[8], 1);
    assert_eq!(response[9], 0b0000_1010);
}

#[test]
fn derived_discrete_inputs_gap_returns_exception() {
    let app = App::default();

    let response = run_once(&[0x00, 0x04, 0x00, 0x01], app);

    assert_eq!(response[7], 0x82);
    assert_eq!(response[8], 0x02);
}
