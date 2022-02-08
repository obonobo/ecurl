package ecurl

import "fmt"

var ErrResponseBodyClosed = fmt.Errorf("reader already closed")

type InvalidUrlError string

func (e InvalidUrlError) Error() string {
	return fmt.Sprintf("invalid url '%v'", string(e))
}

type UnsupportedHttpMethod string

func (e UnsupportedHttpMethod) Error() string {
	return fmt.Sprintf("unsupported http method '%v'", string(e))
}

type UnsupportedProtoError string

func (e UnsupportedProtoError) Error() string {
	return fmt.Sprintf("protocol '%v' is not supported", string(e))
}
