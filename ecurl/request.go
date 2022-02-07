package ecurl

import (
	"bytes"
	"fmt"
	"io"
	"net"
	"net/textproto"
	"path"
	"strconv"
	"strings"
)

type UnsupportedProtoError string

func (e UnsupportedProtoError) Error() string {
	return fmt.Sprintf("protocol '%v' is not supported", string(e))
}

const (
	HTTP  = "http"  // Acceptable protocol #1
	HTTPS = "https" // Acceptable protocol #2

	GET  = "GET"  // Acceptable method #1
	POST = "POST" // Acceptable method #1
)

func isAcceptableProto(proto string) bool {
	return proto == HTTP || proto == HTTPS
}

func isAcceptableMethod(method string) bool {
	return method == GET || method == POST
}

type Response struct {
	Status     string
	StatusCode int
	Proto      string
	Headers    Headers
	Body       io.ReadCloser
}

type Headers map[string]string

func (h Headers) AddAll(headers map[string]string) {
	for k, v := range headers {
		h.Add(k, v)
	}
}

func (h Headers) Add(key, value string) {
	h[textproto.CanonicalMIMEHeaderKey(key)] = value
}

func (h Headers) Del(key string) {
	delete(h, textproto.CanonicalMIMEHeaderKey(key))
}

func (h Headers) Write(w io.Writer) error {
	for k, v := range h {
		if _, err := w.Write([]byte(fmt.Sprintf("%v: %v\r\n", k, v))); err != nil {
			return fmt.Errorf("Headers.Write: %w", err)
		}
	}
	return nil
}

func (h Headers) Printout() (out string) {
	for k, v := range h {
		out += fmt.Sprintf("%v: %v\n", k, v)
	}
	return out
}

type Request struct {
	Method  string
	Host    string
	Path    string
	Port    int
	Headers Headers
	Body    io.Reader
}

func (r *Request) String() string {
	return fmt.Sprintf(""+
		"Request[Method=%v, Host=%v, "+
		"Path=%v, Port=%v, headers=%v, Body=%v]",
		r.Method, r.Host, r.Path, r.Port, r.Headers, r.Body)
}

type UnsupportedHttpMethod string

func (e UnsupportedHttpMethod) Error() string {
	return fmt.Sprintf("unsupported http method '%v'", string(e))
}

func NewRequest(method string, url string, body io.Reader) (*Request, error) {
	method = strings.ToUpper(method)
	if !isAcceptableMethod(method) {
		return nil, UnsupportedHttpMethod(method)
	}

	_, host, path, port, err := splitUrl(url)
	if err != nil {
		return nil, fmt.Errorf("error parsing url: %w", err)
	}

	r := &Request{
		Method: method,
		Port:   port,
		Path:   path,
		Host:   host,
		Body:   body,

		// Request comes with some default headers...
		Headers: Headers{
			"User-Agent": "curl/7.68.0", // Pretending we are curl
			"Accept":     "*/*",         // By default we will accept anything
			"Host":       host,          // Computes host header from params
		},
	}

	// If the body is of a type that supports reporting its length, then we can
	// automatically compute the Content-Length header
	if x, ok := body.(interface{ Len() int }); ok {
		r.Headers.Add("Content-Length", fmt.Sprintf("%v", x.Len()))
	} else if body == nil {
		r.Headers.Add("Content-Length", "0")
	}

	return r, nil
}

// Execute a POST request on the url with the provided content type and body
func Post(url, contentType string, body io.Reader) (*Response, error) {
	req, err := NewRequest(POST, url, body)
	if err != nil {
		return nil, fmt.Errorf("Post(%v, ...) failed: %w", url, err)
	}
	return Do(req)
}

// Executes a GET request on the url
func Get(url string) (*Response, error) {
	req, err := NewRequest(GET, url, nil)
	if err != nil {
		return nil, fmt.Errorf("Get(%v) failed: %w", url, err)
	}
	return Do(req)
}

// Executes a request through a new TCP connection. Uses HTTP/1.1
func Do(req *Request) (*Response, error) {
	conn, err := net.Dial("tcp", fmt.Sprintf("%v:%v", req.Host, req.Port))
	if err != nil {
		return nil, fmt.Errorf("tcp dial error: %w", err)
	}

	// Write request line
	if err := writeRequestLine(conn, req); err != nil {
		conn.Close()
		return nil, err
	}

	// Write headers
	if err := writeHeaders(conn, req); err != nil {
		conn.Close()
		return nil, err
	}

	// Write body (if it is present)
	if err := writeBody(conn, req); err != nil {
		conn.Close()
		return nil, err
	}

	// Return response
	resp, err := readResponse(conn)
	if err != nil {
		conn.Close()
	}
	return resp, err
}

