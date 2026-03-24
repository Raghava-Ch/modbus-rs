#![cfg_attr(
    not(all(
        feature = "coils",
        feature = "registers",
        feature = "discrete-inputs",
        feature = "diagnostics",
        feature = "fifo",
        feature = "file-record"
    )),
    allow(unused_imports, unused_variables, dead_code)
)]

use modbus_rs::{
    ClientServices, CoilResponse, Coils, DeviceIdentificationResponse, DiagnosticSubFunction,
    DiagnosticsResponse, DiscreteInputResponse, DiscreteInputs, EncapsulatedInterfaceType,
    FifoQueue, FifoQueueResponse, FileRecordResponse, MAX_ADU_FRAME_LEN, MbusError, ModbusConfig,
    ModbusTcpConfig, ObjectId, ReadDeviceIdCode, RegisterResponse, Registers, RequestErrorNotifier,
    SubRequest, SubRequestParams, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};

/// Minimal transport used only to demonstrate facade-style API access.
#[derive(Debug, Default)]
struct MockTransport;

impl Transport for MockTransport {
    type Error = MbusError;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn send(&mut self, _adu: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }

    fn recv(&mut self) -> Result<heapless::Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        Err(MbusError::Timeout)
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }
}

#[derive(Debug, Default)]
struct App;

impl RequestErrorNotifier for App {
    fn request_failed(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "request_failed: txn={} unit={} error={:?}",
            txn_id,
            unit_id.get(),
            error
        );
    }
}

impl TimeKeeper for App {
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

#[cfg(feature = "coils")]
impl CoilResponse for App {
    fn read_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &Coils) {}
    fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

#[cfg(feature = "registers")]
impl RegisterResponse for App {
    fn read_multiple_holding_registers_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: &Registers,
    ) {
    }
    fn read_single_holding_register_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
    ) {
    }
    fn read_multiple_input_registers_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: &Registers,
    ) {
    }
    fn read_single_input_register_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
    ) {
    }
    fn read_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_multiple_registers_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn read_write_multiple_registers_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: &Registers,
    ) {
    }
    fn mask_write_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr) {}
}

#[cfg(feature = "discrete-inputs")]
impl DiscreteInputResponse for App {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: &DiscreteInputs,
    ) {
    }
    fn read_single_discrete_input_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: bool,
    ) {
    }
}

#[cfg(feature = "diagnostics")]
impl DiagnosticsResponse for App {
    fn read_device_identification_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: &DeviceIdentificationResponse,
    ) {
    }
    fn encapsulated_interface_transport_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: EncapsulatedInterfaceType,
        _: &[u8],
    ) {
    }
    fn read_exception_status_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u8) {}
    fn diagnostics_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: DiagnosticSubFunction,
        _: &[u16],
    ) {
    }
    fn get_comm_event_counter_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn get_comm_event_log_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
        _: u16,
        _: &[u8],
    ) {
    }
    fn report_server_id_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &[u8]) {}
}

#[cfg(feature = "fifo")]
impl FifoQueueResponse for App {
    fn read_fifo_queue_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &FifoQueue) {}
}

#[cfg(feature = "file-record")]
impl FileRecordResponse for App {
    fn read_file_record_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &[SubRequestParams]) {}
    fn write_file_record_response(&mut self, _: u16, _: UnitIdOrSlaveAddr) {}
}

fn main() -> Result<(), MbusError> {
    if !cfg!(all(
        feature = "coils",
        feature = "registers",
        feature = "discrete-inputs",
        feature = "diagnostics",
        feature = "fifo",
        feature = "file-record"
    )) {
        println!(
            "feature_facades_showcase requires features: coils, registers, discrete-inputs, diagnostics, fifo, file-record"
        );
        return Ok(());
    }

    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);
    let mut client = ClientServices::<_, _, 16>::new(MockTransport, App, config)?;
    let unit = UnitIdOrSlaveAddr::new(1)?;

    println!("--- Feature Facades Showcase ---");

    #[cfg(feature = "coils")]
    {
        client.coils().read_single_coil(1, unit, 0)?;
        client.with_coils(|coils| {
            coils.read_multiple_coils(2, unit, 0, 4)?;
            coils.write_single_coil(3, unit, 0, true)?;
            Ok::<(), MbusError>(())
        })?;
        println!("coils facade: ok");
    }

    #[cfg(feature = "registers")]
    {
        client
            .registers()
            .read_single_holding_register(10, unit, 0)?;
        client.with_registers(|registers| {
            registers.write_single_register(11, unit, 0, 1234)?;
            registers.read_input_registers(12, unit, 0, 2)?;
            Ok::<(), MbusError>(())
        })?;
        println!("registers facade: ok");
    }

    #[cfg(feature = "discrete-inputs")]
    {
        client
            .discrete_inputs()
            .read_single_discrete_input(20, unit, 0)?;
        client.with_discrete_inputs(|inputs| {
            inputs.read_discrete_inputs(21, unit, 0, 4)?;
            Ok::<(), MbusError>(())
        })?;
        println!("discrete_inputs facade: ok");
    }

    #[cfg(feature = "diagnostics")]
    {
        client.diagnostic().read_device_identification(
            30,
            unit,
            ReadDeviceIdCode::Basic,
            ObjectId::from(0x00),
        )?;
        client.with_diagnostic(|diag| {
            diag.encapsulated_interface_transport(
                31,
                unit,
                EncapsulatedInterfaceType::CanopenGeneralReference,
                &[0x01, 0x02],
            )?;
            Ok::<(), MbusError>(())
        })?;
        println!("diagnostic facade: ok");
    }

    #[cfg(feature = "fifo")]
    {
        client.fifo().read_fifo_queue(40, unit, 0)?;
        client.with_fifo(|fifo| {
            fifo.read_fifo_queue(41, unit, 0)?;
            Ok::<(), MbusError>(())
        })?;
        println!("fifo facade: ok");
    }

    #[cfg(feature = "file-record")]
    {
        let mut sub_request = SubRequest::new();
        sub_request.add_read_sub_request(1, 0, 1)?;
        client
            .file_records()
            .read_file_record(50, unit, &sub_request)?;
        client.with_file_records(|files| {
            files.read_file_record(51, unit, &sub_request)?;
            Ok::<(), MbusError>(())
        })?;
        println!("file_records facade: ok");
    }

    // Poll is still owned by the root client runtime.
    client.poll();

    println!("Feature facade showcase completed.");
    Ok(())
}
