package ecurl

import (
	"bytes"
	"io"
	"net"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/obonobo/ecurl/internal/testutils"
)

const (
	url  = "http://localhost:8181/"
	addr = ":8181"
	port = 8181
)

var responseHeadersNoTrailer = strings.Trim(`
HTTP/1.1 200 OK
Content-Type: text/plain
Transfer-Encoding: chunked
`, "\n")

var responseHeadersWithTrailer = strings.Trim(`
HTTP/1.1 200 OK
Content-Type: text/plain
Transfer-Encoding: chunked
Trailer: Expires
`, "\n")

// Tests decoding a big response from the EchoServer, who returns chunked
// encoded data if the payload is large
func TestChunkedBigMessageFromEchoServer(t *testing.T) {
	close := testutils.MustBackgroundServer(t, port)
	defer close()

	msg := strings.Repeat("Hello World!\n", 1<<10)
	resp, err := Post(url, "text/plain", bytes.NewBufferString(msg))
	if err != nil {
		t.Fatalf("Expected POST to succeed but got err: %v", err)
	}
	defer resp.Body.Close()

	bod, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("Expected body to be read but got err: %v", err)
	}

	// Trim the first few lines from the response (the echo of our request headers)
	expected := testutils.Tail(string(bod), -6)
	if actual := msg; actual != expected {
		t.Fatalf("Expected response body to be '%v' but got '%v'", msg, expected)
	}
}

// Tests reading a response with chunked transfer coding
func TestChunkedTransferCoding(t *testing.T) {
	for _, tc := range []struct {
		name string // Test case name
		data string // Socket data
		out  string // Expected output after chunked decoding
	}{
		{
			name: "wikipedia no trailer",
			out:  "Wikipedia in \r\n\r\nchunks.",
			data: responseHeadersNoTrailer + "\r\n\r\n" +
				"4\r\n" +
				"Wiki\r\n" +
				"6\r\n" +
				"pedia \r\n" +
				"E\r\n" +
				"in \r\n" +
				"\r\n" +
				"chunks.\r\n" +
				"0\r\n" +
				"\r\n",
		},
		{
			name: "wikipedia with trailer",
			out:  "Wikipedia in \r\n\r\nchunks.",
			data: responseHeadersWithTrailer + "\r\n\r\n" +
				"4\r\n" +
				"Wiki\r\n" +
				"6\r\n" +
				"pedia \r\n" +
				"E\r\n" +
				"in \r\n" +
				"\r\n" +
				"chunks.\r\n" +
				"0\r\n" +
				"\r\n" +
				"Expires: Sat, 27 Mar 2004 21:12:00 GMT\r\n",
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			conn := &mockNetConn{bytes.NewBufferString(tc.data)}
			resp, err := readResponse(conn, 0)
			if err != nil {
				t.Fatalf("Expected response to succeed but got err: %v", err)
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				t.Fatalf("Expected status code %v but got %v", http.StatusOK, resp.StatusCode)
			}

			bod, err := io.ReadAll(resp.Body)
			if err != nil {
				t.Fatalf("Got an error reading response body: %v", err)
			}

			expected := tc.out
			if actual := string(bod); actual != expected {
				t.Fatalf("Expected response body to be '%v' but got '%v'", expected, actual)
			}
		})
	}
}

// Tests the `readResponse` function with various buffer sizes
func TestReadResponseVariousBufSizes(t *testing.T) {
	close := testutils.MustBackgroundServer(t)
	defer close()

	for _, tc := range []struct {
		name string
		size int
	}{
		{
			name: "zero should use default",
			size: 0,
		},
		{
			name: "negative should use default",
			size: -999,
		},
		{
			name: "too big should use default",
			size: 1 << 30,
		},

		{
			name: "tiny buffer",
			size: 50,
		},

		{
			name: "1KB",
			size: 1 << 10,
		},
		{
			name: "64KB",
			size: 1 << 16,
		},
		{
			name: "128KB",
			size: 1 << 17,
		},
		{
			name: "512KB",
			size: 1 << 19,
		},
		{
			name: "1MB",
			size: 1 << 20,
		},
		{
			name: "1MB",
			size: 1 << 20,
		},
		{
			name: "16MB",
			size: 1 << 24,
		},
		{
			name: "max (128MB)",
			size: 1 << 27,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			// The request needs to have a decent body so that we can verify the
			// smaller buffers are still able to read the response (which will
			// also contain said body) properly
			body := strings.Repeat("Hello world!\n", 20)
			req, err := NewRequest(POST, url, bytes.NewBufferString(body))
			if err != nil {
				t.Fatalf("Failed to create request: %v", err)
			}

			resp, err := do(req, tc.size)
			if err != nil {
				t.Fatalf("Request failed: %v", err)
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				t.Errorf(
					"Expected status code %v but got %v",
					http.StatusOK,
					resp.StatusCode)
			}
		})
	}
}

// A net.Conn that delegates to a reader
type mockNetConn struct {
	io.Reader
}

func (c *mockNetConn) Read(b []byte) (n int, err error) {
	return c.Reader.Read(b)
}

func (c *mockNetConn) Write(b []byte) (n int, err error) {
	panic("not implemented") // TODO: Implement
}

func (c *mockNetConn) Close() error {
	return nil
}

func (c *mockNetConn) LocalAddr() net.Addr {
	panic("not implemented") // TODO: Implement
}

func (c *mockNetConn) RemoteAddr() net.Addr {
	panic("not implemented") // TODO: Implement
}

func (c *mockNetConn) SetDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}

func (c *mockNetConn) SetReadDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}

func (c *mockNetConn) SetWriteDeadline(t time.Time) error {
	panic("not implemented") // TODO: Implement
}