func readResponse(conn net.Conn) (*Response, error) {
	response := &Response{Body: io.NopCloser(bytes.NewBufferString(""))}

	// 1 MB buffer for reading response
	buf := buffer{make([]byte, 1<<20), 0}

	// Read status line
	if err := buf.readStatusLine(response, conn); err != nil {
		conn.Close()
		return nil, err
	}

	// Read response headers
	if err := buf.readHeaders(response, conn); err != nil {
		conn.Close()
		return response, err
	}

	// Read Content-Length header
	cl, err := contentLength(response)
	if err != nil {
		return response, err
	}
	if cl == 0 {
		conn.Close()
		return response, nil
	}

	response.Body = &reader{conn: conn, buf: &buf, clen: cl}
	return response, nil
}

func contentLength(response *Response) (int, error) {
	cl, ok := response.Headers["Content-Length"]
	if !ok {
		return 0, fmt.Errorf("'Content-Length' header is not present in response")
	}
	contentLength, err := strconv.Atoi(cl)
	if err != nil {
		return 0, fmt.Errorf("'Content-Length' header is not valid: %w", err)
	}
	return contentLength, nil
}

type buffer struct {
	b   []byte
	red int
}

// Reads the status line from the io.Reader r, storing it in the response. Note
// that this is the first read into the buffer so there is no read loop, if the
// buffer is not big enough to read the status line, we return an error
func (buf *buffer) readStatusLine(response *Response, r io.Reader) error {
	// Chug
	nn, err := r.Read(buf.b)
	if err != nil && nn == 0 {
		return fmt.Errorf("failed to read response from socket: %w", err)
	}
	buf.b = buf.b[:nn]

	// Read status line
	line, n, err := scanLine(buf.b)
	buf.red += n
	if err != nil {
		return fmt.Errorf("error scanning response line: %w", err)
	}
	split := strings.Split(line, " ")

	// Status line should split in 3
	if len(split) < 2 {
		return fmt.Errorf("malformed status line: '%v'", line)
	}

	response.Proto = split[0]
	response.StatusCode, err = strconv.Atoi(split[1])
	if err != nil {
		return fmt.Errorf("failed to parse status code from status line: %v", err)
	}
	response.Status = strings.Join(split[1:], " ")
	return nil
}

// Reads the response headers from the buffer, storing them in the reponse, if
// necessary this method will load more data from the conn into the buffer to
// continue reading headers
func (buf *buffer) readHeaders(response *Response, conn io.Reader) error {
	response.Headers = make(map[string]string, 20)
	for exit := false; !exit; {
		if exit && buf.red == len(buf.b)-1 {
			return fmt.Errorf("" +
				"malformed response, " +
				"headers have not been properly ended with '\r\n'")
		}

		line, n, err := scanLine(buf.b[buf.red:])
		if err != nil {
			if exit {
				// Then we have read all that we can read and the remainder of
				// the buffer is malformed, mark the remainder as read and
				// return an error
				buf.red = len(buf.b) - 1
				continue
			}
			if buf.red == 0 {
				// Our buffer is not big enough to handle the next line
				// (unlikely), return an error
				return fmt.Errorf("buffer is not big enough: %w", err)
			}

			// Then we need to read more, discard the read portion of the buffer
			// and read from the socket again
			copy(buf.b, buf.b[buf.red:])
			unread := len(buf.b) - buf.red
			buf.red = 0
			nn, err := conn.Read(buf.b[unread:])
			buf.b = buf.b[:unread+nn]
			if err != nil {
				exit = true
				// conn.Close()
			}
			continue
		}
		buf.red += n

		if line == "" {
			// Done reading headers
			break
		}

		split := strings.Split(line, ":")
		if len(split) < 2 {
			// Then we have a malformed header (e.g.: Content-Length\r\n) We
			// will handle it by just assuming that they meant to place a colon
			// and left an empty value (not sure if that is RFC legal)
			key := textproto.CanonicalMIMEHeaderKey(line)
			response.Headers[key] = ""
			continue
		}
		key := textproto.CanonicalMIMEHeaderKey(split[0])
		response.Headers[key] = strings.Trim(strings.Join(split[1:], ":"), " ")
	}
	return nil
}

func writeRequestLine(w io.Writer, req *Request) error {
	_, err := fmt.Fprintf(w,
		"%v %v %v\r\n",
		strings.ToUpper(req.Method),
		req.Path,
		"HTTP/1.1")
	if err != nil {
		return fmt.Errorf("error writing http request line: %w", err)
	}
	return nil
}

