package ecurl

import (
	"bufio"
	"fmt"
	"strconv"
)

// Set of supported protocols
var protos = map[string]struct{}{
	"HTTP/1.1": struct{}{},
	"HTTP/1.0": struct{}{},
}

func isAcceptable(proto string) bool {
	for k := range protos {
		if proto == k {
			return true
		}
	}
	return false
}

type UnsupportedProtoError string

func (e UnsupportedProtoError) Error() string {
	return fmt.Sprintf("protocol '%v' is not supported", string(e))
}

type StatusLine struct {
	Proto      string
	Status     string
	StatusCode int
}

func ReadStatusLine(reader *bufio.Reader) (StatusLine, error) {
	failedToReadStatusLine := func(err error) (StatusLine, error) {
		return StatusLine{}, fmt.Errorf("failed to read response status line: %w", err)
	}

	proto, err := reader.ReadString(' ')
	if err != nil {
		return failedToReadStatusLine(err)
	} else if !isAcceptable(proto) {
		return StatusLine{}, UnsupportedProtoError(proto)
	}

	code, err := reader.ReadString(' ')
	if err != nil {
		return failedToReadStatusLine(err)
	}

	codeInt, err := strconv.Atoi(code)
	if err != nil {
		return failedToReadStatusLine(err)
	}

	msg, err := reader.ReadString('\n')
	if err != nil {
		return failedToReadStatusLine(err)
	}

	return StatusLine{
		Proto:      proto,
		Status:     fmt.Sprintf("%v %v", code, msg),
		StatusCode: codeInt,
	}, nil
}
