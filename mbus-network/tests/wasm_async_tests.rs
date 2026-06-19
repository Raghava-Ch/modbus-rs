#![cfg(target_arch = "wasm32")]

use js_sys::{Function, Reflect, Uint8Array};
use mbus_core::errors::MbusError;
use mbus_core::transport::AsyncTransport;
use mbus_network::WasmAsyncTransport;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn install_fake_websocket() {
    let script = r#"
if (!globalThis.__fakeWsInstalled) {
  class FakeWebSocket {
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this.binaryType = 'arraybuffer';
      this.sent = [];
      this.listeners = {};
      this.onopen = null;
      this.onclose = null;
      this.onerror = null;
      this.onmessage = null;
      globalThis.__fakeWsRegistry.set(url, this);
      const created = globalThis.__fakeWsCreateCount.get(url) ?? 0;
      globalThis.__fakeWsCreateCount.set(url, created + 1);

      // Auto-open on next tick to allow WasmAsyncTransport::connect to resolve
      setTimeout(() => {
        if (this.readyState === 0) {
          this.readyState = 1;
          this.dispatchEvent(new Event('open'));
        }
      }, 0);
    }

    addEventListener(type, listener) {
      if (!this.listeners[type]) {
        this.listeners[type] = [];
      }
      this.listeners[type].push(listener);
    }

    removeEventListener(type, listener) {
      if (this.listeners[type]) {
        this.listeners[type] = this.listeners[type].filter(l => l !== listener);
      }
    }

    dispatchEvent(event) {
      const type = event.type;
      const onprop = this['on' + type];
      if (onprop) {
        try {
          onprop(event);
        } catch (e) {
          console.error('on' + type + ' handler error:', e);
        }
      }
      if (this.listeners[type]) {
        for (const listener of this.listeners[type]) {
          try {
            if (typeof listener === 'function') {
              listener(event);
            } else if (listener && typeof listener.handleEvent === 'function') {
              listener.handleEvent(event);
            }
          } catch (e) {
            console.error('listener error:', e);
          }
        }
      }
      return true;
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
      const event = new Event('close');
      event.code = 1000;
      event.reason = '';
      event.wasClean = true;
      this.dispatchEvent(event);
    }
  }

  globalThis.__fakeWsRegistry = new Map();
  globalThis.__fakeWsCreateCount = new Map();
  globalThis.WebSocket = FakeWebSocket;

  globalThis.__fake_ws_open = (url) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    if (ws.readyState === 0) {
      ws.readyState = 1;
      ws.dispatchEvent(new Event('open'));
    }
    return true;
  };

  globalThis.__fake_ws_close = (url) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    ws.readyState = 3;
    const event = new Event('close');
    event.code = 1000;
    event.reason = '';
    event.wasClean = true;
    ws.dispatchEvent(event);
    return true;
  };

  globalThis.__fake_ws_emit = (url, bytes) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return false;
    const payload = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    const ab = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
    ws.dispatchEvent(new MessageEvent('message', { data: ab }));
    return true;
  };

  globalThis.__fake_ws_get_sent = (url, idx) => {
    const ws = globalThis.__fakeWsRegistry.get(url);
    if (!ws) return null;
    const i = idx ?? 0;
    return ws.sent[i] ?? null;
  };

  globalThis.__fake_ws_created_count = (url) => {
    return globalThis.__fakeWsCreateCount.get(url) ?? 0;
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

fn emit_fake_ws(url: &str, frame: &[u8]) {
    let bytes = Uint8Array::from(frame);
    let ok = call_global_2("__fake_ws_emit", &JsValue::from_str(url), &bytes.into())
        .as_bool()
        .unwrap_or(false);
    assert!(ok, "failed to emit fake websocket frame for {url}");
}

async fn get_sent_frame(url: &str, index: u32) -> Uint8Array {
    gloo_timers::future::TimeoutFuture::new(10).await;
    let v = call_global_2(
        "__fake_ws_get_sent",
        &JsValue::from_str(url),
        &JsValue::from_f64(index as f64),
    );
    v.dyn_into::<Uint8Array>()
        .expect("sent frame is missing or not Uint8Array")
}

#[wasm_bindgen_test(async)]
async fn test_wasm_async_transport_lifecycle() {
    install_fake_websocket();
    let url = "ws://test-async-lifecycle";

    // 1. Connect
    let mut transport = WasmAsyncTransport::connect(url)
        .await
        .expect("Failed to connect WasmAsyncTransport");

    assert!(transport.is_open());
    assert!(transport.is_connected());

    // 2. Send frame
    let adu_request = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x02,
    ];
    transport
        .send_frame(&adu_request)
        .expect("Failed to send frame");

    let sent = get_sent_frame(url, 0).await.to_vec();
    assert_eq!(sent, adu_request);

    // 3. Receive frame
    let adu_response = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0x12, 0x34,
    ];
    emit_fake_ws(url, &adu_response);

    let received = transport
        .recv_frame()
        .await
        .expect("Failed to receive frame");
    assert_eq!(&received[..], &adu_response[..]);

    // 4. Close
    transport.close();
    assert!(!transport.is_open());
    assert!(!transport.is_connected());

    // 5. Send/recv post-close returns ConnectionClosed
    assert!(matches!(
        transport.send_frame(&[0x01]),
        Err(MbusError::ConnectionClosed)
    ));
    assert!(matches!(
        transport.recv_frame().await,
        Err(MbusError::ConnectionClosed)
    ));
}

