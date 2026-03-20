use crate::{
    app::FileRecordResponse,
    services::{
        ClientCommon, ClientServices, OperationMeta,
        file_record::{self, SubRequest},
    },
};
use mbus_core::{
    errors::MbusError,
    transport::{Transport, UnitIdOrSlaveAddr},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + FileRecordResponse,
{
    /// Sends a Read File Record request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `sub_request`: The sub-request structure containing file/record details.
    pub fn read_file_record(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed);
        }
        let frame = file_record::service::ServiceBuilder::read_file_record(
            txn_id,
            unit_id_slave_addr.get(),
            sub_request,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_read_file_record_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write File Record request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID.
    /// - `unit_id`: The Modbus unit ID.
    /// - `sub_request`: The sub-request structure containing file/record details and data.
    pub fn write_file_record(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast for Write File Record
        }

        let frame = file_record::service::ServiceBuilder::write_file_record(
            txn_id,
            unit_id_slave_addr.get(),
            sub_request,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_write_file_record_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
