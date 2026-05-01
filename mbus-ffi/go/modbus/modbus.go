// Package modbus contains shared types and errors used by every other
// Go binding sub-package (client, server, gateway).
package modbus

import (
	"errors"
	"fmt"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"
)

// Status is the integer status code returned by every native call.
//
// Numerically identical to MbusStatusCode in the C header.
type Status = cgo.Status

// Status constants re-exported from the internal cgo package so callers
// don't need to import "internal/...".
const (
	StatusOK                          = cgo.StatusOK
	StatusParseError                  = cgo.StatusParseError
	StatusBasicParseError             = cgo.StatusBasicParseError
	StatusTimeout                     = cgo.StatusTimeout
	StatusModbusException             = cgo.StatusModbusException
	StatusIoError                     = cgo.StatusIoError
	StatusUnexpected                  = cgo.StatusUnexpected
	StatusConnectionLost              = cgo.StatusConnectionLost
	StatusUnsupportedFunction         = cgo.StatusUnsupportedFunction
	StatusReservedSubFunction         = cgo.StatusReservedSubFunction
	StatusInvalidPduLength            = cgo.StatusInvalidPduLength
	StatusInvalidAduLength            = cgo.StatusInvalidAduLength
	StatusConnectionFailed            = cgo.StatusConnectionFailed
	StatusConnectionClosed            = cgo.StatusConnectionClosed
	StatusBufferTooSmall              = cgo.StatusBufferTooSmall
	StatusBufferLenMismatch           = cgo.StatusBufferLenMismatch
	StatusSendFailed                  = cgo.StatusSendFailed
	StatusInvalidAddress              = cgo.StatusInvalidAddress
	StatusInvalidOffset               = cgo.StatusInvalidOffset
	StatusTooManyRequests             = cgo.StatusTooManyRequests
	StatusInvalidFunctionCode         = cgo.StatusInvalidFunctionCode
	StatusNoRetriesLeft               = cgo.StatusNoRetriesLeft
	StatusTooManyFileReadSubRequests  = cgo.StatusTooManyFileReadSubRequests
	StatusFileReadPduOverflow         = cgo.StatusFileReadPduOverflow
	StatusUnexpectedResponse          = cgo.StatusUnexpectedResponse
	StatusInvalidTransport            = cgo.StatusInvalidTransport
	StatusInvalidSlaveAddress         = cgo.StatusInvalidSlaveAddress
	StatusChecksumError               = cgo.StatusChecksumError
	StatusInvalidConfiguration        = cgo.StatusInvalidConfiguration
	StatusInvalidNumOfExpectedRsps    = cgo.StatusInvalidNumOfExpectedRsps
	StatusInvalidDataLen              = cgo.StatusInvalidDataLen
	StatusInvalidQuantity             = cgo.StatusInvalidQuantity
	StatusInvalidValue                = cgo.StatusInvalidValue
	StatusInvalidAndMask              = cgo.StatusInvalidAndMask
	StatusInvalidOrMask               = cgo.StatusInvalidOrMask
	StatusInvalidByteCount            = cgo.StatusInvalidByteCount
	StatusInvalidDeviceIdentification = cgo.StatusInvalidDeviceIdentification
	StatusInvalidDeviceIdCode         = cgo.StatusInvalidDeviceIdCode
	StatusInvalidMeiType              = cgo.StatusInvalidMeiType
	StatusInvalidBroadcastAddress     = cgo.StatusInvalidBroadcastAddress
	StatusBroadcastNotAllowed         = cgo.StatusBroadcastNotAllowed
	StatusNullPointer                 = cgo.StatusNullPointer
	StatusInvalidUtf8                 = cgo.StatusInvalidUtf8
	StatusInvalidClientId             = cgo.StatusInvalidClientId
	StatusPoolFull                    = cgo.StatusPoolFull
	StatusClientTypeMismatch          = cgo.StatusClientTypeMismatch
	StatusBusy                        = cgo.StatusBusy
)

// FunctionCode is the Modbus function code byte.
type FunctionCode uint8

// Standard Modbus function codes.
const (
	FCReadCoils                   FunctionCode = 0x01
	FCReadDiscreteInputs          FunctionCode = 0x02
	FCReadHoldingRegisters        FunctionCode = 0x03
	FCReadInputRegisters          FunctionCode = 0x04
	FCWriteSingleCoil             FunctionCode = 0x05
	FCWriteSingleRegister         FunctionCode = 0x06
	FCReadExceptionStatus         FunctionCode = 0x07
	FCDiagnostics                 FunctionCode = 0x08
	FCGetCommEventCounter         FunctionCode = 0x0B
	FCGetCommEventLog             FunctionCode = 0x0C
	FCWriteMultipleCoils          FunctionCode = 0x0F
	FCWriteMultipleRegisters      FunctionCode = 0x10
	FCReportServerID              FunctionCode = 0x11
	FCReadFileRecord              FunctionCode = 0x14
	FCWriteFileRecord             FunctionCode = 0x15
	FCMaskWriteRegister           FunctionCode = 0x16
	FCReadWriteMultipleRegisters  FunctionCode = 0x17
	FCReadFifoQueue               FunctionCode = 0x18
)

