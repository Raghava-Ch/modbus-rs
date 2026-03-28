#![cfg(target_arch = "wasm32")]

use js_sys::{Array, Function, Reflect, Uint8Array, Uint16Array};
use mbus_ffi::{WasmModbusClient, WasmSerialModbusClient, WasmSerialPortHandle};
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
    let ok = call_global_2("__fake_ws_emit", &JsValue::from_str(url), &bytes.into())
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
        0x01, // unit id
        0x03, // FC
        0x04, // byte count
        0x12, 0x34, 0x56, 0x78,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("promise should resolve");
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
        0x01, // unit id
        0x06, // FC write single register
        0x00, 0x0A, // address
        0x00, 0xFF, // value
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("promise should resolve");
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
    assert!(
        result.is_err(),
        "in-flight request should be rejected on reconnect"
    );
    let err = result.err().unwrap_or(JsValue::NULL);
    let msg = err.as_string().unwrap_or_default();
    assert!(
        msg.contains("ConnectionLost"),
        "unexpected error message: {msg}"
    );
}

// ── TCP coil tests ────────────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_coils_resolves_uint8array() {
    install_fake_websocket();
    let url = "ws://e2e-read-coils";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // Read 8 coils at address 0x0001
    let promise = client.read_coils(0x0001, 8);

    // FC=0x01 in sent frame
    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x01, "FC should be Read Coils (0x01)");

    // Response: byte_count=1, coil_data=0xAB (MBAP len = 1+1+1+1 = 4)
    let rsp = [
        0x00, 0x01, // txn id
        0x00, 0x00, // protocol
        0x00, 0x04, // length: unit(1)+FC(1)+byte_count(1)+data(1)
        0x01, // unit id
        0x01, // FC
        0x01, // byte count
        0xAB, // coil data (8 coils bit-packed)
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_coils should resolve");
    let arr = value
        .dyn_into::<Uint8Array>()
        .expect("result should be Uint8Array");
    assert_eq!(arr.length(), 1);
    assert_eq!(arr.get_index(0), 0xAB);
}

#[wasm_bindgen_test(async)]
async fn e2e_read_single_coil_resolves_bool() {
    install_fake_websocket();
    let url = "ws://e2e-read-single-coil";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_single_coil(0x0005);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x01, "FC should be Read Coils (0x01)");

    // Response: byte_count=1, data=0x01 (bit 0 = true)
    let rsp = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x01, 0x01, 0x01];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_single_coil should resolve");
    assert_eq!(
        value.as_bool().expect("should be a bool"),
        true,
        "coil at bit-0 of 0x01 should be true"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_write_single_coil_resolves_object() {
    install_fake_websocket();
    let url = "ws://e2e-write-single-coil";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.write_single_coil(0x0010, true);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x05, "FC should be Write Single Coil (0x05)");

    // Echo response: addr=0x0010, value=0xFF00 (ON)
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0x10, // address = 0x0010
        0xFF, 0x00, // value = ON
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("write_single_coil should resolve");
    let addr = Reflect::get(&value, &JsValue::from_str("address"))
        .expect("address field missing")
        .as_f64()
        .unwrap_or(-1.0);
    let coil_val = Reflect::get(&value, &JsValue::from_str("value"))
        .expect("value field missing")
        .as_bool()
        .unwrap_or(false);
    assert_eq!(addr as u16, 0x0010);
    assert!(coil_val, "coil value should be true");
}

#[wasm_bindgen_test(async)]
async fn e2e_write_multiple_coils_resolves_object() {
    install_fake_websocket();
    let url = "ws://e2e-write-multi-coils";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // Write 2 coils at 0x0020, both ON (packed=0x03)
    let promise = client.write_multiple_coils(0x0020, 2, &[0x03]);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x0F, "FC should be Write Multiple Coils (0x0F)");

    // Response: addr=0x0020, qty=2
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x0F, 0x00, 0x20, // address = 0x0020
        0x00, 0x02, // quantity = 2
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("write_multiple_coils should resolve");
    let addr = Reflect::get(&value, &JsValue::from_str("address"))
        .expect("address field missing")
        .as_f64()
        .unwrap_or(-1.0);
    let qty = Reflect::get(&value, &JsValue::from_str("quantity"))
        .expect("quantity field missing")
        .as_f64()
        .unwrap_or(-1.0);
    assert_eq!(addr as u16, 0x0020);
    assert_eq!(qty as u16, 2);
}

