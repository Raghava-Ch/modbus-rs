//! # Modbus Client State Machines
//!
//! This document describes the internal state machines for a Modbus master/client.
//!
//! Derived from:
//! - MODBUS Messaging on TCP/IP Implementation Guide V1.0b
//!
//! ---
//!
//! ## TCP Client (Connection + Concurrent Transactions)
//!
//! TCP adds:
//! - Connection management
//! - Transaction ID matching
//! - Multiple outstanding requests
//!
//! ### Connection State Machine
//!
//! ```mermaid
//! stateDiagram-v2
//!
//!     [*] --> Disconnected
//!
//!     Disconnected --> Connecting
//!     Connecting --> Connected : TCP established
//!     Connecting --> Disconnected : Failure
//!
//!     Connected --> Disconnected : Socket error
//! ```
//!
//! ### Per-Transaction State Machine (TCP)
//!
//! ```mermaid
//! stateDiagram-v2
//! [*] --> Inactive
//!
//! Inactive --> Ready: Connection Success
//!
//! Ready --> Created: Request from application
//! Created --> AssignedTID
//! AssignedTID --> Sent
//! Sent --> Waiting
//!
//! Waiting --> Matched : Response with matching TID
//! Waiting --> Timeout
//!
//! Timeout --> Retry : Retry allowed
//! Retry --> Sent
//! Timeout --> Failed : Retry exhausted
//!
//! Matched --> Parse
//! Parse --> Completed : Valid response
//! Parse --> Failed : Exception or protocol error
//!
//! Completed --> Ready
//! Failed --> Ready
//!
//! state join_state <<join>>
//! Parse --> join_state
//! Ready --> join_state
//! Created --> join_state
//! AssignedTID --> join_state
//! Sent --> join_state
//! Waiting --> join_state
//! Matched --> join_state
//! Timeout --> join_state
//! Retry --> join_state
//! Failed --> join_state
//! Completed--> join_state
//!
//! join_state --> Inactive: Connection failed
//! ```
//!

// use crate::errors::MbusError;
// use bitfields::bitfield;
// use heapless::Vec;

// /// Represents the physical or transport layer connection status.
// enum ConnectionState {
//     /// No active TCP connection.
//     Disconnected,
//     /// In the process of establishing a TCP handshake.
//     Connecting,
//     /// TCP connection is established and ready for data.
//     Connected,
// }

// /// Represents the logical state of a Modbus transaction within the client.
// enum ProtocolState {
//     /// The transaction is not yet initialized or the connection is down.
//     Inactive,
//     /// The connection is established and the transaction is ready to be initiated.
//     Ready,
//     /// An application request has been received to initiate a Modbus transaction.
//     Created,
//     /// A unique Transaction Identifier (TID) has been assigned to the request.
//     AssignedTID,
//     /// The Modbus ADU has been formatted and sent over the TCP socket.
//     Sent,
//     /// The client is waiting for a response from the server with the matching TID.
//     Waiting,
//     /// A response has been received and the TID matches the outstanding request.
//     Matched,
//     /// No response was received within the configured response timeout period.
//     Timeout,
//     /// The transaction is being re-attempted after a timeout or recoverable error.
//     Retry,
//     /// The transaction has failed due to a protocol error, exception, or exhausted retries.
//     Failed,
//     /// The transaction has successfully completed and the response data is valid.
//     Completed,
// }

// /// Represents events that trigger state transitions in the protocol state machine.
// enum ProtocolEvent {
//     /// The underlying TCP connection has been successfully established.
//     ConnectionEstablished,
//     /// The TCP connection attempt failed or an existing connection was dropped.
//     ConnectionFailed,
//     /// The application layer has requested a new Modbus operation.
//     ApplicationRequest,
//     /// A unique Transaction Identifier has been allocated for the current request.
//     TIDAssigned,
//     /// The Application Data Unit has been successfully written to the transport layer.
//     ADUSent,
//     /// A response packet has been received from the network.
//     ResponseReceived { tid: u16 },
//     /// The timer for the current transaction has expired without a valid response.
//     ResponseTimeout,
//     /// The system determined that a retry attempt is valid for the current failure.
//     RetryAllowed,
//     /// No more retries are permitted for the current transaction.
//     RetryExhausted,
//     /// The received response was successfully validated and parsed.
//     ParseSuccess,
//     /// The received response contained a protocol error or failed validation.
//     ParseFailure,
// }

// pub struct StateMachine {
//     connection_state: ConnectionState,
//     protocol_state: ProtocolState,
//     /// The transaction identifier for the current or last transaction.
//     current_transaction_id: u16,
//     /// The Modbus PDU provided by the application, awaiting processing.
//     pending_request_pdu: Option<Vec<u8, 253>>, // Max PDU size is 253 bytes (1 FC + 252 data)
//     /// The fully constructed Modbus TCP ADU, ready to be sent over the network.
//     constructed_adu: Option<Vec<u8, 260>>, // Max ADU size is 260 bytes (7 MBAP + 253 PDU)
//     /// The Unit Identifier to be used in the MBAP header.
//     unit_identifier: u8,
// }

// impl StateMachine {
//     pub fn new() -> Self {
//         Self {
//             connection_state: ConnectionState::Disconnected,
//             protocol_state: ProtocolState::Inactive,
//             current_transaction_id: 0,
//             pending_request_pdu: None,
//             constructed_adu: None,
//             unit_identifier: 0, // Default to 0, can be set by application
//         }
//     }

