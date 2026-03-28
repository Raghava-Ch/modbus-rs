//! Modbus Data Unit Module
//!
//! This module provides the core data structures for Modbus communication,
//! encompassing both the Protocol Data Unit (PDU) and the Application Data Unit (ADU).
//!
//! It is organized into two primary sub-modules:
//! - [`common`]: Contains the transport-agnostic [`Pdu`] and the generic [`ModbusMessage`]
//!   structures used across TCP, RTU, and ASCII variants.
//!
//! The data unit logic is designed to be `no_std` compatible, leveraging `heapless`
//! for deterministic memory management, which is critical for embedded systems
//! where dynamic allocation is often restricted.

pub mod common;
