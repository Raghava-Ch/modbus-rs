//! Native .NET (C#) P/Invoke bindings for the Modbus client stack.
//!
//! Sibling of [`crate::c`] and [`crate::python`].  Reuses the same underlying
//! [`mbus-client-async`](::mbus_client_async) crate as the Python and pure-Rust
//! consumers, with no shared FFI code.
//!
//! ## Design summary
//!
//! * **.NET owns the Rust object** via an opaque pointer.  Each constructor
//!   returns a `*mut TcpClientHandle` produced by [`Box::into_raw`]; the
//!   matching `*_free` function reclaims it via [`Box::from_raw`].  The C#
//!   wrapper holds the pointer in a [`SafeHandle`] which guarantees the
//!   destructor runs even if the user forgets `Dispose`.
//! * **Heap-allocated**, no static slab pool — `.NET` is always a `std`
//!   environment so the safety motivation that drove the C bindings'
//!   heapless pool does not apply here.
//! * **Shared Tokio runtime** — a single multi-threaded runtime is created
//!   on first use and reused by every handle, so we never spawn redundant
//!   OS threads.  Mirrors the python helper [`crate::python::client`].
//! * **Blocking call shape (Phase 1)** — every `mbus_dn_*` request function
//!   blocks the calling thread on `runtime.block_on(async { … })`.  The C#
//!   wrapper hides this inside `Task.Run` so callers still `await` a
//!   `Task<T>`.  A future revision can swap the implementation to a true
//!   completion-callback model without changing the managed surface.
//!
//! ## Module layout
//!
//! | Module | Contents |
//! |---|---|
//! | [`runtime`] | Lazy module-wide [`tokio::runtime::Runtime`]. |
//! | [`status`]  | Status code + helpers shared by every entry point. |
//! | [`client`]  | Public client constructors / request methods. |

pub mod client;
pub mod runtime;
pub mod status;

pub use status::{MbusDnStatus, mbus_dn_status_str};
