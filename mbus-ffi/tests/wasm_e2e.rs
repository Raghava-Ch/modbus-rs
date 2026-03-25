#![cfg(target_arch = "wasm32")]

use js_sys::{Function, Reflect, Uint8Array, Uint16Array};
use mbus_ffi::WasmModbusClient;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn install_fake_websocket() {
    // Installs a deterministic in-browser fake WebSocket used by all tests.
    let script = r#"
if (!globalThis.__fakeWsInstalled) {
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
      globalThis.__fakeWsRegistry.set(url, this);
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

  globalThis.__fakeWsRegistry = new Map();
  globalThis.WebSocket = FakeWebSocket;

  globalThis.__fake_ws_open = (url) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    ws.readyState = 1;
    if (ws.onopen) {
      ws.onopen(new Event('open'));
    }
    return true;
  };

  globalThis.__fake_ws_close = (url) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    ws.readyState = 3;
    if (ws.onclose) {
      ws.onclose(new Event('close'));
    }
    return true;
  };

  globalThis.__fake_ws_emit = (url, bytes) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    const payload = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const ab = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
    if (ws.onmessage) {
      ws.onmessage(new MessageEvent('message', { data: ab }));
      return true;
    }
    return false;
  };

  globalThis.__fake_ws_get_sent = (url, idx) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return null;
    const i = idx ?? 0;
    return ws.sent[i] ?? null;
  };

  globalThis.__fake_ws_clear = (url) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    ws.sent = [];
    return true;
  };

  globalThis.__fakeWsInstalled = true;
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

fn call_global_2(name: &str, a1: &JsValue, a2: &JsValue) -> JsValue {
    let global = js_sys::global();
    let f = Reflect::get(&global, &JsValue::from_str(name))
        .expect("global function not found")
        .dyn_into::<Function>()
        .expect("global is not function");
    f.call2(&JsValue::NULL, a1, a2).expect("global call failed")
}

fn open_fake_ws(url: &str) {
    let ok = call_global_1("__fake_ws_open", &JsValue::from_str(url))
        .as_bool()
        .unwrap_or(false);
    assert!(ok, "failed to open fake websocket for {url}");
}

fn emit_fake_ws(url: &str, frame: &[u8]) {
    let bytes = Uint8Array::from(frame);
    let ok = call_global_2(
        "__fake_ws_emit",
        &JsValue::from_str(url),
        &bytes.into(),
    )
    .as_bool()
    .unwrap_or(false);
    assert!(ok, "failed to emit fake websocket frame for {url}");
}

fn get_sent_frame(url: &str, index: u32) -> Uint8Array {
    let v = call_global_2(
        "__fake_ws_get_sent",
        &JsValue::from_str(url),
        &JsValue::from_f64(index as f64),
    );
    v.dyn_into::<Uint8Array>()
        .expect("sent frame is missing or not Uint8Array")
}

#[wasm_bindgen_test(async)]
async fn e2e_read_holding_registers_resolves_typed_array() {
    install_fake_websocket();
    let url = "ws://e2e-read-holding";

    let mut client = WasmModbusClient::new(url, 1, 100, 0, 1).expect("client creation failed");
    open_fake_ws(url);
    assert!(client.is_connected());

    let promise = client.read_holding_registers(0x006B, 2);

    // Validate outbound request frame bytes (txn id starts at 1).
    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[0], 0x00); // txn hi
    assert_eq!(sent[1], 0x01); // txn lo
    assert_eq!(sent[7], 0x03); // FC: read holding regs

    // Respond with two registers: 0x1234, 0x5678
    let rsp = [
        0x00, 0x01, // txn id
        0x00, 0x00, // protocol
        0x00, 0x07, // length
        0x01,       // unit id
        0x03,       // FC
        0x04,       // byte count
        0x12, 0x34, 0x56, 0x78,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise).await.expect("promise should resolve");
    let regs = value
        .dyn_into::<Uint16Array>()
        .expect("result should be Uint16Array");

    assert_eq!(regs.length(), 2);
    assert_eq!(regs.get_index(0), 0x1234);
    assert_eq!(regs.get_index(1), 0x5678);
}

#[wasm_bindgen_test(async)]
async fn e2e_write_single_register_resolves_object() {
    install_fake_websocket();
    let url = "ws://e2e-write-single";

    let mut client = WasmModbusClient::new(url, 1, 100, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.write_single_register(0x000A, 0x00FF);

    let rsp = [
        0x00, 0x01, // txn id
        0x00, 0x00, // protocol
        0x00, 0x06, // length
        0x01,       // unit id
        0x06,       // FC write single register
        0x00, 0x0A, // address
        0x00, 0xFF, // value
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise).await.expect("promise should resolve");
    let addr = Reflect::get(&value, &JsValue::from_str("address"))
        .expect("address field missing")
        .as_f64()
        .unwrap_or(-1.0);
    let reg = Reflect::get(&value, &JsValue::from_str("value"))
        .expect("value field missing")
        .as_f64()
        .unwrap_or(-1.0);

    assert_eq!(addr as u16, 0x000A);
    assert_eq!(reg as u16, 0x00FF);
}

#[wasm_bindgen_test(async)]
async fn e2e_timeout_rejects_promise() {
    install_fake_websocket();
    let url = "ws://e2e-timeout";

    // timeout=20ms, retries=0, tick every 1ms => reject should happen quickly.
    let mut client = WasmModbusClient::new(url, 1, 20, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_holding_registers(0x0000, 1);
    let result = JsFuture::from(promise).await;

    assert!(result.is_err(), "timeout path should reject promise");
}

#[wasm_bindgen_test(async)]
async fn e2e_reconnect_rejects_inflight_requests() {
    install_fake_websocket();
    let url = "ws://e2e-reconnect";

    let mut client = WasmModbusClient::new(url, 1, 1_000, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_holding_registers(0x0000, 1);
    assert!(client.reconnect(), "reconnect should return true");

    let result = JsFuture::from(promise).await;
    assert!(result.is_err(), "in-flight request should be rejected on reconnect");
    let err = result.err().unwrap_or(JsValue::NULL);
    let msg = err.as_string().unwrap_or_default();
    assert!(msg.contains("ConnectionLost"), "unexpected error message: {msg}");
}
