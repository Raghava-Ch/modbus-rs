//! # Server Statistics Tracking
//!
//! Provides optional statistics tracking for Modbus protocol metrics.
//! Enabled with the `diagnostics-stats` feature flag.
//!
//! Track counters include:
//! - **Communication metrics**: Total frames received, CRC/parse errors, serial overrun events
//! - **Exception metrics**: Total exceptions returned to clients
//! - **Response metrics**: Total frames sent, no-response requests, NAK responses, busy responses
//!
//! These counters are automatically incremented by the stack and reported via
//! FC 0x08 (Diagnostics) sub-functions:
//! - 0x0B: Return Bus Message Count
//! - 0x0C: Return Bus Communication Error Count
//! - 0x0D: Return Bus Exception Error Count
//! - 0x0E: Return Server Message Count
//! - 0x0F: Return Server No Response Count
//! - 0x10: Return Server NAK Count
//! - 0x11: Return Server Busy Count
//! - 0x12: Return Bus Character Overrun Count
//! - 0x0A: Clear Counters and Diagnostic Register
//! - 0x14: Clear Overrun Counter and Flag

/// Modbus server statistics counters.
///
/// Tracks protocol-level metrics including message counts, error counts, and state flags.
/// All counters saturate at `u16::MAX` (do not wrap).
#[derive(Clone, Copy, Debug)]
pub struct ServerStatistics {
    /// Total Modbus frames received from any source (including malformed).
    pub message_count: u16,

    /// Frames that failed CRC, parsing, or format validation.
    pub comm_error_count: u16,

    /// Modbus exception responses sent to clients.
    pub exception_error_count: u16,

    /// Total Modbus frames successfully sent to clients.
    pub server_message_count: u16,

    /// Requests that triggered no server response (broadcast, FC04 with no regs, etc).
    pub no_response_count: u16,

    /// NAK responses sent (if applicable; 0 for standard Modbus).
    pub nak_count: u16,

    /// Server busy responses sent (protocol-specific).
    pub busy_count: u16,

    /// Serial line character overrun events (hardware-level).
    pub character_overrun_count: u16,

    /// Sticky flag set when character overrun occurs; cleared only by explicit clear (0x14).
    pub character_overrun_flag: bool,
}

impl ServerStatistics {
    /// Creates a new `ServerStatistics` with all counters at zero.
    pub fn new() -> Self {
        Self {
            message_count: 0,
            comm_error_count: 0,
            exception_error_count: 0,
            server_message_count: 0,
            no_response_count: 0,
            nak_count: 0,
            busy_count: 0,
            character_overrun_count: 0,
            character_overrun_flag: false,
        }
    }

    /// Clears all counters and flags to zero.
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Clears only the character overrun flag; counters remain unchanged.
    pub fn clear_overrun_flag(&mut self) {
        self.character_overrun_flag = false;
    }

    /// Increments message_count with saturation at u16::MAX.
    pub fn increment_message_count(&mut self) {
        self.message_count = self.message_count.saturating_add(1);
    }

    /// Increments comm_error_count with saturation at u16::MAX.
    pub fn increment_comm_error_count(&mut self) {
        self.comm_error_count = self.comm_error_count.saturating_add(1);
    }

    /// Increments exception_error_count with saturation at u16::MAX.
    pub fn increment_exception_error_count(&mut self) {
        self.exception_error_count = self.exception_error_count.saturating_add(1);
    }

    /// Increments server_message_count with saturation at u16::MAX.
    pub fn increment_server_message_count(&mut self) {
        self.server_message_count = self.server_message_count.saturating_add(1);
    }

    /// Increments no_response_count with saturation at u16::MAX.
    pub fn increment_no_response_count(&mut self) {
        self.no_response_count = self.no_response_count.saturating_add(1);
    }

    /// Increments nak_count with saturation at u16::MAX.
    pub fn increment_nak_count(&mut self) {
        self.nak_count = self.nak_count.saturating_add(1);
    }

    /// Increments busy_count with saturation at u16::MAX.
    pub fn increment_busy_count(&mut self) {
        self.busy_count = self.busy_count.saturating_add(1);
    }

    /// Increments character_overrun_count with saturation at u16::MAX and sets the flag.
    pub fn increment_overrun_count(&mut self) {
        self.character_overrun_count = self.character_overrun_count.saturating_add(1);
        self.character_overrun_flag = true;
    }
}

impl Default for ServerStatistics {
    fn default() -> Self {
        Self::new()
    }
}
