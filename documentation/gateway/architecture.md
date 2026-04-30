# Gateway Architecture

## Overview

A Modbus gateway bridges two Modbus networks.  The gateway has two roles
simultaneously:

- **Server** to upstream clients (e.g., a SCADA system connecting over TCP).
- **Client** to downstream devices (e.g., RTU slaves on a serial bus).

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         GatewayServices                                  │
│                                                                          │
│  Upstream Transport          Routing           Downstream Channels       │
│  ┌──────────────────┐   ┌──────────────┐   ┌───────────┬──────────┐    │
│  │ StdTcpServer /   │   │ UnitRoute-   │   │ Channel 0 │Channel 1 │    │
│  │ TokioTcpTransport│──▶│ Table        │──▶│ (RTU bus) │(TCP dev.)│    │
│  │ StdRtuTransport  │   │ RangeRoute-  │   │           │          │    │
│  └──────────────────┘   │ Table        │   └───────────┴──────────┘    │
│                          │ Passthrough  │                                │
│                          └──────────────┘                                │
│                                                                          │
│  TxnMap                      EventHandler                                │
│  ┌──────────────────┐   ┌──────────────────┐                            │
│  │ upstream_txn ↔   │   │ on_forward()     │                            │
│  │ internal_txn     │   │ on_routing_miss() │                            │
│  └──────────────────┘   │ on_timeout()     │                            │
│                          └──────────────────┘                            │
└─────────────────────────────────────────────────────────────────────────┘
```

## Request-Response Flow

### Sync (`GatewayServices::poll`)

1. `upstream.recv()` — non-blocking read from the upstream transport.
2. Accumulate bytes in the upstream rx buffer until a complete ADU is detected.
3. `decompile_adu_frame(bytes, upstream_transport_type)` — extract PDU + unit ID + txn ID.
4. `router.route(unit_id)` — find the downstream channel index.
5. If no route: send exception response upstream and return.
6. `TxnMap::allocate(upstream_txn)` — assign an internal txn ID.
7. `compile_adu_frame(internal_txn, unit_id, pdu, downstream_transport_type)` — re-encode.
8. `downstream.send(adu)` — forward to the downstream device.
9. Loop: `downstream.recv()` until a complete response is received.
10. `TxnMap::remove(internal_txn)` — recover the original upstream txn ID.
11. `compile_adu_frame(upstream_txn, unit_id, response_pdu, upstream_transport_type)` — re-encode for upstream.
12. `upstream.send(response_adu)` — return the response.

### Async (`AsyncTcpGatewayServer`)

The async gateway spawns one tokio task per upstream connection.  Each task
performs the same request-response cycle described above, but uses
`AsyncTransport::recv()` / `AsyncTransport::send()`.  The downstream channel
is shared as `Arc<Mutex<T>>` so only one in-flight request hits the downstream
at a time per channel, preventing interleaving.

### Async WebSocket (`AsyncWsGatewayServer`)

`AsyncWsGatewayServer` is structurally identical to `AsyncTcpGatewayServer`
with one difference: the upstream transport is a `WsUpstreamTransport` wrapping
a `tokio-tungstenite` `WebSocketStream<TcpStream>` instead of a raw
`TcpStream`.

Before the session loop starts, the server:

1. Accepts the TCP connection.
2. Checks the session concurrency cap (`WsGatewayConfig::max_sessions`).
3. Performs the HTTP→WebSocket upgrade handshake via
   `tokio_tungstenite::accept_hdr_async`, validating the `Origin` and
   `Sec-WebSocket-Protocol` headers.
4. Wraps the resulting stream in `WsUpstreamTransport`.
5. Optionally wraps that in `IdleTimeoutTransport` when
   `WsGatewayConfig::idle_timeout` is set.
6. Calls the same generic `run_async_session` loop used by
   `AsyncTcpGatewayServer`.

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

Because `WsUpstreamTransport` uses `TRANSPORT_TYPE = CustomTcp`, the session
loop treats the upstream ADU bytes identically to Modbus TCP — MBAP framing
is used throughout.  The WebSocket binary envelope is transparent to all
framing, routing, and transaction-ID remapping logic.

## Transaction-ID Remapping (`TxnMap`)

Upstream TCP clients each maintain their own transaction-ID counter.  If two
clients both send transaction ID `0x0001` before the gateway has responded to
either, the downstream would see two requests with the same ID — a collision.

The `TxnMap` remaps every upstream txn to a monotonically-incrementing
**internal txn ID** before forwarding.  On receiving the downstream response,
it reverse-looks up `(internal_txn → upstream_txn, session_id)` so the correct
upstream client gets the response with the correct txn ID.

For serial downstream channels (which have no txn IDs on the wire) the txn
remapping is effectively a no-op: the gateway still assigns an internal ID, but
the RTU/ASCII framing ignores it.

## Session Pool

The sync `GatewayServices` handles a **single upstream session** at a time.
The async `AsyncTcpGatewayServer` spawns a dedicated task per upstream TCP
connection (each task is an independent session).

## No_std Guarantees

All of the following are `no_std` compatible and use `heapless`:

| Type | Backing storage |
|------|----------------|
| `UnitRouteTable<N>` | `heapless::Vec<UnitRouteEntry, N>` |
| `RangeRouteTable<N>` | `heapless::Vec<UnitRangeRoute, N>` |
| `TxnMap<N>` | `heapless::Vec<TxnEntry, N>` |
| `DownstreamChannel<T>.rxbuf` | `heapless::Vec<u8, MAX_ADU_FRAME_LEN>` |
| `GatewayServices.upstream_rxbuf` | `heapless::Vec<u8, MAX_ADU_FRAME_LEN>` |
| `GatewayServices.downstream` | `heapless::Vec<DownstreamChannel<T>, N_DOWNSTREAM>` |

The `async` feature (and thus the `AsyncTcpGatewayServer`) requires `std` and
Tokio, but the sync core and all routing types are fully `no_std`.