// ExceptionCode is the Modbus protocol-level exception code.
type ExceptionCode uint8

// Standard Modbus exception codes.
const (
	ExIllegalFunction                    ExceptionCode = 0x01
	ExIllegalDataAddress                 ExceptionCode = 0x02
	ExIllegalDataValue                   ExceptionCode = 0x03
	ExServerDeviceFailure                ExceptionCode = 0x04
	ExAcknowledge                        ExceptionCode = 0x05
	ExServerDeviceBusy                   ExceptionCode = 0x06
	ExNegativeAcknowledge                ExceptionCode = 0x07
	ExMemoryParityError                  ExceptionCode = 0x08
	ExGatewayPathUnavailable             ExceptionCode = 0x0A
	ExGatewayTargetDeviceFailedToRespond ExceptionCode = 0x0B
)

func (c ExceptionCode) String() string {
	switch c {
	case ExIllegalFunction:
		return "illegal function"
	case ExIllegalDataAddress:
		return "illegal data address"
	case ExIllegalDataValue:
		return "illegal data value"
	case ExServerDeviceFailure:
		return "server device failure"
	case ExAcknowledge:
		return "acknowledge"
	case ExServerDeviceBusy:
		return "server device busy"
	case ExNegativeAcknowledge:
		return "negative acknowledge"
	case ExMemoryParityError:
		return "memory parity error"
	case ExGatewayPathUnavailable:
		return "gateway path unavailable"
	case ExGatewayTargetDeviceFailedToRespond:
		return "gateway target device failed to respond"
	default:
		return fmt.Sprintf("unknown(0x%02x)", uint8(c))
	}
}

// SerialMode selects between RTU and ASCII framing on a serial port.
type SerialMode int

// Serial mode constants.
const (
	SerialRTU SerialMode = iota
	SerialASCII
)

// Sentinel errors. Use [errors.Is] to test for them on values returned
// from any client/server call.
var (
	ErrTimeout         = errors.New("modbus: request timed out")
	ErrNotConnected    = errors.New("modbus: not connected")
	ErrClosed          = errors.New("modbus: handle closed")
	ErrInvalidArgument = errors.New("modbus: invalid argument")
	ErrConnectionLost  = errors.New("modbus: connection lost")
	ErrIO              = errors.New("modbus: I/O error")
)

// Error wraps a non-zero [Status] returned by the native library.
type Error struct {
	// Op is a short, human-readable label of the operation that failed
	// (e.g. "ReadHoldingRegisters", "Connect").
	Op string
	// Status is the underlying numeric status code.
	Status Status
	// Cause is the wrapped cause (typically one of the sentinel errors)
	// allowing [errors.Is] checks.
	Cause error
}

func (e *Error) Error() string {
	if e == nil {
		return "<nil>"
	}
	return fmt.Sprintf("modbus: %s: %s", e.Op, e.Status.String())
}

// Unwrap returns the wrapped cause for [errors.Is] / [errors.As].
func (e *Error) Unwrap() error { return e.Cause }

// FromStatus returns a Go error matching `status`. Returns nil if status
// is StatusOK.
func FromStatus(op string, status Status) error {
	if status == StatusOK {
		return nil
	}
	return &Error{Op: op, Status: status, Cause: sentinelFor(status)}
}

func sentinelFor(s Status) error {
	switch s {
	case StatusTimeout:
		return ErrTimeout
	case StatusConnectionFailed, StatusConnectionClosed:
		return ErrNotConnected
	case StatusConnectionLost:
		return ErrConnectionLost
	case StatusInvalidAddress, StatusInvalidQuantity, StatusInvalidValue,
		StatusInvalidByteCount, StatusInvalidUtf8, StatusInvalidPduLength,
		StatusInvalidAduLength, StatusInvalidFunctionCode,
		StatusBufferTooSmall, StatusBufferLenMismatch, StatusInvalidSlaveAddress,
		StatusInvalidConfiguration:
		return ErrInvalidArgument
	case StatusIoError, StatusSendFailed:
		return ErrIO
	default:
		return nil
	}
}
