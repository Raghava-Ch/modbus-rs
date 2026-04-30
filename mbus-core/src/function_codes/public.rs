//! # Modbus Public Function Codes and Sub-functions
//!
//! This module defines the standard function codes and sub-function codes used in the
//! Modbus Application Protocol. It provides enums for:
//!
//! - **[`FunctionCode`]**: The primary operation identifier (e.g., Read Coils, Write Register).
//! - **[`DiagnosticSubFunction`]**: Sub-codes for serial-line diagnostics (FC 0x08).
//! - **[`EncapsulatedInterfaceType`]**: MEI types for tunneling other protocols (FC 0x2B).
//!
//! All types implement `TryFrom` for safe conversion from raw bytes and include
//! documentation referencing the Modbus Application Protocol Specification V1.1b3.
//!
//! This module is `no_std` compatible and uses `repr` attributes to ensure
//! memory layout matches the protocol's byte-level requirements.

use crate::errors::{ExceptionCode, MbusError};

/// Modbus Public Function Codes.
///
/// These are the standardized function codes defined in
/// the Modbus Application Protocol Specification V1.1b3.
///
/// See:
/// - Section 5.1 Public Function Code Definition
/// - Section 6.x for individual function behaviors
///
/// Reference: :contentReference[oaicite:1]{index=1}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FunctionCode {
    // ============================================================
    // Bit Access (Single-bit data)
    // ============================================================
    /// 0x00 — Undefined
    /// This value is not defined in the specification and can be used as a placeholder
    /// for uninitialized or unknown function codes.
    /// It is not a valid function code for actual Modbus transactions.
    #[default]
    Default = 0x00, // Placeholder for uninitialized or unknown function code

    #[cfg(feature = "coils")]
    /// 0x01 — Read Coils
    ///
    /// Reads the ON/OFF status of discrete output coils.
    /// Section 6.1
    ReadCoils = 0x01,

    #[cfg(feature = "discrete-inputs")]
    /// 0x02 — Read Discrete Inputs
    ///
    /// Reads the ON/OFF status of discrete input contacts.
    /// Section 6.2
    ReadDiscreteInputs = 0x02,

    #[cfg(feature = "coils")]
    /// 0x05 — Write Single Coil
    ///
    /// Forces a single coil to ON (0xFF00) or OFF (0x0000).
    /// Section 6.5
    WriteSingleCoil = 0x05,

    #[cfg(feature = "coils")]
    /// 0x0F — Write Multiple Coils
    ///
    /// Forces multiple coils to ON/OFF.
    /// Section 6.11
    WriteMultipleCoils = 0x0F,

    // ============================================================
    // 16-bit Register Access
    // ============================================================
    #[cfg(feature = "registers")]
    /// 0x03 — Read Holding Registers
    ///
    /// Reads one or more 16-bit holding registers.
    /// Section 6.3
    ReadHoldingRegisters = 0x03,

    #[cfg(feature = "registers")]
    /// 0x04 — Read Input Registers
    ///
    /// Reads one or more 16-bit input registers.
    /// Section 6.4
    ReadInputRegisters = 0x04,

    /// 0x06 — Write Single Register
    #[cfg(feature = "registers")]
    ///
    /// Writes a single 16-bit holding register.
    /// Section 6.6
    WriteSingleRegister = 0x06,

    #[cfg(feature = "registers")]
    /// 0x10 — Write Multiple Registers
    ///
    /// Writes multiple 16-bit holding registers.
    /// Section 6.12
    WriteMultipleRegisters = 0x10,

    #[cfg(feature = "registers")]
    /// 0x16 — Mask Write Register
    ///
    /// Performs a bitwise mask write on a single register.
    /// Section 6.16
    MaskWriteRegister = 0x16,

    #[cfg(feature = "registers")]
    /// 0x17 — Read/Write Multiple Registers
    ///
    /// Reads and writes multiple registers in a single transaction.
    /// Section 6.17
    ReadWriteMultipleRegisters = 0x17,

    #[cfg(feature = "fifo")]
    /// 0x18 — Read FIFO Queue
    ///
    /// Reads the contents of a FIFO queue.
    /// Section 6.18
    ReadFifoQueue = 0x18,

    // ============================================================
    // File Record Access
    // ============================================================
    #[cfg(feature = "file-record")]
    /// 0x14 — Read File Record
    ///
    /// Reads structured file records.
    /// Section 6.14
    ReadFileRecord = 0x14,

    /// 0x15 — Write File Record
    #[cfg(feature = "file-record")]
    ///
    /// Writes structured file records.
    /// Section 6.15
    WriteFileRecord = 0x15,

    // ============================================================
    // Diagnostics & Device Information
    // ============================================================
    #[cfg(feature = "diagnostics")]
    /// 0x07 — Read Exception Status (Serial Line Only)
    ///
    /// Returns 8-bit exception status.
    /// Section 6.7
    ReadExceptionStatus = 0x07,

    #[cfg(feature = "diagnostics")]
    /// 0x08 — Diagnostics (Serial Line Only)
    ///
    /// Provides diagnostic and loopback tests.
    /// Requires sub-function codes.
    /// Section 6.8
    Diagnostics = 0x08,

    #[cfg(feature = "diagnostics")]
    /// 0x0B — Get Communication Event Counter (Serial Line Only)
    ///
    /// Returns communication event counter.
    /// Section 6.9
    GetCommEventCounter = 0x0B,

    #[cfg(feature = "diagnostics")]
    /// 0x0C — Get Communication Event Log (Serial Line Only)
    ///
    /// Returns communication event log.
    /// Section 6.10
    GetCommEventLog = 0x0C,

    #[cfg(feature = "diagnostics")]
    /// 0x11 — Report Server ID (Serial Line Only)
    ///
    /// Returns server identification.
    /// Section 6.13
    ReportServerId = 0x11,

    #[cfg(feature = "diagnostics")]
    /// 0x2B — Encapsulated Interface Transport
    ///
    /// Used for:
    /// - CANopen General Reference (Sub-function 0x0D)
    /// - Read Device Identification (Sub-function 0x0E)
    ///
    /// Section 6.19, 6.20, 6.21
    EncapsulatedInterfaceTransport = 0x2B,

    // ============================================================
    // Exception Responses (0x80 bit set)
    // ============================================================
    /// 0x81 — Exception Response for Read Coils (0x01 | 0x80)
    #[cfg(feature = "coils")]
    ReadCoilsException = 0x81,

    /// 0x82 — Exception Response for Read Discrete Inputs (0x02 | 0x80)
    #[cfg(feature = "discrete-inputs")]
    ReadDiscreteInputsException = 0x82,

    /// 0x83 — Exception Response for Read Holding Registers (0x03 | 0x80)
    #[cfg(feature = "registers")]
    ReadHoldingRegistersException = 0x83,

    /// 0x84 — Exception Response for Read Input Registers (0x04 | 0x80)
    #[cfg(feature = "registers")]
    ReadInputRegistersException = 0x84,

    /// 0x85 — Exception Response for Write Single Coil (0x05 | 0x80)
    #[cfg(feature = "coils")]
    WriteSingleCoilException = 0x85,

    /// 0x86 — Exception Response for Write Single Register (0x06 | 0x80)
    #[cfg(feature = "registers")]
    WriteSingleRegisterException = 0x86,

    /// 0x87 — Exception Response for Read Exception Status (0x07 | 0x80)
    #[cfg(feature = "diagnostics")]
    ReadExceptionStatusException = 0x87,

    /// 0x88 — Exception Response for Diagnostics (0x08 | 0x80)
    #[cfg(feature = "diagnostics")]
    DiagnosticsException = 0x88,

    /// 0x8B — Exception Response for Get Communication Event Counter (0x0B | 0x80)
    #[cfg(feature = "diagnostics")]
    GetCommEventCounterException = 0x8B,

    /// 0x8C — Exception Response for Get Communication Event Log (0x0C | 0x80)
    #[cfg(feature = "diagnostics")]
    GetCommEventLogException = 0x8C,

    /// 0x8F — Exception Response for Write Multiple Coils (0x0F | 0x80)
    #[cfg(feature = "coils")]
    WriteMultipleCoilsException = 0x8F,

    /// 0x90 — Exception Response for Write Multiple Registers (0x10 | 0x80)
    #[cfg(feature = "registers")]
    WriteMultipleRegistersException = 0x90,

    /// 0x91 — Exception Response for Report Server ID (0x11 | 0x80)
    #[cfg(feature = "diagnostics")]
    ReportServerIdException = 0x91,

    /// 0x94 — Exception Response for Read File Record (0x14 | 0x80)
    #[cfg(feature = "file-record")]
    ReadFileRecordException = 0x94,

    /// 0x95 — Exception Response for Write File Record (0x15 | 0x80)
    #[cfg(feature = "file-record")]
    WriteFileRecordException = 0x95,

    /// 0x96 — Exception Response for Mask Write Register (0x16 | 0x80)
    #[cfg(feature = "registers")]
    MaskWriteRegisterException = 0x96,

    /// 0x97 — Exception Response for Read/Write Multiple Registers (0x17 | 0x80)
    #[cfg(feature = "registers")]
    ReadWriteMultipleRegistersException = 0x97,

    /// 0x98 — Exception Response for Read FIFO Queue (0x18 | 0x80)
    #[cfg(feature = "fifo")]
    ReadFifoQueueException = 0x98,

    /// 0xAB — Exception Response for Encapsulated Interface Transport (0x2B | 0x80)
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransportException = 0xAB,
}

