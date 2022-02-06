package testutils

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"os"
	"strings"
)

// Consumes the stdout and stderr of the current process, dumping them as a
// single string which is produced when you call the returned `close()`
// function.
func MockStdoutStderr() (close func() string, err error) {
	r, w, err := os.Pipe()
	if err != nil {
		return nil, fmt.Errorf("failed to mock stdout/stderr: %w", err)
	}

	out := make(chan string, 1)
	ready, done := make(chan struct{}, 1), make(chan struct{}, 1)
	go func() {
		old := os.Stdout
		olde := os.Stderr
		defer func() {
			r.Close()
			os.Stdout = old
			os.Stderr = olde
			done <- struct{}{}
		}()

		os.Stdout = w
		os.Stderr = w
		ready <- struct{}{}

		var buf bytes.Buffer
		io.Copy(&buf, r)
		io.Copy(old, bytes.NewBuffer(buf.Bytes()))
		out <- buf.String()
	}()

	<-ready
	return func() string {
		w.Close()
		<-done
		return <-out
	}, nil
}

// Tails the output from the CLI
func Tail(data string, n int) string {
	switch {
	case n < 0:
		reader := bytes.NewBufferString(data)
		for i := n; i < 0; i++ {
			reader.ReadString('\n')
		}
		return reader.String()
	case n > 0:
		lines := make([]string, 0, n*2)
		for reader := bufio.NewScanner(bytes.NewBufferString(data)); reader.Scan(); {
			lines = append(lines, reader.Text())
		}

		if n >= len(lines) {
			return strings.Join(lines, "\n")
		}
		return strings.Join(lines[len(lines)-n:], "\n")
	}
	return data
}
