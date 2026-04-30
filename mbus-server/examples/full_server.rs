//! Full-featured server example — every supported function code in one place.
//!
//! This example implements a single `FullApp` struct that handles all Modbus
//! function codes supported by `mbus-server`.  It is intentionally simple
//! (fixed-size arrays, no locking) so the focus stays on the API surface rather
//! than application business logic.
//!
//! Use it as a starting point or a copy-paste reference.
//!
//! # Function codes covered
//!
//! | FC   | Name                            | Feature flag        |
//! |------|---------------------------------|---------------------|
//! | 0x01 | Read Coils                      | `coils`             |
//! | 0x02 | Read Discrete Inputs            | `discrete-inputs`   |
//! | 0x03 | Read Holding Registers          | `holding-registers` |
//! | 0x04 | Read Input Registers            | `input-registers`   |
//! | 0x05 | Write Single Coil               | `coils`             |
//! | 0x06 | Write Single Register           | `holding-registers` |
//! | 0x07 | Read Exception Status           | `diagnostics`       |
//! | 0x08 | Diagnostics (loopback)          | `diagnostics`       |
//! | 0x0B | Get Comm Event Counter          | `diagnostics`       |
//! | 0x0C | Get Comm Event Log              | `diagnostics`       |
//! | 0x0F | Write Multiple Coils            | `coils`             |
//! | 0x10 | Write Multiple Registers        | `holding-registers` |
//! | 0x11 | Report Server ID                | `diagnostics`       |
//! | 0x14 | Read File Record                | `file-record`       |
//! | 0x15 | Write File Record               | `file-record`       |
//! | 0x16 | Mask Write Register             | `holding-registers` |
//! | 0x17 | Read/Write Multiple Registers   | `holding-registers` |
//! | 0x18 | Read FIFO Queue                 | `fifo`              |
//! | 0x2B | Read Device Identification      | `diagnostics`       |
//!
//! # Run
//!
//! ```text
//! cargo run -p mbus-server --example full_featured_server \
//!   --features "coils,discrete-inputs,holding-registers,input-registers,\
//!               fifo,file-record,diagnostics"
//! ```

use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::{DiagnosticSubFunction, FunctionCode};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiagnosticsHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COIL_COUNT: usize = 16;
const DISCRETE_INPUT_COUNT: usize = 8;
const HOLDING_REG_COUNT: usize = 32;
const INPUT_REG_COUNT: usize = 16;

// FIFO: one queue mapped at pointer address 0x0100
const FIFO_POINTER_ADDR: u16 = 0x0100;
const FIFO_MAX: usize = 8;

// File store: file number 1, 32 records
const FILE_RECORD_COUNT: usize = 32;

// Device ID objects (id, value)
const DEVICE_OBJECTS: &[(u8, &[u8])] = &[
    (0x00, b"Example Corp"),
    (0x01, b"FullFeatured-1"),
    (0x02, b"2.0.0"),
    (0x03, b"https://example.com"),
    (0x04, b"Full Featured Server"),
];
const CONFORMITY_LEVEL: u8 = 0x82; // Regular + Individual

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct FullApp {
    // FC01 / FC05 / FC0F
    coils: [bool; COIL_COUNT],
    // FC02
    discrete_inputs: [bool; DISCRETE_INPUT_COUNT],
    // FC03 / FC06 / FC10 / FC16 / FC17
    holding_regs: [u16; HOLDING_REG_COUNT],
    // FC04
    input_regs: [u16; INPUT_REG_COUNT],
    // FC07
    exception_status: u8,
    // FC18
    fifo_queue: [u16; FIFO_MAX],
    fifo_count: usize,
    // FC14 / FC15
    file_store: [u16; FILE_RECORD_COUNT],
    // FC11
    run_indicator: u8,
    // general stats for on_exception demo
    exception_count: u32,
}

