package ecurl

import (
	"bytes"
	"crypto/tls"
	"errors"
	"fmt"
	"io"
	"net"
	"net/textproto"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// The default tls config that will be used by the client
var defaultTlsConfig = &tls.Config{
	InsecureSkipVerify: true,
}

// A regex for grabbing the Content-Type boundary field
var reContentTypeBoundary = regexp.MustCompile(`(?i)(boundary=)([\w\d]+)`)

// Execute a POST request on the url with the provided content type and body
func Post(url, contentType string, body io.Reader) (*Response, error) {
	req, err := NewRequest(POST, url, body)
	if err != nil {
		return nil, fmt.Errorf("Post(%v, ...) failed: %w", url, err)
	}
	req.Headers.Add("Content-Type", contentType)
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
	return do(req)
}

func do(req *Request, bufsize ...int) (*Response, error) {
	conn, err := createConn(req)
	if err != nil {
		return nil, err
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
	resp, err := readResponse(conn, bufsize...)
	if err != nil {
		conn.Close()
	}

	return resp, err
}

func createConn(req *Request) (net.Conn, error) {
	addr := fmt.Sprintf("%v:%v", req.Host, req.Port)
	if req.tls {
		conn, err := tls.Dial("tcp", addr, defaultTlsConfig)
		if err != nil {
			return nil, fmt.Errorf("tcp dial error: %w", err)
		}
		return conn, err
	}
	conn, err := net.Dial("tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("tcp dial error: %w", err)
	}
	return conn, err
}

func readResponse(conn net.Conn, bufsize ...int) (*Response, error) {
	response := &Response{Body: io.NopCloser(bytes.NewBufferString(""))}

	size := DefaultBufferSize
	if len(bufsize) > 0 {
		size = bufsize[0]
	}
	scnr := NewBufferedScanner(conn, size)

	// Read status line
	if err := readStatusLine(scnr, response); err != nil {
		conn.Close()
		return nil, err
	}

	// Read response headers
	if err := readHeaders(scnr, response); err != nil {
		conn.Close()
		return response, err
	}

	// Attach the appropriate body reader
	if bod := createBodyReader(response, conn, scnr); bod != nil {
		response.Body = bod
	}

	// Attach the gzip decoder if needed
	if needsGzip(response) {
		if r, err := NewGzipper(response.Body); err == nil {
			response.Body = r
		}
	}

	return response, nil
}

func needsGzip(response *Response) bool {
	if ce, ok := response.Headers["Content-Encoding"]; ok {
		return strings.ToLower(ce) == "gzip"
	}
	return false
}

// Creates the approriate body reader depending on how the response body should
// be read.
//
// We need to determine the length of the transfer per RFC 2616:
//
// 1. 1xx, 204, 304 => length = 0
//
// 2. Transfer-Encoding => determine length from chunked transfer coding
//
// 3. Content-Length => length = Content-Length
//
// 4. Media type "multipart/byteranges" => body delimites its own transfer length
//
// 5. Server closes connection...
func createBodyReader(
	response *Response,
	conn net.Conn,
	scnr *BufferedScanner,
) io.ReadCloser {
	// 1. Status code is 1xx, 204, 304, then this message is not supposed to
	// have a body per RFC
	if response.StatusCode == 204 ||
		response.StatusCode == 304 ||
		response.StatusCode >= 100 &&
			response.StatusCode <= 199 {
		// We will discard any body that the server has written to the socket
		conn.Close()
		return nil
	}

	// 2. Check for a Transfer-Encoding header, if it does not read "identity",
	// then we must read the body according to the "chunked" transfer coding
	if useChunked := chunkedCoded(response); useChunked {
		// Return a response whose body reader reads according to chunked
		// transfer coding
		return &chunkedReader{conn: conn, scnr: scnr}
	}

	// 3. Read Content-Length header
	if useContentLength, cl := contentLengthDelimited(response); useContentLength {
		if cl == 0 {
			conn.Close()
			return nil
		}
		return &contentLengthReader{conn: conn, scnr: scnr, clen: cl}
	}

	// 4. Media type `multipart/byteranges` does not require content length
	if useMpbr, boundary := multipartByterangesDelimited(response); useMpbr {
		// ??? noop for now
		// ???
		// ??? I don't want to enable this because it will actually decode the
		// ??? response as multipart/byteranges, which I'm not sure what I want
		// ??? as the intended behaviour of the CLI here. Like maybe we should
		// ??? just forward the body as is (not decoded) if it is
		// ??? multipart/byteranges, and the caller will have to decide
		// ??? themselves if they want to wrap their reader in a byteranges
		// ??? decoder.
		fmt.Printf("boundary: %v\n", boundary)
		// return &MultipartByteRangesReader{Boundary: boundary, Conn: conn, Reader: scnr}
	}

	// 5. Otherwise, we are supposed to read until the server closes the socket.
	// We will set a read deadline and we will reset that deadline if the server
	// sends some more data
	conn.SetReadDeadline(time.Now().Add(10 * time.Second))
	return &infiniteReader{conn: conn, scnr: scnr}
}

// According to the RFC, it is not mandatory to specify Content-Length when the
// response is multipart/byteranges delimited because the media type can
// self-delimit. Usually, Content-Length is specified in this kind of response.
// If that is the case, then the caller of this function should check
// Content-Length before trying to use this function.
func multipartByterangesDelimited(response *Response) (yes bool, boundary string) {
	ct, ok := response.Headers["Content-Type"]
	if !ok {
		return false, ""
	}
	if !strings.HasPrefix("multipart/byteranges", strings.ToLower(ct)) {
		return false, ""
	}
	if boundary = reContentTypeBoundary.FindStringSubmatch(ct)[2]; boundary == "" {
		return false, "" // Cannot determine boundary - is this an error?
	}
	return true, boundary
}

func contentLengthDelimited(response *Response) (yes bool, cl int) {
	cl, err := contentLength(response)
	if err != nil {
		if e := new(strconv.NumError); errors.As(err, &e) {
			// If content length is present but malformed, then we'll say this
			// is an empty transfer and drop the body data
			return true, 0
		}

		// Otherwise, Content-Length header is not present
		return false, 0
	}
	return true, cl
}

func chunkedCoded(response *Response) (yes bool) {
	te, ok := response.Headers["Transfer-Encoding"]
	if !ok {
		return false
	}
	isChunked := strings.ToLower(te) != "identity"
	return isChunked
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

// Reads the status line from the scnr, storing it in the response. Note that
// this is the first read into the buffer so there is no read loop, if the
// buffer is not big enough to read the status line, we return an error
func readStatusLine(scnr *BufferedScanner, response *Response) error {
	line, _, err := scnr.NextLine()
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
func readHeaders(scanner *BufferedScanner, response *Response) error {
	response.Headers = make(Headers, 20)

	for {
		line, _, err := scanner.NextLine()
		switch {
		case errors.Is(err, ErrLineTooLong):
			return err
		case errors.Is(err, ErrNoNewLinesLeft):
			return fmt.Errorf("" +
				"malformed response, " +
				"headers have not been properly ended with '\r\n'")
		}

		if line == "" {
			break // Done reading headers
		}

		split := strings.Split(line, ":")
		if len(split) < 2 {
			// Then we have a malformed header (e.g.: Content-Length\r\n) We
			// will handle it by just assuming that they meant to place a colon
			// and left an empty value (not sure if that is RFC legal)
			key := strings.Trim(textproto.CanonicalMIMEHeaderKey(line), " ")
			response.Headers[key] = ""
		} else {
			key := strings.Trim(textproto.CanonicalMIMEHeaderKey(split[0]), " ")
			response.Headers[key] = strings.Trim(strings.Join(split[1:], ":"), " ")
		}
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
	if req.Body == nil {
		return nil
	}
	_, err := io.Copy(w, req.Body)
	if err != nil {
		return fmt.Errorf("error writing request body: %w", err)
	}
	return nil
}
