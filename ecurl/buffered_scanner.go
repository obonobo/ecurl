package ecurl

import (
	"fmt"
	"io"
	"strings"
)

var (
	ErrLineTooLong    = fmt.Errorf("line is too long")
	ErrNoNewLinesLeft = fmt.Errorf("no newlines left")
)

const (
	MinBufSize        = 1 << 6  // 64 bytes
	MaxBufSize        = 1 << 27 // 128MB
	DefaultBufferSize = 1 << 20 // 1MB
)

type buffer struct {
	bites []byte
	red   int
}

// A custom buffered scanner.
//
// Q: why not use bufio.Scanner?
//
// A: because it is not very flexible, for example you can't switch split
// functions from bufio.ScanLines to bufio.ScanBytes midscan, and if you choose
// to scan lines, you can't tell how many bytes have been read with each line
// because the scanner trims \r\n the same as \n so you will never be able to
// tell if you've read the correct number of bytes if you use this split
// function. You could write a custom split function, but I wanted to add a few
// custom functions like doing both NextLine() and NextByte() at the same time
// as well as implementing io.Reader so that the scanner can be copied from.
// Anyways, I just wrote my own little scanner instead of forcing the
// bufio.Scanner to do the job how I wanted.
type BufferedScanner struct {
	reader io.Reader
	buf    buffer
	err    error
}

// Creates a BufferedScanner with the default buffer size (1MB)
func NewDefaultBufferedScanner(reader io.Reader) *BufferedScanner {
	return NewBufferedScanner(reader, DefaultBufferSize)
}

// Creates a BufferedScanner with a buffer of length `size`, which will be
// coerced into the range of [MinBufSize, MaxBufSize] ~ 64B to 128MB
func NewBufferedScanner(reader io.Reader, size int) *BufferedScanner {
	size = min(max(size, MinBufSize), MaxBufSize)
	return &BufferedScanner{
		reader: reader,
		buf:    buffer{make([]byte, 0, size), 0},
	}
}

// Returns the next line from the scanner with `\r\n` trimmed, as well as how
// many bytes were read to obtain that line. If there is not a full line present
// in the buffer, then the scanner will return an error and will not advance.
func (s *BufferedScanner) NextLine() (string, int, error) {
	if s.cannotReadAnymore() {
		return "", 0, s.err
	}
	s.loadEmpty()

	line, n, err := scanLine(s.buf.bites[s.buf.red:])
	if err != nil {
		if s.err != nil {
			// We have reach EOF and there are no lines left in the buffer
			return "", 0, ErrNoNewLinesLeft
		}

		// If our buffer is not big enough to handle the next line, then we just
		// return the error
		if s.buf.red == 0 {
			return "", 0, fmt.Errorf("buffer is not big enough: %w", err)
		}

		// Otherwise, we may need to load more data. Discard the read portion of
		// the buffer and read from the socket again
		err := s.load()
		if err != nil {
			// Register the error - the reader is dead now, can only read what
			// is left in the buffer
			s.err = io.EOF
		}

		// After loading, we can retry this operation. Recursion is broken by
		// the branches at the top of this scope - this function should not
		// recurse more than once
		return s.NextLine()
	}
	s.buf.red += n
	return line, n, nil
}

// Advances the scanner by one byte
func (s *BufferedScanner) NextByte() (byte, error) {
	if s.cannotReadAnymore() {
		return 0, s.err
	}
	s.loadEmpty()

	// If next byte available from buffer, return it
	if s.buf.red < len(s.buf.bites) {
		ret := s.buf.bites[s.buf.red]
		s.buf.red++
		return ret, nil
	}

	// Otherwise, we need to load more data
	s.load()
	return s.NextByte()
}

func (s *BufferedScanner) Read(b []byte) (red int, err error) {
	s.loadEmpty()
	for red < len(b) {
		if s.cannotReadAnymore() {
			return red, s.err
		}

		// Copy as many bytes as possible
		n := copy(b[red:], s.buf.bites[s.buf.red:])
		s.buf.red += n
		red += n

		// If we have satisfied the read request, then we are done
		if red == len(b) {
			break
		}

		// Otherwise, we need to load more data
		s.load()
	}
	return red, nil
}

func (s *BufferedScanner) loadEmpty() {
	if len(s.buf.bites) == 0 {
		s.load()
	}
}

// Discards the unread portion of the buffer and loads more data from the reader
func (s *BufferedScanner) load() error {
	copy(s.buf.bites[:cap(s.buf.bites)], s.buf.bites[s.buf.red:])
	unread := len(s.buf.bites) - s.buf.red
	s.buf.red = 0
	nn, err := s.reader.Read(s.buf.bites[unread:cap(s.buf.bites)])
	s.buf.bites = s.buf.bites[:unread+nn]
	s.err = err
	return err
}

// Returns whether the scanner is able to read anymore data (whether there is
// data left in the buffer, or the reader has not yet returned an error)
func (s *BufferedScanner) cannotReadAnymore() bool {
	return s.err != nil && s.buf.red == len(s.buf.bites)
}

// Reads a line from the buffer, returns the line read with \r\n trimmed, the
// number of bytes read from the buffer, and an error if the line is too long
func scanLine(buf []byte) (string, int, error) {
	for i := 0; i < len(buf); i++ {
		if buf[i] == '\n' {
			// We have reached a line
			return strings.TrimRight(string(buf[:i]), "\r\n"), i + 1, nil
		}
	}

	// Otherwise, the line is too long
	return strings.TrimRight(string(buf), "\r\n"),
		len(buf),
		fmt.Errorf("read %v bytes without a newline: %w", len(buf), ErrLineTooLong)
}

func min(i1, i2 int) int {
	if i1 < i2 {
		return i1
	}
	return i2
}

func max(i1, i2 int) int {
	if i1 > i2 {
		return i1
	}
	return i2
}