impl FullApp {
    fn new() -> Self {
        let mut holding_regs = [0u16; HOLDING_REG_COUNT];
        for (i, r) in holding_regs.iter_mut().enumerate() {
            *r = 0x0100 + i as u16;
        }

        let mut input_regs = [0u16; INPUT_REG_COUNT];
        for (i, r) in input_regs.iter_mut().enumerate() {
            *r = 0x0200 + i as u16;
        }

        let mut file_store = [0u16; FILE_RECORD_COUNT];
        for (i, r) in file_store.iter_mut().enumerate() {
            *r = 0xF000 + i as u16;
        }

        Self {
            coils: [false; COIL_COUNT],
            discrete_inputs: [true, false, true, true, false, false, true, false],
            holding_regs,
            input_regs,
            exception_status: 0b0000_0101,
            fifo_queue: [
                0xAAAA, 0xBBBB, 0xCCCC, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
            ],
            fifo_count: 3,
            file_store,
            run_indicator: 0xFF, // server is running
            exception_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Split trait implementations
// ---------------------------------------------------------------------------

impl ServerExceptionHandler for FullApp {
    fn on_exception(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        exception_code: ExceptionCode,
        _error: MbusError,
    ) {
        self.exception_count += 1;
        println!(
            "  [on_exception] FC={function_code:?} → {exception_code:?} \
             (total exceptions so far: {})",
            self.exception_count
        );
    }
}

impl ServerCoilHandler for FullApp {
    // ── FC01: Read Coils ──────────────────────────────────────────────────

    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let start = address as usize;
        let end = start + quantity as usize;
        if end > self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }

        let byte_count = (quantity as usize).div_ceil(8);
        for (byte_idx, out_byte) in out.iter_mut().enumerate().take(byte_count) {
            let mut byte = 0u8;
            for bit in 0..8usize {
                let coil_idx = start + byte_idx * 8 + bit;
                if coil_idx < end && self.coils[coil_idx] {
                    byte |= 1 << bit;
                }
            }
            *out_byte = byte;
        }
        Ok(byte_count as u8)
    }

    // ── FC05: Write Single Coil ───────────────────────────────────────────

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let idx = address as usize;
        if idx >= self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        self.coils[idx] = value;
        Ok(())
    }

    // ── FC0F: Write Multiple Coils ────────────────────────────────────────

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        packed_values: &[u8],
    ) -> Result<(), MbusError> {
        let start = starting_address as usize;
        if start + quantity as usize > self.coils.len() {
            return Err(MbusError::InvalidAddress);
        }
        for bit_pos in 0..(quantity as usize) {
            let byte_idx = bit_pos / 8;
            let bit_idx = bit_pos % 8;
            self.coils[start + bit_pos] = (packed_values[byte_idx] >> bit_idx) & 1 != 0;
        }
        Ok(())
    }
}

impl ServerDiscreteInputHandler for FullApp {
    // ── FC02: Read Discrete Inputs ───────────────────────────────────────

    fn read_discrete_inputs_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let start = address as usize;
        let end = start + quantity as usize;
        if end > self.discrete_inputs.len() {
            return Err(MbusError::InvalidAddress);
        }

        let byte_count = (quantity as usize).div_ceil(8);
        for (byte_idx, out_byte) in out.iter_mut().enumerate().take(byte_count) {
            let mut byte = 0u8;
            for bit in 0..8usize {
                let idx = start + byte_idx * 8 + bit;
                if idx < end && self.discrete_inputs[idx] {
                    byte |= 1 << bit;
                }
            }
            *out_byte = byte;
        }
        Ok(byte_count as u8)
    }
}

impl ServerHoldingRegisterHandler for FullApp {
    // ── FC03: Read Holding Registers ─────────────────────────────────────

    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let start = address as usize;
        let end = start + quantity as usize;
        if end > self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        for (i, &v) in self.holding_regs[start..end].iter().enumerate() {
            out[i * 2] = (v >> 8) as u8;
            out[i * 2 + 1] = v as u8;
        }
        Ok((quantity * 2) as u8)
    }

    // ── FC06: Write Single Register ──────────────────────────────────────

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let idx = address as usize;
        if idx >= self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        self.holding_regs[idx] = value;
        Ok(())
    }

    // ── FC10: Write Multiple Registers ───────────────────────────────────

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        let start = starting_address as usize;
        if start + values.len() > self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        self.holding_regs[start..start + values.len()].copy_from_slice(values);
        Ok(())
    }

    // ── FC16: Mask Write Register ─────────────────────────────────────────

    fn mask_write_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        let idx = address as usize;
        if idx >= self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        // Modbus spec: result = (current AND and_mask) OR (or_mask AND NOT and_mask)
        let current = self.holding_regs[idx];
        self.holding_regs[idx] = (current & and_mask) | (or_mask & !and_mask);
        Ok(())
    }

    // ── FC17: Read/Write Multiple Registers ──────────────────────────────

    fn read_write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        // Write first (per Modbus spec)
        let ws = write_address as usize;
        if ws.saturating_add(write_values.len()) > self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        self.holding_regs[ws..ws + write_values.len()].copy_from_slice(write_values);

        // Then read
        let rs = read_address as usize;
        let re = rs + read_quantity as usize;
        if re > self.holding_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        for (i, &v) in self.holding_regs[rs..re].iter().enumerate() {
            out[i * 2] = (v >> 8) as u8;
            out[i * 2 + 1] = v as u8;
        }
        Ok((read_quantity * 2) as u8)
    }
}