// ── TCP register tests ────────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_single_holding_register_resolves_number() {
    install_fake_websocket();
    let url = "ws://e2e-read-single-hreg";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_single_holding_register(0x0100);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x03, "FC should be Read Holding Registers (0x03)");

    // Response: byte_count=2, value=0xBEEF
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0xBE, 0xEF,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_single_holding_register should resolve");
    assert_eq!(value.as_f64().expect("should be a number") as u16, 0xBEEF);
}

#[wasm_bindgen_test(async)]
async fn e2e_read_input_registers_resolves_uint16array() {
    install_fake_websocket();
    let url = "ws://e2e-read-input-regs";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_input_registers(0x0200, 2);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x04, "FC should be Read Input Registers (0x04)");

    // Response: 2 registers 0x1122, 0x3344
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x01, 0x04, 0x04, 0x11, 0x22, 0x33, 0x44,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_input_registers should resolve");
    let regs = value
        .dyn_into::<Uint16Array>()
        .expect("should be Uint16Array");
    assert_eq!(regs.length(), 2);
    assert_eq!(regs.get_index(0), 0x1122);
    assert_eq!(regs.get_index(1), 0x3344);
}

#[wasm_bindgen_test(async)]
async fn e2e_read_single_input_register_resolves_number() {
    install_fake_websocket();
    let url = "ws://e2e-read-single-ireg";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_single_input_register(0x0200);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x04, "FC should be Read Input Registers (0x04)");

    // Response: byte_count=2, value=0xCAFE
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x04, 0x02, 0xCA, 0xFE,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_single_input_register should resolve");
    assert_eq!(value.as_f64().expect("should be a number") as u16, 0xCAFE);
}

#[wasm_bindgen_test(async)]
async fn e2e_write_multiple_registers_resolves_object() {
    install_fake_websocket();
    let url = "ws://e2e-write-multi-regs";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.write_multiple_registers(0x0300, 2, &[0x1234, 0x5678]);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(
        sent[7], 0x10,
        "FC should be Write Multiple Registers (0x10)"
    );

    // Echo response: addr=0x0300, qty=2
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x10, 0x03, 0x00, // address
        0x00, 0x02, // quantity
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("write_multiple_registers should resolve");
    let addr = Reflect::get(&value, &JsValue::from_str("address"))
        .expect("address field missing")
        .as_f64()
        .unwrap_or(-1.0);
    let qty = Reflect::get(&value, &JsValue::from_str("quantity"))
        .expect("quantity field missing")
        .as_f64()
        .unwrap_or(-1.0);
    assert_eq!(addr as u16, 0x0300);
    assert_eq!(qty as u16, 2);
}

#[wasm_bindgen_test(async)]
async fn e2e_read_write_multiple_registers_resolves_uint16array() {
    install_fake_websocket();
    let url = "ws://e2e-rw-multi-regs";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // Read 2 regs from 0x0400, simultaneously write [0x1234] to 0x0401
    let promise = client.read_write_multiple_registers(0x0400, 2, 0x0401, 1, &[0x1234]);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(
        sent[7], 0x17,
        "FC should be Read/Write Multiple Registers (0x17)"
    );

    // Response contains the values read (2 registers)
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x01, 0x17, 0x04, 0xAA, 0xBB, 0xCC, 0xDD,
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_write_multiple_registers should resolve");
    let regs = value
        .dyn_into::<Uint16Array>()
        .expect("should be Uint16Array");
    assert_eq!(regs.length(), 2);
    assert_eq!(regs.get_index(0), 0xAABB);
    assert_eq!(regs.get_index(1), 0xCCDD);
}

#[wasm_bindgen_test(async)]
async fn e2e_mask_write_register_resolves_true() {
    install_fake_websocket();
    let url = "ws://e2e-mask-write-reg";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.mask_write_register(0x0500, 0xFFFF, 0x0000);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x16, "FC should be Mask Write Register (0x16)");

    // Echo response: addr=0x0500, and_mask=0xFFFF, or_mask=0x0000
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x01, 0x16, 0x05, 0x00, // address = 0x0500
        0xFF, 0xFF, // and_mask
        0x00, 0x00, // or_mask
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("mask_write_register should resolve");
    assert_eq!(
        value,
        JsValue::TRUE,
        "mask_write_register should resolve with true"
    );
}

