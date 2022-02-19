package ecurl

import (
	"fmt"
	"io"
	"strings"
)

var defaultHeaders = Headers{
	"User-Agent":      "ecurl/0.1.0", // Custom user agent
	"Accept":          "*/*",         // By default we will accept anything
	"Accept-Encoding": "gzip",        // By default we can accept gzip
	"Connection":      "close",       // This tool doesn't really need to make more requests
}

type Request struct {
	Method  string
	Host    string
	Path    string
	Port    int
	Headers Headers
	Body    io.Reader

	url string
	tls bool
}

func (r *Request) Clone() (*Request, error) {
	rr, err := NewRequest(r.Method, r.url, r.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to clone request: %w", err)
	}
	rr.Headers.AddAll(r.Headers)
	return rr, nil
}

func (r *Request) String() string {
	return fmt.Sprintf(""+
		"Request[Method=%v, Host=%v, "+
		"Path=%v, Port=%v, Headers=%v, Body=%v]",
		r.Method, r.Host, r.Path, r.Port, r.Headers, r.Body)
}

// Creates a new Request with some computed default headers
func NewRequest(method string, url string, body io.Reader) (*Request, error) {
	r, host, err := newBlankRequest(method, url, body)
	if err != nil {
		return r, err
	}

	// Request comes with some default headers...
	r.Headers.AddAll(defaultHeaders)
	r.Headers.Add("Host", host)

	// If the body is of a type that supports reporting its length, then we can
	// automatically compute the Content-Length header
	if x, ok := body.(interface{ Len() int }); ok {
		r.Headers.Add("Content-Length", fmt.Sprintf("%v", x.Len()))
	} else if body == nil && strings.ToUpper(method) != GET {
		r.Headers.Add("Content-Length", "0")
	}

	return r, nil
}

// Creates a new Request with no headers attached. Use the above NewRequest
// function to auto-compute some useful client headers
func NewBlankRequest(method string, url string, body io.Reader) (*Request, error) {
	r, _, err := newBlankRequest(method, url, body)
	return r, err
}

func newBlankRequest(
	method string,
	urll string,
	body io.Reader,
) (
	r *Request,
	host string,
	err error,
) {
	method = strings.ToUpper(method)
	if !isAcceptableMethod(method) {
		return nil, "", UnsupportedHttpMethod(method)
	}

	_, host, path, port, tls, err := splitUrl(urll)
	if err != nil {
		return nil, "", fmt.Errorf("error parsing url: %w", err)
	}

	r = &Request{
		url:     urll,
		tls:     tls,
		Method:  method,
		Port:    port,
		Path:    path,
		Host:    host,
		Body:    body,
		Headers: make(Headers, 20),
	}

	return r, host, nil
}
