package ecurl

import (
	"net"
	"time"
)

type infiniteReader struct {
	conn net.Conn
	scnr *BufferedScanner
}

func (r *infiniteReader) Close() error {
	return r.conn.Close()
}

func (r *infiniteReader) Read(b []byte) (int, error) {
	n, err := r.scnr.Read(b)
	if err == nil {
		// If we get some data then give the server another 5 seconds
		r.conn.SetReadDeadline(time.Now().Add(5 * time.Second))
	}
	return n, err
}
