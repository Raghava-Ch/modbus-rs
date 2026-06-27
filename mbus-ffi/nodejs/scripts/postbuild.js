const fs = require('fs');
const path = require('path');

const dtsPath = path.join(__dirname, '../index.d.ts');
const jsPath = path.join(__dirname, '../index.js');

if (!fs.existsSync(dtsPath)) {
  console.error('index.d.ts not found');
  process.exit(1);
}

let dts = fs.readFileSync(dtsPath, 'utf8');

// 1. Replace signal?: object with signal?: AbortSignal
dts = dts.replace(/signal\?: object/g, 'signal?: AbortSignal');

// 2. Replace handlers: object with handlers: ServerHandlers in bind methods
dts = dts.replace(/static bindRtu\(opts: SerialServerOptions, handlers: object\)/g, 'static bindRtu(opts: SerialServerOptions, handlers: ServerHandlers)');
dts = dts.replace(/static bindAscii\(opts: SerialServerOptions, handlers: object\)/g, 'static bindAscii(opts: SerialServerOptions, handlers: ServerHandlers)');
dts = dts.replace(/static bind\(opts: TcpServerOptions, handlers: object\)/g, 'static bind(opts: TcpServerOptions, handlers: ServerHandlers)');

// 4. Remove count: number from FifoQueueResponse interface
dts = dts.replace(
  /export interface FifoQueueResponse {[\s\S]*?count: number[\s\S]*?}/,
  `export interface FifoQueueResponse {
  /** Queue values. */
  values: Array<number>
}`
);

// 5. Append additional declarations
const extraDeclarations = `
export interface ModbusException {
  exception: number;
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
  onReadCoils?: (req: ReadCoilsRequest) => boolean[] | ModbusException | Promise<boolean[] | ModbusException>;

  /**
   * Handle Read Discrete Inputs (FC 02).
   * @param req The request object.
   * @param req.address The starting discrete input address (0x0000 to 0xFFFF).
   * @param req.quantity The number of inputs to read (1 to 2000).
   * @returns An array of booleans representing the input states, or a ModbusException.
   */
  onReadDiscreteInputs?: (req: ReadDiscreteInputsRequest) => boolean[] | ModbusException | Promise<boolean[] | ModbusException>;

  /**
   * Handle Read Holding Registers (FC 03).
   * @param req The request object.
   * @param req.address The starting holding register address (0x0000 to 0xFFFF).
   * @param req.quantity The number of registers to read (1 to 125).
   * @returns An array of 16-bit numbers representing the registers, or a ModbusException.
   */
  onReadHoldingRegisters?: (req: ReadHoldingRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;

  /**
   * Handle Read Input Registers (FC 04).
   * @param req The request object.
   * @param req.address The starting input register address (0x0000 to 0xFFFF).
   * @param req.quantity The number of registers to read (1 to 125).
   * @returns An array of 16-bit numbers representing the registers, or a ModbusException.
   */
  onReadInputRegisters?: (req: ReadInputRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;

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
   * @param req.subRequests An array of sub-requests, each with \`fileNumber\`, \`recordNumber\`, and \`recordLength\`.
   * @returns An array of register arrays for each sub-request, or a ModbusException.
   */
  onReadFileRecord?: (req: ReadFileRecordRequest) => number[][] | ModbusException | Promise<number[][] | ModbusException>;

  /**
   * Handle Write File Record (FC 21).
   * @param req The request object.
   * @param req.subRequests An array of sub-requests, each with \`fileNumber\`, \`recordNumber\`, and \`recordData\` (a number array).
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
  onReadWriteMultipleRegisters?: (req: ReadWriteMultipleRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;

  /**
   * Handle Read FIFO Queue (FC 24).
   * @param req The request object containing the FIFO pointer \`address\`.
   * @returns An array of 16-bit numbers from the queue, or a ModbusException.
   */
  onReadFifoQueue?: (req: ReadFifoQueueRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;
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
`;

dts += extraDeclarations;
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
`;
  if (!js.includes('module.exports.ModbusErrorCode')) {
    js += jsExports;
    fs.writeFileSync(jsPath, js, 'utf8');
    console.log('Successfully updated index.js');
  } else {
    console.log('index.js already has exports');
  }
}
