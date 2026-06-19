//! `WasmClientTask` — event-driven message loop for WASM Modbus clients.
//!
//! Receives `WasmCommand` requests over a channel, executes them over the transport,
//! and maps the resulting responses back to the oneshot caller.

use futures_channel::mpsc::UnboundedReceiver;
use futures_channel::oneshot::Sender as OneshotSender;
use futures_util::FutureExt;
use futures_util::stream::StreamExt;
use std::collections::HashMap;

use mbus_async::client::command::ClientRequest;
use mbus_async::client::decode::decode_response;
use mbus_async::client::encode::encode_request;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::TransportType;

use super::command::WasmCommand;
use super::response::WasmResponse;

pub(crate) trait WasmAsyncTransportTrait {
    async fn recv_frame(&mut self) -> Result<heapless::Vec<u8, MAX_ADU_FRAME_LEN>, MbusError>;
    fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError>;
}

impl WasmAsyncTransportTrait for mbus_network::WasmAsyncTransport {
    async fn recv_frame(&mut self) -> Result<heapless::Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.recv_frame().await
    }
    fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.send_frame(adu)
    }
}

impl<const ASCII: bool> WasmAsyncTransportTrait for mbus_serial::WasmSerialTransport<ASCII> {
    async fn recv_frame(&mut self) -> Result<heapless::Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.recv_frame().await
    }
    fn send_frame(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.send_frame(adu)
    }
}

pub(crate) struct WasmClientTask<T> {
    transport: T,
    cmd_rx: UnboundedReceiver<WasmCommand>,
    pending: HashMap<u16, (OneshotSender<Result<WasmResponse, String>>, ClientRequest)>,
    next_txn_id: u16,
    transport_type: TransportType,
}

impl<T: WasmAsyncTransportTrait> WasmClientTask<T> {
    pub fn new(
        transport: T,
        cmd_rx: UnboundedReceiver<WasmCommand>,
        transport_type: TransportType,
    ) -> Self {
        Self {
            transport,
            cmd_rx,
            pending: HashMap::new(),
            next_txn_id: 1,
            transport_type,
        }
    }

    fn advance_txn_id(&mut self) -> u16 {
        let id = self.next_txn_id;
        self.next_txn_id = match self.next_txn_id.wrapping_add(1) {
            0 => 1,
            n => n,
        };
        id
    }

    pub async fn run(mut self) {
        loop {
            let in_flight = !self.pending.is_empty();

            if in_flight {
                futures_util::select! {
                    cmd_opt = self.cmd_rx.next().fuse() => {
                        match cmd_opt {
                            Some(cmd) => self.handle_command(cmd).await,
                            None => break,
                        }
                    }
                    frame_res = self.transport.recv_frame().fuse() => {
                        match frame_res {
                            Ok(frame) => self.process_frame(&frame),
                            Err(e) => {
                                self.drain_all_with_error(format!("{:?}", e));
                                break;
                            }
                        }
                    }
                }
            } else {
                match self.cmd_rx.next().await {
                    Some(cmd) => self.handle_command(cmd).await,
                    None => break,
                }
            }
        }
        self.drain_all_with_error("Task closed".to_string());
    }

    fn drain_all_with_error(&mut self, err_msg: String) {
        for (_, (resp_tx, _)) in self.pending.drain() {
            let _ = resp_tx.send(Err(err_msg.clone()));
        }
    }