impl TryFrom<u8> for FunctionCode {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        #[allow(unused_imports)]
        use FunctionCode::*;

        match value {
            #[cfg(feature = "coils")]
            0x01 => Ok(ReadCoils),
            #[cfg(feature = "discrete-inputs")]
            0x02 => Ok(ReadDiscreteInputs),
            #[cfg(feature = "registers")]
            0x03 => Ok(ReadHoldingRegisters),
            #[cfg(feature = "registers")]
            0x04 => Ok(ReadInputRegisters),
            #[cfg(feature = "coils")]
            0x05 => Ok(WriteSingleCoil),
            #[cfg(feature = "registers")]
            0x06 => Ok(WriteSingleRegister),
            #[cfg(feature = "diagnostics")]
            0x07 => Ok(ReadExceptionStatus),
            #[cfg(feature = "diagnostics")]
            0x08 => Ok(Diagnostics),
            #[cfg(feature = "diagnostics")]
            0x0B => Ok(GetCommEventCounter),
            #[cfg(feature = "diagnostics")]
            0x0C => Ok(GetCommEventLog),
            #[cfg(feature = "coils")]
            0x0F => Ok(WriteMultipleCoils),
            #[cfg(feature = "registers")]
            0x10 => Ok(WriteMultipleRegisters),
            #[cfg(feature = "diagnostics")]
            0x11 => Ok(ReportServerId),
            #[cfg(feature = "file-record")]
            0x14 => Ok(ReadFileRecord),
            #[cfg(feature = "file-record")]
            0x15 => Ok(WriteFileRecord),
            #[cfg(feature = "registers")]
            0x16 => Ok(MaskWriteRegister),
            #[cfg(feature = "registers")]
            0x17 => Ok(ReadWriteMultipleRegisters),
            #[cfg(feature = "fifo")]
            0x18 => Ok(ReadFifoQueue),
            #[cfg(feature = "diagnostics")]
            0x2B => Ok(EncapsulatedInterfaceTransport),
            // Exception responses (0x80 bit set)
            #[cfg(feature = "coils")]
            0x81 => Ok(ReadCoilsException),
            #[cfg(feature = "discrete-inputs")]
            0x82 => Ok(ReadDiscreteInputsException),
            #[cfg(feature = "registers")]
            0x83 => Ok(ReadHoldingRegistersException),
            #[cfg(feature = "registers")]
            0x84 => Ok(ReadInputRegistersException),
            #[cfg(feature = "coils")]
            0x85 => Ok(WriteSingleCoilException),
            #[cfg(feature = "registers")]
            0x86 => Ok(WriteSingleRegisterException),
            #[cfg(feature = "diagnostics")]
            0x87 => Ok(ReadExceptionStatusException),
            #[cfg(feature = "diagnostics")]
            0x88 => Ok(DiagnosticsException),
            #[cfg(feature = "diagnostics")]
            0x8B => Ok(GetCommEventCounterException),
            #[cfg(feature = "diagnostics")]
            0x8C => Ok(GetCommEventLogException),
            #[cfg(feature = "coils")]
            0x8F => Ok(WriteMultipleCoilsException),
            #[cfg(feature = "registers")]
            0x90 => Ok(WriteMultipleRegistersException),
            #[cfg(feature = "diagnostics")]
            0x91 => Ok(ReportServerIdException),
            #[cfg(feature = "file-record")]
            0x94 => Ok(ReadFileRecordException),
            #[cfg(feature = "file-record")]
            0x95 => Ok(WriteFileRecordException),
            #[cfg(feature = "registers")]
            0x96 => Ok(MaskWriteRegisterException),
            #[cfg(feature = "registers")]
            0x97 => Ok(ReadWriteMultipleRegistersException),
            #[cfg(feature = "fifo")]
            0x98 => Ok(ReadFifoQueueException),
            #[cfg(feature = "diagnostics")]
            0xAB => Ok(EncapsulatedInterfaceTransportException),
            _ => Err(MbusError::UnsupportedFunction(value)),
        }
    }
}

