// TypeScript type definitions for modbus-rs Node.js bindings

/** Connection options for the TCP client. */
export interface TcpClientOptions {
  /** Server hostname or IP address. */
  host: string;
  /** Server TCP port (default: 502). */
  port: number;
  /** Modbus unit ID / slave address (1-247). */
  unitId: number;
  /** Request timeout in milliseconds (optional). */
  timeoutMs?: number;
}

/** Options for reading coils (FC01) or discrete inputs (FC02). */
export interface ReadBitsOptions {
  /** Starting address (0-65535). */
  address: number;
  /** Number of bits to read (1-2000). */
  quantity: number;
}

/** Options for reading holding registers (FC03) or input registers (FC04). */
export interface ReadRegistersOptions {
  /** Starting address (0-65535). */
  address: number;
  /** Number of registers to read (1-125). */
  quantity: number;
}

/** Options for writing a single coil (FC05). */
export interface WriteSingleCoilOptions {
  /** Coil address (0-65535). */
  address: number;
  /** Value to write (true = ON, false = OFF). */
  value: boolean;
}

/** Options for writing a single register (FC06). */
export interface WriteSingleRegisterOptions {
  /** Register address (0-65535). */
  address: number;
  /** Value to write (0-65535). */
  value: number;
}

/** Options for writing multiple coils (FC15). */
export interface WriteMultipleCoilsOptions {
  /** Starting address (0-65535). */
  address: number;
  /** Coil values to write. */
  values: boolean[];
}

/** Options for writing multiple registers (FC16). */
export interface WriteMultipleRegistersOptions {
  /** Starting address (0-65535). */
  address: number;
  /** Register values to write. */
  values: number[];
}

/** Options for read/write multiple registers (FC23). */
export interface ReadWriteMultipleRegistersOptions {
  /** Starting address for reading. */
  readAddress: number;
  /** Number of registers to read (1-125). */
  readQuantity: number;
  /** Starting address for writing. */
  writeAddress: number;
  /** Values to write. */
  writeValues: number[];
}

/** Options for reading file record (FC20). */
export interface ReadFileRecordOptions {
  /** File number. */
  fileNumber: number;
  /** Starting record number. */
  recordNumber: number;
  /** Number of records to read. */
  recordLength: number;
}

/** File record data. */
export interface FileRecordData {
  /** File number. */
  fileNumber: number;
  /** Record number. */
  recordNumber: number;
  /** Record data. */
  data: number[];
}

/** Options for writing file record (FC21). */
export interface WriteFileRecordOptions {
  /** File number. */
  fileNumber: number;
  /** Starting record number. */
  recordNumber: number;
  /** Data to write. */
  data: number[];
}

/** Options for reading FIFO queue (FC24). */
export interface ReadFifoQueueOptions {
  /** FIFO pointer address. */
  address: number;
}

/** FIFO queue response. */
export interface ReadFifoQueueResponse {
  /** Queue count. */
  count: number;
  /** Queue values. */
  values: number[];
}

/** Options for diagnostics (FC08). */
export interface DiagnosticsOptions {
  /** Diagnostics sub-function code. */
  subFunction: number;
  /** Data for the sub-function. */
  data: number;
}

/** Diagnostics response. */
export interface DiagnosticsResponse {
  /** Sub-function code. */
  subFunction: number;
  /** Response data. */
  data: number;
}

/** Options for read device identification (FC43/14). */
export interface ReadDeviceIdentificationOptions {
  /** Read device ID code (1=Basic, 2=Regular, 3=Extended, 4=Individual). */
  readDeviceIdCode: number;
  /** Starting object ID. */
  objectId: number;
}

/** Device identification object. */
export interface DeviceIdentificationObject {
  /** Object ID. */
  id: number;
  /** Object value. */
  value: string;
}

/** Device identification response. */
export interface DeviceIdentificationResponse {
  /** Conformity level. */
  conformityLevel: number;
  /** More objects follow flag. */
  moreFollows: boolean;
  /** Next object ID. */
  nextObjectId: number;
  /** List of objects. */
  objects: DeviceIdentificationObject[];
}

/** Async TCP Modbus client. */
export class AsyncTcpModbusClient {
  /** Creates and connects a new TCP client. */
  static connect(opts: TcpClientOptions): Promise<AsyncTcpModbusClient>;
  
  /** Closes the connection. */
  close(): Promise<void>;
  
  // Function Code 01 - Read Coils
  readCoils(opts: ReadBitsOptions): Promise<boolean[]>;
  
  // Function Code 02 - Read Discrete Inputs
  readDiscreteInputs(opts: ReadBitsOptions): Promise<boolean[]>;
  
