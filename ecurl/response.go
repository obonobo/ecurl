package ecurl

import "io"

type Response struct {
	Status     string
	StatusCode int
	Proto      string
	Headers    Headers
	Body       io.ReadCloser
}

func (r *Response) Clone() *Response {
	return &Response{
		Status:     r.Status,
		StatusCode: r.StatusCode,
		Proto:      r.Proto,
		Headers:    r.Headers,
		Body:       r.Body,
	}
}