func writeHeaders(w io.Writer, req *Request) error {
	var out string
	for k, v := range req.Headers {
		out += fmt.Sprintf("%v: %v\r\n", k, v)
	}
	out += "\r\n"
	_, err := w.Write([]byte(out))
	if err != nil {
		return fmt.Errorf("error writing http request headers: %w", err)
	}
	return nil
}

func writeBody(w io.Writer, req *Request) error {
	if req.Body != nil {
		_, err := io.Copy(w, req.Body)
		if err != nil {
			return fmt.Errorf("error writing request body: %w", err)
		}
	}
	return nil
}

// A Content-Length limited io.ReadCloser that wraps the raw TCP connection. The
// reader will use buf as a buffer while reading - it will read from the socket
// in chunks of length len(buf.b), and will return io.EOF once it has read up to
type reader struct {
	conn net.Conn // TCP connection
	buf  *buffer  // Buffer for reading - it will be around 1MB
	clen int      // Content-Length
	read int      // Amount read
	err  error    // Recorded error
}

var ErrResponseBodyClosed = fmt.Errorf("reader already closed")

func (r *reader) Close() error {
	// Free the buffer
	r.buf = nil

	// Register a closed error
	r.err = ErrResponseBodyClosed

	// Close the TCP socket
	return r.conn.Close()
}

// Reads up to r.contentLength, or up until the server closes the connection.
// Reads are buffered by the reader's internal buffer
func (r *reader) Read(b []byte) (int, error) {
	if r.err != nil {
		return 0, r.err
	}

	left := r.clen - r.read
	if left <= 0 {
		r.err = io.EOF
		return 0, r.err
	}

	to := left
	if remainderBuf := len(r.buf.b) - r.buf.red; to > remainderBuf {
		to = remainderBuf
	}

	// Read as much as possible
	var read int
	slice := r.buf.b[r.buf.red : r.buf.red+to]
	n := copy(b, slice)
	r.buf.red += n
	r.read += n
	read += n

	if n == len(b) {
		// Then we've satisfied the whole read request
		return read, nil
	}

	// Also check for EOF from Content-Length
	left = r.clen - r.read
	if left <= 0 {
		r.err = io.EOF
		return read, r.err
	}

	// Otherwise, we will have to read more data from the socket
	for {
		if r.err != nil {
			return read, r.err
		}

		left = r.clen - r.read
		if left <= 0 {
			return read, io.EOF
		}

		to = left
		if remainderBuf := len(r.buf.b) - r.buf.red; to > remainderBuf {
			to = remainderBuf
			if remainderBuf == 0 {
				// Then reset the buffer
				r.buf.red = 0
				r.buf.b = r.buf.b[:cap(r.buf.b)]
				to = len(r.buf.b)
			}
		}

		n, err := r.conn.Read(r.buf.b[r.buf.red : r.buf.red+to])
		r.buf.b = r.buf.b[r.buf.red:n]
		if err != nil {
			// Record the error, but still read the contents of the buffer
			r.err = fmt.Errorf("tcp read: %w", err)
		}

		n = copy(b[read:], r.buf.b[r.buf.red:])
		r.buf.red += n
		r.read += n
		read += n

		if read == len(b) {
			// We've read as much as we can
			return read, nil
		}
	}
}

type InvalidUrlError string

func (e InvalidUrlError) Error() string {
	return fmt.Sprintf("invalid url '%v'", string(e))
}

func splitUrl(u string) (proto, host, pth string, port int, err error) {

	split := strings.Split(u, "/")
	if len(split) < 3 || split[1] != "" || split[0][len(split[0])-1] != ':' {
		return "", "", "", 0, InvalidUrlError(u)
	}

	// PROTOCOL
	proto = strings.TrimRight(split[0], ":")
	if !isAcceptableProto(proto) {
		return "", "", "", 0,
			fmt.Errorf(
				"cannot split request url ('%v'): %w",
				u, UnsupportedProtoError(proto))
	}

	// HOST
	spltt := strings.Split(split[2], ":")
	switch len(spltt) {
	case 2:
		host = spltt[0]
		p, err := strconv.Atoi(spltt[1])
		port = p
		if err != nil {
			return "", "", "", 0,
				fmt.Errorf("invalid port in: %w", InvalidUrlError(proto))
		}
	case 1:
		host = spltt[0]
		port = 80
		if proto == "https" {
			port = 443
		}
	default:
		return "", "", "", 0,
			fmt.Errorf("invalid host string: %w", InvalidUrlError(proto))
	}

	// PATH
	pth = "/" + path.Join(split[3:]...)
	return proto, host, pth, port, nil
}

var errLineTooLong = fmt.Errorf("line is too long")

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
		fmt.Errorf("read %v bytes without a newline: %w", len(buf), errLineTooLong)
}