  // Function Code 03 - Read Holding Registers
  readHoldingRegisters(opts: ReadRegistersOptions): Promise<number[]>;
  
  // Function Code 04 - Read Input Registers
  readInputRegisters(opts: ReadRegistersOptions): Promise<number[]>;
  
  // Function Code 05 - Write Single Coil
  writeSingleCoil(opts: WriteSingleCoilOptions): Promise<void>;
  
  // Function Code 06 - Write Single Register
  writeSingleRegister(opts: WriteSingleRegisterOptions): Promise<void>;
  
  // Function Code 15 - Write Multiple Coils
  writeMultipleCoils(opts: WriteMultipleCoilsOptions): Promise<void>;
  
  // Function Code 16 - Write Multiple Registers
  writeMultipleRegisters(opts: WriteMultipleRegistersOptions): Promise<void>;
  
  // Function Code 23 - Read/Write Multiple Registers
  readWriteMultipleRegisters(opts: ReadWriteMultipleRegistersOptions): Promise<number[]>;
  
  // Function Code 20 - Read File Record
  readFileRecord(opts: ReadFileRecordOptions): Promise<FileRecordData[]>;
  
  // Function Code 21 - Write File Record
  writeFileRecord(opts: WriteFileRecordOptions): Promise<void>;
  
  // Function Code 24 - Read FIFO Queue
  readFifoQueue(opts: ReadFifoQueueOptions): Promise<ReadFifoQueueResponse>;
  
  // Function Code 07 - Read Exception Status
  readExceptionStatus(): Promise<number>;
  
  // Function Code 08 - Diagnostics
  diagnostics(opts: DiagnosticsOptions): Promise<DiagnosticsResponse>;
  
  // Function Code 43/14 - Read Device Identification
  readDeviceIdentification(opts: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
}

/** Connection options for the serial client. */
export interface SerialClientOptions {
  /** Serial port path (e.g., "/dev/ttyUSB0" or "COM1"). */
  portPath: string;
  /** Modbus unit ID / slave address (1-247). */
  unitId: number;
  /** Baud rate (default: 19200). */
  baudRate?: number;
  /** Data bits (5, 6, 7, or 8, default: 8). */
  dataBits?: number;
  /** Stop bits (1 or 2, default: 1). */
  stopBits?: number;
  /** Parity ("none", "even", or "odd", default: "even"). */
  parity?: string;
  /** Request timeout in milliseconds (optional). */
  requestTimeoutMs?: number;
  /** Number of retry attempts (optional). */
  retryAttempts?: number;
  /** Backoff strategy ("none", "fixed", or "exponential", default: "fixed"). */
  backoffStrategy?: string;
  /** Backoff delay in milliseconds (optional). */
  backoffDelayMs?: number;
  /** Jitter strategy ("none", "full", or "equal", default: "none"). */
  jitterStrategy?: string;
}

/** Async Serial Modbus client (RTU/ASCII). */
export class AsyncSerialModbusClient {
  /** Creates and connects a new Serial RTU client. */
  static connectRtu(opts: SerialClientOptions): Promise<AsyncSerialModbusClient>;
  
  /** Creates and connects a new Serial ASCII client. */
  static connectAscii(opts: SerialClientOptions): Promise<AsyncSerialModbusClient>;
  
  /** Closes the connection. */
  close(): Promise<void>;
  
  // Same methods as AsyncTcpModbusClient
  readCoils(opts: ReadBitsOptions): Promise<boolean[]>;
  readDiscreteInputs(opts: ReadBitsOptions): Promise<boolean[]>;
  readHoldingRegisters(opts: ReadRegistersOptions): Promise<number[]>;
  readInputRegisters(opts: ReadRegistersOptions): Promise<number[]>;
  writeSingleCoil(opts: WriteSingleCoilOptions): Promise<void>;
  writeSingleRegister(opts: WriteSingleRegisterOptions): Promise<void>;
  writeMultipleCoils(opts: WriteMultipleCoilsOptions): Promise<void>;
  writeMultipleRegisters(opts: WriteMultipleRegistersOptions): Promise<void>;
  readWriteMultipleRegisters(opts: ReadWriteMultipleRegistersOptions): Promise<number[]>;
  readFileRecord(opts: ReadFileRecordOptions): Promise<FileRecordData[]>;
  writeFileRecord(opts: WriteFileRecordOptions): Promise<void>;
  readFifoQueue(opts: ReadFifoQueueOptions): Promise<ReadFifoQueueResponse>;
  readExceptionStatus(): Promise<number>;
  diagnostics(opts: DiagnosticsOptions): Promise<DiagnosticsResponse>;
  readDeviceIdentification(opts: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
}

/** Server bind options. */
export interface ServerBindOptions {
  /** Bind host address (e.g., "0.0.0.0"). */
  host: string;
  /** Bind port. */
  port: number;
}

/** Read coils request handler input. */
export interface ReadCoilsRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Number of coils to read. */
  quantity: number;
}

/** Write single coil request handler input. */
export interface WriteSingleCoilRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Coil address. */
  address: number;
  /** Value to write. */
  value: boolean;
}

