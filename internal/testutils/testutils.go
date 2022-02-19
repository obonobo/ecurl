package testutils

import (
	"bufio"
	"bytes"
	"compress/gzip"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/obonobo/ecurl/echoserver"
)

// Spins up the echo server in the background, waits 30 sec max for server to
// respond on root url
func BackgroundServer(port ...int) (close func(), err error) {
	p := 8181
	if len(port) > 0 && port[0] > 0 {
		p = port[0]
	}
	return CustomBackgroundServer(p, nil)
}

func CustomBackgroundServer(port int, handler http.HandlerFunc) (close func(), err error) {
	addr := fmt.Sprintf(":%v", port)
	url := fmt.Sprintf("http://localhost%v/", addr)
	wait := 30 * time.Second
	errc := make(chan error, 1)
	sleep := func() { time.Sleep(100 * time.Millisecond) }

	var errcc <-chan error
	if handler == nil {
		close, errcc = echoserver.EchoServer(addr)
	} else {
		close, errcc = echoserver.CustomEchoServer(addr, nil, handler)
	}

	// Wait for server to respond, 30 sec timeout
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
				errc <- nil
				return
			}
			sleep()
		}
	}()

	return close, <-errc
}

// Starts the background server by calling the below `backgroundServer`
// function, fails the test if the function returns an error
func MustBackgroundServer(t *testing.T, port ...int) (close func()) {
	p := 8181
	if len(port) > 0 {
		p = port[0]
	}
	return MustCustomBackgroundServer(t, p, nil)
}

func MustCustomBackgroundServer(t *testing.T, port int, handler http.HandlerFunc) (close func()) {
	close, err := CustomBackgroundServer(port, handler)
	if err != nil {
		t.Fatalf("Server failed to start: %v", err)
	}
	return close
}

// Polls the background server until it stops responding
func WaitForBackgroundServerToShutdown(url string) {
	timeout := time.After(10 * time.Second)
	for {
		select {
		case <-timeout:
			return
		default:
		}
		if _, err := http.Get(url); err != nil {
			return
		}
		time.Sleep(100 * time.Millisecond)
	}
}

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
		io.Copy(old, bytes.NewBuffer(buf.Bytes())) // Also tee to stdout
		out <- buf.String()
	}()

	<-ready
	return func() string {
		w.Close()
		<-done
		return <-out
	}, nil
}

// Tails the output from the CLI. Works the same as the `tail` command line
// tool; you can use negative n if you want to trim lines off the top, postive n
// will return you n lines off the bottom
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
	default:
		return data
	}
}

func TrimWhiteSpace(s string) string {
	whitespace := " \r\n\t"
	return strings.Trim(s, whitespace)
}

func TrimLeftWhiteSpace(s string) string {
	whitespace := " \r\n\t"
	return strings.TrimLeft(s, whitespace)
}

func TrimRightWhiteSpace(s string) string {
	whitespace := " \r\n\t"
	return strings.TrimRight(s, whitespace)
}

// Returns the gzip encoded version of your string
func Gzipup(s string) (string, error) {
	w := bytes.NewBuffer(make([]byte, 0, len(s)))
	gzipped := gzip.NewWriter(w)
	gzipped.Write([]byte(s))
	if err := gzipped.Close(); err != nil {
		return "", err
	}
	return w.String(), nil
}
