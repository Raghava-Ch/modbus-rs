export interface WasmTcpTransportOptions {
  responseTimeoutMs?: number;
  retryAttempts?: number;
}

export declare class WasmTcpTransport {
  static connect(wsUrl: string, opts?: WasmTcpTransportOptions): Promise<WasmTcpTransport>;
  createClient(opts: CreateClientOptions): WasmModbusClient;
  get pendingRequests(): boolean;
  reconnect(): Promise<void>;
  close(): void;
}

export declare class WasmModbusClient {
  readHoldingRegisters(opts: ReadRegistersOptions): Promise<Uint16Array>;
  readInputRegisters(opts: ReadRegistersOptions): Promise<Uint16Array>;
  writeSingleRegister(opts: WriteSingleRegisterOptions): Promise<void>;
  writeMultipleRegisters(opts: WriteMultipleRegistersOptions): Promise<void>;
  readWriteMultipleRegisters(opts: ReadWriteMultipleRegistersOptions): Promise<Uint16Array>;
  readCoils(opts: ReadBitsOptions): Promise<boolean[]>;
  writeSingleCoil(opts: WriteSingleCoilOptions): Promise<void>;
  writeMultipleCoils(opts: WriteMultipleCoilsOptions): Promise<void>;
  readDiscreteInputs(opts: ReadBitsOptions): Promise<boolean[]>;
  readFifoQueue(opts: ReadFifoQueueOptions): Promise<Uint16Array>;
  readFileRecord(opts: ReadFileRecordOptions): Promise<Uint16Array[]>;
  writeFileRecord(opts: WriteFileRecordOptions): Promise<void>;
  readExceptionStatus(): Promise<number>;
  diagnostics(opts: DiagnosticsOptions): Promise<DiagnosticsResponse>;
  readDeviceIdentification(opts: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
  maskWriteRegister(opts: MaskWriteRegisterOptions): Promise<void>;
}

export interface MaskWriteRegisterOptions {
  address: number;
  andMask: number;
  orMask: number;
  signal?: AbortSignal;
}

// Serial transport
export interface WasmSerialTransportOptions {
  mode?: "rtu" | "ascii";
  baudRate: number;
  dataBits?: 5 | 6 | 7 | 8;
  stopBits?: 1 | 2;
  parity?: "none" | "even" | "odd";
  responseTimeoutMs?: number;
  retryAttempts?: number;
}

export declare class WasmSerialPortHandle {
  is_valid(): boolean;
}

export declare function request_serial_port(): Promise<WasmSerialPortHandle>;

export declare class WasmSerialTransport {
  constructor(portHandle: WasmSerialPortHandle, opts?: WasmSerialTransportOptions);
  createClient(opts: CreateClientOptions): WasmSerialModbusClient;
  get pendingRequests(): boolean;
  close(): void;
}

export declare class WasmSerialModbusClient {
  readHoldingRegisters(opts: ReadRegistersOptions): Promise<Uint16Array>;
  readInputRegisters(opts: ReadRegistersOptions): Promise<Uint16Array>;
  writeSingleRegister(opts: WriteSingleRegisterOptions): Promise<void>;
  writeMultipleRegisters(opts: WriteMultipleRegistersOptions): Promise<void>;
  readWriteMultipleRegisters(opts: ReadWriteMultipleRegistersOptions): Promise<Uint16Array>;
  readCoils(opts: ReadBitsOptions): Promise<boolean[]>;
  writeSingleCoil(opts: WriteSingleCoilOptions): Promise<void>;
  writeMultipleCoils(opts: WriteMultipleCoilsOptions): Promise<void>;
  readDiscreteInputs(opts: ReadBitsOptions): Promise<boolean[]>;
  readFifoQueue(opts: ReadFifoQueueOptions): Promise<Uint16Array>;
  readFileRecord(opts: ReadFileRecordOptions): Promise<Uint16Array[]>;
  writeFileRecord(opts: WriteFileRecordOptions): Promise<void>;
  readExceptionStatus(): Promise<number>;
  diagnostics(opts: DiagnosticsOptions): Promise<DiagnosticsResponse>;
  readDeviceIdentification(opts: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
  maskWriteRegister(opts: MaskWriteRegisterOptions): Promise<void>;
}

// Shared interfaces
export interface CreateClientOptions {
  unitId: number;
}
export interface ReadRegistersOptions {
  address: number;
  quantity: number;
  signal?: AbortSignal;
}
export interface WriteSingleRegisterOptions {
  address: number;
  value: number;
  signal?: AbortSignal;
}
export interface WriteMultipleRegistersOptions {
  address: number;
  values: number[] | Uint16Array;
  signal?: AbortSignal;
}
export interface ReadWriteMultipleRegistersOptions {
  readAddress: number;
  readQuantity: number;
  writeAddress: number;
  writeValues: number[] | Uint16Array;
  signal?: AbortSignal;
}
export interface ReadBitsOptions {
  address: number;
  quantity: number;
  signal?: AbortSignal;
}
export interface WriteSingleCoilOptions {
  address: number;
  value: boolean;
  signal?: AbortSignal;
}
export interface WriteMultipleCoilsOptions {
  address: number;
  values: boolean[];
  signal?: AbortSignal;
}
export interface ReadFifoQueueOptions {
  address: number;
  signal?: AbortSignal;
}
export interface FileRecordReadRequest {
  fileNumber: number;
  recordNumber: number;
  recordLength: number;
}
export interface ReadFileRecordOptions {
  requests: FileRecordReadRequest[];
  signal?: AbortSignal;
}
export interface FileRecordWriteRequest {
  fileNumber: number;
  recordNumber: number;
  recordData: number[] | Uint16Array;
}
export interface WriteFileRecordOptions {
  requests: FileRecordWriteRequest[];
  signal?: AbortSignal;
}
export interface DiagnosticsOptions {
  subFunction: number;
  data: number[] | Uint16Array;
  signal?: AbortSignal;
}
export interface DiagnosticsResponse {
  subFunction: number;
  data: Uint16Array;
}
export interface ReadDeviceIdentificationOptions {
  readDeviceIdCode: number;
  objectId: number;
  signal?: AbortSignal;
}
export interface DeviceIdentificationObject {
  id: number;
  value: string;
}
export interface DeviceIdentificationResponse {
  conformityLevel: number;
  moreFollows: boolean;
  nextObjectId: number;
  objects: DeviceIdentificationObject[];
}