/** Write multiple coils request handler input. */
export interface WriteMultipleCoilsRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Values to write. */
  values: boolean[];
}

/** Read discrete inputs request handler input. */
export interface ReadDiscreteInputsRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Number of discrete inputs to read. */
  quantity: number;
}

/** Read holding registers request handler input. */
export interface ReadHoldingRegistersRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Number of registers to read. */
  quantity: number;
}

/** Read input registers request handler input. */
export interface ReadInputRegistersRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Number of registers to read. */
  quantity: number;
}

/** Write single register request handler input. */
export interface WriteSingleRegisterRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Register address. */
  address: number;
  /** Value to write. */
  value: number;
}

/** Write multiple registers request handler input. */
export interface WriteMultipleRegistersRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Starting address. */
  address: number;
  /** Values to write. */
  values: number[];
}

/** Read FIFO queue request handler input. */
export interface ReadFifoQueueRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Pointer address. */
  pointerAddress: number;
}

/** Diagnostics request handler input. */
export interface DiagnosticsRequest {
  /** Unit ID / slave address. */
  unitId: number;
  /** Sub-function code. */
  subFunction: number;
  /** Data. */
  data: number;
}

/** Server handler options. */
export interface ServerHandlerOptions {
  /** Handler for read coils requests. */
  onReadCoils?: (req: ReadCoilsRequest) => boolean[] | null;
  /** Handler for write single coil requests. */
  onWriteSingleCoil?: (req: WriteSingleCoilRequest) => boolean;
  /** Handler for write multiple coils requests. */
  onWriteMultipleCoils?: (req: WriteMultipleCoilsRequest) => boolean;
  /** Handler for read discrete inputs requests. */
  onReadDiscreteInputs?: (req: ReadDiscreteInputsRequest) => boolean[] | null;
  /** Handler for read holding registers requests. */
  onReadHoldingRegisters?: (req: ReadHoldingRegistersRequest) => number[] | null;
  /** Handler for read input registers requests. */
  onReadInputRegisters?: (req: ReadInputRegistersRequest) => number[] | null;
  /** Handler for write single register requests. */
  onWriteSingleRegister?: (req: WriteSingleRegisterRequest) => boolean;
  /** Handler for write multiple registers requests. */
  onWriteMultipleRegisters?: (req: WriteMultipleRegistersRequest) => boolean;
  /** Handler for read FIFO queue requests. */
  onReadFifoQueue?: (req: ReadFifoQueueRequest) => number[] | null;
  /** Handler for diagnostics requests. */
  onDiagnostics?: (req: DiagnosticsRequest) => number;
}

/** Async TCP Modbus server. */
export class AsyncTcpModbusServer {
  /** Creates and binds a new TCP server. */
  static bind(opts: ServerBindOptions, handlers: ServerHandlerOptions): Promise<AsyncTcpModbusServer>;
  
  /** Stops the server. */
  shutdown(): Promise<void>;
}

/** Gateway bind options. */
export interface GatewayBindOptions {
  /** Bind host address (e.g., "0.0.0.0"). */
  host: string;
  /** Bind port. */
  port: number;
}

/** Downstream server configuration. */
export interface DownstreamConfig {
  /** Downstream server host. */
  host: string;
  /** Downstream server port. */
  port: number;
}

/** Route entry mapping unit ID to a downstream channel. */
export interface RouteEntry {
  /** Modbus unit ID (1-247). */
  unitId: number;
  /** Index into the downstreams array. */
  channel: number;
}

/** Gateway configuration. */
export interface GatewayConfig {
  /** List of downstream servers. */
  downstreams: DownstreamConfig[];
  /** Routing table mapping unit IDs to downstream channels. */
  routes: RouteEntry[];
}

/** Async Modbus TCP gateway. */
export class AsyncTcpGateway {
  /** Creates and starts a new TCP gateway. */
  static bind(opts: GatewayBindOptions, config: GatewayConfig): Promise<AsyncTcpGateway>;
  
  /** Stops the gateway. */
  shutdown(): Promise<void>;
}
