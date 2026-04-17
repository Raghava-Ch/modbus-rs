//! Unit identifier and slave address types.

use crate::errors::MbusError;

/// A type-safe wrapper for Modbus Unit Identifiers (TCP) and Slave Addresses (Serial).
///
/// ### Address Ranges:
/// - **1 to 247**: Valid Unicast addresses for individual slave devices.
/// - **0**: Reserved for **BROADCAST** operations.
/// - **248 to 255**: Reserved/Invalid addresses.
///
/// ### ⚠️ Important: Broadcasting (Address 0)
/// To prevent accidental broadcast requests (which are processed by all devices and
/// **never** return a response), address `0` cannot be passed to the standard `new()`
/// or `try_from()` constructors.
///
/// Developers **must** explicitly use [`UnitIdOrSlaveAddr::new_broadcast_address()`]
/// to signal intent for a broadcast operation.
///
/// *Note: Broadcasts are generally only supported for Write operations on Serial transports.*
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitIdOrSlaveAddr(u8);

impl UnitIdOrSlaveAddr {
    /// Creates a new `UnitIdOrSlaveAddr` instance with the specified address.
    ///
    /// Valid unicast range is `1..=247`. Address `0` is rejected here; use
    /// [`new_broadcast_address()`](Self::new_broadcast_address) to create a
    /// broadcast address explicitly.
    pub fn new(address: u8) -> Result<Self, MbusError> {
        if (1..=247).contains(&address) {
            return Ok(Self(address));
        }

        if 0 == address {
            return Err(MbusError::InvalidBroadcastAddress);
        }
        Err(MbusError::InvalidSlaveAddress)
    }

    /// Creates a new `UnitIdOrSlaveAddr` instance representing the broadcast address (`0`).
    ///
    /// *Note: Broadcasts are generally only supported for Write operations on Serial transports.*
    pub fn new_broadcast_address() -> Self {
        Self(0)
    }

    /// Returns `true` if the address is the Modbus broadcast address (0).
    pub fn is_broadcast(&self) -> bool {
        self.0 == 0
    }

    /// Returns the raw `u8` value of the slave address.
    pub fn get(&self) -> u8 {
        self.0
    }
}

impl Default for UnitIdOrSlaveAddr {
    /// Provides a default value for initialization or error states.
    ///
    /// # ⚠️ Warning
    /// This returns `255`, which is outside the valid Modbus slave address range (1-247).
    /// It is intended to be used as a sentinel value to represent an uninitialized or
    /// invalid address state that must be handled by the application logic.
    fn default() -> Self {
        Self(255)
    }
}

impl TryFrom<u8> for UnitIdOrSlaveAddr {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        UnitIdOrSlaveAddr::new(value)
    }
}

impl From<UnitIdOrSlaveAddr> for u8 {
    fn from(val: UnitIdOrSlaveAddr) -> Self {
        val.get()
    }
}

/// A trait for types that can be created from a `u8` Unit ID or Slave Address.
pub trait UidSaddrFrom {
    /// Creates an instance from an internal raw `u8` Unit ID / Slave Address.
    ///
    /// This is intended for internal reconstruction paths where the value was
    /// originally produced from a validated `UnitIdOrSlaveAddr` and later stored
    /// as a raw `u8`. Do not use this for external or untrusted input — prefer
    /// `UnitIdOrSlaveAddr::new(...)` or `TryFrom<u8>` for those cases.
    fn from_u8(uid_saddr: u8) -> Self;
}

impl UidSaddrFrom for UnitIdOrSlaveAddr {
    fn from_u8(value: u8) -> Self {
        UnitIdOrSlaveAddr::new(value).unwrap_or_default()
    }
}
