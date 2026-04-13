#![cfg(feature = "coils")]

use core::cell::RefCell;
use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{ForwardingApp, ModbusAppAccess, ModbusAppHandler};
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct CoilApp {
    /// Four coils packed in a nibble; bit N = coil N.
    coils: u8,
    fail_fc05: Option<MbusError>,
}

impl ModbusAppHandler for CoilApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if address != 0 || quantity > 8 {
            return Err(MbusError::InvalidAddress);
        }
        out[0] = self.coils;
        Ok(1)
    }

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        if let Some(err) = self.fail_fc05 {
            return Err(err);
        }

        if address >= 8 {
            return Err(MbusError::InvalidAddress);
        }

        if value {
            self.coils |= 1 << address;
        } else {
            self.coils &= !(1 << address);
        }
        Ok(())
    }

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        if starting_address != 0 || quantity > 8 || values.is_empty() {
            return Err(MbusError::InvalidAddress);
        }
        let mask = if quantity == 8 {
            0xFF
        } else {
            (1u8 << quantity) - 1
        };
        self.coils = (self.coils & !mask) | (values[0] & mask);
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for CoilApp {}

#[derive(Debug, Clone)]
struct CountingMutexAccess {
    inner: Arc<Mutex<CoilApp>>,
    access_calls: Arc<AtomicUsize>,
}

impl ModbusAppAccess for CountingMutexAccess {
    type App = CoilApp;

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
    inner: RefCell<CoilApp>,
}

impl ModbusAppAccess for RefCellAccess {
    type App = CoilApp;

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
fn forwarding_app_with_mutex_access_forwards_coil_read_and_write() {
    let calls = Arc::new(AtomicUsize::new(0));
    let access = CountingMutexAccess {
        inner: Arc::new(Mutex::new(CoilApp::default())),
        access_calls: Arc::clone(&calls),
    };

    let mut app = ForwardingApp::new(access);

    // FC05: set coil 0 and coil 2.
    app.write_single_coil_request(10, unit_id(1), 0, true)
        .expect("fc05 coil 0 should succeed");
    app.write_single_coil_request(11, unit_id(1), 2, true)
        .expect("fc05 coil 2 should succeed");

    // FC01: read back coils, expect bits 0 and 2 set.
    let mut out = [0u8; 1];
    let len = app
        .read_coils_request(12, unit_id(1), 0, 4, &mut out)
        .expect("fc01 should succeed");

    assert_eq!(len, 1);
    assert_eq!(out[0], 0b0000_0101);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn forwarding_app_with_refcell_access_forwards_write_multiple_coils() {
    let access = RefCellAccess {
        inner: RefCell::new(CoilApp::default()),
    };
    let mut app = ForwardingApp::new(access);

    // FC0F: write 4 coils packed as 0b1011.
    app.write_multiple_coils_request(20, unit_id(1), 0, 4, &[0b0000_1011])
        .expect("fc0f should succeed");

    let mut out = [0u8; 1];
    let len = app
        .read_coils_request(21, unit_id(1), 0, 4, &mut out)
        .expect("fc01 should succeed");

    assert_eq!(len, 1);
    assert_eq!(out[0] & 0x0F, 0b0000_1011);
}

#[test]
fn forwarding_app_propagates_coil_write_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let access = CountingMutexAccess {
        inner: Arc::new(Mutex::new(CoilApp {
            coils: 0,
            fail_fc05: Some(MbusError::InvalidAddress),
        })),
        access_calls: Arc::clone(&calls),
    };
    let mut app = ForwardingApp::new(access);

    let err = app
        .write_single_coil_request(30, unit_id(1), 0, true)
        .expect_err("fc05 should return configured app error");

    assert_eq!(err, MbusError::InvalidAddress);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
