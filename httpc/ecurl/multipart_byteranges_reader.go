package ecurl

import (
	"errors"
	"fmt"
	"io"
)

// A reader/decoder for byte range delimited content (Content-Type:
// multipart/byteranges). The reader returns the concatenated contents of all
// byteranges.
//
// Design is similar to a scanner/lexical-analyzer. Uses a state machine with a
// partially dynamic set of byterange boundary detector states.
//
// The decoder works by first compiling its boundary into the internal state
// machine. Once the machine is initialized, the reader pulls bytes
// one-at-a-time through from the byte source. It streams them into the state
// machine who dictates some side effects like whether the byte needs to be
// written to the caller or discarded or buffered by the
// MultipartByteRangesReader.
//
// The state machine tracks the progress of the parse and sets the read error to
// be io.EOF when it reaches its final state after reading the final byterange
// delimiter.
type MultipartByteRangesReader struct {
	// The boundary token that separates byteranges
	Boundary string

	// A source of bytes
	Reader io.Reader

	// A connection to be closed when this reader is closed. Use nil Conn for
	// noop when Close() is called
	Conn io.Closer

	AccumulatorSize int

	// A byte source that wraps the Scnr
	bs *byteSource

	// State machine for decoding - lazy loaded, will be initialized properly
	// the first time that you call MultipartByteRangesReader.Read()
	fsm *fsm

	// An error that has been registered - this error is cached and will be
	// returned again if you call MultipartByteRangesReader.Read()
	err error
}

const (
	DEFAULT_ACC_SIZE = 1024
	MAX_ACC_SIZE     = 1 << 20
	MIN_ACC_SIZE     = 64
)

type fsm struct {
	state       int
	base        []byte
	first       int
	transitions map[[2]int]int
}

func (f *fsm) acc() []byte {
	return f.base[f.first:]
}

// An adapter for io.Reader that reads one byte at a time
type byteSource struct {
	io.Reader
	buf []byte
}

func (r *byteSource) NextByte() (byte, error) {
	if r.buf == nil {
		r.buf = make([]byte, 1)
	}
	n, err := r.Read(r.buf[:cap(r.buf)])
	if n < 1 {
		return 0, err
	}
	return r.buf[0], err
}

// State machine states
const (
	start = iota - 666

	startNewline
	startCarriageReturn
	dash
	dashdash
	boundaryCarriageReturn
	boundaryNewline
	headers
	headersCarriageReturn
	headersNewline
	headersBlankLine

	part

	partCarriageReturn
	partNewline
	partDash
	partDashDash
	partBoundaryDash
	partBoundaryCarriageReturn
	partBoundaryNewline

	done
)

// Initial state machine transitions
var (
	any         = -1
	transitions = map[[2]int]int{
		{start, '\r'}: startCarriageReturn,
		{start, '\n'}: startNewline,
		{start, '-'}:  dash,
		{start, any}:  start,

		{startCarriageReturn, '\n'}: startNewline,
		{startCarriageReturn, any}:  start,

		{startNewline, '-'}: dash,
		{startNewline, any}: start,

		{dash, '-'}: dashdash,
		{dash, any}: start,

		{dashdash, any}: start,

		// The boundary transitions will be written dynamically based on the
		// boundary set on the reader (these transitions are common to every
		// state machine that we will create). They will be transitions 0 to
		// len(boundary)

		{boundaryCarriageReturn, '\n'}: boundaryNewline,
		{boundaryNewline, '\r'}:        boundaryNewline,
		{boundaryNewline, '\n'}:        part, // Empty headers
		{boundaryNewline, any}:         headers,
		{headers, any}:                 headers,
		{headers, '\r'}:                headersCarriageReturn,
		{headers, '\n'}:                headersNewline,
		{headersCarriageReturn, '\n'}:  headersNewline,
		{headersCarriageReturn, any}:   headers,

		{headersNewline, '\r'}: headersBlankLine,
		{headersNewline, '\n'}: part,
		{headersNewline, any}:  headers,

		{headersBlankLine, '\r'}: headersBlankLine,
		{headersBlankLine, '\n'}: part,
		{headersBlankLine, any}:  headers,

		{part, any}:                part,
		{part, '\n'}:               partNewline,
		{part, '\r'}:               partCarriageReturn,
		{partCarriageReturn, '\n'}: partNewline,
		{partNewline, '-'}:         partDash,
		{partNewline, '\n'}:        partNewline,
		{partNewline, any}:         part,
		{partDash, '-'}:            partDashDash,

		// The rest of these transitions will be created dynamically

		{partBoundaryDash, '-'}:            done,
		{partBoundaryCarriageReturn, '\n'}: partBoundaryNewline,
		{partBoundaryNewline, '\r'}:        partBoundaryNewline,
		{partBoundaryNewline, '\n'}:        part,
		{partBoundaryNewline, any}:         headers,
	}
)

