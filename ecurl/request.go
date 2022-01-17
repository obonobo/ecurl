package ecurl

import (
	"io"
	"net/url"
)

type Request struct {
	Method  string
	Host    string
	Port    int
	Headers map[string]string
}

type Response struct {
	Headers map[string]string
	Body    io.ReadCloser
}

func NewRequest(method string, url *url.URL, body io.ReadCloser) *Request {
	return &Request{}
}

func Do(req *Request) *Response {
	return &Response{}
}
