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

include!(concat!(env!("OUT_DIR"), "/generated_server.rs"));
