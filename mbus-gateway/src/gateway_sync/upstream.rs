use heapless::Vec;
use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, compile_adu_frame, decompile_adu_frame};
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, Transport, TransportType};

#[cfg(feature = "upstream-tcp")]
use mbus_network::StdTcpServerTransport;

#[cfg(feature = "upstream-serial-rtu")]
use mbus_serial::StdRtuTransport;

#[cfg(feature = "upstream-serial-ascii")]
use mbus_serial::StdAsciiTransport;

/// A heterogeneous sync upstream transport.
///
/// Wraps TCP, RTU serial, or ASCII serial transports in a single enum.
/// In `no_std` environments where `Box<dyn Transport>` is unavailable,
/// this enum is the idiomatic way to use multiple upstream types in
/// a single `GatewayServices` instance.
pub enum GatewayUpstream {
    /// Modbus TCP server-side upstream.
    #[cfg(feature = "upstream-tcp")]
    Tcp(StdTcpServerTransport),

    /// Modbus RTU serial upstream.
    #[cfg(feature = "upstream-serial-rtu")]
    Rtu(StdRtuTransport),

    /// Modbus ASCII serial upstream.
    #[cfg(feature = "upstream-serial-ascii")]
    Ascii(StdAsciiTransport),
}

impl Transport for GatewayUpstream {
    type Error = MbusError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp; // nominal
    const SUPPORTS_BROADCAST_WRITES: bool = true;

    fn transport_type_rt(&self) -> TransportType {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(_) => StdTcpServerTransport::TRANSPORT_TYPE,
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(_) => StdRtuTransport::TRANSPORT_TYPE,
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(_) => StdAsciiTransport::TRANSPORT_TYPE,
        }
    }

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(t) => t.connect(config).map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(t) => t.connect(config).map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(t) => t.connect(config).map_err(MbusError::from),
        }
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(t) => t.disconnect().map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(t) => t.disconnect().map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(t) => t.disconnect().map_err(MbusError::from),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(t) => t.is_connected(),
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(t) => t.is_connected(),
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(t) => t.is_connected(),
        }
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(t) => t.send(adu).map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(t) => {
                // Translate from TCP MBAP → RTU CRC
                let msg = decompile_adu_frame(adu, TransportType::StdTcp)?;
                let unit = msg.unit_id_or_slave_addr().get();
                let wire = compile_adu_frame(0, unit, msg.pdu, StdRtuTransport::TRANSPORT_TYPE)?;
                t.send(&wire).map_err(MbusError::from)
            }
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(t) => {
                // Translate from TCP MBAP → ASCII LRC
                let msg = decompile_adu_frame(adu, TransportType::StdTcp)?;
                let unit = msg.unit_id_or_slave_addr().get();
                let wire = compile_adu_frame(0, unit, msg.pdu, StdAsciiTransport::TRANSPORT_TYPE)?;
                t.send(&wire).map_err(MbusError::from)
            }
        }
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        match self {
            #[cfg(feature = "upstream-tcp")]
            Self::Tcp(t) => t.recv().map_err(MbusError::from),
            #[cfg(feature = "upstream-serial-rtu")]
            Self::Rtu(t) => {
                // Translate RTU CRC → TCP MBAP
                let wire = t.recv().map_err(MbusError::from)?;
                let msg = decompile_adu_frame(&wire, StdRtuTransport::TRANSPORT_TYPE)?;
                let unit = msg.unit_id_or_slave_addr().get();
                compile_adu_frame(0, unit, msg.pdu, TransportType::StdTcp)
            }
            #[cfg(feature = "upstream-serial-ascii")]
            Self::Ascii(t) => {
                // Translate ASCII LRC → TCP MBAP
                let wire = t.recv().map_err(MbusError::from)?;
                let msg = decompile_adu_frame(&wire, StdAsciiTransport::TRANSPORT_TYPE)?;
                let unit = msg.unit_id_or_slave_addr().get();
                compile_adu_frame(0, unit, msg.pdu, TransportType::StdTcp)
            }
        }
    }
}
