# Gateway Architecture

## Overview

A Modbus gateway bridges two Modbus networks. The gateway has two roles simultaneously:

- **Server** to upstream clients (e.g., a SCADA system connecting over TCP or serial master).
- **Client** to downstream devices (e.g., RTU slaves on a serial bus or TCP servers).

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         GatewayServices                                 │
│                                                                         │
│  Upstream Transports        Routing           Downstream Channels       │
│  ┌──────────────────┐   ┌──────────────┐   ┌───────────┬──────────┐     │
│  │ StdTcpServer /   │   │ UnitRoute-   │   │ Channel 0 │Channel 1 │     │
│  │ UpstreamChannel  │──▶│ Table        │──▶│ (RTU bus) │(TCP dev.)│     │
│  │ (up to N_UP)     │   │ RangeRoute-  │   │           │          │     │
│  └──────────────────┘   │ Table        │   └───────────┴──────────┘     │
│                         │ Passthrough  │                                │
│                         └──────────────┘                                │
│                                                                         │
│  TxnMap & PendingQueue       EventHandler                               │
│  ┌──────────────────┐   ┌─────────────────────┐                         │
│  │ upstream_txn ↔   │   │ on_forward()        │                         │
│  │ internal_txn     │   │ on_routing_miss()   │                         │
│  └──────────────────┘   │ on_timeout()        │                         │
│                         └─────────────────────┘                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Request-Response Flow

### Sync (`GatewayServices::poll`)

The synchronous gateway is completely non-blocking and poll-driven. Every call to `poll(now_ms)` drives the internal state machines of all channels through three distinct phases:

1. **Phase 1: Drain Downstream Channels**
   - For each downstream channel in `ChannelState::AwaitingResponse`:
     - Checks if the response deadline (`now_ms >= deadline_ms`) has passed. If so, it times out the request, frees the transaction in `TxnMap`, sends an exception response (`GatewayPathUnavailable`) back upstream, resets the channel to `Idle`, and attempts to service the next queued pending request.
     - Otherwise, it performs a non-blocking `downstream.transport.recv()`. If new bytes are ready, they are appended to the `DownstreamChannel.rxbuf`.
     - Once a complete downstream ADU frame is fully accumulated, it decompiles the frame, looks up the original upstream transaction and session ID from `TxnMap`, compiles the corresponding upstream ADU response, and transmits it back via `upstream.transport.send()`. The channel returns to `Idle` and processes any queued pending requests.

2. **Phase 2: Drain Upstream Channels**
   - For each active upstream channel:
     - Performs a non-blocking `upstream.transport.recv()`. Received bytes are accumulated in `UpstreamChannel.rxbuf`.
     - When a complete upstream ADU frame is accumulated, it decompiles the frame to extract the PDU, transaction ID, and unit ID.
     - Looks up the downstream channel index using the routing policy (`router.route(unit_id)`). If there is a routing miss, it immediately transmits a Modbus exception response (`GatewayPathUnavailable`) upstream.
     - Rewrites the unit ID if specified by the routing policy (`router.rewrite(unit_id)`).
     - If the target downstream channel is `Idle` and the pending queue is empty, it immediately allocates an internal transaction ID in `TxnMap`, compiles the downstream ADU, sends it via `downstream.transport.send()`, and puts the channel in `AwaitingResponse` state.
     - If the downstream channel is busy, it attempts to queue the request in the `PendingQueue` (or sends a `GatewayTargetDeviceFailedToRespond` exception upstream if the queue is full or disabled).

3. **Phase 3: Session Cleanup**
   - Drains and tears down any disconnected upstream sessions, cleans up their associated transactions in `TxnMap`, and fires the appropriate `on_upstream_disconnect` events.

### Async (`AsyncTcpGatewayServer` / `AsyncSerialGatewayServer`)

The async gateway leverages the Tokio async runtime and the `AsyncTransport` trait:

- It spawns one dedicated async task (`run_async_session`) per accepted upstream connection.
- Downstream channels are represented as `Arc<Mutex<DS>>` and shared concurrently across all upstream tasks.
- When a task receives a complete upstream ADU frame:
  - It resolves the downstream channel index via the `GatewayRoutingPolicy`.
  - It acquires a lock on the target downstream channel's `Mutex` for the **entire duration** of the downstream transaction (sending the request and waiting for the complete response frame with a timeout).
  - This whole-transaction mutex lock guarantees that no two upstream requests can interleave or collide on the same downstream channel, eliminating the need for a global `TxnMap` or request queuing in the async runtime. It simply uses a per-session monotonic transaction counter to tag downstream requests.
  - The response is then compiled and returned back to the upstream client before releasing the mutex.

