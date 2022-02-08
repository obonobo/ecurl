package testutils

import (
	"bufio"
	"bytes"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/obonobo/ecurl/echoserver"
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

// Spins up the echo server in the background, waits 30 sec max for server to
// respond on root url
func BackgroundServer(port ...int) (close func(), err error) {
	p := 8181
	if len(port) > 0 && port[0] > 0 {
		p = port[0]
	}
	addr := fmt.Sprintf(":%v", p)
	url := fmt.Sprintf("http://localhost%v/", addr)
	wait := 30 * time.Second
	errc := make(chan error, 1)
	close, errcc := echoserver.EchoServer(addr)

	// Wait for server to respond, 60 sec timeout
	go func() {
		timeout := time.After(wait)
		for {
			select {
			case <-timeout:
				errc <- fmt.Errorf("timeout (%v) waiting for server to start", wait)
				close()
				return
			case e := <-errcc:
				errc <- e
				close()
				return
			default:
			}

			resp, err := http.Get(url)
			if err == nil && resp.StatusCode == http.StatusOK {
				// Server is responsive
				errc <- nil
				return
			}
			time.Sleep(50 * time.Millisecond)
		}
	}()

	return close, <-errc
}

// Starts the background server by calling the below `backgroundServer`
// function, fails the test if the function returns an error
func MustBackgroundServer(t *testing.T, port ...int) (close func()) {
	close, err := BackgroundServer(port...)
	if err != nil {
		t.Fatalf("Server failed to start: %v", err)
	}
	return close
}
