package mocks

import (
	"io"
	"net"
	"time"
)

// A net.Conn that delegates to a reader
type MockNetConn struct {
	io.Reader
}

func (c *MockNetConn) Read(b []byte) (n int, err error) {
	return c.Reader.Read(b)
}

func (c *MockNetConn) Write(b []byte) (n int, err error) {
	panic("not implemented") // TODO: Implement
}

func (c *MockNetConn) Close() error {
	return nil
}

func (c *MockNetConn) LocalAddr() net.Addr {
	panic("not implemented") // TODO: Implement
}

func (c *MockNetConn) RemoteAddr() net.Addr {
	panic("not implemented") // TODO: Implement
}

func (c *MockNetConn) SetDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}

func (c *MockNetConn) SetReadDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}

func (c *MockNetConn) SetWriteDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}