impl ServerInputRegisterHandler for FullApp {
    // ── FC04: Read Input Registers ───────────────────────────────────────

    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let start = address as usize;
        let end = start + quantity as usize;
        if end > self.input_regs.len() {
            return Err(MbusError::InvalidAddress);
        }
        for (i, &v) in self.input_regs[start..end].iter().enumerate() {
            out[i * 2] = (v >> 8) as u8;
            out[i * 2 + 1] = v as u8;
        }
        Ok((quantity * 2) as u8)
    }
}

impl ServerFifoHandler for FullApp {
    // ── FC18: Read FIFO Queue ─────────────────────────────────────────────

    fn read_fifo_queue_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if pointer_address != FIFO_POINTER_ADDR {
            return Err(MbusError::InvalidAddress);
        }
        let count = self.fifo_count as u16;
        out[0] = (count >> 8) as u8;
        out[1] = count as u8;
        for (i, &v) in self.fifo_queue[..self.fifo_count].iter().enumerate() {
            out[2 + i * 2] = (v >> 8) as u8;
            out[2 + i * 2 + 1] = v as u8;
        }
        Ok(2 + (count * 2) as u8)
    }
}

impl ServerFileRecordHandler for FullApp {
    // ── FC14: Read File Record ────────────────────────────────────────────

    fn read_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if file_number != 1 {
            return Err(MbusError::InvalidAddress);
        }
        let start = record_number as usize;
        let len = record_length as usize;
        if start.saturating_add(len) > self.file_store.len() {
            return Err(MbusError::InvalidAddress);
        }
        for i in 0..len {
            let v = self.file_store[start + i];
            out[i * 2] = (v >> 8) as u8;
            out[i * 2 + 1] = v as u8;
        }
        Ok((len * 2) as u8)
    }

    // ── FC15: Write File Record ───────────────────────────────────────────

    fn write_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        _record_length: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        if file_number != 1 {
            return Err(MbusError::InvalidAddress);
        }
        let start = record_number as usize;
        if start.saturating_add(values.len()) > self.file_store.len() {
            return Err(MbusError::InvalidAddress);
        }
        self.file_store[start..start + values.len()].copy_from_slice(values);
        Ok(())
    }
}

impl ServerDiagnosticsHandler for FullApp {
    // ── FC07: Read Exception Status ──────────────────────────────────────

    fn read_exception_status_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        Ok(self.exception_status)
    }

    // ── FC08: Diagnostics (loopback only; stack handles counter sub-fns) ──

    fn diagnostics_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: u16,
    ) -> Result<u16, MbusError> {
        match sub_function {
            DiagnosticSubFunction::ReturnQueryData => Ok(data), // loopback
            _ => Err(MbusError::ReservedSubFunction(sub_function as u16)),
        }
    }

    // ── FC0B: Get Comm Event Counter ──────────────────────────────────────

    fn get_comm_event_counter_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(u16, u16), MbusError> {
        // status word bit-15 = 1 → ready; event_count delegated to stack
        Ok((0x0000, 0x0000))
    }

    // ── FC0C: Get Comm Event Log ──────────────────────────────────────────

    fn get_comm_event_log_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _out_events: &mut [u8],
    ) -> Result<(u16, u16, u16, u8), MbusError> {
        // (status, event_count, message_count, event_bytes_written)
        Ok((0x0000, 0, 0, 0))
    }

    // ── FC11: Report Server ID ────────────────────────────────────────────

    fn report_server_id_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        let id = b"FullFeaturedServer-v2";
        let n = id.len().min(out_server_id.len());
        out_server_id[..n].copy_from_slice(&id[..n]);
        Ok((n as u8, self.run_indicator))
    }

    // ── FC2B / MEI 0x0E: Read Device Identification ───────────────────────

    fn read_device_identification_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        if read_device_id_code == 0x04 {
            // Individual access
            for &(id, val) in DEVICE_OBJECTS {
                if id == start_object_id {
                    let needed = 2 + val.len();
                    if needed > out.len() {
                        return Err(MbusError::BufferTooSmall);
                    }
                    out[0] = id;
                    out[1] = val.len() as u8;
                    out[2..needed].copy_from_slice(val);
                    return Ok((needed as u8, CONFORMITY_LEVEL, false, 0x00));
                }
            }
            return Err(MbusError::InvalidAddress);
        }

        // Stream access
        let mut written = 0usize;
        let mut more_follows = false;
        let mut next_id = 0x00u8;
        for &(id, val) in DEVICE_OBJECTS
            .iter()
            .filter(|&&(id, _)| id >= start_object_id)
        {
            let needed = 2 + val.len();
            if written + needed > out.len() {
                more_follows = true;
                next_id = id;
                break;
            }
            out[written] = id;
            out[written + 1] = val.len() as u8;
            out[written + 2..written + needed].copy_from_slice(val);
            written += needed;
        }
        Ok((written as u8, CONFORMITY_LEVEL, more_follows, next_id))
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for FullApp {}