impl FunctionCode {
    /// Maps an application error to the corresponding Modbus exception code.
    ///
    /// This method determines the appropriate exception code to return based on the
    /// error that occurred during request processing. For errors that don't map to
    /// a specific exception code, `ServerDeviceFailure` is used as a default.
    ///
    /// # Arguments
    /// * `error` - The error that occurred during processing
    ///
    /// # Returns
    /// The Modbus exception code to send in the response
    ///
    /// # Example
    /// ```ignore
    /// let fc = FunctionCode::ReadHoldingRegisters;
    /// let error = MbusError::InvalidAddress;
    /// let exc_code = fc.exception_code_for_error(&error);
    /// assert_eq!(exc_code, ExceptionCode::IllegalDataAddress);
    /// ```
    pub fn exception_code_for_error(&self, error: &MbusError) -> ExceptionCode {
        match error {
            // Protocol/address errors
            MbusError::InvalidAddress | MbusError::InvalidOffset => {
                ExceptionCode::IllegalDataAddress
            }
            // Data length and parsing errors — the data field itself is malformed
            MbusError::InvalidDataLen
            | MbusError::ParseError
            | MbusError::BasicParseError
            | MbusError::InvalidPduLength => ExceptionCode::IllegalDataAddress,
            // Quantity/value errors
            MbusError::InvalidQuantity
            | MbusError::InvalidValue
            | MbusError::InvalidByteCount
            | MbusError::InvalidAndMask
            | MbusError::InvalidOrMask
            | MbusError::InvalidDeviceIdCode => ExceptionCode::IllegalDataValue,
            // Function code errors — also includes illegal sub-function / MEI types
            MbusError::InvalidFunctionCode
            | MbusError::UnsupportedFunction(_)
            | MbusError::ReservedSubFunction(_)
            | MbusError::InvalidMeiType
            | MbusError::BroadcastNotAllowed
            | MbusError::InvalidBroadcastAddress => ExceptionCode::IllegalFunction,
            // Default: all other errors map to server device failure
            _ => ExceptionCode::ServerDeviceFailure,
        }
    }

