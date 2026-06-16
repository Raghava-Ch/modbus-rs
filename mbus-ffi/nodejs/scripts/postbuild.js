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

export interface ServerHandlers {
  onReadCoils?: (req: ReadCoilsRequest) => boolean[] | ModbusException | Promise<boolean[] | ModbusException>;
  onReadDiscreteInputs?: (req: ReadDiscreteInputsRequest) => boolean[] | ModbusException | Promise<boolean[] | ModbusException>;
  onReadHoldingRegisters?: (req: ReadHoldingRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;
  onReadInputRegisters?: (req: ReadInputRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;
  onWriteSingleCoil?: (req: WriteSingleCoilRequest) => void | ModbusException | Promise<void | ModbusException>;
  onWriteSingleRegister?: (req: WriteSingleRegisterRequest) => void | ModbusException | Promise<void | ModbusException>;
  onReadExceptionStatus?: (req: ReadExceptionStatusRequest) => number | ModbusException | Promise<number | ModbusException>;
  onDiagnostics?: (req: DiagnosticsRequest) => ServerDiagnosticsResponse | ModbusException | Promise<ServerDiagnosticsResponse | ModbusException>;
  onWriteMultipleCoils?: (req: WriteMultipleCoilsRequest) => void | ModbusException | Promise<void | ModbusException>;
  onWriteMultipleRegisters?: (req: WriteMultipleRegistersRequest) => void | ModbusException | Promise<void | ModbusException>;
  onReadFileRecord?: (req: ReadFileRecordRequest) => number[][] | ModbusException | Promise<number[][] | ModbusException>;
  onWriteFileRecord?: (req: WriteFileRecordRequest) => void | ModbusException | Promise<void | ModbusException>;
  onReadWriteMultipleRegisters?: (req: ReadWriteMultipleRegistersRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;
  onReadFifoQueue?: (req: ReadFifoQueueRequest) => number[] | ModbusException | Promise<number[] | ModbusException>;
}

export declare const ModbusErrorCode: {
  readonly EXCEPTION: 'MODBUS_EXCEPTION';
  readonly TIMEOUT: 'MODBUS_TIMEOUT';
  readonly TRANSPORT: 'MODBUS_TRANSPORT';
  readonly INVALID_ARGUMENT: 'MODBUS_INVALID_ARGUMENT';
  readonly CONNECTION_CLOSED: 'MODBUS_CONNECTION_CLOSED';
  readonly INTERNAL: 'MODBUS_INTERNAL';
};

export declare function getModbusErrorCode(err: Error): string | undefined;
`;

dts += extraDeclarations;
fs.writeFileSync(dtsPath, dts, 'utf8');
console.log('Successfully updated index.d.ts');

if (fs.existsSync(jsPath)) {
  let js = fs.readFileSync(jsPath, 'utf8');

  // 6. Append helper exports to index.js
  const jsExports = `
// Error code constants
module.exports.ModbusErrorCode = {
  EXCEPTION: 'MODBUS_EXCEPTION',
  TIMEOUT: 'MODBUS_TIMEOUT',
  TRANSPORT: 'MODBUS_TRANSPORT',
  INVALID_ARGUMENT: 'MODBUS_INVALID_ARGUMENT',
  CONNECTION_CLOSED: 'MODBUS_CONNECTION_CLOSED',
  INTERNAL: 'MODBUS_INTERNAL',
}

// Helper to extract the code from a Modbus error message
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
