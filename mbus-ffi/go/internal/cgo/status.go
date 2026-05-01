//go:build cgo

package cgo

/*
#include "modbus_rs_go.h"
*/
import "C"

// Status is the integer status code returned by every native call.
//
// Numerically identical to MbusStatusCode in the C header. Defined as a
// typed int so that callers can `switch` on it without importing "C".
type Status int32

// Status constants. Sourced directly from the C enum so they stay in
// sync with the FFI even if values are reordered.
const (
	StatusOK                          Status = Status(C.MbusOk)
	StatusParseError                  Status = Status(C.MbusErrParseError)
	StatusBasicParseError             Status = Status(C.MbusErrBasicParseError)
	StatusTimeout                     Status = Status(C.MbusErrTimeout)
	StatusModbusException             Status = Status(C.MbusErrModbusException)
	StatusIoError                     Status = Status(C.MbusErrIoError)
	StatusUnexpected                  Status = Status(C.MbusErrUnexpected)
	StatusConnectionLost              Status = Status(C.MbusErrConnectionLost)
	StatusUnsupportedFunction         Status = Status(C.MbusErrUnsupportedFunction)
	StatusReservedSubFunction         Status = Status(C.MbusErrReservedSubFunction)
	StatusInvalidPduLength            Status = Status(C.MbusErrInvalidPduLength)
	StatusInvalidAduLength            Status = Status(C.MbusErrInvalidAduLength)
	StatusConnectionFailed            Status = Status(C.MbusErrConnectionFailed)
	StatusConnectionClosed            Status = Status(C.MbusErrConnectionClosed)
	StatusBufferTooSmall              Status = Status(C.MbusErrBufferTooSmall)
	StatusBufferLenMismatch           Status = Status(C.MbusErrBufferLenMismatch)
	StatusSendFailed                  Status = Status(C.MbusErrSendFailed)
	StatusInvalidAddress              Status = Status(C.MbusErrInvalidAddress)
	StatusInvalidOffset               Status = Status(C.MbusErrInvalidOffset)
	StatusTooManyRequests             Status = Status(C.MbusErrTooManyRequests)
	StatusInvalidFunctionCode         Status = Status(C.MbusErrInvalidFunctionCode)
	StatusNoRetriesLeft               Status = Status(C.MbusErrNoRetriesLeft)
	StatusTooManyFileReadSubRequests  Status = Status(C.MbusErrTooManyFileReadSubRequests)
	StatusFileReadPduOverflow         Status = Status(C.MbusErrFileReadPduOverflow)
	StatusUnexpectedResponse          Status = Status(C.MbusErrUnexpectedResponse)
	StatusInvalidTransport            Status = Status(C.MbusErrInvalidTransport)
	StatusInvalidSlaveAddress         Status = Status(C.MbusErrInvalidSlaveAddress)
	StatusChecksumError               Status = Status(C.MbusErrChecksumError)
	StatusInvalidConfiguration        Status = Status(C.MbusErrInvalidConfiguration)
	StatusInvalidNumOfExpectedRsps    Status = Status(C.MbusErrInvalidNumOfExpectedRsps)
	StatusInvalidDataLen              Status = Status(C.MbusErrInvalidDataLen)
	StatusInvalidQuantity             Status = Status(C.MbusErrInvalidQuantity)
	StatusInvalidValue                Status = Status(C.MbusErrInvalidValue)
	StatusInvalidAndMask              Status = Status(C.MbusErrInvalidAndMask)
	StatusInvalidOrMask               Status = Status(C.MbusErrInvalidOrMask)
	StatusInvalidByteCount            Status = Status(C.MbusErrInvalidByteCount)
	StatusInvalidDeviceIdentification Status = Status(C.MbusErrInvalidDeviceIdentification)
	StatusInvalidDeviceIdCode         Status = Status(C.MbusErrInvalidDeviceIdCode)
	StatusInvalidMeiType              Status = Status(C.MbusErrInvalidMeiType)
	StatusInvalidBroadcastAddress     Status = Status(C.MbusErrInvalidBroadcastAddress)
	StatusBroadcastNotAllowed         Status = Status(C.MbusErrBroadcastNotAllowed)
	StatusNullPointer                 Status = Status(C.MbusErrNullPointer)
	StatusInvalidUtf8                 Status = Status(C.MbusErrInvalidUtf8)
	StatusInvalidClientId             Status = Status(C.MbusErrInvalidClientId)
	StatusPoolFull                    Status = Status(C.MbusErrPoolFull)
	StatusClientTypeMismatch          Status = Status(C.MbusErrClientTypeMismatch)
	StatusBusy                        Status = Status(C.MbusErrBusy)
)

// String returns the static description from the native library.
func (s Status) String() string {
	cs := C.mbus_go_status_str(C.MbusGoStatus(s))
	if cs == nil {
		return "unknown"
	}
	return C.GoString(cs)
}
