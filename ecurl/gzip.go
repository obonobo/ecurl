package ecurl

import (
	"compress/gzip"
	"io"
)

// A decoder for gzipped response data. Use by wrapping the outermost io.Reader
// in the Gzipper. This decoding should be applied AFTER the transfer coding
// reader (i.e. after the chunkedReader)
type Gzipper struct {
	r io.ReadCloser
	g *gzip.Reader
}

func NewGzipper(r io.ReadCloser) (*Gzipper, error) {
	g, err := gzip.NewReader(r)
	return &Gzipper{r, g}, err
}

// Read + decode gzip data at the same time
func (z *Gzipper) Read(b []byte) (int, error) {
	return z.g.Read(b)
}

func (z *Gzipper) Close() error {
	return z.r.Close()
}
