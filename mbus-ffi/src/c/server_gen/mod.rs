//! Generated C server dispatcher integration.
//!
//! This module is generated from a user-supplied YAML config at build time
//! via `build.rs` + `mbus-codegen`. The source is written to `$OUT_DIR/generated_server.rs`
//! and included here so it is never tracked in version control.
//!
//! To build with the `c-server` feature, set `MBUS_SERVER_APP_CONFIG`:
//!   ```bash
//!   MBUS_SERVER_APP_CONFIG=/path/to/server_app.yaml cargo build -p mbus-ffi --features c-server
//!   ```

// Generated code contains raw-pointer extern signatures and safety docs that
// are produced by codegen; lint at the generator level rather than here.
mod generated {
	#![allow(clippy::missing_safety_doc, clippy::not_unsafe_ptr_arg_deref)]
	include!(concat!(env!("OUT_DIR"), "/generated_server.rs"));
}

pub use generated::*;
