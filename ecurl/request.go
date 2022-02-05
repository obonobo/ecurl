package ecurl

import (
	"bufio"
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
	Body       io.Reader
}

type Headers map[string]string

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

	// Write body (if it is present)
	if req.Body != nil {
		_, err := io.Copy(conn, req.Body)
		if err != nil {
			return nil, fmt.Errorf("error writing request body: %w", err)
		}
	}

	scnr := bufio.NewScanner(conn)

	// Read status line
	var statusLine string
	if scnr.Scan() {
		statusLine = scnr.Text()
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
		responseHeaders[split[0]] = strings.Join(split[1:], ":")
	}

	// Read body

	return &Response{
		Status:  statusLine,
		Headers: responseHeaders,
	}, nil
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
