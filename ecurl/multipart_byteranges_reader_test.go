package ecurl

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"reflect"
	"strings"
	"testing"

	"github.com/obonobo/ecurl/internal/mocks"
)

func TestAccumulatorBufferSizes(t *testing.T) {
	src := func(boundary string) string {
		return strings.TrimLeft(clean(fmt.Sprintf(`
		--%v
		Content-Type: text/html
		Content-Range: bytes 0-50/1270

		<!doctype html>
		<html>
		<head>
			<title>Example Do
		--%v
		Content-Type: text/html
		Content-Range: bytes 100-150/1270

		eta http-equiv="Content-type" content="text/html; c
		--%v--
		`, boundary, boundary, boundary)), "\n")
	}

	expectedOutput := strings.Trim(clean(`
	<!doctype html>
	<html>
	<head>
		<title>Example Doeta http-equiv="Content-type" content="text/html; c
	`), "\n\t\r")

	for _, tc := range []struct {
		name       string
		boundary   string
		bufferSize int
	}{
		{
			name:       "smaller than min/defaults to min",
			boundary:   "asd",
			bufferSize: -666,
		},
		{
			name:       "min size",
			boundary:   "asd",
			bufferSize: MIN_ACC_SIZE,
		},
		{
			name:       "tiny",
			boundary:   "asd",
			bufferSize: 2 * MIN_ACC_SIZE,
		},
		{
			name:       "small",
			boundary:   "asd",
			bufferSize: 10 * MIN_ACC_SIZE,
		},
		{
			name:       "max size",
			boundary:   "asd",
			bufferSize: MAX_ACC_SIZE,
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			input := src(tc.boundary)
			output := expectedOutput

			red, err := io.ReadAll(&MultipartByteRangesReader{
				AccumulatorSize: tc.bufferSize,
				Boundary:        tc.boundary,
				Reader:          bytes.NewBufferString(input),
			})

			// Assertions
			if err != nil {
				t.Fatalf("Expected no error but got '%v'", err)
			}
			if expected, actual := output, string(red); expected != actual {
				t.Fatalf("Expected output to be '%v' but got '%v'", expected, actual)
			}
		})
	}
}

func TestErrUnexpectedEOF(t *testing.T) {
	for _, tc := range []struct {
		name     string
		input    string
		output   string
		boundary string
	}{
		{
			name:     "EOF after first boundary",
			boundary: "asd",
			input: `
			--asd
			Content-Type: text/plain
		 	Content-Length: bytes 0-50/100
			`,
			output: ``,
		},
		{
			name:     "no headers and EOF after first boundary",
			boundary: "asd",
			input: `
			--asd

			sdsdas
			`,
			output: `sdsdas
			`,
		},
		{
			name:     "EOF on closing boundary delim",
			boundary: "asd",
			input: `
			--asd

			sdsdas
			--asd

			123
			--asd-`,
			output: `sdsdas123
			--asd-`,
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			tc.input = clean(tc.input)
			tc.output = clean(tc.output)
			red, err := io.ReadAll(&MultipartByteRangesReader{
				Boundary: tc.boundary,
				Reader:   bytes.NewBufferString(strings.TrimLeft(tc.input, "\n")),
			})

			// Assert that the error is an ErrMalformedByterange error
			if e := new(ErrMalformedByterange); !errors.As(err, &e) {
				t.Fatalf("Expected reader to return err '%v' but got '%v'", e, err)
			}

			// Assert that the error is an ErrUnexpectedEOF error
			if e := (ErrUnexpectedEOF{}); !errors.As(err, &e) {
				t.Fatalf("Expected reader to return err '%v' but got '%v'", e, err)
			}

			// Assert expected output
			if expected, actual := tc.output, string(red); actual != expected {
				t.Fatalf("Expected output '%v' but got '%v'", expected, actual)
			}
		})
	}
}

func TestErrMalformedByteRange(t *testing.T) {
	t.Parallel()

	var (
		wrapMe   = fmt.Errorf("bad!")
		boundary = "asd"
	)

	_, err := io.ReadAll(&MultipartByteRangesReader{
		Boundary: boundary,
		Reader: &MockReader{
			Err:    wrapMe,
			Reader: bytes.NewBufferString(""),
		},
	})

	if err == nil {
		t.Fatalf("Expected error not to be nil")
	}

	// A non-EOF error should be wrapped in the ErrMalformedByterange
	if e := new(ErrMalformedByterange); !errors.As(err, &e) {
		t.Fatalf(
			"Expected error to be of type %v but got '%v'",
			reflect.TypeOf(*e).Name(),
			err)
	}

	// Also check that our error is wrapped somewhere in there
	if !errors.Is(err, wrapMe) {
		t.Fatalf("Expected err to wrap '%v' but got '%v'", wrapMe, err)
	}
}