    /// Returns the exception function code variant (with 0x80 bit set) for this function code.
    ///
    /// Exception responses use function codes with the high bit (0x80) set to indicate
    /// that an exception occurred. This method maps normal function codes to their
    /// exception equivalents.
    ///
    /// # Returns
    /// The exception function code variant, or `None` if this is not a valid function code
    /// that can have exceptions.
    ///
    /// # Example
    /// ```ignore
    /// let fc = FunctionCode::ReadHoldingRegisters;
    /// let exc_fc = fc.exception_response();
    /// assert_eq!(exc_fc, Some(FunctionCode::ReadHoldingRegistersException));
    /// ```
    pub fn exception_response(&self) -> Option<FunctionCode> {
        match self {
            #[cfg(feature = "coils")]
            FunctionCode::ReadCoils => Some(FunctionCode::ReadCoilsException),
            #[cfg(feature = "discrete-inputs")]
            FunctionCode::ReadDiscreteInputs => Some(FunctionCode::ReadDiscreteInputsException),
            #[cfg(feature = "registers")]
            FunctionCode::ReadHoldingRegisters => Some(FunctionCode::ReadHoldingRegistersException),
            #[cfg(feature = "registers")]
            FunctionCode::ReadInputRegisters => Some(FunctionCode::ReadInputRegistersException),
            #[cfg(feature = "coils")]
            FunctionCode::WriteSingleCoil => Some(FunctionCode::WriteSingleCoilException),
            #[cfg(feature = "registers")]
            FunctionCode::WriteSingleRegister => Some(FunctionCode::WriteSingleRegisterException),
            #[cfg(feature = "diagnostics")]
            FunctionCode::ReadExceptionStatus => Some(FunctionCode::ReadExceptionStatusException),
            #[cfg(feature = "diagnostics")]
            FunctionCode::Diagnostics => Some(FunctionCode::DiagnosticsException),
            #[cfg(feature = "diagnostics")]
            FunctionCode::GetCommEventCounter => Some(FunctionCode::GetCommEventCounterException),
            #[cfg(feature = "diagnostics")]
            FunctionCode::GetCommEventLog => Some(FunctionCode::GetCommEventLogException),
            #[cfg(feature = "coils")]
            FunctionCode::WriteMultipleCoils => Some(FunctionCode::WriteMultipleCoilsException),
            #[cfg(feature = "registers")]
            FunctionCode::WriteMultipleRegisters => {
                Some(FunctionCode::WriteMultipleRegistersException)
            }
            #[cfg(feature = "diagnostics")]
            FunctionCode::ReportServerId => Some(FunctionCode::ReportServerIdException),
            #[cfg(feature = "file-record")]
            FunctionCode::ReadFileRecord => Some(FunctionCode::ReadFileRecordException),
            #[cfg(feature = "file-record")]
            FunctionCode::WriteFileRecord => Some(FunctionCode::WriteFileRecordException),
            #[cfg(feature = "registers")]
            FunctionCode::MaskWriteRegister => Some(FunctionCode::MaskWriteRegisterException),
            #[cfg(feature = "registers")]
            FunctionCode::ReadWriteMultipleRegisters => {
                Some(FunctionCode::ReadWriteMultipleRegistersException)
            }
            #[cfg(feature = "fifo")]
            FunctionCode::ReadFifoQueue => Some(FunctionCode::ReadFifoQueueException),
            #[cfg(feature = "diagnostics")]
            FunctionCode::EncapsulatedInterfaceTransport => {
                Some(FunctionCode::EncapsulatedInterfaceTransportException)
            }
            // Already exception codes or default
            _ => None,
        }
    }
}

