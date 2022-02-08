package ecurl

import (
	"fmt"
	"io"
	"strings"
)

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
