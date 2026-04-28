# Gateway Routing

## Overview

The gateway determines which downstream channel to use for an incoming request
by consulting a **routing policy** — any type that implements
`GatewayRoutingPolicy`.

```rust
pub trait GatewayRoutingPolicy {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize>;
}
```

`Some(channel_idx)` selects the downstream channel.  `None` causes the gateway
to send a Modbus exception response (`ServerDeviceFailure`) back upstream.

## Built-in Policies

### `UnitRouteTable<N>`

Exact unit-ID → channel mapping.  Backed by `heapless::Vec<_, N>` so the
capacity is fixed at compile time and no allocator is needed.

```rust
use mbus_gateway::UnitRouteTable;
use mbus_core::transport::UnitIdOrSlaveAddr;

let mut table: UnitRouteTable<8> = UnitRouteTable::new();
table.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap(); // unit 1 → channel 0
table.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap(); // unit 2 → channel 1
```

Attempting to register the same unit ID twice returns
`Err(MbusError::InvalidAddress)`.  Exceeding the capacity returns
`Err(MbusError::TooManyRequests)`.

### `RangeRouteTable<N>`

Contiguous ranges of unit IDs mapped to one channel.  Ranges must be
non-overlapping; `add()` validates this at runtime.

```rust
use mbus_gateway::RangeRouteTable;

let mut table: RangeRouteTable<4> = RangeRouteTable::new();
table.add(1, 32, 0).unwrap();   // units 1–32  → channel 0
table.add(33, 64, 1).unwrap();  // units 33–64 → channel 1
```

### `PassthroughRouter`

Forwards every unit to channel 0.  Useful for single-downstream setups.

```rust
use mbus_gateway::PassthroughRouter;
let router = PassthroughRouter;
```

### `UnitIdRewriteRouter<R>`

Wraps any inner routing policy and also applies an additive offset to the
downstream unit ID.  Routing decisions still use the **original** unit ID;
only the downstream frame gets the rewritten ID.

```rust
use mbus_gateway::{GatewayRoutingPolicy, UnitIdRewriteRouter, PassthroughRouter};
use mbus_core::transport::UnitIdOrSlaveAddr;

// Route everything to channel 0, shift unit IDs by +100 on the downstream.
let router = UnitIdRewriteRouter::new(PassthroughRouter, 100);

// Use rewrite() to compute the downstream unit ID before calling compile_adu_frame:
let downstream_unit = router.rewrite(UnitIdOrSlaveAddr::new(5).unwrap());
assert_eq!(downstream_unit.get(), 105);
```

> **Note:** `GatewayServices` does **not** automatically apply the rewrite
> offset — you need to either integrate `UnitIdRewriteRouter` at the transport
> level or apply `rewrite()` before calling `compile_adu_frame`.  A future
> release will integrate this transparently.

## Custom Policy

Implement `GatewayRoutingPolicy` directly:

```rust
use mbus_gateway::GatewayRoutingPolicy;
use mbus_core::transport::UnitIdOrSlaveAddr;

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
use mbus_gateway::{UnitIdRewriteRouter, RangeRouteTable};

let mut base: RangeRouteTable<4> = RangeRouteTable::new();
base.add(1, 32, 0).unwrap();
base.add(33, 64, 1).unwrap();

let router = UnitIdRewriteRouter::new(base, 0); // no rewrite, just wrap
```
