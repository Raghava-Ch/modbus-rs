# Modbus-rs Architecture Overview

This document provides a high-level overview of the `modbus-rs` library's architecture, focusing on its core components and their interactions. The library is designed for `no_std` compatibility, offering a robust and flexible Modbus client implementation suitable for embedded systems.

## Core Components

The `modbus-rs` ecosystem is composed of several crates, each with a distinct responsibility:

-   **`mbus-core`**: Provides fundamental Modbus data structures, error types, and the `Transport` trait for abstracting communication layers.
-   **`modbus-client`**: Implements the Modbus client state machine and services for handling various Modbus function codes.
-   **`mbus-tcp`**: (Implied, based on `ModbusTcpConfig` in `mbus-core`) Likely provides a standard TCP transport implementation.
-   **`mbus-serial`**: (Implied, based on `ModbusSerialConfig` in `mbus-core`) Likely provides standard Serial (RTU/ASCII) transport implementations.

## Modbus TCP Client State Machine

The client activity flow for Modbus TCP is defined by a deterministic state machine, adhering to "MODBUS Messaging on TCP/IP Implementation Guide V1.0b" Figure 11: *MODBUS Client Activity Diagram*. This state machine describes the lifecycle of a single transaction in a sequential (non-concurrent) Modbus TCP client.

### Design Goals

-   **Deterministic**: Predictable behavior for reliable operation.
-   **Transport-agnostic**: Decoupled from the underlying communication medium.
-   **Bare-metal friendly**: Designed for resource-constrained environments.
-   **Suitable for `poll()`-driven execution**: Enables non-blocking operation.
-   **Easily extensible for retry logic**: Facilitates robust communication.

### Overview

The client operates in a loop, managing the following steps for each transaction:

1.  Waiting for user request.
2.  Building the MODBUS request.
3.  Sending the request to the TCP management layer.
4.  Waiting for a response (with a configurable timeout).
5.  Processing the confirmation.
6.  Returning the result to the user.

Retry logic is applied if a timeout occurs.

### State Diagram

```mermaid
stateDiagram-v2

    [*] --> Idle

    Idle --> Wait

    Wait --> BuildRequest : RequestFromUser

    BuildRequest --> SendRequest

    SendRequest --> WaitingForResponse : SendOK
    SendRequest --> SendNegativeConfirmation : SendNotOK

    WaitingForResponse --> ProcessConfirmation : ResponseReceived
    WaitingForResponse --> FindPendingTransaction : WaitingResponseTimerExpired

    ProcessConfirmation --> SendPositiveConfirmation : ConfirmationOK
    ProcessConfirmation --> SendNegativeConfirmation : ConfirmationError

    FindPendingTransaction --> SendRequest : RetriesNotReached
    FindPendingTransaction --> SendNegativeConfirmation : RetriesReached

    SendPositiveConfirmation --> Wait
    SendNegativeConfirmation --> Wait
```

### State Descriptions

-   **Idle**: The initial state, indicating no active transaction.
-   **Wait**: The client is awaiting a user request, a TCP response, or a response timeout.
-   **BuildRequest**: The Modbus PDU is constructed and wrapped into an ADU.
-   **SendRequest**: The constructed request is sent to the TCP transport layer.
-   **WaitingForResponse**: A response timer is active, and the client is awaiting confirmation from the server.
-   **ProcessConfirmation**: The received response is parsed and validated.
-   **FindPendingTransaction**: Triggered by a response timeout, this state determines if a retry is allowed based on the retry logic.
-   **SendPositiveConfirmation**: A successful result is sent to the user application.
-   **SendNegativeConfirmation**: A failure result is sent to the user application.

## Transport Layer (`mbus-core/src/transport/mod.rs`)

The transport layer provides abstractions for transmitting Modbus Application Data Units (ADUs) over various physical and logical mediums.

### Core Concepts

-   **`Transport` Trait**: A unified interface that abstracts the underlying communication (TCP, Serial, or Mock) from the high-level protocol logic. Implementors are responsible for connection management, framing, sending, and receiving data.
-   **`ModbusConfig`**: A comprehensive configuration enum for setting up TCP/IP or Serial (RTU/ASCII) parameters.
-   **`UnitIdOrSlaveAddr`**: A type-safe wrapper for Modbus addresses, ensuring validity and explicitly handling broadcast addresses.

### Design Goals

-   **`no_std` Compatibility**: Utilizes `heapless` data structures and `core` traits for bare-metal embedded systems.
-   **Non-blocking I/O**: The `Transport::recv` interface is designed to be polled, allowing the client to remain responsive without requiring an OS-level thread.
-   **Extensibility**: Users can implement the `Transport` trait for custom hardware.

### Error Handling

Errors are categorized into `TransportError`, which can be seamlessly converted into the top-level `MbusError` used throughout the crate.

## Client Services (`modbus-client/src/lib.rs`)

The `modbus-client` crate provides the `ClientServices` struct, which acts as the central coordinator for Modbus transactions.

### Responsibilities

-   **Request Lifecycle Management**: Manages ADU construction, transmission, response tracking, timeouts, and retries.
-   **Pipelining**: Supports multiple concurrent outstanding requests (configurable via const generics).
-   **Reliability**: Built-in support for automatic retries and configurable response timeouts.
-   **Memory Safety**: Employs `heapless` for all internal buffering, eliminating dynamic allocation.
-   **Protocol Coverage**: Implements standard function codes for various Modbus operations.
-   **`App` Traits**: Defines traits (e.g., `CoilResponse`, `RegisterResponse`) that users implement to receive asynchronous callbacks when a response is parsed.

The `ClientServices` orchestrates the interaction between the state machine, the transport layer, and the application-specific response handlers.