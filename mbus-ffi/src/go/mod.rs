//! Native Go (cgo) bindings for the Modbus client/server/gateway stack.
//!
//! Sibling of [`crate::c`], [`crate::python`] and [`crate::dotnet`].  Reuses
//! the same underlying [`mbus-client-async`](::mbus_client_async),
//! [`mbus-server-async`](::mbus_server_async) and
//! [`mbus-gateway`](::mbus_gateway) crates as every other binding —
//! protocol code is **not** duplicated.
//!
//! ## Design summary
//!
//! * **Go owns the Rust object** via an opaque pointer.  Each constructor
//!   returns a `*mut Handle` produced by [`Box::into_raw`]; the matching
//!   `*_free` function reclaims it via [`Box::from_raw`].  The Go wrapper
//!   stores the pointer in a struct guarded by a `runtime.SetFinalizer`
//!   so the destructor runs even if the user forgets to call `Close()`.
//! * **Heap-allocated**, no static slab pool — Go is always a `std`
//!   environment so the safety motivation that drove the C bindings'
//!   heapless pool does not apply here.
//! * **Shared Tokio runtime** — a single multi-threaded runtime is created
//!   on first use and reused by every handle, mirroring [`crate::dotnet`].
//! * **Blocking call shape** — every `mbus_go_*` request function blocks
//!   the calling thread on `runtime.block_on(async { … })`.  cgo releases
//!   the Go scheduler P during the call so the Go runtime is never
//!   pinned.  The Go wrapper exposes a `context.Context`-based async API
//!   on top.
//! * **Symbol prefix** — every entry point is `mbus_go_*`, distinct from
//!   the `mbus_dn_*` (.NET) and `mbus_*` (C) prefixes so the headers
//!   never clash and the Go binding can evolve independently.
//!
//! ## Module layout
//!
//! | Module | Contents |
//! |---|---|
//! | [`runtime`] | Lazy module-wide [`tokio::runtime::Runtime`]. |
//! | [`status`]  | Status code + helpers shared by every entry point. |
//! | [`client`]  | TCP (and, in a follow-up, Serial) client constructors / request methods. |
//! | [`server`]  | TCP (and, in a follow-up, Serial) server with vtable-based request dispatch. |
//! | [`gateway`] | TCP-to-TCP gateway forwarding with a Vec-backed router. |

pub mod client;
pub mod gateway;
pub mod runtime;
pub mod server;
pub mod status;

pub use status::{MbusGoStatus, mbus_go_status_str};
