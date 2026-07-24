//! Shared wasm-bindgen types for the WASM target.
//! Provides TypeScript interface definitions and extern type mappings.
#![cfg(target_arch = "wasm32")]

use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

/// Represents a Modbus coil or discrete input state.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoilState {
    /// Coil is OFF (wire value: 0).
    Off = 0,
    /// Coil is ON (wire value: 1).
    On = 1,
}

// ── 1. TypeScript custom sections (class overrides and server callbacks) ──────

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &str = r#"
/** Represents a Modbus discrete input pin state (alias for CoilState). */
export type DiscreteInputState = CoilState;
export const DiscreteInputState: typeof CoilState;

/** Modbus exception returned from a server handler. */
export interface ModbusException {
    /** The Modbus exception code. */
    exceptionCode: ModbusExceptionCode | number;
}

// ── Server Request Interfaces ──────────────────────────────────────────────────

/** Request parameters for reading coils (Function Code 01). */
export interface ReadCoilsRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the coils to read (0-based). */
    address: number;
    /** The number of coils to read. */
    quantity: number;
}

/** Request parameters for reading discrete inputs (Function Code 02). */
export interface ReadDiscreteInputsRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the discrete inputs to read (0-based). */
    address: number;
    /** The number of discrete inputs to read. */
    quantity: number;
}

/** Request parameters for reading holding registers (Function Code 03). */
export interface ReadHoldingRegistersRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the holding registers to read (0-based). */
    address: number;
    /** The number of holding registers to read. */
    quantity: number;
}

/** Request parameters for reading input registers (Function Code 04). */
export interface ReadInputRegistersRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the input registers to read (0-based). */
    address: number;
    /** The number of input registers to read. */
    quantity: number;
}

/** Request parameters for writing a single coil (Function Code 05). */
export interface WriteSingleCoilRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The target address of the coil to write (0-based). */
    address: number;
    /** The coil state to write (`CoilState.On` or `CoilState.Off`). */
    value: CoilState;
}

/** Request parameters for writing a single register (Function Code 06). */
export interface WriteSingleRegisterRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The target address of the register to write (0-based). */
    address: number;
    /** The 16-bit register value to write. */
    value: number;
}

/** Request parameters for reading the exception status (Function Code 07). */
export interface ReadExceptionStatusRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
}

/** Request parameters for executing diagnostics (Function Code 08). */
export interface DiagnosticsRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The diagnostic sub-function code. */
    subFunction: number;
    /** The data payload for the diagnostic function. */
    data: Uint16Array;
}

/** Request parameters for writing multiple coils (Function Code 15). */
export interface WriteMultipleCoilsRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the coils to write (0-based). */
    address: number;
    /** The array of coil states to write. */
    values: CoilState[];
}

/** Request parameters for writing multiple registers (Function Code 16). */
export interface WriteMultipleRegistersRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the registers to write (0-based). */
    address: number;
    /** The array of 16-bit register values to write. */
    values: Uint16Array;
}

export interface FileRecordReadServerSubRequest {
    fileNumber: number;
    recordNumber: number;
    recordLength: number;
}

export interface FileRecordWriteSubRequest {
    fileNumber: number;
    recordNumber: number;
    recordData: Uint16Array;
}

/** Request parameters for reading one or more file records (Function Code 14). */
export interface ReadFileRecordRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The list of sub-request objects containing file number, record number, and record length. */
    requests: FileRecordReadServerSubRequest[];
}

/** Request parameters for writing one or more file records (Function Code 15). */
export interface WriteFileRecordRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The list of sub-request objects containing file number, record number, and record data. */
    requests: FileRecordWriteSubRequest[];
}

/** Request parameters for atomic read and write of holding registers (Function Code 23). */
export interface ReadWriteMultipleRegistersRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address for the read operation. */
    readAddress: number;
    /** The number of registers to read. */
    readQuantity: number;
    /** The starting address for the write operation. */
    writeAddress: number;
    /** The array of 16-bit values to write. */
    writeValues: Uint16Array;
}

/** Request parameters for reading a FIFO queue (Function Code 24). */
export interface ReadFifoQueueRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The starting address of the FIFO queue. */
    address: number;
}

/** Request parameters for reading device identification (MEI FC43/14). */
export interface ReadDeviceIdentificationRequest {
    /** The unit ID of the device that sent the request. */
    unitId: number;
    /** The read device ID code (1, 2, 3, or 4). */
    readDeviceIdCode: number;
    /** The starting object ID to read (0 to 255). */
    objectId: number;
}

/** Response payload returned from a diagnostic request handler. */
export interface ServerDiagnosticsResponse {
    /** The diagnostic sub-function code. */
    subFunction: number;
    /** The response data payload. */
    data: Uint16Array;
}

// ── ServerHandlers Interface ───────────────────────────────────────────────────

/**
 * Interface containing callback handlers for processing incoming Modbus requests.
 *
 * Implement these callbacks to define the custom behavior of a Modbus server.
 */