// ── TCP discrete-input tests ──────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_discrete_inputs_resolves_uint8array() {
    install_fake_websocket();
    let url = "ws://e2e-read-disc-inputs";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_discrete_inputs(0x0600, 8);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x02, "FC should be Read Discrete Inputs (0x02)");

    // Response: byte_count=1, data=0x5A
    let rsp = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x02, 0x01, 0x5A];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_discrete_inputs should resolve");
    let arr = value
        .dyn_into::<Uint8Array>()
        .expect("should be Uint8Array");
    assert_eq!(arr.length(), 1);
    assert_eq!(arr.get_index(0), 0x5A);
}

#[wasm_bindgen_test(async)]
async fn e2e_read_single_discrete_input_resolves_bool() {
    install_fake_websocket();
    let url = "ws://e2e-read-single-di";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_single_discrete_input(0x0601);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x02, "FC should be Read Discrete Inputs (0x02)");

    // Response: bit 0 = 1 → true
    let rsp = [0x00, 0x01, 0x00, 0x00, 0x00, 0x04, 0x01, 0x02, 0x01, 0x01];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_single_discrete_input should resolve");
    assert_eq!(value.as_bool().expect("should be bool"), true);
}

// ── TCP FIFO test ─────────────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_fifo_queue_resolves_uint16array() {
    install_fake_websocket();
    let url = "ws://e2e-read-fifo";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    let promise = client.read_fifo_queue(0x0700);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x18, "FC should be Read FIFO Queue (0x18)");

    // FIFO response: fifo_byte_count=6 (2+2*2), fifo_count=2, values=[1, 2]
    // PDU data: [fifo_byte_count_hi, fifo_byte_count_lo, fifo_count_hi, fifo_count_lo, v1_hi, v1_lo, v2_hi, v2_lo]
    // MBAP: unit(1) + FC(1) + 8 PDU_data = 10 = 0x0A
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x0A, 0x01, 0x18, 0x00,
        0x06, // fifo_byte_count = 6 (2 for count field + 4 for data)
        0x00, 0x02, // fifo_count = 2
        0x00, 0x01, // value[0] = 1
        0x00, 0x02, // value[1] = 2
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_fifo_queue should resolve");
    let arr = value
        .dyn_into::<Uint16Array>()
        .expect("should be Uint16Array");
    assert_eq!(arr.length(), 2);
    assert_eq!(arr.get_index(0), 1);
    assert_eq!(arr.get_index(1), 2);
}

// ── TCP file-record tests ─────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_file_record_resolves_array() {
    install_fake_websocket();
    let url = "ws://e2e-read-file-rec";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // Read file 1, record 0, length 1
    let promise = client.read_file_record(1, 0, 1);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x14, "FC should be Read File Record (0x14)");

    // Response: 1 sub-response with 1 register value 0xABCD
    // byte_count=4, [file_resp_len=3, ref_type=0x06, 0xAB, 0xCD]
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x01, 0x14, 0x04, // byte_count = 4
        0x03, // file_resp_len = 3 (ref_type + 2 bytes data)
        0x06, // ref_type
        0xAB, 0xCD, // register value
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_file_record should resolve");
    let arr = value.dyn_into::<Array>().expect("should be Array");
    assert_eq!(arr.length(), 1);

    let entry = arr.get(0);
    let data_field = Reflect::get(&entry, &JsValue::from_str("data")).expect("data field missing");
    let data_arr = data_field
        .dyn_into::<Uint16Array>()
        .expect("data should be Uint16Array");
    assert_eq!(data_arr.length(), 1);
    assert_eq!(data_arr.get_index(0), 0xABCD);
}

#[wasm_bindgen_test(async)]
async fn e2e_write_file_record_resolves_true() {
    install_fake_websocket();
    let url = "ws://e2e-write-file-rec";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // Write value [0xDEAD] to file 1, record 0
    let promise = client.write_file_record(1, 0, &[0xDEAD]);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(sent[7], 0x15, "FC should be Write File Record (0x15)");

    // Response echoes the request (byte_count=9, sub-req with file=1, rec=0, len=1, val=0xDEAD)
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x0C, 0x01, 0x15, 0x09, // byte_count = 9
        0x06, // ref_type
        0x00, 0x01, // file_number = 1
        0x00, 0x00, // record_number = 0
        0x00, 0x01, // record_length = 1
        0xDE, 0xAD, // data value
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("write_file_record should resolve");
    assert_eq!(
        value,
        JsValue::TRUE,
        "write_file_record should resolve with true"
    );
}

