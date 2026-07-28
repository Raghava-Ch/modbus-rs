const fs = require('fs');
const path = require('path');

const dtsPath = path.join(__dirname, '../dist/index.d.ts');
const jsPath = path.join(__dirname, '../dist/index.js');

if (!fs.existsSync(dtsPath)) {
  console.error('index.d.ts not found');
  process.exit(1);
}

let dts = fs.readFileSync(dtsPath, 'utf8');

// 2. Replace signal?: object with signal?: AbortSignal
dts = dts.replace(/signal\?: object/g, 'signal?: AbortSignal');

// 3. Align pendingRequests getter with WASM readonly property syntax
dts = dts.replace(/get pendingRequests\(\): boolean/g, 'readonly pendingRequests: boolean');

// 4. Strongly type server bind handlers
dts = dts.replace(/static bindRtu\(options: SerialServerOptions, handlers: object\)/g, 'static bindRtu(options: SerialServerOptions, handlers: ServerHandlers)');
dts = dts.replace(/static bindAscii\(options: SerialServerOptions, handlers: object\)/g, 'static bindAscii(options: SerialServerOptions, handlers: ServerHandlers)');
dts = dts.replace(/static bind\(options: TcpServerOptions, handlers: object\)/g, 'static bind(options: TcpServerOptions, handlers: ServerHandlers)');

// 3. Harmonize enums: strip 'declare const ' so it matches WASM's 'export enum'
dts = dts.replace(/export declare const enum /g, 'export enum ');