#[wasm_bindgen_test(async)]
async fn test_wasm_async_transport_oversized_frame() {
    install_fake_websocket();
    let url = "ws://test-async-oversized";

    let mut transport = WasmAsyncTransport::connect(url).await.unwrap();

    // Emit a message larger than MAX_ADU_FRAME_LEN (which is 513)
    let mut oversized_frame = vec![0u8; 600];
    oversized_frame[4] = 0x02; // length field = 594, total_len = 600 > 513
    oversized_frame[5] = 0x52;
    emit_fake_ws(url, &oversized_frame);

    let recv_res = transport.recv_frame().await;
    assert!(matches!(recv_res, Err(MbusError::BufferTooSmall)));
}

#[wasm_bindgen_test(async)]
async fn test_wasm_async_transport_server_disconnect() {
    install_fake_websocket();
    let url = "ws://test-async-server-disconnect";

    let mut transport = WasmAsyncTransport::connect(url).await.unwrap();

    // Trigger close callback via fake JS WebSocket helper
    let closed = call_global_1("__fake_ws_close", &JsValue::from_str(url))
        .as_bool()
        .unwrap_or(false);
    assert!(closed);

    // Yield control to let the stream processing task register the close
    gloo_timers::future::TimeoutFuture::new(10).await;

    assert!(!transport.is_open());
    assert!(matches!(
        transport.recv_frame().await,
        Err(MbusError::ConnectionClosed)
    ));
}

#[wasm_bindgen_test(async)]
async fn test_wasm_async_transport_trait_impl() {
    install_fake_websocket();
    let url = "ws://test-async-trait";

    let mut transport = WasmAsyncTransport::connect(url).await.unwrap();

    // Test send via AsyncTransport trait
    let adu = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x02,
    ];
    AsyncTransport::send(&mut transport, &adu)
        .await
        .expect("Trait send failed");

    let sent = get_sent_frame(url, 0).await.to_vec();
    assert_eq!(sent, adu);

    // Test recv via AsyncTransport trait
    emit_fake_ws(url, &adu);
    let received = AsyncTransport::recv(&mut transport)
        .await
        .expect("Trait recv failed");
    assert_eq!(&received[..], &adu[..]);
}

#[wasm_bindgen_test(async)]
async fn test_wasm_async_transport_coalesced_frames() {
    install_fake_websocket();
    let url = "ws://test-async-coalesced";

    let mut transport = WasmAsyncTransport::connect(url).await.unwrap();

    // Two valid frames coalesced into one WebSocket message:
    // Frame 1: length 5 (total 11)
    let adu1 = vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03, 0x02, 0x12, 0x34,
    ];
    // Frame 2: length 6 (total 12)
    let adu2 = vec![
        0x00, 0x02, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x02,
    ];

    let mut coalesced = Vec::new();
    coalesced.extend_from_slice(&adu1);
    coalesced.extend_from_slice(&adu2);

    emit_fake_ws(url, &coalesced);

    // Read the first frame
    let recv1 = transport
        .recv_frame()
        .await
        .expect("Failed to receive first frame");
    assert_eq!(&recv1[..], &adu1[..]);

    // Read the second frame (should be fetched from rx_buf without needing another ws event)
    let recv2 = transport
        .recv_frame()
        .await
        .expect("Failed to receive second frame");
    assert_eq!(&recv2[..], &adu2[..]);
}