func (r *MultipartByteRangesReader) Read(b []byte) (n int, err error) {
	defer func() {
		if e, ok := recover().(error); ok {
			err = e
		}
	}()

	r.load() // Lazy load the state machine
	if r.cannotReadAnymore() {
		return 0, r.err // If an error has been registered, return that instead
	}

	var red int
	for ; red < len(b); red++ {
		bite, err := r.nextByte()
		if err != nil {
			r.err = err
			return red, r.err
		}
		b[red] = bite
	}

	return red, nil
}

func (r *MultipartByteRangesReader) Close() error {
	if r.Conn != nil { // noop if the conn is nil
		return r.Conn.Close()
	}
	return nil
}

func (r *MultipartByteRangesReader) cannotReadAnymore() bool {
	return r.err != nil && len(r.fsm.acc()) == 0
}

func (r *MultipartByteRangesReader) nextByte() (byte, error) {
	for {
		if r.err != nil {
			// Then we may still have some bytes to read from acc
			if len(r.fsm.acc()) > 0 {
				return r.dequeue(), nil
			}

			// Finally, throw the error when the acc is empty
			return 0, r.err
		}

		// Grab the next byte
		bite, err := r.bs.NextByte()
		if err != nil {
			if errors.Is(err, io.EOF) {
				// If we haven't yet read anything (i.e. we have an empty body),
				// then we can just bubble up this EOF, its not an
				// ErrMalformedByterange
				if r.fsm.state == start {
					r.err = io.EOF
				} else {
					// Otherwise, if we have read some stuff, then we have a
					// malformed byterange
					r.err = ErrUnexpectedEOF{}
				}
			} else {
				// Here we return a generic ErrMalformedByterange (the
				// "super-error" of the two above more specific errors)
				r.err = &ErrMalformedByterange{err}
			}
			continue
		}

		// Throw the byte into the state machine - this switch is for managing
		// side effects (like whether we should return this byte or collect it
		// an build a boundary token)
		prev := r.fsm.state
		switch r.fsm.state = r.shift(bite); r.fsm.state {
		case startNewline, partNewline:
			if len(r.fsm.acc()) >= 1 {
				return r.shiftQueue(bite), nil
			}
			r.enqueue(bite)

		case
			// partBoundaryNewline,
			startCarriageReturn,
			dash,
			dashdash,
			boundaryCarriageReturn,
			partCarriageReturn,
			partDash,
			partDashDash,
			partBoundaryDash,
			partBoundaryCarriageReturn:
			r.enqueue(bite)

		case boundaryNewline, partBoundaryNewline:
			// Then we have completed the newline and matched an entire boundary
			// token, we will dump the accumulator
			if len(r.fsm.acc()) > 0 {
				r.dropqueue()
			}

		case headers:
			if prev == partBoundaryNewline && len(r.fsm.acc()) > 0 {
				r.dropqueue()
			}

		case any:
			r.err = ErrUnexpectedSymbol(bite)

		case headersNewline, headersCarriageReturn:
		case done:
			// Here we have read the final boundary delimiter `--boundary--`
			r.err = io.EOF
			return 0, r.err

		default:
			// We have to handle the boundary transitions here
			if r.fsm.state >= 0 {
				r.enqueue(bite)
				continue
			}

			// We only print the 'part' state self-loops
			if prev != part &&
				prev != partNewline &&
				prev != startNewline {
				continue
			}

			// Otherwise, we print the byte
			return r.shiftQueue(bite), nil
		}
	}
}

// Move the state machine
//
// The FSM needs to do:
// - When on start state, eat the bytes
// - Start state can transition to headers state by reading the boundary
// - headers state to part state once all headers are read (\r\n)
//
func (r *MultipartByteRangesReader) shift(bite byte) int {

	// Check for normal transition first
	if next, ok := r.fsm.transitions[[2]int{r.fsm.state, int(bite)}]; ok {
		return next
	}

	// Check for 'any' transition
	if next, ok := r.fsm.transitions[[2]int{r.fsm.state, any}]; ok {
		return next
	}

	// If we return 'any', that indicates that there is no transition available
	return any
}