const extraDeclarations = `
/** Represents a Modbus discrete input pin state (alias for CoilState). */
export type DiscreteInputState = CoilState;
export declare const DiscreteInputState: typeof CoilState;

/** Modbus exception returned from a server handler. */
export interface ModbusException {
  /** The Modbus exception code. */
  exceptionCode: ModbusExceptionCode | number;
}

/**
 * Callback functions to handle Modbus server requests.
 * 
 * Each handler corresponds to a specific Modbus function code. If a handler is not provided,
 * the server will respond with an "Illegal Function" exception (0x01).
 * 
 * Handlers can return the expected data directly or return a Promise for async operations.
 * To return a Modbus exception, return an object matching the \`ModbusException\` interface.
 */
export interface ServerHandlers {
  /**
   * Handle Read Coils (FC 01).
   * @param req The request object.
   * @param req.address The starting coil address (0x0000 to 0xFFFF).
   * @param req.quantity The number of coils to read (1 to 2000).
   * @returns An array of booleans representing the coil states, or a ModbusException.
   */
  onReadCoils?: (req: ReadCoilsRequest) => CoilState[] | ModbusException | Promise<CoilState[] | ModbusException>;

  /**
   * Handle Read Discrete Inputs (FC 02).
   * @param req The request object.
   * @param req.address The starting discrete input address (0x0000 to 0xFFFF).
   * @param req.quantity The number of inputs to read (1 to 2000).
   * @returns An array of discrete input states, or a ModbusException.
   */
  onReadDiscreteInputs?: (req: ReadDiscreteInputsRequest) => DiscreteInputState[] | ModbusException | Promise<DiscreteInputState[] | ModbusException>;

  /**
   * Handle Read Holding Registers (FC 03).
   * @param req The request object.
   * @param req.address The starting holding register address (0x0000 to 0xFFFF).
   * @param req.quantity The number of registers to read (1 to 125).
   * @returns An array of 16-bit numbers representing the registers, or a ModbusException.
   */
  onReadHoldingRegisters?: (req: ReadHoldingRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;

  /**
   * Handle Read Input Registers (FC 04).
   * @param req The request object.
   * @param req.address The starting input register address (0x0000 to 0xFFFF).
   * @param req.quantity The number of registers to read (1 to 125).
   * @returns An array of 16-bit numbers representing the registers, or a ModbusException.
   */
  onReadInputRegisters?: (req: ReadInputRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;

  /**
   * Handle Write Single Coil (FC 05).
   * @param req The request object.
   * @param req.address The address of the coil to write (0x0000 to 0xFFFF).
   * @param req.value The boolean value to write (true for ON, false for OFF).
   * @returns void on success, or a ModbusException.
   */
  onWriteSingleCoil?: (req: WriteSingleCoilRequest) => void | ModbusException | Promise<void | ModbusException>;

  /**
   * Handle Write Single Register (FC 06).
   * @param req The request object.
   * @param req.address The address of the register to write (0x0000 to 0xFFFF).
   * @param req.value The 16-bit value to write.
   * @returns void on success, or a ModbusException.
   */
  onWriteSingleRegister?: (req: WriteSingleRegisterRequest) => void | ModbusException | Promise<void | ModbusException>;

  /**
   * Handle Read Exception Status (FC 07).
   * @param req The read exception status request object (empty).
   * @returns An 8-bit exception status byte, or a ModbusException.
   */
  onReadExceptionStatus?: (req: ReadExceptionStatusRequest) => number | ModbusException | Promise<number | ModbusException>;

  /**
   * Handle Diagnostics (FC 08).
   * @param req The diagnostics request object.
   * @param req.subFunction The 16-bit sub-function code.
   * @param req.data The 16-bit data payload for the sub-function.
   * @returns A response containing the sub-function and data, or a ModbusException.
   */
  onDiagnostics?: (req: DiagnosticsRequest) => ServerDiagnosticsResponse | ModbusException | Promise<ServerDiagnosticsResponse | ModbusException>;

  /**
   * Handle Write Multiple Coils (FC 15).
   * @param req The request object.
   * @param req.address The starting address of the coils to write.
   * @param req.values An array of booleans to write.
   * @returns void on success, or a ModbusException.
   */
  onWriteMultipleCoils?: (req: WriteMultipleCoilsRequest) => void | ModbusException | Promise<void | ModbusException>;

  /**
   * Handle Write Multiple Registers (FC 16).
   * @param req The request object.
   * @param req.address The starting address of the registers to write.
   * @param req.values An array of 16-bit numbers to write.
   * @returns void on success, or a ModbusException.
   */
  onWriteMultipleRegisters?: (req: WriteMultipleRegistersRequest) => void | ModbusException | Promise<void | ModbusException>;

  /**
   * Handle Read File Record (FC 20).
   * @param req The request object.
   * @param req.requests An array of sub-requests, each with \`fileNumber\`, \`recordNumber\`, and \`recordLength\`.
   * @returns An array of register arrays for each sub-request, or a ModbusException.
   */
  onReadFileRecord?: (req: ReadFileRecordRequest) => Uint16Array[] | ModbusException | Promise<Uint16Array[] | ModbusException>;

  /**
   * Handle Write File Record (FC 21).
   * @param req The request object.
   * @param req.requests An array of sub-requests, each with \`fileNumber\`, \`recordNumber\`, and \`recordData\` (a Uint16Array).
   * @returns void on success, or a ModbusException.
   */
  onWriteFileRecord?: (req: WriteFileRecordRequest) => void | ModbusException | Promise<void | ModbusException>;

  /**
   * Handle Read/Write Multiple Registers (FC 23).
   * @param req The request containing addresses and values to read and write.
   * @param req.readAddress The starting address for the read operation.
   * @param req.readQuantity The number of registers to read.
   * @param req.writeAddress The starting address for the write operation.
   * @param req.values An array of 16-bit numbers to write.
   * @returns An array of 16-bit numbers read, or a ModbusException.
   */
  onReadWriteMultipleRegisters?: (req: ReadWriteMultipleRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;

  /**
   * Handle Read FIFO Queue (FC 24).
   * @param req The request object containing the FIFO pointer \`address\`.
   * @returns An array of 16-bit numbers from the queue, or a ModbusException.
   */
  onReadFifoQueue?: (req: ReadFifoQueueRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;

  /**
   * Handle Read Device Identification (FC 43/14).
   * @param req The request object.
   * @returns Device identification response, or a ModbusException.
   */
  onReadDeviceIdentification?: (req: ReadDeviceIdentificationRequest) => DeviceIdentificationResponse | ModbusException | Promise<DeviceIdentificationResponse | ModbusException>;
}

/**
 * Stable error codes for identifying Modbus-related errors.
 * These can be used with the \`getModbusErrorCode\` helper to check for specific error types.
 */
export declare const ModbusErrorCode: {
  /** A Modbus exception response was received from the server (e.g., illegal function). */
  readonly EXCEPTION: 'MODBUS_EXCEPTION';
  /** The request timed out waiting for a response. */
  readonly TIMEOUT: 'MODBUS_TIMEOUT';
  /** A transport-level error occurred (e.g., framing error, checksum mismatch). */
  readonly TRANSPORT: 'MODBUS_TRANSPORT';
  /** An invalid argument was provided to a client or server function. */
  readonly INVALID_ARGUMENT: 'MODBUS_INVALID_ARGUMENT';
  /** The underlying connection was closed. */
  readonly CONNECTION_CLOSED: 'MODBUS_CONNECTION_CLOSED';
  /** An unexpected internal error occurred within the library. */
  readonly INTERNAL: 'MODBUS_INTERNAL';
};

/**
 * Extracts a stable error code from a Modbus error object.
 * @param err The error object.
 * @returns The corresponding code from \`ModbusErrorCode\`, or undefined if not a Modbus error.
 */
export declare function getModbusErrorCode(err: Error): string | undefined;

export {
  WasmWsTransport,
  WasmWsModbusClient,
  WasmWsModbusServer,
  WasmSerialModbusClient,
  WasmSerialModbusServer,
  WasmSerialPortHandle,
  WasmRtuTransport,
  WasmAsciiTransport,
  WasmServerTransportKind,
  requestSerialPort
} from 'modbus-rs-wasm';
`;