func TestEmptyStringAndErrors(t *testing.T) {
	for _, tc := range []struct {
		name     string
		input    string
		output   string
		boundary string
		err      *string
	}{
		{
			// According to the rules of our decoder, empty string is fine
			// actually. It represents content split into 0 byteranges
			name:     "empty",
			input:    ``,
			output:   ``,
			boundary: "asd",
			err:      nil,
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			tc.input = clean(tc.input)
			tc.output = strings.Trim(clean(tc.output), "\n\t\r")
			conn := &mocks.MockNetConn{Reader: bytes.NewBufferString(tc.input)}
			r := &MultipartByteRangesReader{
				Boundary: tc.boundary,
				Conn:     conn,
				Reader:   NewDefaultBufferedScanner(conn),
			}

			red, err := io.ReadAll(r)
			if expected, actual := tc.err, err; !(expected == nil && actual == nil) &&
				(actual == nil && expected != nil ||
					actual != nil && expected == nil ||
					actual.Error() != *expected) {
				t.Fatalf("Expected err '%v' but got '%v'", expected, actual)
			}
			if expected, actual := tc.output, string(red); expected != actual {
				t.Fatalf("Expected output '%v' but got '%v'", expected, actual)
			}
		})
	}
}

func TestParseHappyPath(t *testing.T) {
	for _, tc := range []struct {
		name     string
		input    string
		output   string
		boundary string
	}{
		{
			name:     "basic 1",
			boundary: "3d6b6a416f9b5",
			input: `
			--3d6b6a416f9b5
			Content-Type: text/html
			Content-Range: bytes 0-50/1270

			<!doctype html>
			<html>
			<head>
				<title>Example Do
			--3d6b6a416f9b5
			Content-Type: text/html
			Content-Range: bytes 100-150/1270

			eta http-equiv="Content-type" content="text/html; c
			--3d6b6a416f9b5--
			`,
			output: `
			<!doctype html>
			<html>
			<head>
				<title>Example Doeta http-equiv="Content-type" content="text/html; c
			`,
		},

		{
			name:     "basic 2",
			boundary: "asd123asd123",
			input: `
			--asd123asd123
			Content-Type: text/plain
			Content-Range: bytes 0-50/1270

			123asd123asd123asd123asd123asd123asd123asd123asd12
			--asd123asd123
			Content-Type: text/plain
			Content-Range: bytes 50-100/1270

			123asd123asd123asd123asd123asd123asd123asd123asd12
			--asd123asd123
			Content-Type: text/plain
			Content-Range: bytes 100-150/1270

			123asd123asd123asd123asd123asd123asd123asd123asd12
			--asd123asd123
			Content-Type: text/plain
			Content-Range: bytes 150-200/1270

			123asd123asd123asd123asd123asd123asd123asd123asd12
			--asd123asd123--
			`,
			output: `123asd123asd123asd123asd123asd123asd123asd123asd12123asd123asd123asd123asd123asd123asd123asd123asd12123asd123asd123asd123asd123asd123asd123asd123asd12123asd123asd123asd123asd123asd123asd123asd123asd12`,
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			tc.input = clean(tc.input)
			tc.output = strings.Trim(clean(tc.output), "\n\t\r")
			conn := &mocks.MockNetConn{Reader: bytes.NewBufferString(tc.input)}
			r := &MultipartByteRangesReader{
				Boundary: tc.boundary,
				Conn:     conn,
				Reader:   NewDefaultBufferedScanner(conn),
			}

			red, err := io.ReadAll(r)
			if err != nil {
				t.Fatalf("Failed to read from byte ranges reader: %v", err)
			}
			if expected, actual := tc.output, string(red); actual != expected {
				t.Fatalf("Expected output '%v' but got '%v'", expected, actual)
			}
		})
	}
}

func clean(s string) string {
	for _, sym := range []string{"\t"} {
		s = strings.ReplaceAll(s, sym, "")
	}
	return s
}

// A mock io.Reader that throws an error when the delegate reader throws io.EOF
type MockReader struct {
	io.Reader
	Err error
}

func (r *MockReader) Read(b []byte) (int, error) {
	n, err := r.Reader.Read(b)
	if errors.Is(err, io.EOF) && r.Err != nil {
		err = r.Err
	}
	return n, err
}