// Returns the next byte from the accumulator or the provided byte if the
// accumulator is empty
func (r *MultipartByteRangesReader) shiftQueue(bite byte) byte {
	r.enqueue(bite)
	return r.dequeue()
}

func (r *MultipartByteRangesReader) dropqueue() {
	r.fsm.first = 0
	r.fsm.base = r.fsm.base[:0]
}

func (r *MultipartByteRangesReader) dequeue() byte {
	if len(r.fsm.acc()) == 0 {
		return 0
	}
	if r.fsm.first >= len(r.fsm.base)-1 {
		r.fsm.base = r.fsm.base[:copy(r.fsm.base[:cap(r.fsm.base)], r.fsm.acc())]
		r.fsm.first = 0
	}
	bite := r.fsm.acc()[0]
	r.fsm.first++
	return bite
}

func (r *MultipartByteRangesReader) enqueue(bite byte) {
	r.fsm.base = append(r.fsm.base, bite)
}

// Compiles the boundary transitions to the transition table
func (r *MultipartByteRangesReader) compile(m map[[2]int]int, boundary string) map[[2]int]int {
	base := len(boundary)
	if base == 0 {
		return m
	} else if base > r.AccumulatorSize {
		panic(ErrBoundaryTooLong{})
	}

	// Set the initial transition where we start reading the boundary token
	m[[2]int{dashdash, int(boundary[0])}] = 0
	m[[2]int{partDashDash, int(boundary[0])}] = base
	if len(boundary) == 1 {
		return m
	}

	// Adds all the transitions between letters
	for i, b := range boundary[1:] {
		m[[2]int{i, int(b)}] = i + 1
		m[[2]int{i, any}] = start
		m[[2]int{i + base, int(b)}] = base + i + 1
		m[[2]int{i + base, any}] = part
	}

	m[[2]int{base - 1, '\r'}] = boundaryCarriageReturn
	m[[2]int{base - 1, '\n'}] = boundaryNewline
	m[[2]int{base - 1, any}] = start
	m[[2]int{base + base - 1, '-'}] = partBoundaryDash
	m[[2]int{base + base - 1, '\r'}] = partBoundaryCarriageReturn
	m[[2]int{base + base - 1, '\n'}] = partBoundaryNewline
	m[[2]int{base + base - 1, any}] = part
	return m
}

func mapcopy(m map[[2]int]int) map[[2]int]int {
	mm := make(map[[2]int]int, len(m)+64)
	for k, v := range m {
		mm[k] = v
	}
	return mm
}

// Initializes the reader
func (r *MultipartByteRangesReader) load() {
	if r.AccumulatorSize == 0 {
		// Then the caller did not set a specific size, use our constant
		r.AccumulatorSize = DEFAULT_ACC_SIZE
	}
	if r.bs == nil {
		r.bs = &byteSource{Reader: r.Reader}
	}
	if r.fsm == nil {
		r.AccumulatorSize = max(min(MAX_ACC_SIZE, r.AccumulatorSize), MIN_ACC_SIZE)
		base := make([]byte, 0, r.AccumulatorSize)
		r.fsm = &fsm{
			state:       start,
			transitions: r.compile(mapcopy(transitions), r.Boundary),
			base:        base,
		}
	}
}

type ErrMalformedByterange struct{ Err error }

func (e *ErrMalformedByterange) Error() string {
	out := "malformed byterange"
	if e.Err != nil {
		out += fmt.Sprintf(": %v", e.Err)
	}
	return out
}

func (e *ErrMalformedByterange) Unwrap() error {
	return e.Err
}

type ErrUnexpectedSymbol byte

func (e ErrUnexpectedSymbol) Error() string {
	var s string
	switch e {
	case '\t':
		s = "\t"
	case '\r':
		s = "\r"
	case '\n':
		s = "\n"
	default:
		s = string(e)
	}
	return fmt.Sprintf("unexpected symbol '%v': %v", s, &ErrMalformedByterange{})
}

func (e ErrUnexpectedSymbol) Unwrap() error {
	return &ErrMalformedByterange{}
}

type ErrUnexpectedEOF struct{}

func (e ErrUnexpectedEOF) Error() string {
	return fmt.Sprintf("unexpected EOF: %v", &ErrMalformedByterange{io.EOF})
}

func (e ErrUnexpectedEOF) Unwrap() error {
	return &ErrMalformedByterange{io.EOF}
}

type ErrBoundaryTooLong struct{}

func (e ErrBoundaryTooLong) Error() string {
	return ""
}
