# Gateway Routing

## Overview

The gateway determines which downstream channel to use for an incoming request by consulting a **routing policy** — any type that implements `GatewayRoutingPolicy`.

```rust
use modbus_rs::UnitIdOrSlaveAddr;

pub trait GatewayRoutingPolicy {
    /// Map a unit ID to a downstream channel index.
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize>;

    /// Optionally rewrite the unit ID before building the downstream ADU.
    fn rewrite(&self, unit: UnitIdOrSlaveAddr) -> UnitIdOrSlaveAddr {
        unit
    }
}
```

`Some(channel_idx)` selects the downstream channel index. `None` indicates a routing miss, causing the gateway to send a Modbus exception response (`GatewayPathUnavailable` or `ServerDeviceFailure`) back upstream.

## Built-in Policies

### `UnitRouteTable<N>`

Exact unit-ID → channel mapping. Backed by `heapless::Vec<_, N>` so the capacity is fixed at compile time and no allocator is needed.

```rust
use modbus_rs::gateway::UnitRouteTable;
use modbus_rs::UnitIdOrSlaveAddr;

let mut table: UnitRouteTable<8> = UnitRouteTable::new();
table.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap(); // unit 1 → channel 0
table.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap(); // unit 2 → channel 1
```

Attempting to register the same unit ID twice returns `Err(MbusError::InvalidAddress)`. Exceeding the capacity returns `Err(MbusError::TooManyRequests)`.

### `RangeRouteTable<N>`

Contiguous ranges of unit IDs mapped to one channel. Ranges must be non-overlapping; `add()` validates this at runtime.

```rust
use modbus_rs::gateway::RangeRouteTable;

let mut table: RangeRouteTable<4> = RangeRouteTable::new();
table.add(1, 32, 0).unwrap();   // units 1–32  → channel 0
table.add(33, 64, 1).unwrap();  // units 33–64 → channel 1
```

### `PassthroughRouter`

Forwards every unit to channel 0. Useful for single-downstream setups.

```rust
use modbus_rs::gateway::PassthroughRouter;
let router = PassthroughRouter;
```

### `UnitIdRewriteRouter<R>`

Wraps any inner routing policy and applies an additive offset to the downstream unit ID. Routing decisions still use the **original** unit ID; only the downstream frame gets the rewritten ID. The rewrite is applied **automatically** by `GatewayServices` and `AsyncTcpGatewayServer`.

```rust
use modbus_rs::gateway::{GatewayRoutingPolicy, UnitIdRewriteRouter, PassthroughRouter};
use modbus_rs::UnitIdOrSlaveAddr;

// Route everything to channel 0, shift unit IDs by +100 on the downstream.
let router = UnitIdRewriteRouter::new(PassthroughRouter, 100);

// Verify the rewrite calculation (unit 5 → downstream unit 105):
let downstream_unit = router.rewrite(UnitIdOrSlaveAddr::new(5).unwrap());
assert_eq!(downstream_unit.get(), 105);
```

## Dynamic / Runtime Routing

For std applications that need to update routes at runtime (e.g. an operator dashboard or UI that lets users add/remove slave units without restarting the gateway), you can wrap your routing table in an `Arc<RwLock<R>>` or `Arc<Mutex<R>>`.

A blanket implementation is provided so that `Arc<RwLock<R>>` and `Arc<Mutex<R>>` automatically implement `GatewayRoutingPolicy`:

```rust
use std::sync::{Arc, RwLock};
use modbus_rs::gateway::{GatewayRoutingPolicy, UnitRouteTable};
use modbus_rs::UnitIdOrSlaveAddr;

let table: UnitRouteTable<8> = UnitRouteTable::new();
let shared = Arc::new(RwLock::new(table));

// The gateway task holds a clone of the Arc as its router
let gw_router = shared.clone();

// The UI / configuration task can safely update routes on the fly:
{
    let mut w = shared.write().unwrap();
    w.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
}

// The gateway will automatically pick up the new route on the next request:
assert_eq!(gw_router.route(UnitIdOrSlaveAddr::new(1).unwrap()), Some(0));
```

## Custom Policy

Implement `GatewayRoutingPolicy` directly for your custom types:

```rust
use modbus_rs::gateway::GatewayRoutingPolicy;
use modbus_rs::UnitIdOrSlaveAddr;

struct OddEvenRouter;

impl GatewayRoutingPolicy for OddEvenRouter {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        // Even unit IDs → channel 0, odd → channel 1
        Some((unit.get() % 2) as usize)
    }
}
```

## Combining Policies

Chain policies by composing types:

```rust
use modbus_rs::gateway::{UnitIdRewriteRouter, RangeRouteTable};

let mut base: RangeRouteTable<4> = RangeRouteTable::new();
base.add(1, 32, 0).unwrap();
base.add(33, 64, 1).unwrap();

let router = UnitIdRewriteRouter::new(base, 0); // no rewrite, just wrap
```

