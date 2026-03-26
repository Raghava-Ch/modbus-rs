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
//! - [`data_unit`]: Definitions for PDU (Protocol Data Unit) and ADU (Application Data Unit).
//! - [`errors`]: Centralized error handling for the Modbus stack.
//! - [`function_codes`]: Definitions for public and user-defined Modbus function codes.
//! - [`models`]: Modbus data models (feature-gated where applicable).
//! - [`transport`]: Traits and configurations for physical/link layer communication.
//!
//! ## Usage
//!
//! This crate is typically used as a dependency for specific transport implementations like `mbus-network`
//! or `mbus-rtu`, or by users implementing custom Modbus devices.
#![cfg_attr(not(doc), no_std)]
#![warn(missing_docs)]

pub mod data_unit;
pub mod errors;
pub mod function_codes;
pub mod models;
pub mod transport;
