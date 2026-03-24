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
    /// Sends a Read File Record request (Function Code 0x14).
    ///
    /// This function performs a data reference operation to allow reading of "File Records".
    /// A Modbus file is a collection of records. Each file can contain up to 10000 records
    /// (addressed 0000 to 9999 decimal).
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `sub_request`: A structure containing one or more sub-requests. Each sub-request
    ///     specifies the file number, record number, and the number of registers to read.
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully built, the expectation was queued,
    ///   and the frame was transmitted.
    /// - `Err(MbusError)`: If the address is a broadcast address (not allowed for FC 0x14),
    ///   if the PDU exceeds the maximum allowed size, or if transport fails.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_file_record(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        // Modbus protocol specification: Broadcast is not supported for Read File Record.
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::broadcast_not_allowed());
        }

        // Construct the ADU frame (MBAP/Serial Header + PDU + CRC/LRC if applicable)
        let frame = file_record::service::ServiceBuilder::read_file_record(
            txn_id,
            unit_id_slave_addr.get(),
            sub_request,
            self.transport.transport_type(),
        )?;

        // Register an expectation so the client knows how to route the incoming response.
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_read_file_record_response,
        )?;

        // Dispatch the frame through the underlying transport (TCP/RTU/ASCII).
        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write File Record request (Function Code 0x15).
    ///
    /// This function performs a data reference operation to allow writing of "File Records".
    /// The request can contain multiple sub-requests, each writing a sequence of registers
    /// to a specific file and record.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID for tracking the request/response pair.
    /// - `unit_id_slave_addr`: The target Modbus unit ID (TCP) or slave address (Serial).
    /// - `sub_request`: A structure containing one or more sub-requests. Each sub-request
    ///   specifies the file number, record number, record length, and the actual data to write.
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully built and transmitted.
    /// - `Err(MbusError)`: If broadcast is attempted, if the PDU is malformed, or transport fails.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn write_file_record(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_request: &SubRequest,
    ) -> Result<(), MbusError> {
        // Modbus protocol specification: Broadcast is not supported for Write File Record.
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::broadcast_not_allowed()); // Modbus forbids broadcast for Write File Record
        }

        // Construct the ADU frame using the service builder.
        let frame = file_record::service::ServiceBuilder::write_file_record(
            txn_id,
            unit_id_slave_addr.get(),
            sub_request,
            self.transport.transport_type(),
        )?;

        // Register the expectation for the response handler.
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_write_file_record_response,
        )?;

        // Send the compiled frame.
        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
