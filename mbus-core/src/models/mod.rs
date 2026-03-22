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

pub mod coil;
pub mod diagnostic;
pub mod discrete_input;
pub mod fifo_queue;
pub mod file_record;
pub mod register;