// ---------------------------------------------------------------------------
// Main — exercise every callback directly (no transport loop needed)
// ---------------------------------------------------------------------------

fn main() {
    let mut app = FullApp::new();
    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");

    println!("=== full_featured_server — callback smoke test ===\n");

    // FC03: read holding registers
    #[cfg(feature = "holding-registers")]
    {
        let mut out = [0u8; 8];
        let n = app
            .read_multiple_holding_registers_request(1, uid, 0, 4, &mut out)
            .expect("FC03");
        let regs: Vec<u16> = (0..4)
            .map(|i| u16::from_be_bytes([out[i * 2], out[i * 2 + 1]]))
            .collect();
        println!("FC03 holding[0..4] = {regs:04X?}  ({n} bytes)");
    }

    // FC06: write single register then read it back
    #[cfg(feature = "holding-registers")]
    {
        app.write_single_register_request(2, uid, 5, 0xDEAD)
            .expect("FC06");
        let mut out = [0u8; 2];
        app.read_multiple_holding_registers_request(3, uid, 5, 1, &mut out)
            .expect("FC03 readback");
        let v = u16::from_be_bytes([out[0], out[1]]);
        println!("FC06 write reg[5]=0xDEAD → read back {v:#06X}");
    }

    // FC16: mask write register
    #[cfg(feature = "holding-registers")]
    {
        app.write_single_register_request(4, uid, 10, 0xFFFF)
            .expect("setup");
        app.mask_write_register_request(5, uid, 10, 0xFF00, 0x00AB)
            .expect("FC16");
        let mut out = [0u8; 2];
        app.read_multiple_holding_registers_request(6, uid, 10, 1, &mut out)
            .expect("readback");
        let v = u16::from_be_bytes([out[0], out[1]]);
        // (0xFFFF & 0xFF00) | (0x00AB & 0x00FF) = 0xFF00 | 0x00AB = 0xFFAB
        println!("FC16 mask_write reg[10]: 0xFFFF → {v:#06X}  (expected 0xFFAB)");
    }

    // FC01 / FC05 / FC0F: coils
    #[cfg(feature = "coils")]
    {
        app.write_single_coil_request(10, uid, 3, true)
            .expect("FC05");
        app.write_multiple_coils_request(11, uid, 8, 4, &[0b0000_1010])
            .expect("FC0F");
        let mut out = [0u8; 2]; // 16 coils → 2 bytes
        let n = app
            .read_coils_request(12, uid, 0, 16, &mut out)
            .expect("FC01");
        println!(
            "FC01 coils[0..16] packed = {:#010b} {:#010b}  ({n} bytes)",
            out[0], out[1]
        );
    }

    // FC02: discrete inputs
    #[cfg(feature = "discrete-inputs")]
    {
        let mut out = [0u8; 1];
        let n = app
            .read_discrete_inputs_request(20, uid, 0, 8, &mut out)
            .expect("FC02");
        println!(
            "FC02 discrete_inputs[0..8] packed = {:#010b}  ({n} bytes)",
            out[0]
        );
    }

    // FC04: input registers
    #[cfg(feature = "input-registers")]
    {
        let mut out = [0u8; 4];
        let n = app
            .read_multiple_input_registers_request(30, uid, 0, 2, &mut out)
            .expect("FC04");
        let r: Vec<u16> = (0..2)
            .map(|i| u16::from_be_bytes([out[i * 2], out[i * 2 + 1]]))
            .collect();
        println!("FC04 input[0..2] = {r:04X?}  ({n} bytes)");
    }

    // FC17: read/write multiple registers
    #[cfg(feature = "holding-registers")]
    {
        let write_vals = [0x1111u16, 0x2222];
        let mut out = [0u8; 6];
        let n = app
            .read_write_multiple_registers_request(40, uid, 0, 3, 20, &write_vals, &mut out)
            .expect("FC17");
        let r: Vec<u16> = (0..3)
            .map(|i| u16::from_be_bytes([out[i * 2], out[i * 2 + 1]]))
            .collect();
        println!("FC17 write[20..22]=[0x1111,0x2222], read[0..3] = {r:04X?}  ({n} bytes)");
    }

    // FC18: FIFO queue
    #[cfg(feature = "fifo")]
    {
        let mut out = [0u8; 64];
        let n = app
            .read_fifo_queue_request(50, uid, FIFO_POINTER_ADDR, &mut out)
            .expect("FC18");
        let fifo_count = u16::from_be_bytes([out[0], out[1]]);
        let vals: Vec<u16> = (0..fifo_count as usize)
            .map(|i| u16::from_be_bytes([out[2 + i * 2], out[2 + i * 2 + 1]]))
            .collect();
        println!(
            "FC18 fifo[{FIFO_POINTER_ADDR:#06X}]: count={fifo_count}, values={vals:04X?}  ({n} bytes)"
        );
    }

    // FC14 / FC15: file record
    #[cfg(feature = "file-record")]
    {
        app.write_file_record_request(60, uid, 1, 10, 2, &[0xABCD, 0xEF01])
            .expect("FC15");
        let mut out = [0u8; 4];
        let n = app
            .read_file_record_request(61, uid, 1, 10, 2, &mut out)
            .expect("FC14");
        let r: Vec<u16> = (0..2)
            .map(|i| u16::from_be_bytes([out[i * 2], out[i * 2 + 1]]))
            .collect();
        println!(
            "FC14/FC15 file1 record[10..12]: wrote [0xABCD, 0xEF01], read {r:04X?}  ({n} bytes)"
        );
    }

    // FC07: exception status
    #[cfg(feature = "diagnostics")]
    {
        let status = app.read_exception_status_request(70, uid).expect("FC07");
        println!("FC07 exception_status = {status:#010b}");
    }

    // FC11: report server ID
    #[cfg(feature = "diagnostics")]
    {
        let mut out = [0u8; 64];
        let (n, run) = app
            .report_server_id_request(80, uid, &mut out)
            .expect("FC11");
        let id = core::str::from_utf8(&out[..n as usize]).unwrap_or("<binary>");
        println!("FC11 server_id=\"{id}\" run={run:#04X}");
    }

    // FC2B: device identification — stream
    #[cfg(feature = "diagnostics")]
    {
        let mut out = [0u8; 246];
        let (written, conformity, more_follows, next_obj) = app
            .read_device_identification_request(90, uid, 0x01, 0x00, &mut out)
            .expect("FC2B");
        println!(
            "FC2B device_id: conformity={conformity:#04X} more={more_follows} next={next_obj:#04X} written={written}"
        );
        let mut offset = 0usize;
        while offset < written as usize {
            let id = out[offset];
            let len = out[offset + 1] as usize;
            let val = core::str::from_utf8(&out[offset + 2..offset + 2 + len]).unwrap_or("?");
            println!("  object {id:#04X} = \"{val}\"");
            offset += 2 + len;
        }
    }

    // FC08: diagnostics loopback
    #[cfg(feature = "diagnostics")]
    {
        let echo = app
            .diagnostics_request(100, uid, DiagnosticSubFunction::ReturnQueryData, 0xABCD)
            .expect("FC08 loopback");
        println!("FC08 loopback: sent {:#06X}, echo {echo:#06X}", 0xABCDu16);
    }

    println!("\n=== all callbacks exercised ===");
}
