package ecurl

import (
	"io"
	"net"
)

// A Content-Length limited io.ReadCloser that wraps the raw TCP connection. The
// contentLengthReader will use buf as a buffer while reading - it will read from the socket
// in chunks of length len(buf.b), and will return io.EOF once it has read up to
type contentLengthReader struct {
	conn net.Conn         // TCP connection
	scnr *BufferedScanner // Scanner for reading from TCP connection
	clen int              // Content-Length
	red  int              // Amount read
	err  error            // Recorded error
}

func (r *contentLengthReader) Close() error {
	r.err = ErrResponseBodyClosed
	return r.conn.Close()
}

// Reads up to r.contentLength, or up until the server closes the connection.
// Reads are buffered by the reader's internal buffer
func (r *contentLengthReader) Read(b []byte) (int, error) {
	if r.err != nil {
		return 0, r.err
	}

	remaining := r.clen - r.red
	if remaining <= 0 {
		r.err = io.EOF
		return 0, r.err
	}
	remaining = min(remaining, len(b))

	n, err := r.scnr.Read(b[:remaining])
	r.red += n
	return n, err
}
