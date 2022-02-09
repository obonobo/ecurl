package ecurl

import (
	"fmt"
	"io"
	"net"
	"strconv"
)

type chunk struct {
	len  int
	red  int
	last bool
}

// A reader/decoder for chunked transfer coding. Throws EOF after the last-chunk
// plus any trailer is read.
type chunkedReader struct {
	conn  net.Conn         // TCP connection, is not read from directly
	scnr  *BufferedScanner // Scanner to read from conn
	chunk chunk            // The chunk that is currently being read
	err   error            // Recorded error
}

func (c *chunkedReader) Read(b []byte) (int, error) {
	// If an error has been registered, then just return that error
	if c.err != nil {
		return 0, c.err
	}

	// Read more data
	red := 0
	for red < len(b) {
		// If we are on the last chunk, finish reading the message, including
		// all the trailers
		if c.chunk.last {
			_, err := c.readTrailers()
			return red, err // Possibly an EOF which needs to be returned
		}

		// If we are done reading the chunk, then load another chunk
		if c.chunkIsDone() {
			if err := c.loadNextChunk(); err != nil {
				return red, err
			}
		}

		// Read current chunk
		n, err := c.readChunk(b[red:])
		red += n
		if err != nil {
			return red, err
		}
	}
	return red, nil
}

// Reads the trailers and final CRLF - this method actually just chugs the
// remaineder of the scanner
func (c *chunkedReader) readTrailers() (int, error) {
	// We will leave this unimplemented for now - we don't actually care about
	// the trailers, we are going to discard them anyways
	return 0, io.EOF
}

func (c *chunkedReader) readChunk(b []byte) (int, error) {
	red := 0
	for ; c.chunk.red < c.chunk.len && red < len(b); red++ {
		bite, err := c.scnr.NextByte()
		if err != nil {
			return red, fmt.Errorf("malformed chunk: %w", err)
		}
		b[red] = bite
		c.chunk.red++
	}

	// Chunk data is CRLF-terminated so we read that too
	if c.chunkIsDone() {
		c.scnr.NextLine()
	}
	return red, nil
}

func (c *chunkedReader) loadNextChunk() error {
	c.chunk = chunk{}

	// Read another chunk
	line, _, err := c.scnr.NextLine()
	if err != nil {
		c.err = err
		return c.err
	}

	// Parse the chunk length - its hexadecimal btw
	len, err := strconv.ParseInt(string(line), 16, 64)
	if err != nil {
		c.err = fmt.Errorf("malformed chunk: %w", err)
		return c.err
	}

	c.chunk.len = int(len)
	c.chunk.last = c.chunk.len == 0
	return nil
}

func (c *chunkedReader) chunkIsDone() bool {
	return c.chunk.len == c.chunk.red
}

func (c *chunkedReader) Close() error {
	c.scnr = nil
	c.err = ErrResponseBodyClosed
	return c.conn.Close()
}
