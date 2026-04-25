#![cfg(target_arch = "wasm32")]

use js_sys::{Function, Object, Reflect};
use mbus_ffi::{WasmSerialServer, WasmSerialServerConfig, WasmTcpGatewayConfig, WasmTcpServer};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

fn js_echo_handler() -> Function {
    Function::new_with_args("req", "return req;")
}

fn install_fake_websocket() {
        let script = r#"
if (!globalThis.__fakeWsInstalledSB) {
    class FakeWebSocket {
        constructor(url) {
            this.url = url;
            this.readyState = 0;
            this.binaryType = 'arraybuffer';
            this.sent = [];
            this.onopen = null;
            this.onclose = null;
            this.onerror = null;
            this.onmessage = null;
            globalThis.__fakeWsRegistrySB.set(url, this);
            const created = globalThis.__fakeWsCreateCountSB.get(url) ?? 0;
            globalThis.__fakeWsCreateCountSB.set(url, created + 1);
        }

        send(data) {
            let bytes;
            if (data instanceof Uint8Array) {
                bytes = data;
            } else if (data instanceof ArrayBuffer) {
                bytes = new Uint8Array(data);
            } else {
                bytes = new Uint8Array(data);
            }
            this.sent.push(new Uint8Array(bytes));
        }

        close() {
            this.readyState = 3;
            if (this.onclose) {
                this.onclose(new Event('close'));
            }
        }
    }

    globalThis.__fakeWsRegistrySB = new Map();
    globalThis.__fakeWsCreateCountSB = new Map();
    globalThis.WebSocket = FakeWebSocket;

    globalThis.__fake_ws_open_sb = (url) => {
        const ws = globalThis.__fakeWsRegistrySB.get(url);
        if (!ws) return false;
        ws.readyState = 1;
        if (ws.onopen) {
            ws.onopen(new Event('open'));
        }
        return true;
    };

    globalThis.__fake_ws_created_count_sb = (url) => {
        return globalThis.__fakeWsCreateCountSB.get(url) ?? 0;
    };

    globalThis.__fakeWsInstalledSB = true;
}
"#;

        let _ = js_sys::eval(script).expect("failed to install fake websocket");
}

fn call_global_1(name: &str, a1: &JsValue) -> JsValue {
        let global = js_sys::global();
        let f = Reflect::get(&global, &JsValue::from_str(name))
                .expect("global function not found")
                .dyn_into::<Function>()
                .expect("global is not function");
        f.call1(&JsValue::NULL, a1).expect("global call failed")
}

fn open_fake_ws(url: &str) {
        let ok = call_global_1("__fake_ws_open_sb", &JsValue::from_str(url))
                .as_bool()
                .unwrap_or(false);
        assert!(ok, "failed to open fake websocket for {url}");
}

#[wasm_bindgen_test]
fn tcp_recv_frame_idle_timeout_is_not_last_error() {
    let server = WasmTcpServer::new(
        WasmTcpGatewayConfig::new("ws://127.0.0.1:8080"),
        js_echo_handler(),
    )
    .expect("tcp server should construct");

    let frame = server
        .recv_frame()
        .expect("idle recv should map timeout to empty frame");

    assert!(frame.is_empty());
    assert_eq!(server.last_error_message(), None);

    let snap = server.status_snapshot();
    assert!(!snap.last_error_present());
    assert_eq!(snap.received_frames(), 0);
}

#[wasm_bindgen_test]
fn serial_recv_frame_idle_timeout_is_not_last_error() {
    let server = WasmSerialServer::new(WasmSerialServerConfig::rtu(), js_echo_handler())
        .expect("serial server should construct");

    let frame = server
        .recv_frame()
        .expect("idle recv should map timeout to empty frame");

    assert!(frame.is_empty());
    assert_eq!(server.last_error_message(), None);

    let snap = server.status_snapshot();
    assert!(!snap.last_error_present());
    assert_eq!(snap.received_frames(), 0);
}

#[wasm_bindgen_test(async)]
async fn tcp_dispatch_when_stopped_sets_last_error_snapshot() {
    let server = WasmTcpServer::new(
        WasmTcpGatewayConfig::new("ws://127.0.0.1:8080"),
        js_echo_handler(),
    )
    .expect("tcp server should construct");

    let req = Object::new();
    let err = server
        .dispatch_request(req.into())
        .await
        .expect_err("dispatch should fail while server is stopped");

    let msg = err.as_string().unwrap_or_default();
    assert!(msg.contains("not running"));
    assert!(server.status_snapshot().last_error_present());
}

#[wasm_bindgen_test(async)]
async fn tcp_start_is_idempotent_does_not_recreate_socket() {
    install_fake_websocket();
    let url = "ws://server-bindings-start-idempotent";
    let server = WasmTcpServer::new(WasmTcpGatewayConfig::new(url), js_echo_handler())
        .expect("tcp server should construct");

    server.start().expect("first start should succeed");
    let created_1 = call_global_1("__fake_ws_created_count_sb", &JsValue::from_str(url))
        .as_f64()
        .unwrap_or(-1.0) as u32;
    assert_eq!(created_1, 1);

    server.start().expect("second start should be no-op and succeed");
    let created_2 = call_global_1("__fake_ws_created_count_sb", &JsValue::from_str(url))
        .as_f64()
        .unwrap_or(-1.0) as u32;
    assert_eq!(created_2, 1);

    open_fake_ws(url);
    assert!(server.transport_connected());
}