dts += extraDeclarations;

// Add semicolons to class and interface signatures lacking them
const newline = dts.includes('\r\n') ? '\r\n' : '\n';
const lines = dts.split(newline);
const processedLines = lines.map((line) => {
  const trimmed = line.trim();
  if (trimmed.length > 0 &&
    !trimmed.includes('*') &&
    !trimmed.includes('/') &&
    !trimmed.endsWith('{') &&
    !trimmed.endsWith('}') &&
    !trimmed.endsWith(';') &&
    !trimmed.endsWith(',')) {
    if (/^\s+/.test(line) && (trimmed.includes(':') || trimmed.includes('('))) {
      return line + ';';
    }
  }
  return line;
});
dts = processedLines.join(newline);

fs.writeFileSync(dtsPath, dts, 'utf8');
console.log('Successfully updated index.d.ts');

if (fs.existsSync(jsPath)) {
  let js = fs.readFileSync(jsPath, 'utf8');

  // 6. Append helper exports to index.js
  const jsExports = `
/**
 * Stable error codes for identifying Modbus-related errors.
 * These can be used with the \`getModbusErrorCode\` helper to check for specific error types.
 */
module.exports.ModbusErrorCode = {
  /** A Modbus exception response was received from the server (e.g., illegal function). */
  EXCEPTION: 'MODBUS_EXCEPTION',
  /** The request timed out waiting for a response. */
  TIMEOUT: 'MODBUS_TIMEOUT',
  /** A transport-level error occurred (e.g., framing error, checksum mismatch). */
  TRANSPORT: 'MODBUS_TRANSPORT',
  /** An invalid argument was provided to a client or server function. */
  INVALID_ARGUMENT: 'MODBUS_INVALID_ARGUMENT',
  /** The underlying connection was closed. */
  CONNECTION_CLOSED: 'MODBUS_CONNECTION_CLOSED',
  /** An unexpected internal error occurred within the library. */
  INTERNAL: 'MODBUS_INTERNAL',
}

/**
 * Extracts a stable error code from a Modbus error object.
 * @param {Error} err The error object.
 * @returns {string | undefined} The corresponding code from \`ModbusErrorCode\`, or undefined if not a Modbus error.
 */
module.exports.getModbusErrorCode = function getModbusErrorCode(err) {
  if (!err || typeof err.message !== 'string') return undefined
  const m = err.message.match(/^\\[([A-Z_]+)(?::[^\\]]*)?\\]/)
  return m ? m[1] : undefined
}

module.exports.DiscreteInputState = module.exports.CoilState;

class WasmWsTransport {
  static connect() {
    throw new Error('WasmWsTransport is only available in browser environments.');
  }
}
class WasmWsModbusClient {}
class WasmWsModbusServer {}
class WasmSerialModbusClient {}
class WasmSerialModbusServer {}
class WasmSerialPortHandle {}
class WasmRtuTransport {
  static open() {
    throw new Error('WasmRtuTransport is only available in browser environments.');
  }
}
class WasmAsciiTransport {
  static open() {
    throw new Error('WasmAsciiTransport is only available in browser environments.');
  }
}
const WasmServerTransportKind = {};
function requestSerialPort() {
  throw new Error('requestSerialPort is only available in browser environments.');
}

module.exports.WasmWsTransport = WasmWsTransport;
module.exports.WasmWsModbusClient = WasmWsModbusClient;
module.exports.WasmWsModbusServer = WasmWsModbusServer;
module.exports.WasmSerialModbusClient = WasmSerialModbusClient;
module.exports.WasmSerialModbusServer = WasmSerialModbusServer;
module.exports.WasmSerialPortHandle = WasmSerialPortHandle;
module.exports.WasmRtuTransport = WasmRtuTransport;
module.exports.WasmAsciiTransport = WasmAsciiTransport;
module.exports.WasmServerTransportKind = WasmServerTransportKind;
module.exports.requestSerialPort = requestSerialPort;

function wrapServerHandlers(handlers) {
  if (!handlers) return handlers;
  const wrapped = { ...handlers };

  async function convertOutgoing(promiseOrVal) {
    const val = await promiseOrVal;
    if (val && (val instanceof Uint16Array || val instanceof Uint8Array || val instanceof Int16Array || val instanceof Int8Array)) {
      return Array.from(val);
    }
    if (Array.isArray(val) && val.length > 0 && val[0] && (val[0] instanceof Uint16Array || val[0] instanceof Uint8Array)) {
      return val.map(v => Array.from(v));
    }
    return val;
  }

  if (typeof handlers.onWriteMultipleRegisters === 'function') {
    const orig = handlers.onWriteMultipleRegisters;
    wrapped.onWriteMultipleRegisters = function(req) {
      if (req && Array.isArray(req.values)) {
        req.values = Uint16Array.from(req.values);
      }
      return orig(req);
    };
  }
  if (typeof handlers.onReadWriteMultipleRegisters === 'function') {
    const orig = handlers.onReadWriteMultipleRegisters;
    wrapped.onReadWriteMultipleRegisters = function(req) {
      if (req && Array.isArray(req.writeValues)) {
        req.writeValues = Uint16Array.from(req.writeValues);
      }
      return convertOutgoing(orig(req));
    };
  }
  if (typeof handlers.onWriteFileRecord === 'function') {
    const orig = handlers.onWriteFileRecord;
    wrapped.onWriteFileRecord = function(req) {
      if (req && Array.isArray(req.requests)) {
        req.requests = req.requests.map(r => {
          if (r && Array.isArray(r.recordData)) {
            return { ...r, recordData: Uint16Array.from(r.recordData) };
          }
          return r;
        });
      }
      return orig(req);
    };
  }
  if (typeof handlers.onDiagnostics === 'function') {
    const orig = handlers.onDiagnostics;
    wrapped.onDiagnostics = async function(req) {
      if (req && Array.isArray(req.data)) {
        req.data = Uint16Array.from(req.data);
      }
      const val = await orig(req);
      if (val && typeof val === 'object' && val.data && (val.data instanceof Uint16Array || val.data instanceof Uint8Array)) {
        return { ...val, data: Array.from(val.data) };
      }
      return val;
    };
  }

  const readHandlers = [
    'onReadCoils',
    'onReadDiscreteInputs',
    'onReadHoldingRegisters',
    'onReadInputRegisters',
    'onReadFifoQueue',
    'onReadFileRecord'
  ];
  for (const name of readHandlers) {
    if (typeof handlers[name] === 'function') {
      const orig = handlers[name];
      wrapped[name] = function(req) {
        return convertOutgoing(orig(req));
      };
    }
  }

  return wrapped;
}

const OriginalAsyncTcpModbusServer = module.exports.AsyncTcpModbusServer;
class AsyncTcpModbusServer extends OriginalAsyncTcpModbusServer {
  static bind(options, handlers) {
    return OriginalAsyncTcpModbusServer.bind(options, wrapServerHandlers(handlers));
  }
}
module.exports.AsyncTcpModbusServer = AsyncTcpModbusServer;

const OriginalAsyncSerialModbusServer = module.exports.AsyncSerialModbusServer;
class AsyncSerialModbusServer extends OriginalAsyncSerialModbusServer {
  static bindRtu(options, handlers) {
    return OriginalAsyncSerialModbusServer.bindRtu(options, wrapServerHandlers(handlers));
  }
  static bindAscii(options, handlers) {
    return OriginalAsyncSerialModbusServer.bindAscii(options, wrapServerHandlers(handlers));
  }
}
module.exports.AsyncSerialModbusServer = AsyncSerialModbusServer;


`;
  if (!js.includes('module.exports.WasmWsTransport')) {
    js += jsExports;
    fs.writeFileSync(jsPath, js, 'utf8');
    console.log('Successfully updated index.js');
  } else {
    console.log('index.js already has exports');
  }
}