    async fn handle_command(&mut self, cmd: WasmCommand) {
        let (req, resp_tx) = match cmd {
            WasmCommand::ReadCoils {
                unit_id,
                address,
                quantity,
                resp,
            } => (
                ClientRequest::ReadMultipleCoils {
                    unit: unit_id,
                    address,
                    quantity,
                },
                resp,
            ),
            WasmCommand::ReadDiscreteInputs {
                unit_id,
                address,
                quantity,
                resp,
            } => (
                ClientRequest::ReadDiscreteInputs {
                    unit: unit_id,
                    address,
                    quantity,
                },
                resp,
            ),
            WasmCommand::ReadHoldingRegisters {
                unit_id,
                address,
                quantity,
                resp,
            } => (
                ClientRequest::ReadHoldingRegisters {
                    unit: unit_id,
                    address,
                    quantity,
                },
                resp,
            ),
            WasmCommand::ReadInputRegisters {
                unit_id,
                address,
                quantity,
                resp,
            } => (
                ClientRequest::ReadInputRegisters {
                    unit: unit_id,
                    address,
                    quantity,
                },
                resp,
            ),
            WasmCommand::WriteSingleCoil {
                unit_id,
                address,
                value,
                resp,
            } => (
                ClientRequest::WriteSingleCoil {
                    unit: unit_id,
                    address,
                    value,
                },
                resp,
            ),
            WasmCommand::WriteSingleRegister {
                unit_id,
                address,
                value,
                resp,
            } => (
                ClientRequest::WriteSingleRegister {
                    unit: unit_id,
                    address,
                    value,
                },
                resp,
            ),
            WasmCommand::WriteMultipleCoils {
                unit_id,
                address,
                values,
                resp,
            } => {
                use mbus_core::models::coil::Coils;
                match Coils::new(address, values.len() as u16) {
                    Ok(mut coils) => {
                        let mut ok = true;
                        for (i, &v) in values.iter().enumerate() {
                            if coils.set_value(address + i as u16, v).is_err() {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            (
                                ClientRequest::WriteMultipleCoils {
                                    unit: unit_id,
                                    address,
                                    coils,
                                },
                                resp,
                            )
                        } else {
                            let _ = resp.send(Err("Failed to set coil values".to_string()));
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = resp.send(Err(format!("{:?}", e)));
                        return;
                    }
                }
            }
            WasmCommand::WriteMultipleRegisters {
                unit_id,
                address,
                values,
                resp,
            } => {
                let mut heapless_vals = heapless::Vec::new();
                if heapless_vals.extend_from_slice(&values).is_err() {
                    let _ = resp.send(Err("Too many registers".to_string()));
                    return;
                }
                (
                    ClientRequest::WriteMultipleRegisters {
                        unit: unit_id,
                        address,
                        values: heapless_vals,
                    },
                    resp,
                )
            }
            WasmCommand::ReadWriteMultipleRegisters {
                unit_id,
                read_address,
                read_quantity,
                write_address,
                write_values,
                resp,
            } => {
                let mut heapless_vals = heapless::Vec::new();
                if heapless_vals.extend_from_slice(&write_values).is_err() {
                    let _ = resp.send(Err("Too many write registers".to_string()));
                    return;
                }
                (
                    ClientRequest::ReadWriteMultipleRegisters {
                        unit: unit_id,
                        read_address,
                        read_quantity,
                        write_address,
                        write_values: heapless_vals,
                    },
                    resp,
                )
            }
            WasmCommand::ReadFifoQueue {
                unit_id,
                address,
                resp,
            } => (
                ClientRequest::ReadFifoQueue {
                    unit: unit_id,
                    address,
                },
                resp,
            ),
            WasmCommand::MaskWriteRegister {
                unit_id,
                address,
                and_mask,
                or_mask,
                resp,
            } => (
                ClientRequest::MaskWriteRegister {
                    unit: unit_id,
                    address,
                    and_mask,
                    or_mask,
                },
                resp,
            ),
            #[cfg(feature = "file-record")]
            WasmCommand::ReadFileRecord {
                unit_id,
                requests,
                resp,
            } => {
                use mbus_core::models::file_record::SubRequest;
                let mut sub_req = SubRequest::new();
                let mut ok = true;
                for req in requests {
                    if sub_req
                        .add_read_sub_request(req.file_number, req.record_number, req.record_length)
                        .is_err()
                    {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    (
                        ClientRequest::ReadFileRecord {
                            unit: unit_id,
                            sub_request: sub_req,
                        },
                        resp,
                    )
                } else {
                    let _ = resp.send(Err(
                        "Failed to build read file record sub-request".to_string()
                    ));
                    return;
                }
            }
            #[cfg(feature = "file-record")]
            WasmCommand::WriteFileRecord {
                unit_id,
                requests,
                resp,
            } => {
                use mbus_core::models::file_record::SubRequest;
                let mut sub_req = SubRequest::new();
                let mut ok = true;
                for req in requests {
                    if let Some(data) = req.record_data {
                        if sub_req
                            .add_write_sub_request(
                                req.file_number,
                                req.record_number,
                                req.record_length,
                                data,
                            )
                            .is_err()
                        {
                            ok = false;
                            break;
                        }
                    } else {
                        ok = false;
                        break;
                    }
                }
                if ok {
                    (
                        ClientRequest::WriteFileRecord {
                            unit: unit_id,
                            sub_request: sub_req,
                        },
                        resp,
                    )
                } else {
                    let _ = resp.send(Err(
                        "Failed to build write file record sub-request".to_string()
                    ));
                    return;
                }
            }
            WasmCommand::ReadExceptionStatus { unit_id, resp } => {
                (ClientRequest::ReadExceptionStatus { unit: unit_id }, resp)
            }
            #[cfg(feature = "diagnostics")]
            WasmCommand::Diagnostics {
                unit_id,
                sub_function,
                data,
                resp,
            } => {
                use mbus_core::function_codes::public::DiagnosticSubFunction;
                match DiagnosticSubFunction::try_from(sub_function) {
                    Ok(sub_fn) => {
                        let mut heapless_data = heapless::Vec::new();
                        if heapless_data.extend_from_slice(&data).is_err() {
                            let _ = resp.send(Err("Too much diagnostic data".to_string()));
                            return;
                        }
                        (
                            ClientRequest::Diagnostics {
                                unit: unit_id,
                                sub_function: sub_fn,
                                data: heapless_data,
                            },
                            resp,
                        )
                    }
                    Err(e) => {
                        let _ = resp.send(Err(format!("{:?}", e)));
                        return;
                    }
                }
            }
            #[cfg(feature = "diagnostics")]
            WasmCommand::ReadDeviceIdentification {
                unit_id,
                read_device_id_code,
                object_id,
                resp,
            } => {
                use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
                match (
                    ReadDeviceIdCode::try_from(read_device_id_code),
                    ObjectId::try_from(object_id),
                ) {
                    (Ok(code), Ok(obj_id)) => (
                        ClientRequest::ReadDeviceIdentification {
                            unit: unit_id,
                            read_device_id_code: code,
                            object_id: obj_id,
                        },
                        resp,
                    ),
                    _ => {
                        let _ = resp.send(Err("Invalid readDeviceIdCode or objectId".to_string()));
                        return;
                    }
                }
            }
        };

        let txn_id = self.advance_txn_id();
        match encode_request(txn_id, &req, self.transport_type) {
            Ok(frame) => match self.transport.send_frame(&frame) {
                Ok(()) => {
                    self.pending.insert(txn_id, (resp_tx, req));
                }
                Err(e) => {
                    let _ = resp_tx.send(Err(format!("{:?}", e)));
                }
            },
            Err(e) => {
                let _ = resp_tx.send(Err(format!("{:?}", e)));
            }
        }
    }

    fn process_frame(&mut self, frame: &[u8]) {
        match decode_response(frame, self.transport_type) {
            Ok((decoded_txn_id, _unit, response_res)) => {
                let key = if decoded_txn_id != 0 {
                    Some(decoded_txn_id)
                } else {
                    self.pending.keys().next().copied()
                };

                if let Some(k) = key {
                    if let Some((resp_tx, req)) = self.pending.remove(&k) {
                        match response_res {
                            Ok(resp) => {
                                let wasm_resp = self.map_response(resp, &req);
                                let _ = resp_tx.send(Ok(wasm_resp));
                            }
                            Err(e) => {
                                let _ = resp_tx.send(Err(format!("{:?}", e)));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if let Some(k) = self.pending.keys().next().copied() {
                    if let Some((resp_tx, _)) = self.pending.remove(&k) {
                        let _ = resp_tx.send(Err(format!("{:?}", e)));
                    }
                }
            }
        }
    }

    fn map_response(
        &self,
        resp: mbus_async::client::response::ClientResponse,
        req: &ClientRequest,
    ) -> WasmResponse {
        use mbus_async::client::response::ClientResponse as R;
        match resp {
            #[cfg(feature = "coils")]
            R::Coils(coils) => match req {
                ClientRequest::ReadMultipleCoils { quantity, .. } => {
                    let mut vec = Vec::with_capacity(*quantity as usize);
                    for i in 0..*quantity {
                        let byte_idx = (i / 8) as usize;
                        let bit_idx = i % 8;
                        let val = (coils.values()[byte_idx] & (1 << bit_idx)) != 0;
                        vec.push(val);
                    }
                    WasmResponse::BoolArray(vec)
                }
                _ => WasmResponse::Void,
            },
            #[cfg(feature = "discrete-inputs")]
            R::DiscreteInputs(inputs) => match req {
                ClientRequest::ReadDiscreteInputs { quantity, .. } => {
                    let mut vec = Vec::with_capacity(*quantity as usize);
                    for i in 0..*quantity {
                        let byte_idx = (i / 8) as usize;
                        let bit_idx = i % 8;
                        let val = (inputs.values()[byte_idx] & (1 << bit_idx)) != 0;
                        vec.push(val);
                    }
                    WasmResponse::BoolArray(vec)
                }
                _ => WasmResponse::Void,
            },
            #[cfg(feature = "holding-registers")]
            R::HoldingRegisters(registers) => match req {
                ClientRequest::ReadHoldingRegisters { quantity, .. } => {
                    WasmResponse::U16Array(registers.values()[..*quantity as usize].to_vec())
                }
                ClientRequest::ReadWriteMultipleRegisters { read_quantity, .. } => {
                    WasmResponse::U16Array(registers.values()[..*read_quantity as usize].to_vec())
                }
                _ => WasmResponse::Void,
            },
            #[cfg(feature = "input-registers")]
            R::InputRegisters(registers) => {
                let quantity = match req {
                    ClientRequest::ReadInputRegisters { quantity, .. } => *quantity,
                    _ => registers.quantity(),
                };
                WasmResponse::U16Array(registers.values()[..quantity as usize].to_vec())
            }
            #[cfg(feature = "holding-registers")]
            R::SingleRegisterWrite { .. } => WasmResponse::Void,
            #[cfg(feature = "holding-registers")]
            R::MaskWriteRegister => WasmResponse::Void,
            #[cfg(feature = "fifo")]
            R::FifoQueue(fifo) => WasmResponse::U16Array(fifo.queue()[..fifo.length()].to_vec()),
            #[cfg(feature = "file-record")]
            R::FileRecordRead(sub_requests) => {
                let mut vec = Vec::new();
                for sub in sub_requests {
                    if let Some(data) = sub.record_data {
                        vec.push(data.to_vec());
                    } else {
                        vec.push(Vec::new());
                    }
                }
                WasmResponse::FileRecord(vec)
            }
            #[cfg(feature = "file-record")]
            R::FileRecordWrite => WasmResponse::Void,
            #[cfg(feature = "diagnostics")]
            R::ExceptionStatus(status) => WasmResponse::U8(status),
            #[cfg(feature = "diagnostics")]
            R::DiagnosticsData { sub_function, data } => WasmResponse::Diagnostics {
                sub_function: sub_function.into(),
                data: data.to_vec(),
            },
            #[cfg(feature = "diagnostics")]
            R::DeviceIdentification(resp) => {
                let mut objects = Vec::new();
                for item in resp.objects() {
                    if let Ok(o) = item {
                        let val_str = core::str::from_utf8(&o.value)
                            .map(|s| s.to_owned())
                            .unwrap_or_else(|_| {
                                o.value
                                    .iter()
                                    .map(|b| format!("{:02X}", b))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            });
                        objects.push((o.object_id.into(), val_str));
                    }
                }
                WasmResponse::DeviceIdentification {
                    read_device_id_code: resp.read_device_id_code as u8,
                    conformity_level: resp.conformity_level as u8,
                    more_follows: resp.more_follows,
                    objects,
                }
            }
            _ => WasmResponse::Void,
        }
    }
}
