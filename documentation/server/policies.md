# Server Policies

Configurable timeout, retry, and overflow policies for Modbus servers.

---

## Overview

The server supports configurable resilience policies via `ResilienceConfig`:

| Policy | Purpose |
|--------|---------|
| **App Callback Timeout** | Warn if callbacks take too long |
| **Send Timeout** | Warn if transport send takes too long |
| **Request Deadline** | Discard stale queued requests |
| **Retry Budget** | Number of send retry attempts |
| **Overflow Policy** | How to handle queue overflow |
| **Priority Queue** | Dispatch by function code priority |
| **Broadcast Writes** | Enable serial broadcast processing |

---

## `ResilienceConfig`

```rust
use mbus_server::{ResilienceConfig, TimeoutConfig, OverflowPolicy};

fn my_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

let resilience = ResilienceConfig {
    timeouts: TimeoutConfig {
        app_callback_ms: 20,           // Warn if callback > 20 ms
        send_ms: 50,                   // Warn if send > 50 ms
        response_retry_interval_ms: 100, // Min delay between retries
        request_deadline_ms: 500,      // Drop requests queued > 500 ms
        strict_mode: true,             // Send exception before drop
        overflow_policy: OverflowPolicy::RejectRequest,
    },
    clock_fn: Some(my_clock_ms),
    max_send_retries: 3,
    enable_priority_queue: true,
    enable_broadcast_writes: false,
};
```

---

## Field Reference

### `ResilienceConfig` Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `timeouts` | `TimeoutConfig` | see below | Per-phase threshold settings |
| `clock_fn` | `Option<ClockFn>` | `None` | Monotonic millisecond clock |
| `max_send_retries` | `u8` | `3` | Retry budget per failed response |
| `enable_priority_queue` | `bool` | `false` | Queue and prioritize requests |
| `enable_broadcast_writes` | `bool` | `false` | Process serial broadcast writes |

### `TimeoutConfig` Fields

| Field | Default | Description |
|-------|---------|-------------|
| `app_callback_ms` | `0` (off) | Warn when callback exceeds this |
| `send_ms` | `0` (off) | Warn when `transport.send()` exceeds this |
| `response_retry_interval_ms` | `0` (off) | Minimum delay between retry attempts |
| `request_deadline_ms` | `0` (off) | Discard requests queued longer than this |
| `strict_mode` | `false` | Send exception before discarding stale request |
| `overflow_policy` | `DropResponse` | How to handle retry queue overflow |

**Note:** A `clock_fn` must be provided for any timeout to take effect.

---

## Clock Function

```rust
pub type ClockFn = fn() -> u64;
```

Returns a monotonic timestamp in **milliseconds**.

### std Example

```rust
fn my_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```

### Embedded Example

```rust
fn my_clock_ms() -> u64 {
    // Return hardware timer tick scaled to milliseconds
    hal::timer::millis()
}
```

---

## Overflow Policy

When the retry queue is full:

### `DropResponse` (Default)

Silently drop the failed response. The client will timeout and may retry.

```rust
overflow_policy: OverflowPolicy::DropResponse,
```

### `RejectRequest`

When queue exceeds 80% utilization, reject new unicast requests with an exception.

```rust
overflow_policy: OverflowPolicy::RejectRequest,
```

- Avoids applying state changes that cannot be confirmed
- Exception response: `ServerDeviceFailure`
- Broadcast frames are always silently discarded

---

## Priority Queue

When `enable_priority_queue = true`, incoming requests are buffered and dispatched in priority order (highest first):

| Priority | Function Codes |
|----------|----------------|
| **Maintenance** | FC08, FC0B, FC0C, FC11, FC2B |
| **Write** | FC05, FC06, FC0F, FC10, FC16, FC15 |
| **Read** | FC01, FC02, FC03, FC04, FC18, FC14 |
| **Other** | everything else |

When disabled (default), requests are dispatched immediately for minimum latency.

---

## Broadcast Writes

When `enable_broadcast_writes = true` (Serial only):

- FC05, FC06, FC0F, FC10, FC15 with slave address `0` are processed
- The callback receives `uid.is_broadcast() == true`
- **No response is sent** for broadcast writes
- TCP broadcast frames (unit ID `0`) are always discarded

```rust
fn write_single_coil_request(
    &mut self,
    _txn_id: u16,
    uid: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
) -> Result<(), MbusError> {
    if uid.is_broadcast() {
        // Apply write, no response will be sent
    }
    self.coils[address as usize] = value;
    Ok(())
}
```

---

## Complete Example

```rust
use mbus_server::{ServerServices, ResilienceConfig, TimeoutConfig, OverflowPolicy};

fn my_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

let resilience = ResilienceConfig {
    timeouts: TimeoutConfig {
        app_callback_ms: 20,
        send_ms: 50,
        response_retry_interval_ms: 100,
        request_deadline_ms: 500,
        strict_mode: true,
        overflow_policy: OverflowPolicy::RejectRequest,
    },
    clock_fn: Some(my_clock),
    max_send_retries: 3,
    enable_priority_queue: true,
    enable_broadcast_writes: false,
};

let mut server = ServerServices::<_, _, 8>::with_resilience(
    transport,
    app,
    config,
    resilience,
)?;
```

---

## See Also

- [Building Applications](building_applications.md)
- [Architecture](architecture.md)
- [Function Codes](function_codes.md)