// Generate browser/web wrapper files in dist/
const distDir = path.join(__dirname, '../dist');
const wrappers = {
  'index.browser.js': [
    `export * from 'modbus-rs-wasm';`,
    `export default async function init() {}`,
    ``
  ].join('\n'),
  'index.browser.d.ts': [
    `export * from 'modbus-rs-wasm';`,
    `export default function init(): Promise<void>;`,
    ``
  ].join('\n'),
  'index.web.js': [
    `import rawInit, * as wasmExports from 'modbus-rs-wasm/web';`,
    ``,
    `let initPromise = null;`,
    ``,
    `/**`,
    ` * Ensures the WebAssembly module is initialized.`,
    ` * Can be called explicitly or automatically triggered by factory methods.`,
    ` * @param {RequestInfo | URL | Response | BufferSource | WebAssembly.Module} [moduleOrPath] Optional WASM source/URL`,
    ` * @returns {Promise<void>}`,
    ` */`,
    `export function ensureInit(moduleOrPath) {`,
    `  if (!initPromise) {`,
    `    initPromise = rawInit(moduleOrPath).then(() => {}).catch((err) => {`,
    `      initPromise = null;`,
    `      throw err;`,
    `    });`,
    `  }`,
    `  return initPromise;`,
    `}`,
    ``,
    `export default ensureInit;`,
    ``,
    `function wrapStaticMethod(targetClass, methodName) {`,
    `  if (targetClass && typeof targetClass[methodName] === 'function') {`,
    `    const original = targetClass[methodName];`,
    `    targetClass[methodName] = async function (...args) {`,
    `      await ensureInit();`,
    `      return original.apply(this, args);`,
    `    };`,
    `  }`,
    `}`,
    ``,
    `wrapStaticMethod(wasmExports.WasmWsTransport, 'connect');`,
    `wrapStaticMethod(wasmExports.WasmRtuTransport, 'open');`,
    `wrapStaticMethod(wasmExports.WasmAsciiTransport, 'open');`,
    `wrapStaticMethod(wasmExports.WasmWsModbusServer, 'bind');`,
    `wrapStaticMethod(wasmExports.WasmSerialModbusServer, 'bindRtu');`,
    `wrapStaticMethod(wasmExports.WasmSerialModbusServer, 'bindAscii');`,
    ``,
    `export const requestSerialPort = async function (...args) {`,
    `  await ensureInit();`,
    `  return wasmExports.requestSerialPort(...args);`,
    `};`,
    ``,
    `export * from 'modbus-rs-wasm/web';`,
    ``
  ].join('\n'),
  'index.web.d.ts': [
    `import init from 'modbus-rs-wasm/web';`,
    `export function ensureInit(moduleOrPath?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module): Promise<void>;`,
    `export default init;`,
    `export * from 'modbus-rs-wasm/web';`,
    ``
  ].join('\n'),
};
for (const [name, content] of Object.entries(wrappers)) {
  fs.writeFileSync(path.join(distDir, name), content, 'utf8');
  console.log(`Generated ${name}`);
}

