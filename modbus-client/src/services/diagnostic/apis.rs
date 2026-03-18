use crate::{
    app::DiagnosticsResponse,
    services::{
        ClientCommon, ClientServices, Diag, ExpectedResponse, OperationMeta,
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
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `read_device_id_code`: The type of access (01=Basic, 02=Regular, 03=Extended, 04=Specific).
    /// - `object_id`: The object ID to start reading from.
    pub fn read_device_identification(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::read_device_identification(
            txn_id,
            unit_id_slave_addr.get(),
            read_device_id_code,
            object_id,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_device_identification_rsp,
                operation_meta: OperationMeta::Diag(Diag {
                    device_id_code: read_device_id_code,
                    encap_type: EncapsulatedInterfaceType::Err,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a generic Encapsulated Interface Transport request (FC 43).
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `mei_type`: The MEI type (e.g., `CanopenGeneralReference`).
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

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_encapsulated_interface_transport_rsp,
                operation_meta: OperationMeta::Diag(Diag {
                    device_id_code: ReadDeviceIdCode::Err,
                    encap_type: mei_type,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read Exception Status request (FC 07). Serial Line only.
    pub fn read_exception_status(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::read_exception_status(
            unit_id_slave_addr.get(),
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_exception_status_rsp,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Diagnostics request (FC 08). Serial Line only.
    pub fn diagnostics(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::diagnostics(
            unit_id_slave_addr.get(),
            sub_function,
            data,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_diagnostics_rsp,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Counter request (FC 11). Serial Line only.
    pub fn get_comm_event_counter(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::get_comm_event_counter(
            unit_id,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_get_comm_event_counter_rsp,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Get Comm Event Log request (FC 12). Serial Line only.
    pub fn get_comm_event_log(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::get_comm_event_log(
            unit_id,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_get_comm_event_log_rsp,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Report Server ID request (FC 17). Serial Line only.
    pub fn report_server_id(&mut self, txn_id: u16, unit_id: u8) -> Result<(), MbusError> {
        let frame = diagnostic::service::ServiceBuilder::report_server_id(
            unit_id,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id,
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_report_server_id_rsp,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;
        self.transport
            .send(&frame)
            .map_err(|_| MbusError::SendFailed)?;
        Ok(())
    }
}
