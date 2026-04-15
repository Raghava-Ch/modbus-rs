# Server Configuration Reference

This document covers all configuration options available when constructing a
`ServerServices` instance.

## `ResilienceConfig`

Passed to `ServerServices::new(...)` to control queueing, retry, timeout, and
broadcast policy.  All fields default to **disabled** — a plain
`ResilienceConfig::default()` is safe to use and adds no overhead.

```rust
use mbus_server::{OverflowPolicy, ResilienceConfig, TimeoutConfig};

let resilience = ResilienceConfig {
    timeouts: TimeoutConfig {
        app_callback_ms: 20,       // warn if callback takes > 20 ms
        send_ms: 50,               // warn if send takes > 50 ms
        response_retry_interval_ms: 100, // minimum delay between retry attempts
        request_deadline_ms: 500,  // drop requests queued > 500 ms
        strict_mode: true,         // send GatewayPathUnavailable before drop
        overflow_policy: OverflowPolicy::RejectRequest,
    },
    clock_fn: Some(my_monotonic_ms),
    max_send_retries: 3,
    enable_priority_queue: true,
    enable_broadcast_writes: false,
};
```

### `ResilienceConfig` fields

| Field                    | Type              | Default         | Description                                                         |
|--------------------------|-------------------|-----------------|---------------------------------------------------------------------|
| `timeouts`               | `TimeoutConfig`   | see below       | Per-phase threshold settings                                        |
| `clock_fn`               | `Option<ClockFn>` | `None`          | Monotonic millisecond clock; required for all threshold enforcement |
| `max_send_retries`       | `u8`              | `3`             | Retry budget per queued response (0 = disable retries)              |
| `enable_priority_queue`  | `bool`            | `false`         | Queue incoming requests and dispatch in priority order              |
| `enable_broadcast_writes`| `bool`            | `false`         | Enable Serial broadcast write processing (no response sent)         |

### `TimeoutConfig` fields

All thresholds are in milliseconds; `0` disables the check.  A `clock_fn` must
also be configured for any threshold to take effect.

| Field                        | Default          | Description                                                           |
|------------------------------|------------------|-----------------------------------------------------------------------|
| `app_callback_ms`            | `0` (off)        | Warn (debug log) when a callback exceeds this threshold               |
| `send_ms`                    | `0` (off)        | Warn when a `transport.send()` call exceeds this threshold            |
| `response_retry_interval_ms` | `0` (off)        | Minimum delay between retry attempts for a queued failed response     |
| `request_deadline_ms`        | `0` (off)        | Discard requests queued longer than this (see `strict_mode`)          |
| `strict_mode`                | `false`          | When `true`, send `GatewayPathUnavailable` before discarding a stale request |
| `overflow_policy`            | `DropResponse`   | How to handle response queue overflow (see below)                     |

### `OverflowPolicy`

| Variant         | Behavior                                                                                             |
|-----------------|------------------------------------------------------------------------------------------------------|
| `DropResponse`  | (default) When the retry queue is full, silently drop the failed response (possible client retry)    |
| `RejectRequest` | When the queue exceeds 80% utilization, reject new addressed unicast requests with an exception response |

`RejectRequest` avoids applying state changes that cannot be confirmed.  The
exception response maps to `ServerDeviceFailure` (via the `TooManyRequests`
internal error).  Broadcast and misaddressed frames are always silently discarded
regardless of policy.

## `ClockFn`

```rust
pub type ClockFn = fn() -> u64;
```

Returns a monotonic timestamp in **milliseconds**.  Example with `std`:

```rust
fn my_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```

On bare-metal, return a tick counter scaled to milliseconds.

## Priority Queue

When `enable_priority_queue = true`, incoming requests are buffered and
dispatched in this priority order (highest first):

| Priority      | Function codes                                   |
|---------------|--------------------------------------------------|
| `Maintenance` | FC08, FC0B, FC0C, FC11, FC2B                     |
| `Write`       | FC05, FC06, FC0F, FC10, FC16, FC15               |
| `Read`        | FC01, FC02, FC03, FC04, FC18, FC14               |
| `Other`       | everything else                                  |

When `enable_priority_queue = false` (default), requests are dispatched
immediately on receipt for minimum latency.

## Broadcast Writes

Broadcast writes use Modbus slave address `0` and are Serial-only.

```rust
let resilience = ResilienceConfig {
    enable_broadcast_writes: true,
    ..Default::default()
};
```

Supported FCs: 0x05, 0x06, 0x0F, 0x10, 0x15.

- The callback receives `unit_id_or_slave_addr.is_broadcast() == true`.
- No response is ever sent.
- TCP broadcast (unit id `0` over TCP) is always silently discarded.
- Back-pressure (`OverflowPolicy::RejectRequest`) does **not** apply to broadcast
  writes — they bypass the response queue entirely.

See `examples/broadcast_writes.rs`.

## `ServerServices` Queue Depth API

Call these methods on a live `ServerServices` instance to observe runtime
back-pressure:

| Method                       | Returns   | Description                                    |
|------------------------------|-----------|------------------------------------------------|
| `pending_request_count()`    | `usize`   | Requests waiting in the priority queue         |
| `pending_response_count()`   | `usize`   | Responses waiting in the retry queue           |
| `dropped_response_count()`   | `u32`     | Cumulative responses dropped due to overflow   |
| `rejected_request_count()`   | `u32`     | Cumulative requests rejected by `RejectRequest`|
| `peak_response_queue_size()` | `usize`   | High-water mark of the response retry queue    |

## Unit ID / Slave Address Filtering

`UnitIdOrSlaveAddr` is the discriminated address type passed to every callback.

```rust
match uid {
    UnitIdOrSlaveAddr::UnitId(id) => { /* TCP: 1–247 */ }
    UnitIdOrSlaveAddr::SlaveAddr(sa) => { /* Serial: 1–247 */ }
    UnitIdOrSlaveAddr::Broadcast => { /* Serial broadcast: addr 0 */ }
}

if uid.is_broadcast() { /* suppress any response */ }
```

The server stack performs its own address filtering.  Callbacks are only invoked
for requests addressed to the server's own address (or broadcasts when enabled).

## Feature Flag Summary

```toml
[dependencies]
mbus-server = { version = "0.2.0", features = [
    # Data model groups (all enabled by default):
    "coils",
    "discrete-inputs",
    "holding-registers",
    "input-registers",
    "fifo",
    "file-record",
    "diagnostics",
    # Optional extensions:
    "diagnostics-stats",    # automatic FC08 counter tracking
    "traffic",              # TX/RX callback hooks (TrafficNotifier)
    "logging",              # log facade instrumentation
    "serial-ascii",         # ASCII-mode ADU buffer sizing
] }
```

To build a minimal server with only holding-register support:

```toml
mbus-server = { version = "0.2.0", default-features = false, features = ["holding-registers"] }
```

See [feature_flags.md](feature_flags.md) for the cross-crate feature
propagation map.
