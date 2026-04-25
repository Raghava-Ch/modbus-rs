//! JS bridge layer for server-side WASM bindings.
//!
//! Converts Rust-side request dispatch into a JS callback contract:
//! - callback receives one request object (opaque `JsValue`)
//! - callback may return either a direct value or a Promise
//! - Promise results are awaited and returned to Rust callers

use js_sys::{Function, Promise};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

/// Thin wrapper around a JS request handler callback.
#[derive(Clone)]
pub struct JsServerHandler {
    on_request: Function,
}

impl JsServerHandler {
    /// Create a new bridge from a JS function.
    pub fn new(on_request: Function) -> Self {
        Self { on_request }
    }

    /// Dispatch one request payload into the JS callback.
    ///
    /// If the callback returns a Promise, this method awaits it.
    pub async fn dispatch(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let out = self.on_request.call1(&JsValue::NULL, &request)?;
        if out.is_instance_of::<Promise>() {
            let p: Promise = out
                .dyn_into()
                .map_err(|_| JsValue::from_str("on_request returned non-promise value shape"))?;
            JsFuture::from(p).await
        } else {
            Ok(out)
        }
    }
}
