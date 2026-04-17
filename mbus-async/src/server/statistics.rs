//! Protocol-level statistics counters for the async server session.
//!
//! Enabled with the `diagnostics-stats` feature flag.  Reported via the FC08 Diagnostics
//! counter sub-functions (0x000B – 0x0011) and cleared by sub-function 0x000A or 0x0014.

/// Modbus server statistics counters.
///
/// Tracked automatically by [`AsyncServerSession::run()`](super::session::AsyncServerSession::run)
/// and reported via the FC08 Diagnostics counter sub-functions when the `diagnostics-stats`
/// feature is enabled.
///
/// All counters saturate at `u16::MAX` (they do not wrap).
#[derive(Clone, Copy, Debug, Default)]
pub struct AsyncServerStatistics {
    /// Total Modbus ADU frames received (including malformed or wrong-unit frames).
    pub message_count: u16,

    /// Frames that failed ADU framing / parsing validation.
    pub comm_error_count: u16,

    /// Exception responses sent to clients.
    pub exception_error_count: u16,

    /// Requests successfully dispatched to the application handler.
    pub server_message_count: u16,

    /// Requests for which no response was sent (listen-only drops, broadcast writes,
    /// `FC08/0x0004 ForceListenOnlyMode`).
    pub no_response_count: u16,

    /// NAK responses sent (always 0 for standard Modbus; tracked for spec completeness).
    pub nak_count: u16,

    /// Server-busy responses sent (always 0 unless the app returns a Busy exception).
    pub busy_count: u16,

    /// Character overrun events.  Always 0 for TCP sessions; may be non-zero on
    /// serial transports that surface overrun notifications upward.
    pub character_overrun_count: u16,
}

impl AsyncServerStatistics {
    /// Creates a new instance with all counters at zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all counters to zero.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Clears only the character overrun counter (FC08 sub-function 0x0014).
    pub fn clear_overrun(&mut self) {
        self.character_overrun_count = 0;
    }

    pub(crate) fn increment_message_count(&mut self) {
        self.message_count = self.message_count.saturating_add(1);
    }
    pub(crate) fn increment_comm_error_count(&mut self) {
        self.comm_error_count = self.comm_error_count.saturating_add(1);
    }
    pub(crate) fn increment_exception_error_count(&mut self) {
        self.exception_error_count = self.exception_error_count.saturating_add(1);
    }
    pub(crate) fn increment_server_message_count(&mut self) {
        self.server_message_count = self.server_message_count.saturating_add(1);
    }
    pub(crate) fn increment_no_response_count(&mut self) {
        self.no_response_count = self.no_response_count.saturating_add(1);
    }
}
