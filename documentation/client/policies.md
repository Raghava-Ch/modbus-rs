# Client Policies

Configurable retry, backoff, jitter, and timeout policies for Modbus clients.

---

## Overview

The client supports configurable resilience policies:

| Policy | Purpose |
|--------|---------|
| **Response Timeout** | How long to wait for a device response once dispatched to the wire |
| **Queue Timeout** | How long a request may wait in the client queue during congestion before failing early |
| **Retry Attempts** | How many times to retry a failed request |
| **Backoff Strategy** | How to space out retry attempts |
| **Jitter Strategy** | Add randomness to prevent thundering herd |

---

## Response Timeout

Time in milliseconds to wait for a response from the server/device once the request frame has been transmitted onto the physical wire.

```rust
use modbus_rs::{ModbusTcpConfig, ModbusSerialConfig};

// TCP
let mut config = ModbusTcpConfig::new("192.168.1.10", 502)?;
config.response_timeout_ms = 1500;  // 1.5 seconds

// Serial
let mut config = ModbusSerialConfig::default();
config.response_timeout_ms = 1000;  // 1 second
```

**Recommendations:**

| Transport | Typical Value | Notes |
|-----------|---------------|-------|
| TCP LAN | 1000-2000 ms | Local network |
| TCP WAN | 3000-5000 ms | Internet/VPN |
| Serial RTU | 500-1000 ms | Depends on baud rate |
| Serial ASCII | 1000-2000 ms | Slower encoding |

On multi-drop RS-485 serial buses, a response timeout on an unresponsive slave does **not** close the transport or disrupt communication with other healthy units; subsequent requests proceed without reconnecting.

---

## Queue Timeout

Maximum time in milliseconds a request is allowed to wait in the client transmission queue before being dispatched onto the wire.

In async client architectures—especially on serial RTU/ASCII lines where concurrency is limited to a single transaction at a time ($N=1$)—bursts of requests can accumulate in the queue. If a request is stuck behind slow transactions or retries and exceeds its queue timeout, it fails immediately with `AsyncError::Timeout` without ever touching the wire, preventing stale commands from being placed onto the bus.

```rust
use std::time::Duration;

// Configure independent queue and response timeouts on an active async client:
client.set_queue_timeout(Duration::from_millis(200));     // Max queue wait: 200ms
client.set_response_timeout(Duration::from_millis(1500)); // Max wire turnaround: 1.5s
```

> [!NOTE]
> **RFC Note**: The `request_timeout_ms` / queue timeout feature is effective in v0.15+. Based on ongoing RFC discussions, its API naming and relationship with transport policies might be revised in a future release. If you consider this queue admission timeout a mandatory feature for your application, please leave a comment on the GitHub RFC discussion.

---

## Retry Attempts

How many times to retry a request after timeout (not counting the initial attempt).

```rust
config.retry_attempts = 3;  // Try up to 4 times total (1 initial + 3 retries)
```

Set to `0` for no retries.

---

## Backoff Strategy

Controls the delay before each retry attempt.

### Immediate

Retry immediately with no delay:

```rust
use modbus_rs::BackoffStrategy;

config.retry_backoff_strategy = BackoffStrategy::Immediate;
```

**Use case:** Fast local network where delays hurt responsiveness.

---

### Fixed Delay

Wait a constant time between retries:

```rust
config.retry_backoff_strategy = BackoffStrategy::Fixed { delay_ms: 200 };
```

Timeline: `[request] --timeout-- [200ms] [retry1] --timeout-- [200ms] [retry2] ...`

**Use case:** Simple, predictable timing.

---

### Linear Backoff

Delay increases linearly each retry:

```rust
config.retry_backoff_strategy = BackoffStrategy::Linear {
    initial_delay_ms: 100,
    increment_ms: 100,
    max_delay_ms: 500,
};
```

Timeline: `[request] --timeout-- [100ms] [retry1] --timeout-- [200ms] [retry2] --timeout-- [300ms] ...`

**Use case:** Gradual back-off without exponential growth.

---

### Exponential Backoff

Delay doubles each retry (capped at max):

```rust
config.retry_backoff_strategy = BackoffStrategy::Exponential {
    base_delay_ms: 100,
    max_delay_ms: 3000,
};
```

Timeline: `[request] --timeout-- [100ms] [retry1] --timeout-- [200ms] [retry2] --timeout-- [400ms] ... [capped at 3000ms]`

**Use case:** Cloud/WAN connections, congestion avoidance.

---

## Jitter Strategy

Adds randomness to retry delays to prevent multiple clients from retrying simultaneously (thundering herd).

### No Jitter

```rust
config.retry_jitter_strategy = JitterStrategy::None;
```

**Use case:** Single client, or predictable timing needed.

---

### Percentage Jitter

Add ±N% randomness to the computed delay:

```rust
use modbus_rs::JitterStrategy;

config.retry_jitter_strategy = JitterStrategy::Percentage { percent: 20 };
```

A 200ms delay becomes 160ms–240ms randomly.

**Use case:** Most applications with multiple clients.

---

### Bounded Millisecond Jitter

Add 0 to N ms randomly:

```rust
config.retry_jitter_strategy = JitterStrategy::BoundedMs { max_jitter_ms: 50 };
```

A 200ms delay becomes 150ms-250ms randomly.

**Use case:** When you want a random offset, not percentage.

---

## Random Function

Jitter requires a random number generator. Provide one via config:

```rust
fn my_random_u32() -> u32 {
    // Your RNG implementation
    rand::random()
}

config.retry_random_fn = Some(my_random_u32);
```

**On std targets:** You can use `rand::random` or similar.

**On embedded:** Use your hardware RNG or PRNG.

If `retry_random_fn` is `None` and jitter is enabled, no jitter is applied.

---

## Complete Example

```rust
use modbus_rs::{
    ModbusTcpConfig, ModbusConfig, BackoffStrategy, JitterStrategy,
};

fn my_random() -> u32 {
    rand::random()
}

let mut config = ModbusTcpConfig::new("192.168.1.10", 502)?;

// Timeout
config.response_timeout_ms = 2000;

// Retry policy
config.retry_attempts = 3;
config.retry_backoff_strategy = BackoffStrategy::Exponential {
    base_delay_ms: 200,
    max_delay_ms: 5000,
};
config.retry_jitter_strategy = JitterStrategy::Percentage { percent: 25 };
config.retry_random_fn = Some(my_random);

let config = ModbusConfig::Tcp(config);
```

---

## Serial Timing Notes

Serial timing behavior is transport-implementation specific. The shared
`ModbusSerialConfig` includes request timeout, retry attempts, backoff, and jitter.
Use `response_timeout_ms` and retry policies first; tune transport-specific behavior
in your serial transport implementation only when needed.

---

## See Also

- [Sync Development](sync.md)
- [Architecture](architecture.md)
- [Examples](examples.md) — `backoff_jitter` examples
