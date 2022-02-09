package ecurl

import "net"

type multipartByteRangesReader struct {
	conn net.Conn
	scnr *BufferedScanner
	err  error
}

func (r *multipartByteRangesReader) Close() error {
	return r.conn.Close()
}

func (r *multipartByteRangesReader) Read(b []byte) (int, error) {
	return 0, nil
}
