//! # Modbus Client State Machines
//!
//! This document describes the internal state machines for a Modbus master/client.
//!
//! Derived from:
//! - MODBUS Application Protocol Specification V1.1b3
//!
//! ---
//!
//! ## 1️⃣ Sequential Client (RTU / Bare-Metal Friendly)
//!
//! This model supports exactly one outstanding transaction at a time.
//!
//! ```mermaid
//! stateDiagram-v2
//!
//!     [*] --> Idle
//!
//!     Idle --> BuildRequest : Application call
//!     BuildRequest --> SendFrame
//!     SendFrame --> WaitingForResponse
//!
//!     WaitingForResponse --> ParseResponse : Frame received
//!     WaitingForResponse --> Retry : Timeout
//!
//!     Retry --> SendFrame : Retry allowed
//!     Retry --> CompleteError : Max retries exceeded
//!
//!     ParseResponse --> CompleteSuccess : Valid response
//!     ParseResponse --> CompleteError : Exception or invalid frame
//!
//!     CompleteSuccess --> Idle
//!     CompleteError --> Idle
//! ```
//!
//! ### Notes
//! - No concurrency.
//! - Suitable for microcontrollers.
//! - Driven by `poll()` or blocking call.
//! - Transport-agnostic (UART, SPI, custom).
//!
//! ---
