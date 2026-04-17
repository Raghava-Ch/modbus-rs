//! Device Identification (FC 0x2B / MEI 0x0E) server-side example.
//!
//! Demonstrates how to implement the `read_device_identification_request` callback so
//! that a Modbus client can discover who made this device.
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example device_identification --features diagnostics
//! ```

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerDiagnosticsHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

// ---------------------------------------------------------------------------
// Static device identity strings
// ---------------------------------------------------------------------------

const VENDOR_NAME: &[u8] = b"ACME Corp";
const PRODUCT_CODE: &[u8] = b"Widget-9000";
const REVISION: &[u8] = b"1.4.2";
const VENDOR_URL: &[u8] = b"https://acme.example.com";
const PRODUCT_NAME: &[u8] = b"Super Widget";

// All objects in ascending id order: (object_id, value)
const OBJECTS: &[(u8, &[u8])] = &[
    (0x00, VENDOR_NAME),
    (0x01, PRODUCT_CODE),
    (0x02, REVISION),
    (0x03, VENDOR_URL),
    (0x04, PRODUCT_NAME),
];

// Conformity level: RegularStreamAndIndividual (supports codes 1-4, objects 0x00-0x04)
const CONFORMITY: u8 = 0x82;

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

struct MyDevice;

impl ServerExceptionHandler for MyDevice {}

impl ServerCoilHandler for MyDevice {}

impl ServerDiscreteInputHandler for MyDevice {}

impl ServerHoldingRegisterHandler for MyDevice {}

impl ServerInputRegisterHandler for MyDevice {}

impl ServerFifoHandler for MyDevice {}

impl ServerFileRecordHandler for MyDevice {}

impl ServerDiagnosticsHandler for MyDevice {
    fn read_device_identification_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        if read_device_id_code == 0x04 {
            // Individual access: return exactly the one requested object
            for &(id, val) in OBJECTS {
                if id == start_object_id {
                    let needed = 2 + val.len();
                    if needed > out.len() {
                        return Err(MbusError::BufferTooSmall);
                    }
                    out[0] = id;
                    out[1] = val.len() as u8;
                    out[2..needed].copy_from_slice(val);
                    return Ok((needed as u8, CONFORMITY, false, 0x00));
                }
            }
            return Err(MbusError::InvalidAddress);
        }

        // Stream access: return objects with id >= start_object_id, in order
        let mut written = 0usize;
        let mut more_follows = false;
        let mut next_id = 0x00u8;

        for &(id, val) in OBJECTS.iter().filter(|&&(id, _)| id >= start_object_id) {
            let needed = 2 + val.len();
            if written + needed > out.len() {
                // Not enough space — signal the client to ask again from `id`
                more_follows = true;
                next_id = id;
                break;
            }
            out[written] = id;
            out[written + 1] = val.len() as u8;
            out[written + 2..written + needed].copy_from_slice(val);
            written += needed;
        }

        Ok((written as u8, CONFORMITY, more_follows, next_id))
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for MyDevice {}

// ---------------------------------------------------------------------------
// main: demonstrate the callback directly (no server loop needed)
// ---------------------------------------------------------------------------

fn main() {
    let mut device = MyDevice;
    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");
    let mut out = [0u8; 246];

    // Simulate a Basic stream request (code=1, starting from object 0x00)
    let (written, conformity, more_follows, next_id) = device
        .read_device_identification_request(42, uid, 0x01, 0x00, &mut out)
        .expect("callback should succeed");

    println!("=== FC 0x2B / MEI 0x0E — Read Device Identification ===");
    println!("  conformity_level : {conformity:#04X}");
    println!("  more_follows     : {more_follows}");
    println!("  next_object_id   : {next_id:#04X}");
    println!("  bytes written    : {written}");

    // Walk the object triples
    let mut offset = 0usize;
    while offset < written as usize {
        let id = out[offset];
        let len = out[offset + 1] as usize;
        let val = &out[offset + 2..offset + 2 + len];
        let val_str = core::str::from_utf8(val).unwrap_or("<binary>");
        println!("  object {id:#04X} : {val_str}");
        offset += 2 + len;
    }

    // Simulate individual access (code=4, object 0x02 = MajorMinorRevision)
    let (written2, _, _, _) = device
        .read_device_identification_request(43, uid, 0x04, 0x02, &mut out)
        .expect("individual access should succeed");
    let rev = core::str::from_utf8(&out[2..2 + out[1] as usize]).unwrap_or("?");
    println!("\nIndividual access → revision: {rev} ({written2} bytes)");
}