export interface ServerHandlers {
    /**
     * Callback to handle read coils requests (FC01).
     *
     * @param {ReadCoilsRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting coil address.
     * @param {number} req.quantity - Number of coils to read.
     */
    onReadCoils?: (req: ReadCoilsRequest) => CoilState[] | ModbusException | Promise<CoilState[] | ModbusException>;
    /**
     * Callback to handle read discrete inputs requests (FC02).
     *
     * @param {ReadDiscreteInputsRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting input address.
     * @param {number} req.quantity - Number of inputs to read.
     */
    onReadDiscreteInputs?: (req: ReadDiscreteInputsRequest) => DiscreteInputState[] | ModbusException | Promise<DiscreteInputState[] | ModbusException>;
    /**
     * Callback to handle read holding registers requests (FC03).
     *
     * @param {ReadHoldingRegistersRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting register address.
     * @param {number} req.quantity - Number of registers to read.
     */
    onReadHoldingRegisters?: (req: ReadHoldingRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;
    /**
     * Callback to handle read input registers requests (FC04).
     *
     * @param {ReadInputRegistersRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting register address.
     * @param {number} req.quantity - Number of registers to read.
     */
    onReadInputRegisters?: (req: ReadInputRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;
    /**
     * Callback to handle write single coil requests (FC05).
     *
     * @param {WriteSingleCoilRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Target coil address.
     * @param {boolean} req.value - State to write.
     */
    onWriteSingleCoil?: (req: WriteSingleCoilRequest) => void | ModbusException | Promise<void | ModbusException>;
    /**
     * Callback to handle write single register requests (FC06).
     *
     * @param {WriteSingleRegisterRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Target register address.
     * @param {number} req.value - Value to write.
     */
    onWriteSingleRegister?: (req: WriteSingleRegisterRequest) => void | ModbusException | Promise<void | ModbusException>;
    /**
     * Callback to handle read exception status requests (FC07).
     *
     * @param {ReadExceptionStatusRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     */
    onReadExceptionStatus?: (req: ReadExceptionStatusRequest) => number | ModbusException | Promise<number | ModbusException>;
    /**
     * Callback to handle diagnostics requests (FC08).
     *
     * @param {DiagnosticsRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.subFunction - Sub-function code.
     * @param {number[]} req.data - Request payload.
     */
    onDiagnostics?: (req: DiagnosticsRequest) => ServerDiagnosticsResponse | ModbusException | Promise<ServerDiagnosticsResponse | ModbusException>;
    /**
     * Callback to handle write multiple coils requests (FC15).
     *
     * @param {WriteMultipleCoilsRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting coil address.
     * @param {boolean[]} req.values - States to write.
     */
    onWriteMultipleCoils?: (req: WriteMultipleCoilsRequest) => void | ModbusException | Promise<void | ModbusException>;
    /**
     * Callback to handle write multiple registers requests (FC16).
     *
     * @param {WriteMultipleRegistersRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Starting register address.
     * @param {number[]} req.values - Values to write.
     */
    onWriteMultipleRegisters?: (req: WriteMultipleRegistersRequest) => void | ModbusException | Promise<void | ModbusException>;
    /**
     * Callback to handle read file record requests (FC14).
     *
     * @param {ReadFileRecordRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {object[]} req.subRequests - List of sub-request objects.
     */
    onReadFileRecord?: (req: ReadFileRecordRequest) => Uint16Array[] | ModbusException | Promise<Uint16Array[] | ModbusException>;
    /**
     * Callback to handle write file record requests (FC15).
     *
     * @param {WriteFileRecordRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {object[]} req.subRequests - List of sub-request objects.
     */
    onWriteFileRecord?: (req: WriteFileRecordRequest) => void | ModbusException | Promise<void | ModbusException>;
    /**
     * Callback to handle read/write multiple registers requests (FC23).
     *
     * @param {ReadWriteMultipleRegistersRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.readAddress - Starting address for read.
     * @param {number} req.readQuantity - Number of registers to read.
     * @param {number} req.writeAddress - Starting address for write.
     * @param {Uint16Array} req.writeValues - Values to write.
     */
    onReadWriteMultipleRegisters?: (req: ReadWriteMultipleRegistersRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;
    /**
     * Callback to handle read FIFO queue requests (FC24).
     *
     * @param {ReadFifoQueueRequest} req - The request parameters.
     * @param {number} req.unitId - Target device address.
     * @param {number} req.address - Target FIFO queue address.
     */
    onReadFifoQueue?: (req: ReadFifoQueueRequest) => Uint16Array | ModbusException | Promise<Uint16Array | ModbusException>;
    /**
     * Callback to handle read device identification requests (FC43).
     */
    onReadDeviceIdentification?: (req: ReadDeviceIdentificationRequest) => DeviceIdentificationResponse | ModbusException | Promise<DeviceIdentificationResponse | ModbusException>;
}

// ── Typed overrides for WasmWsModbusClient & WasmSerialModbusClient ──────────────

/**
 * A browser-facing Modbus client bound to a specific unit ID (slave address).
 *
 * This class provides methods for all standard Modbus function codes. All operations
 * are asynchronous and return a `Promise`.
 */
export declare class WasmWsModbusClient {
    /**
     * Reads a sequence of coils (Function Code 01).
     *
     * @param {ReadBitsOptions} options - The request parameters.
     * @param {number} options.address - Starting coil address (0-based).
     * @param {number} options.quantity - Number of coils to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<CoilState[]>} A promise that resolves to an array representing the coil states.
     *
     * @example
     * ```javascript
     * const coils = await client.readCoils({ address: 0, quantity: 8 });
     * console.log(coils); // e.g., [CoilState.On, CoilState.Off, ...]
     * ```
     */
    readCoils(options: ReadBitsOptions): Promise<CoilState[]>;
    /**
     * Reads a sequence of discrete inputs (Function Code 02).
     *
     * These are read-only boolean inputs.
     *
     * @param {ReadBitsOptions} options - The request parameters.
     * @param {number} options.address - Starting input address (0-based).
     * @param {number} options.quantity - Number of inputs to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DiscreteInputState[]>} A promise that resolves to an array of states.
     *
     * @example
     * ```javascript
     * const inputs = await client.readDiscreteInputs({ address: 0, quantity: 4 });
     * ```
     */
    readDiscreteInputs(options: ReadBitsOptions): Promise<DiscreteInputState[]>;
    /**
     * Reads a sequence of holding registers (Function Code 03).
     *
     * These are 16-bit read/write registers.
     *
     * @param {ReadRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {number} options.quantity - Number of registers to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
     *
     * @example
     * ```javascript
     * const regs = await client.readHoldingRegisters({ address: 100, quantity: 10 });
     * console.log(regs); // Access the first register value
     * ```
     */
    readHoldingRegisters(options: ReadRegistersOptions): Promise<Uint16Array>;
    /**
     * Reads a sequence of input registers (Function Code 04).
     *
     * These are 16-bit read-only registers.
     *
     * @param {ReadRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {number} options.quantity - Number of registers to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
     *
     * @example
     * ```javascript
     * const inputRegs = await client.readInputRegisters({ address: 50, quantity: 2 });
     * ```
     */
    readInputRegisters(options: ReadRegistersOptions): Promise<Uint16Array>;
    /**
     * Writes a single coil state (Function Code 05).
     *
     * @param {WriteSingleCoilOptions} options - The request parameters.
     * @param {number} options.address - Target coil address (0-based).
     * @param {boolean} options.value - State to write (`true` for ON, `false` for OFF).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeSingleCoil({ address: 10, value: true });
     * ```
     */
    writeSingleCoil(options: WriteSingleCoilOptions): Promise<void>;
    /**
     * Writes a single holding register (Function Code 06).
     *
     * @param {WriteSingleRegisterOptions} options - The request parameters.
     * @param {number} options.address - Target register address (0-based).
     * @param {number} options.value - 16-bit value to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeSingleRegister({ address: 100, value: 42 });
     * ```
     */
    writeSingleRegister(options: WriteSingleRegisterOptions): Promise<void>;
    /**
     * Writes a sequence of coil states (Function Code 15).
     *
     * @param {WriteMultipleCoilsOptions} options - The request parameters.
     * @param {number} options.address - Starting coil address (0-based).
     * @param {boolean[]} options.values - Array of boolean states to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeMultipleCoils({ address: 20, values: [true, false, true, true] });
     * ```
     */
    writeMultipleCoils(options: WriteMultipleCoilsOptions): Promise<void>;
    /**
     * Writes a sequence of holding registers (Function Code 16).
     *
     * @param {WriteMultipleRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {Uint16Array} options.values - Array of 16-bit values to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeMultipleRegisters({ address: 200, values: new Uint16Array([1, 2, 3]) });
     * ```
     */
    writeMultipleRegisters(options: WriteMultipleRegistersOptions): Promise<void>;
    /**
     * Modifies a single holding register using a bitwise AND/OR mask (Function Code 22).
     *
     * The operation is `(current_value AND andMask) OR (orMask AND (NOT andMask))`.
     *
     * @param {MaskWriteRegisterOptions} options - The request parameters.
     * @param {number} options.address - Target register address.
     * @param {number} options.andMask - Bitwise AND mask.
     * @param {number} options.orMask - Bitwise OR mask.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the operation is complete.
     *
     * @example
     * ```javascript
     * // Set bits 0-7 and clear bits 8-15 of the register at address 300
     * await client.maskWriteRegister({
     *   address: 300, andMask: 0x00FF, orMask: 0xFF00
     * });
     * ```
     */
    maskWriteRegister(options: MaskWriteRegisterOptions): Promise<void>;
    /**
     * Performs an atomic read and write of holding registers in a single transaction (Function Code 23).
     *
     * The write operation is performed before the read.
     *
     * @param {ReadWriteMultipleRegistersOptions} options - The request parameters.
     * @param {number} options.readAddress - Starting address for read.
     * @param {number} options.readQuantity - Number of registers to read.
     * @param {number} options.writeAddress - Starting address for write.
     * @param {Uint16Array} options.writeValues - Values to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of the registers read.
     *
     * @example
     * ```javascript
     * const readData = await client.readWriteMultipleRegisters({
     *   readAddress: 10, readQuantity: 2, writeAddress: 20, writeValues: new Uint16Array([5, 6])
     * });
     * ```
     */
    readWriteMultipleRegisters(options: ReadWriteMultipleRegistersOptions): Promise<Uint16Array>;
    /**
     * Reads the contents of a FIFO queue of 16-bit registers (Function Code 18).
     *
     * @param {ReadFifoQueueOptions} options - The request parameters.
     * @param {number} options.address - Starting address of the FIFO queue.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<FifoQueueResponse>} A promise that resolves to a `FifoQueueResponse` object.
     *
     * @example
     * ```javascript
     * const fifoContents = await client.readFifoQueue({ address: 42 });
     * ```
     */
    readFifoQueue(options: ReadFifoQueueOptions): Promise<FifoQueueResponse>;
    /**
     * Reads one or more file records (Function Code 14).
     *
     * @param {ReadFileRecordOptions} options - The request parameters.
     * @param {object[]} options.requests - An array of sub-request objects.
     * @param {number} options.requests[].fileNumber - The file number.
     * @param {number} options.requests[].recordNumber - The starting record number within the file.
     * @param {number} options.requests[].recordLength - The number of registers to read for this record.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array[]>} A promise that resolves to an array of `Uint16Array`, with each element corresponding to a sub-request.
     *
     * @example
     * ```javascript
     * const records = await client.readFileRecord({
     *   requests: [
     *     { fileNumber: 4, recordNumber: 1, recordLength: 2 }
     *   ]
     * });
     * ```
     */
    readFileRecord(options: ReadFileRecordOptions): Promise<Uint16Array[]>;
    /**
     * Writes one or more file records (Function Code 15).
     *
     * @param {WriteFileRecordOptions} options - The request parameters.
     * @param {object[]} options.requests - An array of sub-request objects to write.
     * @param {number} options.requests[].fileNumber - The file number.
     * @param {number} options.requests[].recordNumber - The starting record number within the file.
     * @param {Uint16Array} options.requests[].recordData - The register data to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeFileRecord({
     *   requests: [
     *     { fileNumber: 4, recordNumber: 1, recordData: new Uint16Array([0xDEAD, 0xBEEF]) }
     *   ]
     * });
     * ```
     */
    writeFileRecord(options: WriteFileRecordOptions): Promise<void>;
    /**
     * Reads the device's exception status (Function Code 07).
     *
     * The result is an 8-bit value where each bit corresponds to a specific exception flag.
     *
     * @returns {Promise<number>} A promise that resolves to the 8-bit exception status.
     *
     * @example
     * const status = await client.readExceptionStatus();
     */
    readExceptionStatus(): Promise<number>;
    /**
     * Reads device identification information (MEI Function Code 43, Sub-code 14).
     *
     * This allows reading standard device information like Vendor Name, Product Code, etc.
     *
     * @param {ReadDeviceIdentificationOptions} options - The request parameters.
     * @param {number} [options.readDeviceIdCode] - The type of read (1=Basic, 2=Regular, 3=Extended).
     * @param {number} [options.objectId] - The specific object ID to start reading from.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DeviceIdentificationResponse>} A promise that resolves to a DeviceIdentificationResponse containing the device identification data.
     *
     * @example
     * ```javascript
     * const id = await client.readDeviceIdentification({
     *   readDeviceIdCode: 1, // Basic device identification
     *   objectId: 0,
     * });
     *
     * // id.objects will be an array like:
     * // [{ id: 0, value: "VendorName" }, { id: 1, value: "ProductCode" }]
     * ```
     */
    readDeviceIdentification(options: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
    /**
     * Performs a diagnostic function on the device (Function Code 08).
     *
     * @param {DiagnosticsOptions} options - The request parameters.
     * @param {number} options.subFunction - The diagnostic sub-function code to execute.
     * @param {Uint16Array} [options.data] - Optional data to send with the request.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DiagnosticsResponse>} A promise that resolves to a DiagnosticsResponse containing the `subFunction` and `data` from the response.
     *
     * @example
     * ```javascript
     * // Example: Return query data
     * const response = await client.diagnostics({
     *   subFunction: 0,
     *   data: new Uint16Array([0x12, 0x34])
     * });
     * ```
     */
    diagnostics(options: DiagnosticsOptions): Promise<DiagnosticsResponse>;
    /**
     * Returns `true` if the underlying transport is considered connected.
     */
    isConnected(): boolean;
    /**
     * Returns `true` if there are any in-flight Modbus requests pending a response.
     */
    readonly pendingRequests: boolean;
}

/**
 * A browser-facing Modbus serial client bound to a specific unit ID (slave address).
 *
 * This class provides methods for all standard Modbus function codes. All operations
 * are asynchronous and return a `Promise`. It is created via `WasmSerialTransport.createClient()`.
 */
export declare class WasmSerialModbusClient {
    /**
     * Reads a sequence of coils (Function Code 01).
     *
     * @param {ReadBitsOptions} options - The request parameters.
     * @param {number} options.address - Starting coil address (0-based).
     * @param {number} options.quantity - Number of coils to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<CoilState[]>} A promise that resolves to an array representing the coil states.
     *
     * @example
     * ```javascript
     * const coils = await client.readCoils({ address: 0, quantity: 8 });
     * console.log(coils); // e.g., [CoilState.On, CoilState.Off, ...]
     * ```
     */
    readCoils(options: ReadBitsOptions): Promise<CoilState[]>;
    /**
     * Reads a sequence of discrete inputs (Function Code 02).
     *
     * These are read-only boolean inputs.
     *
     * @param {ReadBitsOptions} options - The request parameters.
     * @param {number} options.address - Starting input address (0-based).
     * @param {number} options.quantity - Number of inputs to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DiscreteInputState[]>} A promise that resolves to an array of states.
     *
     * @example
     * ```javascript
     * const inputs = await client.readDiscreteInputs({ address: 0, quantity: 4 });
     * ```
     */
    readDiscreteInputs(options: ReadBitsOptions): Promise<DiscreteInputState[]>;
    /**
     * Reads a sequence of holding registers (Function Code 03).
     *
     * These are 16-bit read/write registers.
     *
     * @param {ReadRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {number} options.quantity - Number of registers to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
     *
     * @example
     * ```javascript
     * const regs = await client.readHoldingRegisters({ address: 100, quantity: 10 });
     * console.log(regs); // Access the first register value
     * ```
     */
    readHoldingRegisters(options: ReadRegistersOptions): Promise<Uint16Array>;
    /**
     * Reads a sequence of input registers (Function Code 04).
     *
     * These are 16-bit read-only registers.
     *
     * @param {ReadRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {number} options.quantity - Number of registers to read (1-125).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
     *
     * @example
     * ```javascript
     * const inputRegs = await client.readInputRegisters({ address: 50, quantity: 2 });
     * ```
     */
    readInputRegisters(options: ReadRegistersOptions): Promise<Uint16Array>;
    /**
     * Writes a single coil state (Function Code 05).
     *
     * @param {WriteSingleCoilOptions} options - The request parameters.
     * @param {number} options.address - Target coil address (0-based).
     * @param {boolean} options.value - State to write (`true` for ON, `false` for OFF).
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeSingleCoil({ address: 10, value: true });
     * ```
     */
    writeSingleCoil(options: WriteSingleCoilOptions): Promise<void>;
    /**
     * Writes a single holding register (Function Code 06).
     *
     * @param {WriteSingleRegisterOptions} options - The request parameters.
     * @param {number} options.address - Target register address (0-based).
     * @param {number} options.value - 16-bit value to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeSingleRegister({ address: 100, value: 42 });
     * ```
     */
    writeSingleRegister(options: WriteSingleRegisterOptions): Promise<void>;
    /**
     * Writes a sequence of coil states (Function Code 15).
     *
     * @param {WriteMultipleCoilsOptions} options - The request parameters.
     * @param {number} options.address - Starting coil address (0-based).
     * @param {boolean[]} options.values - Array of boolean states to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeMultipleCoils({ address: 20, values: [true, false, true, true] });
     * ```
     */
    writeMultipleCoils(options: WriteMultipleCoilsOptions): Promise<void>;
    /**
     * Writes a sequence of holding registers (Function Code 16).
     *
     * @param {WriteMultipleRegistersOptions} options - The request parameters.
     * @param {number} options.address - Starting register address (0-based).
     * @param {Uint16Array} options.values - Array of 16-bit values to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeMultipleRegisters({ address: 200, values: new Uint16Array([1, 2, 3]) });
     * ```
     */
    writeMultipleRegisters(options: WriteMultipleRegistersOptions): Promise<void>;
    /**
     * Modifies a single holding register using a bitwise AND/OR mask (Function Code 22).
     *
     * The operation is `(current_value AND andMask) OR (orMask AND (NOT andMask))`.
     *
     * @param {MaskWriteRegisterOptions} options - The request parameters.
     * @param {number} options.address - Target register address.
     * @param {number} options.andMask - Bitwise AND mask.
     * @param {number} options.orMask - Bitwise OR mask.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the operation is complete.
     *
     * @example
     * ```javascript
     * // Set bits 0-7 and clear bits 8-15 of the register at address 300
     * await client.maskWriteRegister({
     *   address: 300, andMask: 0x00FF, orMask: 0xFF00
     * });
     * ```
     */
    maskWriteRegister(options: MaskWriteRegisterOptions): Promise<void>;
    /**
     * Performs an atomic read and write of holding registers in a single transaction (Function Code 23).
     *
     * The write operation is performed before the read.
     *
     * @param {ReadWriteMultipleRegistersOptions} options - The request parameters.
     * @param {number} options.readAddress - Starting address for read.
     * @param {number} options.readQuantity - Number of registers to read.
     * @param {number} options.writeAddress - Starting address for write.
     * @param {Uint16Array} options.writeValues - Values to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of the registers read.
     *
     * @example
     * ```javascript
     * const readData = await client.readWriteMultipleRegisters({
     *   readAddress: 10, readQuantity: 2, writeAddress: 20, writeValues: new Uint16Array([5, 6])
     * });
     * ```
     */
    readWriteMultipleRegisters(options: ReadWriteMultipleRegistersOptions): Promise<Uint16Array>;
    /**
     * Reads the contents of a FIFO queue of 16-bit registers (Function Code 18).
     *
     * @param {ReadFifoQueueOptions} options - The request parameters.
     * @param {number} options.address - Starting address of the FIFO queue.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<FifoQueueResponse>} A promise that resolves to a `FifoQueueResponse` object.
     *
     * @example
     * ```javascript
     * const fifoContents = await client.readFifoQueue({ address: 42 });
     * ```
     */
    readFifoQueue(options: ReadFifoQueueOptions): Promise<FifoQueueResponse>;
    /**
     * Reads one or more file records (Function Code 14).
     *
     * @param {ReadFileRecordOptions} options - The request parameters.
     * @param {object[]} options.requests - An array of sub-request objects.
     * @param {number} options.requests[].fileNumber - The file number.
     * @param {number} options.requests[].recordNumber - The starting record number within the file.
     * @param {number} options.requests[].recordLength - The number of registers to read for this record.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<Uint16Array[]>} A promise that resolves to an array of `Uint16Array`, with each element corresponding to a sub-request.
     *
     * @example
     * ```javascript
     * const records = await client.readFileRecord({
     *   requests: [
     *     { fileNumber: 4, recordNumber: 1, recordLength: 2 }
     *   ]
     * });
     * ```
     */
    readFileRecord(options: ReadFileRecordOptions): Promise<Uint16Array[]>;
    /**
     * Writes one or more file records (Function Code 15).
     *
     * @param {WriteFileRecordOptions} options - The request parameters.
     * @param {object[]} options.requests - An array of sub-request objects to write.
     * @param {number} options.requests[].fileNumber - The file number.
     * @param {number} options.requests[].recordNumber - The starting record number within the file.
     * @param {Uint16Array} options.requests[].recordData - The register data to write.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<void>} A promise that resolves when the write is complete.
     *
     * @example
     * ```javascript
     * await client.writeFileRecord({
     *   requests: [
     *     { fileNumber: 4, recordNumber: 1, recordData: new Uint16Array([0xDEAD, 0xBEEF]) }
     *   ]
     * });
     * ```
     */
    writeFileRecord(options: WriteFileRecordOptions): Promise<void>;
    /**
     * Reads the device's exception status (Function Code 07).
     *
     * The result is an 8-bit value where each bit corresponds to a specific exception flag.
     *
     * @returns {Promise<number>} A promise that resolves to the 8-bit exception status.
     *
     * @example
     * const status = await client.readExceptionStatus();
     */
    readExceptionStatus(): Promise<number>;
    /**
     * Reads device identification information (MEI Function Code 43, Sub-code 14).
     *
     * This allows reading standard device information like Vendor Name, Product Code, etc.
     *
     * @param {ReadDeviceIdentificationOptions} options - The request parameters.
     * @param {number} [options.readDeviceIdCode] - The type of read (1=Basic, 2=Regular, 3=Extended).
     * @param {number} [options.objectId] - The specific object ID to start reading from.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DeviceIdentificationResponse>} A promise that resolves to a DeviceIdentificationResponse containing the device identification data.
     *
     * @example
     * ```javascript
     * const id = await client.readDeviceIdentification({
     *   readDeviceIdCode: 1, // Basic device identification
     *   objectId: 0,
     * });
     *
     * // id.objects will be an array like:
     * // [{ id: 0, value: "VendorName" }, { id: 1, value: "ProductCode" }]
     * ```
     */
    readDeviceIdentification(options: ReadDeviceIdentificationOptions): Promise<DeviceIdentificationResponse>;
    /**
     * Performs a diagnostic function on the device (Function Code 08).
     *
     * @param {DiagnosticsOptions} options - The request parameters.
     * @param {number} options.subFunction - The diagnostic sub-function code to execute.
     * @param {Uint16Array} [options.data] - Optional data to send with the request.
     * @param {AbortSignal} [options.signal] - Optional cancellation signal.
     * @returns {Promise<DiagnosticsResponse>} A promise that resolves to a DiagnosticsResponse containing the `subFunction` and `data` from the response.
     *
     * @example
     * ```javascript
     * // Example: Return query data
     * const response = await client.diagnostics({
     *   subFunction: 0,
     *   data: new Uint16Array([0x12, 0x34])
     * });
     * ```
     */
    diagnostics(options: DiagnosticsOptions): Promise<DiagnosticsResponse>;
    /**
     * Returns `true` if there are any in-flight Modbus requests pending a response.
     */
    readonly pendingRequests: boolean;
    /**
     * Checks if the client is connected to the transport.
     */
    isConnected(): boolean;
}

/**
 * Connection manager for browser Modbus RTU Serial clients using the Web Serial API.
 */
export class WasmRtuTransport {
    /**
     * Creates and opens a new RTU Serial transport using the provided port handle and options.
     *
     * @param {WasmSerialPortHandle} port_handle - The opaque handle obtained from `request_serial_port()`.
     * @param {WasmSerialTransportOptions} [options] - Configuration for the serial connection.
     * @param {number} [options.baudRate] - The serial baud rate (default: 9600).
     * @param {number} [options.dataBits] - Number of data bits (7 or 8, default: 8).
     * @param {number} [options.stopBits] - Number of stop bits (1 or 2, default: 1).
     * @param {string} [options.parity] - Parity selection ('none' | 'even' | 'odd', default: 'none').
     * @param {number} [options.responseTimeoutMs] - Response timeout in milliseconds (default: 1000).
     * @returns {Promise<WasmRtuTransport>} A promise that resolves to the new transport instance.
     */
    static open(port_handle: WasmSerialPortHandle, options?: WasmSerialTransportOptions): Promise<WasmRtuTransport>;
    /**
     * Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
     *
     * @param {CreateClientOptions} options - The client configuration.
     * @param {number} options.unitId - The Modbus unit ID (1-247) of the target slave device.
     * @returns {WasmSerialModbusClient} A new client instance.
     */
    createClient(options: CreateClientOptions): WasmSerialModbusClient;
    /**
     * Closes the serial port connection and terminates the background task.
     */
    close(): void;
    /**
     * Returns `true` if there are any in-flight Modbus requests pending a response.
     */
    readonly pendingRequests: boolean;
    /**
     * Sets a temporary request timeout override (in milliseconds) for all clients of this transport.
     */
    setRequestTimeout(ms: number): void;
    /**
     * Clears any request timeout override and restores the default timeout.
     */
    clearRequestTimeout(): void;
    /**
     * Drop all pending in-flight requests and attempt to reconnect.
     */
    reconnect(): Promise<void>;

}

/**
 * Connection manager for browser Modbus ASCII Serial clients using the Web Serial API.
 */
export class WasmAsciiTransport {
    /**
     * Creates and opens a new ASCII Serial transport using the provided port handle and options.
     *
     * @param {WasmSerialPortHandle} port_handle - The opaque handle obtained from `request_serial_port()`.
     * @param {WasmSerialTransportOptions} [options] - Configuration for the serial connection.
     * @param {number} [options.baudRate] - The serial baud rate (default: 9600).
     * @param {number} [options.dataBits] - Number of data bits (7 or 8, default: 8).
     * @param {number} [options.stopBits] - Number of stop bits (1 or 2, default: 1).
     * @param {string} [options.parity] - Parity selection ('none' | 'even' | 'odd', default: 'none').
     * @param {number} [options.responseTimeoutMs] - Response timeout in milliseconds (default: 1000).
     * @returns {Promise<WasmAsciiTransport>} A promise that resolves to the new transport instance.
     */
    static open(port_handle: WasmSerialPortHandle, options?: WasmSerialTransportOptions): Promise<WasmAsciiTransport>;
    /**
     * Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
     *
     * @param {CreateClientOptions} options - The client configuration.
     * @param {number} options.unitId - The Modbus unit ID (1-247) of the target slave device.
     * @returns {WasmSerialModbusClient} A new client instance.
     */
    createClient(options: CreateClientOptions): WasmSerialModbusClient;
    /**
     * Closes the serial port connection and terminates the background task.
     */
    close(): void;
    /**
     * Returns `true` if there are any in-flight Modbus requests pending a response.
     */
    readonly pendingRequests: boolean;
    /**
     * Sets a temporary request timeout override (in milliseconds) for all clients of this transport.
     */
    setRequestTimeout(ms: number): void;
    /**
     * Clears any request timeout override and restores the default timeout.
     */
    clearRequestTimeout(): void;
    /**
     * Drop all pending in-flight requests and attempt to reconnect.
     */
    reconnect(): Promise<void>;
}
"#;

// ── 2. Strongly-typed option/response structures generated by tsify ────────────

/// Options for creating a client bound to a specific unit ID.
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CreateClientOptions {
    /// The Modbus unit ID / slave address (1-247).
    pub unit_id: u8,
}

/// Options for reading coils (FC01) or discrete inputs (FC02).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadBitsOptions {
    /// Starting coil/input address.
    pub address: u16,
    /// Number of points to read.
    pub quantity: u16,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for reading holding (FC03) or input (FC04) registers.
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadRegistersOptions {
    /// Starting register address.
    pub address: u16,
    /// Number of registers to read.
    pub quantity: u16,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for writing a single coil (FC05).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WriteSingleCoilOptions {
    /// Target coil address.
    pub address: u16,
    /// The coil state to write (ON/OFF).
    #[tsify(type = "CoilState")]
    #[serde(deserialize_with = "deserialize_bool_or_int")]
    pub value: bool,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for writing a single register (FC06).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WriteSingleRegisterOptions {
    /// Target register address.
    pub address: u16,
    /// 16-bit value to write.
    pub value: u16,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for writing multiple coils (FC15).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WriteMultipleCoilsOptions {
    /// Starting coil address.
    pub address: u16,
    /// The sequence of coil states to write.
    #[tsify(type = "CoilState[]")]
    #[serde(deserialize_with = "deserialize_vec_bool_or_int")]
    pub values: Vec<bool>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for writing multiple registers (FC16).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WriteMultipleRegistersOptions {
    /// Starting register address.
    pub address: u16,
    /// Array of 16-bit register values to write.
    #[tsify(type = "Uint16Array")]
    pub values: Vec<u16>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for bitwise register masking (FC22).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct MaskWriteRegisterOptions {
    /// Target register address.
    pub address: u16,
    /// Bitwise AND mask to apply.
    pub and_mask: u16,
    /// Bitwise OR mask to apply.
    pub or_mask: u16,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for combined atomic read/write of holding registers (FC23).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadWriteMultipleRegistersOptions {
    /// Starting address for read operation.
    pub read_address: u16,
    /// Number of registers to read.
    pub read_quantity: u16,
    /// Starting address for write operation.
    pub write_address: u16,
    /// Array of 16-bit register values to write.
    #[tsify(type = "Uint16Array")]
    pub write_values: Vec<u16>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for reading a FIFO queue (FC24).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadFifoQueueOptions {
    /// Starting address of the FIFO queue.
    pub address: u16,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// A sub-request for reading a single record within a file.
#[derive(Tsify, Deserialize, Serialize)]
#[tsify(from_wasm_abi, into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FileRecordReadRequest {
    /// The target file number.
    pub file_number: u16,
    /// The starting record number within the file.
    pub record_number: u16,
    /// The number of registers to read.
    pub record_length: u16,
}

/// Options for reading file records (FC20).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileRecordOptions {
    /// List of file record sub-requests.
    pub requests: Vec<FileRecordReadRequest>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// A sub-request for writing a single record within a file.
#[derive(Tsify, Deserialize, Serialize)]
#[tsify(from_wasm_abi, into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FileRecordWriteRequest {
    /// The target file number.
    pub file_number: u16,
    /// The starting record number within the file.
    pub record_number: u16,
    /// The 16-bit register data to write.
    #[tsify(type = "Uint16Array")]
    pub record_data: Vec<u16>,
}

/// Options for writing file records (FC21).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct WriteFileRecordOptions {
    /// List of file record write sub-requests.
    pub requests: Vec<FileRecordWriteRequest>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for reading device identification metadata (MEI FC43/14).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ReadDeviceIdentificationOptions {
    /// The type of read operation (Basic/Regular/Extended).
    #[tsify(optional)]
    pub read_device_id_code: Option<u8>,
    /// The object ID to begin reading from.
    #[tsify(optional)]
    pub object_id: Option<u8>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Options for running diagnostic operations (FC08).
#[derive(Tsify, Deserialize)]
#[tsify(from_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsOptions {
    /// Diagnostic sub-function code.
    pub sub_function: u16,
    /// Sub-function data payload.
    #[tsify(optional, type = "Uint16Array")]
    pub data: Option<Vec<u16>>,
    /// Optional cancellation signal.
    #[tsify(optional, type = "AbortSignal")]
    #[serde(default, with = "serde_wasm_bindgen::preserve")]
    pub signal: JsValue,
}

/// Response payload from diagnostics (FC08).
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsResponse {
    /// Diagnostic sub-function code.
    pub sub_function: u16,
    /// The data returned by the diagnostics function.
    #[tsify(type = "Uint16Array")]
    pub data: Vec<u16>,
}

/// A single device identification metadata object.
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentificationObject {
    /// The object ID.
    pub id: u8,
    /// The string metadata value.
    pub value: String,
}

/// Response payload containing device identification metadata (MEI FC43/14).
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentificationResponse {
    /// Conformity level of the device.
    pub conformity_level: u8,
    /// Whether more objects follow.
    pub more_follows: bool,
    /// ID of the next object to request.
    pub next_object_id: u8,
    /// Array of metadata objects.
    pub objects: Vec<DeviceIdentificationObject>,
}

/// Response payload from reading a FIFO queue (FC24).
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FifoQueueResponse {
    /// Number of elements in the FIFO queue.
    pub count: u16,
    /// Array of 16-bit FIFO register values.
    #[tsify(type = "Uint16Array")]
    pub values: Vec<u16>,
}

#[wasm_bindgen]
extern "C" {
    /// Opaque handle to the JavaScript ServerHandlers interface callbacks.
    #[wasm_bindgen(typescript_type = "ServerHandlers")]
    pub type ServerHandlers;
}

/// Standard Modbus exception codes.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModbusExceptionCode {
    /// Function code received in the query is not an allowable action for the server.
    IllegalFunction = 1,
    /// The data address received in the query is not an allowable address for the server.
    IllegalDataAddress = 2,
    /// A value contained in the query data field is not an allowable value for the server.
    IllegalDataValue = 3,
    /// An unrecoverable error occurred while the server was attempting to perform the requested action.
    ServerDeviceFailure = 4,
    /// The server has accepted the request and is processing it, but a long duration of time will be required.
    Acknowledge = 5,
    /// The server is engaged in processing a long-duration program command.
    SlaveDeviceBusy = 6,
    /// The server attempted to read record file, but detected a parity error in the memory.
    MemoryParityError = 8,
    /// Specialized for gateways: The gateway was unable to allocate an internal communication path.
    GatewayPathUnavailable = 10,
    /// Specialized for gateways: No response was received from the target device.
    GatewayTargetDeviceFailedToRespond = 11,
}

fn deserialize_bool_or_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolOrIntVisitor;

    impl<'de> serde::de::Visitor<'de> for BoolOrIntVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or an integer")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0)
        }

        fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v != 0.0)
        }
    }

    deserializer.deserialize_any(BoolOrIntVisitor)
}

fn deserialize_vec_bool_or_int<'de, D>(deserializer: D) -> Result<Vec<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct VecBoolOrIntVisitor;

    impl<'de> serde::de::Visitor<'de> for VecBoolOrIntVisitor {
        type Value = Vec<bool>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of booleans or integers")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            #[derive(Deserialize)]
            struct Element(#[serde(deserialize_with = "deserialize_bool_or_int")] bool);

            while let Some(Element(val)) = seq.next_element()? {
                vec.push(val);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_seq(VecBoolOrIntVisitor)
}