// Consolidated WASM postbuild logic (only if wasm output exists in dist/npm/wasm)
const wasmNpmDir = path.join(distDir, 'npm/wasm');
if (fs.existsSync(wasmNpmDir)) {
  const filesToDelete = [
    'dist/web/.gitignore',
    'dist/bundler/.gitignore',
    'dist/web/package.json',
    'dist/bundler/package.json',
    'dist/web/README.md',
    'dist/bundler/README.md',
  ];

  for (const file of filesToDelete) {
    const filePath = path.join(wasmNpmDir, file);
    if (fs.existsSync(filePath)) {
      fs.unlinkSync(filePath);
      console.log(`Deleted ${file}`);
    }
  }

  // Write minimal target-specific README.md and copy LICENSE to dist/npm/wasm/
  const rootDir = path.join(__dirname, '../../..');
  const licenseSrc = path.join(rootDir, 'LICENSE');

  const wasmReadmeContent = [
    `# \`modbus-rs-wasm\``,
    ``,
    `Prebuilt WebAssembly package for [\`modbus-rs\`](https://www.npmjs.com/package/modbus-rs).`,
    ``,
    `Installed automatically by \`modbus-rs\` via \`optionalDependencies\`. You do not need to install this package directly.`,
    ``,
    `License: GPL-3.0-only`,
    ``,
    `Commercial licenses are also available for proprietary use; contact ch.raghava44@gmail.com.`,
    ``
  ].join('\n');
  fs.writeFileSync(path.join(wasmNpmDir, 'README.md'), wasmReadmeContent, 'utf8');
  console.log('Generated minimal WASM README.md');
  if (fs.existsSync(licenseSrc)) {
    fs.copyFileSync(licenseSrc, path.join(wasmNpmDir, 'LICENSE'));
    console.log('Copied LICENSE');
  }

  // Post-process generated .d.ts files to align signatures
  const dtsFiles = [
    'dist/web/modbus-rs.d.ts',
    'dist/bundler/modbus-rs.d.ts',
  ];

  for (const dtsFile of dtsFiles) {
    const filePath = path.join(wasmNpmDir, dtsFile);
    if (fs.existsSync(filePath)) {
      let content = fs.readFileSync(filePath, 'utf8');
      content = content.replace(/close\(\): Promise<any>/g, 'close(): Promise<void>');
      content = content.replace(/reconnect\(\): Promise<any>/g, 'reconnect(): Promise<void>');
      fs.writeFileSync(filePath, content, 'utf8');
      console.log(`Processed and aligned typings in ${dtsFile}`);
    }
  }

  const modbusErrorCodeExport = `
export const ModbusErrorCode = {
  EXCEPTION: 'MODBUS_EXCEPTION',
  TIMEOUT: 'MODBUS_TIMEOUT',
  TRANSPORT: 'MODBUS_TRANSPORT',
  INVALID_ARGUMENT: 'MODBUS_INVALID_ARGUMENT',
  CONNECTION_CLOSED: 'MODBUS_CONNECTION_CLOSED',
  INTERNAL: 'MODBUS_INTERNAL',
};
`;

  const bundlerJsPath = path.join(wasmNpmDir, 'dist/bundler/modbus-rs.js');
  const webJsPath = path.join(wasmNpmDir, 'dist/web/modbus-rs.js');

  const targetsToAppend = [bundlerJsPath, webJsPath];
  let appendedAny = false;

  for (const targetPath of targetsToAppend) {
    if (fs.existsSync(targetPath)) {
      const content = fs.readFileSync(targetPath, 'utf8');
      if (!content.includes('export const ModbusErrorCode')) {
        fs.appendFileSync(targetPath, modbusErrorCodeExport);
        appendedAny = true;
      }
    }
  }

  if (appendedAny) {
    console.log('Appended ModbusErrorCode export to WASM build files.');
  }

  console.log('WASM postbuild tasks complete.');
}

// Force all native packages to un-ignore .node files for npm publish
const npmDistDir = path.join(distDir, 'npm');
if (fs.existsSync(npmDistDir)) {
  const dirs = fs.readdirSync(npmDistDir, { withFileTypes: true })
    .filter(d => d.isDirectory() && d.name !== 'wasm')
    .map(d => d.name);
  for (const d of dirs) {
    const npmignorePath = path.join(npmDistDir, d, '.npmignore');
    fs.writeFileSync(npmignorePath, '!*.node\n', 'utf8');
    console.log(`Generated .npmignore for ${d} to un-ignore .node files`);
  }
}
