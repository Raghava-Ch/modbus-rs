use anyhow::Result;
use modbus_rs::{
    ClientServices, CoilResponse, Coils, MbusError, ModbusConfig, ModbusTcpConfig,
    RequestErrorNotifier, StdTcpTransport, TimeKeeper, TrafficDirection, TrafficNotifier,
    UnitIdOrSlaveAddr,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
struct TrafficApp;

impl RequestErrorNotifier for TrafficApp {
    fn request_failed(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "request_failed txn={} unit={} error={:?}",
            txn_id,
            unit_id.get(),
            error
        );
    }
}

impl TimeKeeper for TrafficApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl TrafficNotifier for TrafficApp {
    fn on_tx_frame(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        frame_bytes: &[u8],
    ) {
        println!(
            "[{:?}] txn={} unit={} bytes={:02X?}",
            TrafficDirection::Tx,
            txn_id,
            unit_id_slave_addr.get(),
            frame_bytes
        );
    }

    fn on_rx_frame(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        frame_bytes: &[u8],
    ) {
        println!(
            "[{:?}] txn={} unit={} bytes={:02X?}",
            TrafficDirection::Rx,
            txn_id,
            unit_id_slave_addr.get(),
            frame_bytes
        );
    }

    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame_bytes: &[u8],
    ) {
        println!(
            "[{:?}] txn={} unit={} error={error:?} bytes={:02X?}",
            TrafficDirection::Tx,
            txn_id,
            unit_id_slave_addr.get(),
            frame_bytes
        );
    }

    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame_bytes: &[u8],
    ) {
        println!(
            "[{:?}] txn={} unit={} error={error:?} bytes={:02X?}",
            TrafficDirection::Rx,
            txn_id,
            unit_id_slave_addr.get(),
            frame_bytes
        );
    }
}

impl CoilResponse for TrafficApp {
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        println!(
            "read_coils_response txn={} unit={} addr={} qty={}",
            txn_id,
            unit_id.get(),
            coils.from_address(),
            coils.quantity()
        );
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
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _value: bool,
    ) {
    }

    fn write_multiple_coils_response(
        &mut self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
    ) {
    }
}

fn main() -> Result<()> {
    let transport = StdTcpTransport::new();
    let app = TrafficApp::default();
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.55.200", 502)?);

    let mut client = ClientServices::<_, _, 4>::new(transport, app, config)?;
    client.connect()?;

    let unit = UnitIdOrSlaveAddr::new(1)?;
    client.coils().read_multiple_coils(1, unit, 0, 8)?;

    while client.has_pending_requests() {
        client.poll();
    }

    Ok(())
}
