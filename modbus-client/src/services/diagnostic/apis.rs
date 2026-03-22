use crate::{
    app::DiagnosticsResponse,
    services::{
        ClientCommon, ClientServices, Diag, OperationMeta,
        diagnostic::{self, ObjectId, ReadDeviceIdCode},
    },
};
use mbus_core::{
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    transport::{Transport, UnitIdOrSlaveAddr},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + DiagnosticsResponse,
{
    /// Sends a Read Device Identification request (FC 43 / 14).
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `read_device_id_code`: The type of access (01=Basic, 02=Regular, 03=Extended, 04=Specific).
    /// - `object_id`: The object ID to start reading from.
    pub fn read_device_identification(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = diagnostic::service::ServiceBuilder::read_device_identification(
            txn_id,
            unit_id_slave_addr.get(),
            read_device_id_code,
            object_id,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Diag(Diag {
                device_id_code: read_device_id_code,
                encap_type: EncapsulatedInterfaceType::Err,
            }),
            Self::handle_read_device_identification_rsp,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a generic Encapsulated Interface Transport request (FC 43).
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `mei_type`: The MEI type (e.g., `CanopenGeneralReference`).
    /// - `data`: The data payload to be sent with the request.
    pub fn encapsulated_interface_transport(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::encapsulated_interface_transport(
            txn_id,
            unit_id_slave_addr.get(),
            mei_type,
            data,
            self.transport.transport_type(),
        )?;

        // If this is a broadcast and serial transport, we do not expect a response. Do not queue it.
        if unit_id_slave_addr.is_broadcast() {
            if self.transport.transport_type().is_tcp_type() {
                return Err(MbusError::BoradcastNotAllowed);
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Diag(Diag {
                    device_id_code: ReadDeviceIdCode::Err,
                    encap_type: mei_type,
                }),
                Self::handle_encapsulated_interface_transport_rsp,
            )?;
        }

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read Exception Status request (Function Code 07).
    ///
    /// This function is specific to **Serial Line** Modbus. It is used to read the contents
    /// of eight Exception Status outputs in a remote device. The meaning of these status
    /// bits is device-dependent.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    pub fn read_exception_status(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(), MbusError> {
        // FC 07 does not support broadcast addresses as it requires a specific device response.
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed);
        }
        // Delegate PDU and ADU construction to the ServiceBuilder.
        let frame = diagnostic::service::ServiceBuilder::read_exception_status(
            unit_id_slave_addr.get(),
            self.transport.transport_type(),
        )?;

        // Register the expectation so the client knows how to handle the incoming response byte.
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_read_exception_status_rsp,
        )?;

        // Dispatch the frame through the configured serial transport.
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Diagnostics request (Function Code 08).
    ///
    /// This function provides a series of tests for checking the communication system
    /// between a client (Master) and a server (Slave), or for checking various internal
    /// error conditions within a server.
    ///
    /// **Note:** This function code is supported on **Serial Line only** (RTU/ASCII).
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `sub_function`: The specific diagnostic test to perform (e.g., `ReturnQueryData`,
    ///     `RestartCommunicationsOption`, `ClearCounters`).
    /// - `data`: A slice of 16-bit words required by the specific sub-function. Many
    ///   sub-functions expect a single word (e.g., `0x0000` or `0xFF00`).
    ///
    /// # Broadcast Support
    /// Only the following sub-functions are allowed with a broadcast address:
    /// - `RestartCommunicationsOption`
    /// - `ForceListenOnlyMode`
    /// - `ClearCountersAndDiagnosticRegister`
    /// - `ClearOverrunCounterAndFlag`
    ///
    /// If a broadcast is sent, no response is expected and no expectation is queued.
    pub fn diagnostics(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<(), MbusError> {
        const ALLOWED_BROADCAST_SUB_FUNCTIONS: [DiagnosticSubFunction; 4] = [
            DiagnosticSubFunction::RestartCommunicationsOption,
            DiagnosticSubFunction::ForceListenOnlyMode,
            DiagnosticSubFunction::ClearCountersAndDiagnosticRegister,
            DiagnosticSubFunction::ClearOverrunCounterAndFlag,
        ];
        if unit_id_slave_addr.is_broadcast()
            && !ALLOWED_BROADCAST_SUB_FUNCTIONS.contains(&sub_function)
        {
            return Err(MbusError::BoradcastNotAllowed);
        }
        let frame = diagnostic::service::ServiceBuilder::diagnostics(
            unit_id_slave_addr.get(),
            sub_function,
            data,
            self.transport.transport_type(),
        )?;

        // If this is a broadcast and serial transport, we do not expect a response. Do not queue it.
        // Note: TCP evaluation isn't strictly needed here because ServiceBuilder::diagnostics
        // already restricts this to serial only, but we check broadcast to avoid queuing.

        if !unit_id_slave_addr.is_broadcast() {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Other,
                Self::handle_diagnostics_rsp,
            )?;
        }

        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Counter request (FC 11). Serial Line only.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    pub fn get_comm_event_counter(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed);
        }
        let frame = diagnostic::service::ServiceBuilder::get_comm_event_counter(
            unit_id_slave_addr.get(),
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_get_comm_event_counter_rsp,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Log request (FC 12). Serial Line only.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    pub fn get_comm_event_log(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed);
        }
        let frame = diagnostic::service::ServiceBuilder::get_comm_event_log(
            unit_id_slave_addr.get(),
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_get_comm_event_log_rsp,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Report Server ID request (FC 17). Serial Line only.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    pub fn report_server_id(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed);
        }

        let frame = diagnostic::service::ServiceBuilder::report_server_id(
            unit_id_slave_addr.get(),
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_report_server_id_rsp,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }
}
