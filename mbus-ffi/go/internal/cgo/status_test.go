package cgo

import "testing"

func TestStatusString(t *testing.T) {
	cases := []struct {
		status Status
		want   string
	}{
		{StatusOK, "OK"},
		{StatusTimeout, "timeout"},
		{StatusNullPointer, "null pointer"},
	}
	for _, tc := range cases {
		t.Run(tc.want, func(t *testing.T) {
			if got := tc.status.String(); got != tc.want {
				t.Fatalf("Status(%d).String() = %q, want %q", tc.status, got, tc.want)
			}
		})
	}
}

func TestTcpClientWrapperLifecycleDoesNotPanic(t *testing.T) {
	c := TcpClientNew("127.0.0.1", 1)
	if c == nil {
		t.Fatal("TcpClientNew returned nil for a syntactically valid host")
	}
	TcpClientSetRequestTimeoutMs(c, 10)
	if TcpClientHasPendingRequests(c) {
		t.Fatal("new disconnected client unexpectedly has pending requests")
	}
	TcpClientFree(c)
	TcpClientFree(nil)
}
