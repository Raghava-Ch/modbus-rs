//! # mbus-core
//!
//! `mbus-core` is the foundational crate for a Rust implementation of the Modbus protocol,
//! designed with a focus on `no_std` compatibility for embedded systems while remaining
//! flexible enough for standard environments.
//!
//! ## Features
//!
//! - **Protocol Agnostic**: Supports core logic for Modbus TCP, RTU, and ASCII.
//! - **no_std Support**: Core data structures and logic do not require the standard library.
//! - **Strongly Typed**: Leverages Rust's type system to ensure valid PDU/ADU construction.
//! - **Extensible**: Provides traits for custom transport implementations and user-defined function codes.
//!
//! ## Module Structure
//!
//! - [`app`]: High-level application logic and coordination.
//! - [`client`]: Modbus client (master) traits and service abstractions.
//! - [`data_unit`]: Definitions for PDU (Protocol Data Unit) and ADU (Application Data Unit).
//! - [`device_identification`]: Support for Modbus Device Identification (FC 0x2B / 0x0E).
//! - [`errors`]: Centralized error handling for the Modbus stack.
//! - [`function_codes`]: Definitions for public and user-defined Modbus function codes.
//! - [`transport`]: Traits and configurations for physical/link layer communication.
//!
//! ## Usage
//!
//! This crate is typically used as a dependency for specific transport implementations like `mbus-tcp`
//! or `mbus-rtu`, or by users implementing custom Modbus devices.
#![cfg_attr(not(doc), no_std)]
#![warn(missing_docs)]

pub mod app;
pub mod client;
pub mod data_unit;
pub mod device_identification;
pub mod errors;
pub mod function_codes;
pub mod transport;
