package ecurl

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"net"
	"net/textproto"
	"path"
	"strconv"
	"strings"
)

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
	Headers    map[string]string
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
	_, err = fmt.Fprintf(
		conn,
		"%v %v %v\r\n",
		strings.ToUpper(req.Method),
		req.Path,
		"HTTP/1.1")

	if err != nil {
		return nil, fmt.Errorf("error writing http request line: %w", err)
	}

	// Write headers
	for k, v := range req.Headers {
		fmt.Fprintf(conn, "%v: %v\r\n", k, v)
	}
	fmt.Fprint(conn, "\r\n")

	// Write body (if it is present)
	if req.Body != nil {
		_, err := io.Copy(conn, req.Body)
		if err != nil {
			conn.Close()
			return nil, fmt.Errorf("error writing request body: %w", err)
		}
	} else {
		// fmt.Fprintln(conn)
	}

	scnr := bufio.NewScanner(conn)

	// Read status line
	var status, proto string
	var statusCode int
	if scnr.Scan() {
		line := scnr.Text()
		split := strings.Split(line, " ")
		if len(split) > 0 {
			proto = split[0]
		}
		if len(split) > 1 {
			statusCode, err = strconv.Atoi(split[1])
			if err != nil {
				conn.Close()
				return nil, fmt.Errorf("failed to parse status code: %w", err)
			}
			status = strings.Join(split[1:], " ")
		}
	}

	// Read headers
	responseHeaders := make(map[string]string, 20)
	for scnr.Scan() {
		line := scnr.Text()
		if line == "" {
			break
		}
		split := strings.Split(line, ":")
		if len(split) == 0 {
			break
		}
		responseHeaders[split[0]] = strings.Trim(strings.Join(split[1:], ":"), " ")
	}

	// Read Content-Length header
	var contentLength int
	if cl, ok := responseHeaders["Content-Length"]; ok {
		contentLength, err = strconv.Atoi(cl)
		if err != nil {
			conn.Close()
			return nil, fmt.Errorf("'Content-Length' header is not valid: %w", err)
		}
	}

	response := &Response{
		Proto:      proto,
		Status:     status,
		StatusCode: statusCode,
		Headers:    responseHeaders,
		Body: &reader{
			Conn:          conn,
			contentLength: contentLength,
		},
	}

	if contentLength == 0 {
		conn.Close()
		response.Body = io.NopCloser(bytes.NewBufferString(""))
	}

	return response, nil
}

type reader struct {
	net.Conn
	contentLength int
	read          int
}

// Reads up to r.contentLength, or up until the server closes the connection
func (r *reader) Read(b []byte) (int, error) {
	if r.read >= r.contentLength {
		return 0, fmt.Errorf("Content-Length reached (%v bytes): %w",
			r.contentLength, io.EOF)
	}

	n := r.contentLength
	if len(b) < r.contentLength {
		n = len(b)
	}

	red, err := r.Conn.Read(b[:n])
	r.read += red
	return red, err
}

type InvalidUrlError string

func (e InvalidUrlError) Error() string {
	return fmt.Sprintf("invalid url '%v'", string(e))
}

func splitUrl(url string) (proto, host, pth string, port int, err error) {
	splt := strings.Split(url, "/")
	if len(splt) < 3 || splt[1] != "" || splt[0][len(splt)] != ':' {
		return "", "", "", 0, InvalidUrlError(url)
	}

	// PROTOCOL
	proto = splt[0][:len(splt)]
	if !isAcceptableProto(proto) {
		return "", "", "", 0,
			fmt.Errorf(
				"cannot split request url ('%v'): %w",
				url, UnsupportedProtoError(proto))
	}

	// HOST
	spltt := strings.Split(splt[2], ":")
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
	pth = "/" + path.Join(splt[3:]...)

	return proto, host, pth, port, nil
}
