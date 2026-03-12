//! # Modbus Function Codes Module
//!
//! This module serves as the central definition point for all Modbus function codes
//! supported by the stack. It categorizes function codes into two primary groups:
//!
//! - **[`public`]**: Contains standard function codes defined by the Modbus Application
//!   Protocol Specification V1.1b3 (e.g., Read Coils, Write Holding Registers).
//! - **[`user_defined`]**: Provides a space for vendor-specific or proprietary function
//!   codes (ranges 65-72 and 100-110) as permitted by the standard.
//!
//! Function codes are the core of the Modbus PDU, determining the action to be
//! performed by the server. This module ensures that these codes are handled
//! in a type-safe manner, providing conversions between raw bytes and structured enums.
//!
//! This module is `no_std` compatible.

/// Standard Modbus function codes and their associated sub-functions.
pub mod public;

/// Extension point for non-standard, vendor-specific function codes.
pub mod user_defined;