/// Sub-function codes for Function Code 0x08 (Diagnostics).
///
/// Serial line only.
/// See Modbus Application Protocol Specification V1.1b3, Section 6.8.
///
/// These values are 16-bit and encoded big-endian inside the PDU data field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DiagnosticSubFunction {
    /// 0x0000 — Return Query Data (Loopback test)
    ReturnQueryData = 0x0000,

    /// 0x0001 — Restart Communications Option
    RestartCommunicationsOption = 0x0001,

    /// 0x0002 — Return Diagnostic Register
    ReturnDiagnosticRegister = 0x0002,

    /// 0x0003 — Change ASCII Input Delimiter
    ChangeAsciiInputDelimiter = 0x0003,

    /// 0x0004 — Force Listen Only Mode
    ForceListenOnlyMode = 0x0004,

    /// 0x000A — Clear Counters and Diagnostic Register
    ClearCountersAndDiagnosticRegister = 0x000A,

    /// 0x000B — Return Bus Message Count
    ReturnBusMessageCount = 0x000B,

    /// 0x000C — Return Bus Communication Error Count
    ReturnBusCommunicationErrorCount = 0x000C,

    /// 0x000D — Return Bus Exception Error Count
    ReturnBusExceptionErrorCount = 0x000D,

    /// 0x000E — Return Server Message Count
    ReturnServerMessageCount = 0x000E,

    /// 0x000F — Return Server No Response Count
    ReturnServerNoResponseCount = 0x000F,

    /// 0x0010 — Return Server NAK Count
    ReturnServerNakCount = 0x0010,

    /// 0x0011 — Return Server Busy Count
    ReturnServerBusyCount = 0x0011,

    /// 0x0012 — Return Bus Character Overrun Count
    ReturnBusCharacterOverrunCount = 0x0012,

    /// 0x0014 — Clear Overrun Counter and Flag
    ClearOverrunCounterAndFlag = 0x0014,
}

