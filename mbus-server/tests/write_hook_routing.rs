use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;

#[cfg(feature = "coils")]
use mbus_server::CoilsModel;
#[cfg(feature = "holding-registers")]
use mbus_server::HoldingRegistersModel;
use mbus_server::modbus_app;

#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

#[cfg(feature = "coils")]
#[derive(Debug, Default, CoilsModel)]
struct HookCoils {
    #[coil(addr = 0)]
    direct: bool,
    #[coil(addr = 1, notify_via_batch = true)]
    via_batch: bool,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default)]
#[modbus_app(coils(coils, on_batch_write = on_coil_batch, on_write_0 = on_direct_coil))]
struct CoilHookApp {
    coils: HookCoils,
    reject_direct: bool,
    reject_batch: bool,
    direct_calls: u16,
    batch_calls: u16,
    last_direct_address: u16,
    last_direct_old: bool,
    last_direct_new: bool,
    last_batch_start: u16,
    last_batch_qty: u16,
    last_batch_byte: u8,
}

#[cfg(feature = "coils")]
impl CoilHookApp {
    fn on_direct_coil(&mut self, address: u16, old: bool, new: bool) -> Result<(), MbusError> {
        self.direct_calls += 1;
        self.last_direct_address = address;
        self.last_direct_old = old;
        self.last_direct_new = new;
        if self.reject_direct {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
    }

    fn on_coil_batch(&mut self, start: u16, qty: u16, values: &[u8]) -> Result<(), MbusError> {
        self.batch_calls += 1;
        self.last_batch_start = start;
        self.last_batch_qty = qty;
        self.last_batch_byte = values.first().copied().unwrap_or(0);
        if self.reject_batch {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for CoilHookApp {}

#[cfg(feature = "coils")]
#[test]
fn single_coil_on_write_hook_runs_before_commit_and_can_reject() {
    let mut app = CoilHookApp {
        reject_direct: true,
        ..Default::default()
    };

    let err = app
        .write_single_coil_request(41, unit_id(1), 0, true)
        .expect_err("individual coil hook rejection must abort the write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.direct_calls, 1);
    assert_eq!(app.batch_calls, 0);
    assert_eq!(app.last_direct_address, 0);
    assert!(!app.last_direct_old);
    assert!(app.last_direct_new);
    assert!(!app.coils.direct);

    app.reject_direct = false;
    app.write_single_coil_request(42, unit_id(1), 0, true)
        .expect("approved individual coil hook should commit the write");

    assert_eq!(app.direct_calls, 2);
    assert!(app.coils.direct);
}

#[cfg(feature = "coils")]
#[test]
fn single_coil_notify_via_batch_uses_batch_hook_with_qty_one() {
    let mut app = CoilHookApp::default();

    app.write_single_coil_request(43, unit_id(1), 1, true)
        .expect("notify_via_batch coil write should succeed");

    assert_eq!(app.direct_calls, 0);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 1);
    assert_eq!(app.last_batch_qty, 1);
    assert_eq!(app.last_batch_byte, 0b0000_0001);
    assert!(app.coils.via_batch);
}

#[cfg(feature = "coils")]
#[test]
fn single_coil_notify_via_batch_rejection_aborts_commit() {
    let mut app = CoilHookApp {
        reject_batch: true,
        ..Default::default()
    };

    let err = app
        .write_single_coil_request(43, unit_id(1), 1, true)
        .expect_err("notify_via_batch coil rejection must abort the write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.direct_calls, 0);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 1);
    assert_eq!(app.last_batch_qty, 1);
    assert_eq!(app.last_batch_byte, 0b0000_0001);
    assert!(!app.coils.via_batch);
}

#[cfg(feature = "coils")]
#[test]
fn multiple_coil_batch_hook_rejection_keeps_write_atomic() {
    let mut app = CoilHookApp {
        reject_batch: true,
        ..Default::default()
    };

    let err = app
        .write_multiple_coils_request(44, unit_id(1), 0, 2, &[0b0000_0011])
        .expect_err("batch hook rejection must abort the whole coil write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 0);
    assert_eq!(app.last_batch_qty, 2);
    assert_eq!(app.last_batch_byte, 0b0000_0011);
    assert!(!app.coils.direct);
    assert!(!app.coils.via_batch);
}

#[cfg(feature = "coils")]
#[test]
fn multiple_coil_batch_hook_success_commits_values() {
    let mut app = CoilHookApp::default();

    app.write_multiple_coils_request(45, unit_id(1), 0, 2, &[0b0000_0011])
        .expect("approved batch hook should commit the whole coil write");

    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 0);
    assert_eq!(app.last_batch_qty, 2);
    assert_eq!(app.last_batch_byte, 0b0000_0011);
    assert!(app.coils.direct);
    assert!(app.coils.via_batch);
}

#[cfg(feature = "holding-registers")]
#[derive(Debug, Default, HoldingRegistersModel)]
struct HookHoldingRegisters {
    #[reg(addr = 10)]
    direct: u16,
    #[reg(addr = 11, notify_via_batch = true)]
    via_batch: u16,
}

#[cfg(feature = "holding-registers")]
#[derive(Debug, Default)]
#[modbus_app(holding_registers(regs, on_batch_write = on_register_batch, on_write_10 = on_direct_register))]
struct HoldingRegisterHookApp {
    regs: HookHoldingRegisters,
    reject_direct: bool,
    reject_batch: bool,
    direct_calls: u16,
    batch_calls: u16,
    last_direct_address: u16,
    last_direct_old: u16,
    last_direct_new: u16,
    last_batch_start: u16,
    last_batch_qty: u16,
    last_batch_values: [u16; 2],
}

#[cfg(feature = "holding-registers")]
impl HoldingRegisterHookApp {
    fn on_direct_register(&mut self, address: u16, old: u16, new: u16) -> Result<(), MbusError> {
        self.direct_calls += 1;
        self.last_direct_address = address;
        self.last_direct_old = old;
        self.last_direct_new = new;
        if self.reject_direct {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
    }

    fn on_register_batch(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError> {
        self.batch_calls += 1;
        self.last_batch_start = start;
        self.last_batch_qty = qty;
        self.last_batch_values = [0u16; 2];
        for (index, value) in values.iter().copied().enumerate() {
            if index < self.last_batch_values.len() {
                self.last_batch_values[index] = value;
            }
        }
        if self.reject_batch {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for HoldingRegisterHookApp {}

#[cfg(feature = "holding-registers")]
#[test]
fn single_register_on_write_hook_runs_before_commit_and_can_reject() {
    let mut app = HoldingRegisterHookApp {
        reject_direct: true,
        ..Default::default()
    };

    let err = app
        .write_single_register_request(51, unit_id(1), 10, 0x1234)
        .expect_err("individual register hook rejection must abort the write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.direct_calls, 1);
    assert_eq!(app.batch_calls, 0);
    assert_eq!(app.last_direct_address, 10);
    assert_eq!(app.last_direct_old, 0);
    assert_eq!(app.last_direct_new, 0x1234);
    assert_eq!(app.regs.direct, 0);

    app.reject_direct = false;
    app.write_single_register_request(52, unit_id(1), 10, 0x1234)
        .expect("approved individual register hook should commit the write");

    assert_eq!(app.direct_calls, 2);
    assert_eq!(app.regs.direct, 0x1234);
}

#[cfg(feature = "holding-registers")]
#[test]
fn single_register_notify_via_batch_uses_batch_hook_with_qty_one() {
    let mut app = HoldingRegisterHookApp::default();

    app.write_single_register_request(53, unit_id(1), 11, 0x4321)
        .expect("notify_via_batch register write should succeed");

    assert_eq!(app.direct_calls, 0);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 11);
    assert_eq!(app.last_batch_qty, 1);
    assert_eq!(app.last_batch_values[0], 0x4321);
    assert_eq!(app.regs.via_batch, 0x4321);
}

#[cfg(feature = "holding-registers")]
#[test]
fn single_register_notify_via_batch_rejection_aborts_commit() {
    let mut app = HoldingRegisterHookApp {
        reject_batch: true,
        ..Default::default()
    };

    let err = app
        .write_single_register_request(53, unit_id(1), 11, 0x4321)
        .expect_err("notify_via_batch register rejection must abort the write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.direct_calls, 0);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 11);
    assert_eq!(app.last_batch_qty, 1);
    assert_eq!(app.last_batch_values[0], 0x4321);
    assert_eq!(app.regs.via_batch, 0);
}

#[cfg(feature = "holding-registers")]
#[test]
fn multiple_register_batch_hook_rejection_keeps_write_atomic() {
    let mut app = HoldingRegisterHookApp {
        reject_batch: true,
        ..Default::default()
    };

    let err = app
        .write_multiple_registers_request(54, unit_id(1), 10, &[0x1111, 0x2222])
        .expect_err("batch hook rejection must abort the whole register write");

    assert_eq!(err, MbusError::InvalidValue);
    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 10);
    assert_eq!(app.last_batch_qty, 2);
    assert_eq!(app.last_batch_values, [0x1111, 0x2222]);
    assert_eq!(app.regs.direct, 0);
    assert_eq!(app.regs.via_batch, 0);
}

#[cfg(feature = "holding-registers")]
#[test]
fn multiple_register_batch_hook_success_commits_values() {
    let mut app = HoldingRegisterHookApp::default();

    app.write_multiple_registers_request(55, unit_id(1), 10, &[0x1111, 0x2222])
        .expect("approved batch hook should commit the whole register write");

    assert_eq!(app.batch_calls, 1);
    assert_eq!(app.last_batch_start, 10);
    assert_eq!(app.last_batch_qty, 2);
    assert_eq!(app.last_batch_values, [0x1111, 0x2222]);
    assert_eq!(app.regs.direct, 0x1111);
    assert_eq!(app.regs.via_batch, 0x2222);
}
