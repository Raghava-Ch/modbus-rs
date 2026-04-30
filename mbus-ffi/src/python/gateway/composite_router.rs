//! A simple `Vec`-backed router used by the Python bindings.
//!
//! Heap-allocated, so it sidesteps the `const N: usize` parameter of the
//! `no_std` `UnitRouteTable`/`RangeRouteTable`. Routes are checked in
//! insertion order; the first match wins.

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::GatewayRoutingPolicy;

#[derive(Debug, Clone)]
enum Route {
    Unit { unit: u8, channel: usize },
    Range { min: u8, max: u8, channel: usize },
}

#[derive(Debug, Default, Clone)]
pub struct PyRouter {
    routes: Vec<Route>,
}

impl PyRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_unit(&mut self, unit: u8, channel: usize) {
        self.routes.push(Route::Unit { unit, channel });
    }

    pub fn add_range(&mut self, min: u8, max: u8, channel: usize) {
        self.routes.push(Route::Range { min, max, channel });
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl GatewayRoutingPolicy for PyRouter {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        let v = unit.get();
        for r in &self.routes {
            match *r {
                Route::Unit { unit: u, channel } if u == v => return Some(channel),
                Route::Range { min, max, channel } if min <= v && v <= max => {
                    return Some(channel);
                }
                _ => {}
            }
        }
        None
    }
}
