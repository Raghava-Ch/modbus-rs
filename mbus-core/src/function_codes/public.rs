use crate::errors::MbusError;

pub const MAX_PDU_DATA_LEN: usize = 252; // Maximum data length for a PDU (excluding function code)

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

    /// 0x01 — Read Coils
    ///
    /// Reads the ON/OFF status of discrete output coils.
    /// Section 6.1
    ReadCoils = 0x01,

    /// 0x02 — Read Discrete Inputs
    ///
    /// Reads the ON/OFF status of discrete input contacts.
    /// Section 6.2
    ReadDiscreteInputs = 0x02,

    /// 0x05 — Write Single Coil
    ///
    /// Forces a single coil to ON (0xFF00) or OFF (0x0000).
    /// Section 6.5
    WriteSingleCoil = 0x05,

    /// 0x0F — Write Multiple Coils
    ///
    /// Forces multiple coils to ON/OFF.
    /// Section 6.11
    WriteMultipleCoils = 0x0F,

    // ============================================================
    // 16-bit Register Access
    // ============================================================
    /// 0x03 — Read Holding Registers
    ///
    /// Reads one or more 16-bit holding registers.
    /// Section 6.3
    ReadHoldingRegisters = 0x03,

    /// 0x04 — Read Input Registers
    ///
    /// Reads one or more 16-bit input registers.
    /// Section 6.4
    ReadInputRegisters = 0x04,

    /// 0x06 — Write Single Register
    ///
    /// Writes a single 16-bit holding register.
    /// Section 6.6
    WriteSingleRegister = 0x06,

    /// 0x10 — Write Multiple Registers
    ///
    /// Writes multiple 16-bit holding registers.
    /// Section 6.12
    WriteMultipleRegisters = 0x10,

    /// 0x16 — Mask Write Register
    ///
    /// Performs a bitwise mask write on a single register.
    /// Section 6.16
    MaskWriteRegister = 0x16,

    /// 0x17 — Read/Write Multiple Registers
    ///
    /// Reads and writes multiple registers in a single transaction.
    /// Section 6.17
    ReadWriteMultipleRegisters = 0x17,

    /// 0x18 — Read FIFO Queue
    ///
    /// Reads the contents of a FIFO queue.
    /// Section 6.18
    ReadFifoQueue = 0x18,

    // ============================================================
    // File Record Access
    // ============================================================
    /// 0x14 — Read File Record
    ///
    /// Reads structured file records.
    /// Section 6.14
    ReadFileRecord = 0x14,

    /// 0x15 — Write File Record
    ///
    /// Writes structured file records.
    /// Section 6.15
    WriteFileRecord = 0x15,

    // ============================================================
    // Diagnostics & Device Information
    // ============================================================
    /// 0x07 — Read Exception Status (Serial Line Only)
    ///
    /// Returns 8-bit exception status.
    /// Section 6.7
    ReadExceptionStatus = 0x07,

    /// 0x08 — Diagnostics (Serial Line Only)
    ///
    /// Provides diagnostic and loopback tests.
    /// Requires sub-function codes.
    /// Section 6.8
    Diagnostics = 0x08,

    /// 0x0B — Get Communication Event Counter (Serial Line Only)
    ///
    /// Returns communication event counter.
    /// Section 6.9
    GetCommEventCounter = 0x0B,

    /// 0x0C — Get Communication Event Log (Serial Line Only)
    ///
    /// Returns communication event log.
    /// Section 6.10
    GetCommEventLog = 0x0C,

    /// 0x11 — Report Server ID (Serial Line Only)
    ///
    /// Returns server identification.
    /// Section 6.13
    ReportServerId = 0x11,

    /// 0x2B — Encapsulated Interface Transport
    ///
    /// Used for:
    /// - CANopen General Reference (Sub-function 0x0D)
    /// - Read Device Identification (Sub-function 0x0E)
    ///
    /// Section 6.19, 6.20, 6.21
    EncapsulatedInterfaceTransport = 0x2B,
}

impl TryFrom<u8> for FunctionCode {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use FunctionCode::*;

        match value {
            0x01 => Ok(ReadCoils),
            0x02 => Ok(ReadDiscreteInputs),
            0x03 => Ok(ReadHoldingRegisters),
            0x04 => Ok(ReadInputRegisters),
            0x05 => Ok(WriteSingleCoil),
            0x06 => Ok(WriteSingleRegister),
            0x07 => Ok(ReadExceptionStatus),
            0x08 => Ok(Diagnostics),
            0x0B => Ok(GetCommEventCounter),
            0x0C => Ok(GetCommEventLog),
            0x0F => Ok(WriteMultipleCoils),
            0x10 => Ok(WriteMultipleRegisters),
            0x11 => Ok(ReportServerId),
            0x14 => Ok(ReadFileRecord),
            0x15 => Ok(WriteFileRecord),
            0x16 => Ok(MaskWriteRegister),
            0x17 => Ok(ReadWriteMultipleRegisters),
            0x18 => Ok(ReadFifoQueue),
            0x2B => Ok(EncapsulatedInterfaceTransport),
            _ => Err(MbusError::UnsupportedFunction(value)),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EncapsulatedInterfaceType {
    /// 0x0D — CANopen General Reference
    CanopenGeneralReference = 0x0D,

    /// 0x0E — Read Device Identification
    ReadDeviceIdentification = 0x0E,
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