### Async WebSocket (`AsyncWsGatewayServer`)

`AsyncWsGatewayServer` is structurally identical to `AsyncTcpGatewayServer` with one difference: the upstream transport is a `WsUpstreamTransport` wrapping a `tokio-tungstenite` `WebSocketStream<TcpStream>` instead of a raw `TcpStream`.

Before the session loop starts, the server:

1. Accepts the TCP connection.
2. Checks the session concurrency cap (`WsGatewayConfig::max_sessions`).
3. Performs the HTTP→WebSocket upgrade handshake via `tokio_tungstenite::accept_hdr_async`, validating the `Origin` and `Sec-WebSocket-Protocol` headers.
4. Wraps the resulting stream in `WsUpstreamTransport`.
5. Optionally wraps that in `IdleTimeoutTransport` when `WsGatewayConfig::idle_timeout` is set.
6. Calls the same generic `run_async_session` loop used by the TCP gateway.

```
Browser WASM              AsyncWsGatewayServer           Downstream
─────────────             ────────────────────────       ──────────────
WasmModbusClient  ──WS──►  WsUpstreamTransport
                            (TRANSPORT_TYPE=CustomTcp)
                                   │
                                   ▼
                             run_async_session()    ──────► Arc<Mutex<DS>>
                            (same as TCP gateway)           (any AsyncTransport)
```

Because `WsUpstreamTransport` uses `TRANSPORT_TYPE = CustomTcp`, the session loop treats the upstream ADU bytes identically to Modbus TCP — MBAP framing is used throughout. The WebSocket binary envelope is transparent to all framing, routing, and transaction-ID remapping logic.

## Transaction-ID Remapping (`TxnMap`)

Upstream TCP clients each maintain their own transaction-ID counter. If two clients both send transaction ID `0x0001` before the gateway has responded to either, the downstream would see two requests with the same ID — a collision.

In the sync `GatewayServices`, the `TxnMap` remaps every upstream txn to a unique, internally allocated **internal txn ID** before forwarding. On receiving the downstream response, it reverse-looks up `(internal_txn → upstream_txn, session_id)` so the correct upstream client gets the response with the correct txn ID.

For serial downstream channels (which have no txn IDs on the wire), the txn remapping is still performed internally for state tracking, but the actual RTU/ASCII framing ignores the ID.

In the async gateway, the exclusive downstream mutex lock prevents overlapping requests per channel entirely, meaning no collision is possible.

## Session Pool and Multiplexing

- **Sync (`GatewayServices`)**: Fully multiplexes up to `N_UPSTREAM` concurrent upstream sessions and multiple downstream channels in a single thread without blocking. It maps concurrent requests to downstreams using `TxnMap` and queues requests using a `PendingQueue` if a downstream is currently busy.
- **Async (`AsyncTcpGatewayServer` / `AsyncSerialGatewayServer` / `AsyncWsGatewayServer`)**: Spawns a dedicated Tokio task per upstream session. Mutex locks are used on the downstream channels to synchronize access across concurrent sessions.

## No_std Guarantees

All of the following are `no_std` compatible and use `heapless`:

| Type | Backing storage |
|------|----------------|
| `UnitRouteTable<N>` | `heapless::Vec<UnitRouteEntry, N>` |
| `RangeRouteTable<N>` | `heapless::Vec<UnitRangeRoute, N>` |
| `TxnMap<N>` | `heapless::Vec<TxnEntry, N>` |
| `PendingQueue<N>` | `heapless::Vec<PendingRequest, N>` |
| `UpstreamChannel<T>.rxbuf` | `heapless::Vec<u8, MAX_ADU_FRAME_LEN>` |
| `DownstreamChannel<T>.rxbuf` | `heapless::Vec<u8, MAX_ADU_FRAME_LEN>` |
| `GatewayServices.upstreams` | `heapless::Vec<UpstreamChannel<T>, N_UPSTREAM>` |
| `GatewayServices.downstreams` | `heapless::Vec<DownstreamChannel<T>, N_DOWNSTREAM>` |

The `async` feature (and thus all async server runtimes) requires `std` and Tokio, but the sync core and all routing types are fully `no_std`.