impl DiagnosticSubFunction {
    /// Converts the `DiagnosticSubFunction` enum variant into its 2-byte big-endian representation.
    pub fn to_be_bytes(self) -> [u8; 2] {
        (self as u16).to_be_bytes()
    }
}

impl From<DiagnosticSubFunction> for u16 {
    fn from(sub_func: DiagnosticSubFunction) -> Self {
        sub_func as u16
    }
}

impl TryFrom<u16> for DiagnosticSubFunction {
    type Error = MbusError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        use DiagnosticSubFunction::*;

        match value {
            0x0000 => Ok(ReturnQueryData),
            0x0001 => Ok(RestartCommunicationsOption),
            0x0002 => Ok(ReturnDiagnosticRegister),
            0x0003 => Ok(ChangeAsciiInputDelimiter),
            0x0004 => Ok(ForceListenOnlyMode),

            // 0x0005–0x0009 Reserved
            0x000A => Ok(ClearCountersAndDiagnosticRegister),
            0x000B => Ok(ReturnBusMessageCount),
            0x000C => Ok(ReturnBusCommunicationErrorCount),
            0x000D => Ok(ReturnBusExceptionErrorCount),
            0x000E => Ok(ReturnServerMessageCount),
            0x000F => Ok(ReturnServerNoResponseCount),
            0x0010 => Ok(ReturnServerNakCount),
            0x0011 => Ok(ReturnServerBusyCount),
            0x0012 => Ok(ReturnBusCharacterOverrunCount),

            // 0x0013 Reserved
            0x0014 => Ok(ClearOverrunCounterAndFlag),

            // Everything else reserved per spec
            _ => Err(MbusError::ReservedSubFunction(value)),
        }
    }
}

/// MEI (Modbus Encapsulated Interface) types
/// for Function Code 0x2B.
///
/// See Section 6.19–6.21 of the specification.
///
/// Encoded as 1 byte following the function code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum EncapsulatedInterfaceType {
    /// Placeholder default value used before a concrete MEI type is parsed.
    /// This value should not appear in a valid decoded protocol frame.
    #[default]
    Err,
    /// 0x0D — CANopen General Reference
    CanopenGeneralReference = 0x0D,

    /// 0x0E — Read Device Identification
    ReadDeviceIdentification = 0x0E,
}

impl From<EncapsulatedInterfaceType> for u8 {
    fn from(val: EncapsulatedInterfaceType) -> Self {
        val as u8
    }
}

impl TryFrom<u8> for EncapsulatedInterfaceType {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0D => Ok(Self::CanopenGeneralReference),
            0x0E => Ok(Self::ReadDeviceIdentification),
            _ => Err(MbusError::ReservedSubFunction(value as u16)),
        }
    }
}
