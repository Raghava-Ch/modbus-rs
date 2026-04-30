//! Async client traffic notification trait.
//!
//! Enabled only when the `traffic` feature is active.  Applications opt in by
//! implementing [`AsyncClientNotifier`] and registering the implementation
//! through [`AsyncClientCore::set_traffic_notifier`].
//!
//! All methods carry default no-op implementations so that implementors only
//! override the hooks they care about.  This mirrors the pattern used by the
//! async server's `AsyncTrafficNotifier`.
//!
//! [`AsyncClientCore::set_traffic_notifier`]:
//!     crate::client::client_core::AsyncClientCore::set_traffic_notifier

use std::sync::Arc;

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use tokio::sync::Mutex;

// ─── AsyncClientNotifier ──────────────────────────────────────────────────────

/// Optional raw-frame traffic notifications emitted by the async client task.
///
/// Enabled only when the `traffic` feature is active.  All methods have
/// default no-op implementations — override only the ones you care about.
///
/// This is the async-client counterpart of the server's `AsyncTrafficNotifier`.
///
/// # Thread safety
///
/// Implementations must be `Send`.  The notifier is wrapped in a shared
/// [`tokio::sync::Mutex`] so a single instance is shared between the
/// public [`AsyncClientCore`] and the background task without cloning.
///
/// # Example
/// ```rust,ignore
/// #[cfg(feature = "traffic")]
/// impl AsyncClientNotifier for MyNotifier {
///     fn on_tx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
///         println!("tx txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
///     }
///     fn on_rx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
///         println!("rx txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
///     }
/// }
/// ```
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
pub trait AsyncClientNotifier {
    /// Called after a request frame is successfully sent to the device.
    fn on_tx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {}

    /// Called after a complete response frame is received from the device.
    fn on_rx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {}

    /// Called when transmitting a request frame fails.
    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }

    /// Called when receiving or parsing a response frame fails.
    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }
}

// ─── NotifierStore ────────────────────────────────────────────────────────────

/// Shared, mutable slot for the optional traffic notifier.
///
/// Cloned into both [`AsyncClientCore`] and the background task so both sides
/// can reach the same instance without an extra channel.
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
pub(crate) type NotifierStore = Arc<Mutex<Option<Box<dyn AsyncClientNotifier + Send + 'static>>>>;

/// Creates a new, empty [`NotifierStore`].
pub(crate) fn new_notifier_store() -> NotifierStore {
    Arc::new(Mutex::new(None))
}
