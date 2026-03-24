//! Modbus Application Layer Module
//!
//! This module provides the high-level abstractions and traits required for
//! application-level interaction with the Modbus protocol.
//!
//! It defines:
//! - Response handling traits ([`CoilResponse`], [`RegisterResponse`], etc.) that
//!   allow users to define custom logic for processing server responses.
//! - Error notification mechanisms ([`RequestErrorNotifier`]).
//! - Re-exports of core data structures used by the application layer for
//!   convenient access.

mod app_trait;

pub use app_trait::*;
