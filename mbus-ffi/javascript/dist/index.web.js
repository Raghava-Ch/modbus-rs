import rawInit, * as wasmExports from 'modbus-rs-wasm/web';

let initPromise = null;

/**
 * Ensures the WebAssembly module is initialized.
 * Can be called explicitly or automatically triggered by factory methods.
 * @param {RequestInfo | URL | Response | BufferSource | WebAssembly.Module} [moduleOrPath] Optional WASM source/URL
 * @returns {Promise<void>}
 */
export function ensureInit(moduleOrPath) {
  if (!initPromise) {
    initPromise = rawInit(moduleOrPath).then(() => {}).catch((err) => {
      initPromise = null;
      throw err;
    });
  }
  return initPromise;
}

export default ensureInit;

function wrapStaticMethod(targetClass, methodName) {
  if (targetClass && typeof targetClass[methodName] === 'function') {
    const original = targetClass[methodName];
    targetClass[methodName] = async function (...args) {
      await ensureInit();
      return original.apply(this, args);
    };
  }
}

wrapStaticMethod(wasmExports.WasmWsTransport, 'connect');
wrapStaticMethod(wasmExports.WasmRtuTransport, 'open');
wrapStaticMethod(wasmExports.WasmAsciiTransport, 'open');
wrapStaticMethod(wasmExports.WasmWsModbusServer, 'bind');
wrapStaticMethod(wasmExports.WasmSerialModbusServer, 'bindRtu');
wrapStaticMethod(wasmExports.WasmSerialModbusServer, 'bindAscii');

export const requestSerialPort = async function (...args) {
  await ensureInit();
  return wasmExports.requestSerialPort(...args);
};

export * from 'modbus-rs-wasm/web';