//     pub fn poll(&mut self) -> Result<(), MbusError> {
//         match self.connection_state {
//             ConnectionState::Disconnected => {
//                 // The protocol state should be Inactive, handled by `connection_failed` event.
//             }
//             ConnectionState::Connecting => {
//                 // The protocol state should be Ready if connection succeeds, handled by `connection_established` event.
//             }
//             _ => {}
//         }

//         match self.protocol_state {
//             ProtocolState::Inactive => {
//                 // Waiting for the underlying TCP connection to be established.
//             }
//             ProtocolState::Ready => {
//                 // The client is ready to accept a new application request via `send_request`.
//             }
//             ProtocolState::Created => {
//                 // An application request has been received.
//                 // Assign a Transaction ID.
//                 self.current_transaction_id = self.generate_transaction_id();
//                 self.protocol_state_change(ProtocolEvent::TIDAssigned);
//             }
//             ProtocolState::AssignedTID => {
//                 // A Transaction ID has been assigned.
//                 // Construct the full Modbus TCP ADU.
//                 let pdu = self.pending_request_pdu.take().ok_or(MbusError::Unexpected)?;
//                 // let adu = self.construct_adu(
//                 //     self.current_transaction_id,
//                 //     self.unit_identifier,
//                 //     pdu.as_slice(),
//                 // )?;
//                 // self.constructed_adu = Some(adu);
//                 // The ADU is now ready to be sent, transition to Sent state.
//                 self.protocol_state_change(ProtocolEvent::ADUSent);
//             }
//             ProtocolState::Sent => {
//                 // The ADU is ready in `self.constructed_adu`.
//                 // An external entity should retrieve it via `take_adu_for_sending()`
//                 // and then call `adu_transmitted()` after physically sending it.
//             }
//             ProtocolState::Waiting => {
//                 // The client is actively waiting for a response from the server.
//                 // This state will be exited by a `ResponseReceived` or `ResponseTimeout` event.
//             }
//             ProtocolState::Matched => {
//                 // A response with a matching TID has been received.
//                 // Now, parse the response PDU.
//                 // This state will be exited by `ParseSuccess` or `ParseFailure` events.
//             }
//             ProtocolState::Timeout => {
//                 // The response timer expired.
//                 // Determine if a retry is allowed.
//                 // This state will be exited by `RetryAllowed` or `RetryExhausted` events.
//             }
//             ProtocolState::Retry => {
//                 // A retry is allowed, so the request needs to be resent.
//                 // The ADU should still be in `self.constructed_adu`.
//                 if self.constructed_adu.is_some() {
//                     // Transition back to Sent state, implying the ADU is being re-sent.
//                     self.protocol_state_change(ProtocolEvent::ADUSent);
//                 } else {
//                     // This indicates an internal logic error if we are in Retry state
//                     // but don't have an ADU to send.
//                     return Err(MbusError::Unexpected);
//                 }
//             }
//             ProtocolState::Failed => {
//                 // Log failure and reset to Ready
//                 self.protocol_state = ProtocolState::Ready;
//             }
//             ProtocolState::Completed => {
//                 // Process valid response and reset to Ready
//                 self.protocol_state = ProtocolState::Ready;
//             }
//         }
//         Ok(())
//     }

//     fn protocol_state_change(&mut self, protocol_event: ProtocolEvent) {
//         match protocol_event {
//             ProtocolEvent::ConnectionEstablished => {
//                 self.protocol_state = ProtocolState::Ready;
//             }
//             ProtocolEvent::ConnectionFailed => {
//                 self.protocol_state = ProtocolState::Inactive;
//             }
//             ProtocolEvent::ApplicationRequest => {
//                 self.protocol_state = ProtocolState::Created;
//             }
//             ProtocolEvent::TIDAssigned => {
//                 self.protocol_state = ProtocolState::AssignedTID;
//             }
//             ProtocolEvent::ADUSent => {
//                 self.protocol_state = ProtocolState::Sent;
//             }
//             ProtocolEvent::ResponseReceived { tid: _ } => {
//                 self.protocol_state = ProtocolState::Matched;
//             }
//             ProtocolEvent::ResponseTimeout => {
//                 self.protocol_state = ProtocolState::Timeout;
//             }
//             ProtocolEvent::RetryAllowed => {
//                 self.protocol_state = ProtocolState::Retry;
//             }
//             ProtocolEvent::RetryExhausted => {
//                 self.protocol_state = ProtocolState::Failed;
//             }
//             ProtocolEvent::ParseSuccess => {
//                 self.protocol_state = ProtocolState::Completed;
//             }
//             ProtocolEvent::ParseFailure => {
//                 self.protocol_state = ProtocolState::Failed;
//             }
//         }
//     }

//     /// Generates a new Transaction Identifier (TID).
//     /// In Modbus TCP, this is a 2-byte field used for matching requests and responses.
//     fn generate_transaction_id(&mut self) -> u16 {
//         let tid = self.current_transaction_id;
//         // Increment and wrap around (0-65535)
//         self.current_transaction_id = tid.wrapping_add(1);
//         tid
//     }

//     /// Resets transaction-specific data after a transaction completes or fails.
//     fn reset_transaction_data(&mut self) {
//         self.pending_request_pdu = None;
//         self.constructed_adu = None;
//         // Optionally reset unit_identifier if it's per-transaction, but usually it's per-connection or per-device.
//         // For now, keep it as is, as it's set by `send_request`.
//     }
// }
