package ecurl

import (
	"fmt"
	"io"
	"net/textproto"
)

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

func (h Headers) Write(w io.Writer) error {
	for k, v := range h {
		if _, err := w.Write([]byte(fmt.Sprintf("%v: %v\r\n", k, v))); err != nil {
			return fmt.Errorf("Headers.Write: %w", err)
		}
	}
	return nil
}

func (h Headers) Printout() (out string) {
	for k, v := range h {
		out += fmt.Sprintf("%v: %v\n", k, v)
	}
	return out
}
