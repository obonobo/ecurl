package ecurl

import "io"

type Response struct {
	Status     string
	StatusCode int
	Proto      string
	Headers    Headers
	Body       io.ReadCloser
}
