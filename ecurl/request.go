package ecurl

import (
	"fmt"
	"io"
	"net"
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

type Request struct {
	Method  string
	Host    string
	Path    string
	Port    int
	Headers map[string]string
	Body    io.Reader
}

func (r *Request) AddHeader(key, value string) *Request {
	r.Headers[key] = value
	return r
}

func (r *Request) SetHeaders(headers map[string]string) *Request {
	r.Headers = make(map[string]string, len(headers))
	for k, v := range headers {
		r.Headers[k] = v
	}
	return r
}

type Response struct {
	Status     string
	StatusCode int
	Proto      string
	Headers    map[string]string
	Body       io.Reader
}

type UnsupportedHttpMethod string

func (e UnsupportedHttpMethod) Error() string {
	return fmt.Sprintf("unsupported http method '%v'", string(e))
}

func NewRequest(method string, url string, body io.Reader) (*Request, error) {
	if !isAcceptableMethod(method) {
		return nil, UnsupportedHttpMethod(method)
	}

	_, host, path, port, err := splitUrl(url)
	if err != nil {
		return nil, fmt.Errorf("error parsing url: %w", err)
	}

	return &Request{
		Method: method,
		Port:   port,
		Path:   path,
		Host:   host,
		Body:   body,

		// Request comes with some default headers...
		Headers: map[string]string{
			"User-Agent": "curl/7.68.0", // Pretending we are curl
			"Accept":     "*/*",         // By default we will accept anything
		},
	}, nil
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

	return &Response{}, nil
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
