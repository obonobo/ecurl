package ecurl

import "net"

type multipartByterangesReader struct {
	conn net.Conn
	scnr *BufferedScanner
	err  error
}

func (r *multipartByterangesReader) Close() error {
	return r.conn.Close()
}

func (r *multipartByterangesReader) Read(b []byte) (int, error) {
	return 0, nil
}
