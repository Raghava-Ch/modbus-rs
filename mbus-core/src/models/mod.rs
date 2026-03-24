//! # Modbus Data Models
//!
//! This module contains the core data structures representing the different Modbus
//! data types and their associated access logic.
//!
//! Each sub-module corresponds to specific Modbus Function Codes and provides
//! `no_std` compatible, memory-efficient models for handling protocol data.
//!
//! ## Supported Models
//! - **Coils**: Single-bit read-write status (FC 0x01, 0x05, 0x0F).
//! - **Discrete Inputs**: Single-bit read-only status (FC 0x02).
//! - **Registers**: 16-bit read-write or read-only data (FC 0x03, 0x04, 0x06, 0x10).
//! - **FIFO Queue**: Specialized register reading (FC 0x18).
//! - **File Records**: Structured memory access (FC 0x14, 0x15).
//! - **Diagnostic**: Device identification and MEI transport (FC 0x2B).

#[cfg(feature = "coils")]
pub mod coil;
#[cfg(feature = "diagnostics")]
pub mod diagnostic;
#[cfg(feature = "discrete-inputs")]
pub mod discrete_input;
#[cfg(feature = "fifo")]
pub mod fifo_queue;
#[cfg(feature = "file-record")]
pub mod file_record;
#[cfg(feature = "registers")]
pub mod register;