// ── TCP diagnostics tests ─────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_read_exception_status_rejects_on_tcp() {
    install_fake_websocket();
    let url = "ws://e2e-exception-status";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // FC 0x07 is serial-line only; TCP path should reject immediately.
    let promise = client.read_exception_status();
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "read_exception_status over TCP should reject"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_diagnostics_rejects_on_tcp() {
    install_fake_websocket();
    let url = "ws://e2e-diagnostics";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // FC 0x08 is serial-line only; TCP path should reject immediately.
    let promise = client.diagnostics(0, &[0x0000]);
    let result = JsFuture::from(promise).await;
    assert!(result.is_err(), "diagnostics over TCP should reject");
}

#[wasm_bindgen_test(async)]
async fn e2e_get_comm_event_counter_rejects_on_tcp() {
    install_fake_websocket();
    let url = "ws://e2e-comm-event-ctr";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // FC 0x0B is serial-line only; TCP path should reject immediately.
    let promise = client.get_comm_event_counter();
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "get_comm_event_counter over TCP should reject"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_get_comm_event_log_rejects_on_tcp() {
    install_fake_websocket();
    let url = "ws://e2e-comm-event-log";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // FC 0x0C is serial-line only; TCP path should reject immediately.
    let promise = client.get_comm_event_log();
    let result = JsFuture::from(promise).await;
    assert!(result.is_err(), "get_comm_event_log over TCP should reject");
}

#[wasm_bindgen_test(async)]
async fn e2e_report_server_id_rejects_on_tcp() {
    install_fake_websocket();
    let url = "ws://e2e-report-server-id";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // FC 0x11 is serial-line only; TCP path should reject immediately.
    let promise = client.report_server_id();
    let result = JsFuture::from(promise).await;
    assert!(result.is_err(), "report_server_id over TCP should reject");
}

#[wasm_bindgen_test(async)]
async fn e2e_read_device_identification_resolves_object() {
    install_fake_websocket();
    let url = "ws://e2e-read-device-id";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // read_device_id_code=1 (Basic), object_id=0 (VendorName)
    let promise = client.read_device_identification(1, 0);

    let sent = get_sent_frame(url, 0).to_vec();
    assert_eq!(
        sent[7], 0x2B,
        "FC should be Encapsulated Interface Transport (0x2B)"
    );

    // Response: MEI=0x0E, code=1 (Basic), conformity=0x01, no_more, next=0, 1 object {id=0, "ACME"}
    let rsp = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x0E, 0x01, 0x2B,
        0x0E, // MEI type = ReadDeviceIdentification
        0x01, // read_device_id_code = 1 (Basic)
        0x01, // conformity_level = 0x01 (BasicStreamOnly)
        0x00, // more_follows = 0x00 (false)
        0x00, // next_object_id
        0x01, // number_of_objects = 1
        0x00, // object id = 0 (VendorName)
        0x04, // object length = 4
        0x41, 0x43, 0x4D, 0x45, // "ACME"
    ];
    emit_fake_ws(url, &rsp);

    let value = JsFuture::from(promise)
        .await
        .expect("read_device_identification should resolve");
    let code = Reflect::get(&value, &JsValue::from_str("readDeviceIdCode"))
        .expect("readDeviceIdCode missing")
        .as_f64()
        .unwrap_or(-1.0);
    let more = Reflect::get(&value, &JsValue::from_str("moreFollows"))
        .expect("moreFollows missing")
        .as_bool()
        .unwrap_or(true);
    let objects_field =
        Reflect::get(&value, &JsValue::from_str("objects")).expect("objects missing");
    let objects = objects_field
        .dyn_into::<Array>()
        .expect("objects should be Array");
    assert_eq!(code as u8, 1);
    assert!(!more, "moreFollows should be false");
    assert_eq!(objects.length(), 1);

    let obj0 = objects.get(0);
    let obj_value = Reflect::get(&obj0, &JsValue::from_str("value"))
        .expect("value field missing")
        .as_string()
        .expect("value should be string");
    assert_eq!(obj_value, "ACME");
}

// ── TCP error-path tests ──────────────────────────────────────────────────────

