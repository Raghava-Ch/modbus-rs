#![cfg(feature = "registers")]

use core::cell::RefCell;
use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{ForwardingApp, ModbusAppAccess, ModbusAppHandler};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct TestApp {
    holding0: u16,
    holding1: u16,
    fail_fc06: Option<MbusError>,
}

impl ModbusAppHandler for TestApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        match (address, quantity) {
            (0, 1) => {
                out[0] = (self.holding0 >> 8) as u8;
                out[1] = self.holding0 as u8;
                Ok(2)
            }
            (0, 2) => {
                out[0] = (self.holding0 >> 8) as u8;
                out[1] = self.holding0 as u8;
                out[2] = (self.holding1 >> 8) as u8;
                out[3] = self.holding1 as u8;
                Ok(4)
            }
            _ => Err(MbusError::InvalidAddress),
        }
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        if let Some(err) = self.fail_fc06 {
            return Err(err);
        }

        match address {
            0 => {
                self.holding0 = value;
                Ok(())
            }
            1 => {
                self.holding1 = value;
                Ok(())
            }
            _ => Err(MbusError::InvalidAddress),
        }
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        if starting_address != 0 || values.len() != 2 {
            return Err(MbusError::InvalidAddress);
        }

        self.holding0 = values[0];
        self.holding1 = values[1];
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct CountingMutexAccess {
    inner: Arc<Mutex<TestApp>>,
    access_calls: Arc<AtomicUsize>,
}

impl ModbusAppAccess for CountingMutexAccess {
    type App = TestApp;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        self.access_calls.fetch_add(1, Ordering::SeqCst);
        let mut app = self.inner.lock().expect("mutex poisoned");
        f(&mut app)
    }
}

#[derive(Debug)]
struct RefCellAccess {
    inner: RefCell<TestApp>,
}

impl ModbusAppAccess for RefCellAccess {
    type App = TestApp;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        let mut app = self.inner.borrow_mut();
        f(&mut app)
    }
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new(v).expect("valid unit id")
}

#[test]
fn forwarding_app_with_mutex_access_forwards_reads_and_writes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let access = CountingMutexAccess {
        inner: Arc::new(Mutex::new(TestApp::default())),
        access_calls: Arc::clone(&calls),
    };

    let mut app = ForwardingApp::new(access);

    app.write_single_register_request(10, unit_id(1), 0, 0x1234)
        .expect("fc06 should succeed");

    let mut out = [0u8; 4];
    let len = app
        .read_multiple_holding_registers_request(11, unit_id(1), 0, 1, &mut out)
        .expect("fc03 should succeed");

    assert_eq!(len, 2);
    assert_eq!(&out[..2], &[0x12, 0x34]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn forwarding_app_with_refcell_access_works_without_mutex() {
    let access = RefCellAccess {
        inner: RefCell::new(TestApp::default()),
    };
    let mut app = ForwardingApp::new(access);

    app.write_multiple_registers_request(20, unit_id(1), 0, &[0x1111, 0x2222])
        .expect("fc16 should succeed");

    let mut out = [0u8; 4];
    let len = app
        .read_multiple_holding_registers_request(21, unit_id(1), 0, 2, &mut out)
        .expect("fc03 should succeed");

    assert_eq!(len, 4);
    assert_eq!(out, [0x11, 0x11, 0x22, 0x22]);
}

#[test]
fn forwarding_app_propagates_inner_app_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let access = CountingMutexAccess {
        inner: Arc::new(Mutex::new(TestApp {
            holding0: 0,
            holding1: 0,
            fail_fc06: Some(MbusError::InvalidAddress),
        })),
        access_calls: Arc::clone(&calls),
    };
    let mut app = ForwardingApp::new(access);

    let err = app
        .write_single_register_request(30, unit_id(1), 0, 55)
        .expect_err("fc06 should return configured app error");

    assert_eq!(err, MbusError::InvalidAddress);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