#[wasm_bindgen_test(async)]
async fn e2e_diagnostics_invalid_sub_function_rejects() {
    install_fake_websocket();
    let url = "ws://e2e-diag-invalid-sf";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // 0xFFFF is a reserved/invalid sub_function
    let promise = client.diagnostics(0xFFFF, &[]);
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "invalid sub_function should reject immediately"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_read_device_identification_invalid_code_rejects() {
    install_fake_websocket();
    let url = "ws://e2e-dev-id-bad-code";
    let mut client = WasmModbusClient::new(url, 1, 200, 0, 1).expect("client creation failed");
    open_fake_ws(url);

    // 0x00 is not a valid ReadDeviceIdCode (valid: 1–4)
    let promise = client.read_device_identification(0x00, 0);
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "invalid read_device_id_code should reject immediately"
    );
}

// ── Serial client construction / error tests ──────────────────────────────────

/// Returns a fake JS SerialPort-like object suitable for wasm browser tests.
fn make_fake_serial_port() -> JsValue {
    js_sys::eval(
        r#"(() => {
            const reader = {
                read: () => Promise.resolve({ done: true, value: new Uint8Array(0) }),
                releaseLock: () => {}
            };
            const writer = {
                write: (_data) => Promise.resolve(undefined),
                releaseLock: () => {}
            };
            return {
                open: function(_opts) { return Promise.resolve(undefined); },
                close: function() { return Promise.resolve(undefined); },
                readable: {
                    getReader: () => reader
                },
                writable: {
                    getWriter: () => writer
                }
            };
        })()"#,
    )
    .expect("eval of fake serial port object failed")
}

#[wasm_bindgen_test]
fn e2e_serial_client_new_valid_params_succeeds() {
    let port = make_fake_serial_port();
    let handle = WasmSerialPortHandle::new_for_testing(port);
    assert!(handle.is_valid(), "fake port handle should be valid");

    let result = WasmSerialModbusClient::new(
        &handle, 1, // unit_id
        "rtu", 9600, 8, 1, "none", 500, 0, 5,
    );
    assert!(
        result.is_ok(),
        "valid RTU construction should succeed: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn e2e_serial_client_new_ascii_mode_succeeds() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let result = WasmSerialModbusClient::new(&handle, 1, "ascii", 19200, 7, 1, "even", 500, 0, 5);
    assert!(
        result.is_ok(),
        "ASCII mode construction should succeed: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn e2e_serial_client_new_invalid_mode_rejects() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let result = WasmSerialModbusClient::new(&handle, 1, "notamode", 9600, 8, 1, "none", 500, 0, 5);
    assert!(result.is_err(), "invalid mode should fail construction");
}

#[wasm_bindgen_test]
fn e2e_serial_client_new_invalid_parity_rejects() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let result = WasmSerialModbusClient::new(&handle, 1, "rtu", 9600, 8, 1, "space", 500, 0, 5);
    assert!(result.is_err(), "invalid parity should fail construction");
}

#[wasm_bindgen_test]
fn e2e_serial_client_new_invalid_data_bits_rejects() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let result = WasmSerialModbusClient::new(
        &handle, 1, "rtu", 9600, 9, /* invalid */
        1, "none", 500, 0, 5,
    );
    assert!(
        result.is_err(),
        "invalid data_bits (9) should fail construction"
    );
}

#[wasm_bindgen_test]
fn e2e_serial_client_is_connected_true_while_opening_or_connected() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let client = WasmSerialModbusClient::new(&handle, 1, "rtu", 9600, 8, 1, "none", 500, 0, 5)
        .expect("construction should succeed");
    // `is_connected` returns true for both opening and connected states.
    assert!(
        client.is_connected(),
        "serial client should report connected/opening after construction"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_serial_client_diagnostics_invalid_sub_function_rejects() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let mut client = WasmSerialModbusClient::new(&handle, 1, "rtu", 9600, 8, 1, "none", 500, 0, 5)
        .expect("construction should succeed");

    // 0xFFFF is a reserved/invalid sub_function — reject_immediate path.
    let promise = client.diagnostics(0xFFFF, &[]);
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "invalid sub_function should reject immediately"
    );
}

#[wasm_bindgen_test(async)]
async fn e2e_serial_client_read_device_id_invalid_code_rejects() {
    let handle = WasmSerialPortHandle::new_for_testing(make_fake_serial_port());
    let mut client = WasmSerialModbusClient::new(&handle, 1, "rtu", 9600, 8, 1, "none", 500, 0, 5)
        .expect("construction should succeed");

    // 0x00 is invalid (valid ReadDeviceIdCode: 1–4).
    let promise = client.read_device_identification(0x00, 0);
    let result = JsFuture::from(promise).await;
    assert!(
        result.is_err(),
        "invalid read_device_id_code should reject immediately"
    );
}
