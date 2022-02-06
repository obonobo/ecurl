package cmd

import (
	"fmt"
	"net/http"
	"reflect"
	"strings"
	"testing"
	"time"

	"github.com/obonobo/ecurl/echoserver"
	"github.com/obonobo/ecurl/internal/testutils"
)

const (
	url  = "http://localhost:8181/"
	addr = ":8181"
	tool = "ecurl"
)

// Special trim function, should only be used in these tests
var trim = func(s string) string {
	return strings.
		ReplaceAll(strings.
			ReplaceAll(strings.
				Trim(s, " \n\r\t"),
				"\t", ""),
			"\r", "")
}

func TestGetAndPostEchoServer(t *testing.T) {
	close := mustBackgroundServer(t)
	defer close()

	for _, tc := range []struct {
		name   string
		args   []string
		exit   int
		output string
	}{
		// GET
		{
			name: fmt.Sprintf("%v %v", GET, url),
			args: []string{tool, GET, url},
			exit: 0,
			output: `
			GET / HTTP/1.1
			Host: localhost
			Accept: */*
			Content-Length: 0
			User-Agent: curl/7.68.0
			`,
		},
		{
			name: fmt.Sprintf("%v --verbose %v", GET, url),
			args: []string{tool, GET, "--verbose", url},
			exit: 0,
			output: `
			HTTP/1.1 200 OK
			Content-Length: 90
			Content-Type: text/plain; charset=utf-8
			Date: Mon, 07 Feb 2022 18:11:54 GMT

			GET / HTTP/1.1
			Host: localhost
			Accept: */*
			Content-Length: 0
			User-Agent: curl/7.68.0
			`,
		},

		// POST, no body data
		// {
		// 	name: fmt.Sprintf("no body %v %v", POST, url),
		// 	args: []string{tool, POST, url},
		// 	exit: 0,
		// 	output: `
		// 	POST / HTTP/1.1
		// 	Host: localhost
		// 	Accept: */*
		// 	Content-Length: 0
		// 	User-Agent: curl/7.68.0
		// 	`,
		// },
		// {
		// 	name: fmt.Sprintf("no body %v --verbose %v", POST, url),
		// 	args: []string{tool, POST, "--verbose", url},
		// 	exit: 0,
		// 	output: `
		// 	HTTP/1.1 200 OK
		// 	Content-Length: 90
		// 	Content-Type: text/plain; charset=utf-8
		// 	Date: Mon, 07 Feb 2022 18:11:54 GMT

		// 	POST / HTTP/1.1
		// 	Host: localhost
		// 	Accept: */*
		// 	Content-Length: 0
		// 	User-Agent: curl/7.68.0
		// 	`,
		// },

		// // POST, with body data
		// {
		// 	name: fmt.Sprintf("no body %v %v", POST, url),
		// 	args: []string{tool, POST, url},
		// 	exit: 0,
		// 	output: `
		// 	POST / HTTP/1.1
		// 	Host: localhost
		// 	Accept: */*
		// 	Content-Length: 0
		// 	User-Agent: curl/7.68.0
		// 	`,
		// },
		// {
		// 	name: fmt.Sprintf("no body %v --verbose %v", POST, url),
		// 	args: []string{tool, POST, "--verbose", url},
		// 	exit: 0,
		// 	output: `
		// 	HTTP/1.1 200 OK
		// 	Content-Length: 90
		// 	Content-Type: text/plain; charset=utf-8
		// 	Date: Mon, 07 Feb 2022 18:11:54 GMT

		// 	POST / HTTP/1.1
		// 	Host: localhost
		// 	Accept: */*
		// 	Content-Length: 0
		// 	User-Agent: curl/7.68.0
		// 	`,
		// },
	} {
		t.Run(tc.name, func(t *testing.T) {
			assertCliOutput(t, tc.args, tc.exit, trim(tc.output))
		})
	}
}

// Runs the CLI tool, asserts exit code, and stdout/stderr output
func assertCliOutput(
	t *testing.T,
	args []string,
	expectedExitCode int,
	expectedOutput string,
) {
	// Run the CLI
	stopRecording := mockStdoutStderr(t)
	if exit := Run(args); exit != expectedExitCode {
		stopRecording()
		t.Errorf("Expected exit code %v but got %v", expectedExitCode, exit)
	}
	output := stopRecording()

	// Parse the actual and input (headers can be printed in any order)
	actual := lineSet(trim(output))
	expected := lineSet(trim(expectedOutput))

	// Assert the output
	if !reflect.DeepEqual(expected, actual) {
		t.Errorf("Expected CLI output '%v' but got '%v'", expected, actual)
	}
}

// Dumb parsing function, splits input into a set of lines which can be used to
// assert equality regardless of line order
func lineSet(s string) (lineSet map[string]struct{}) {
	lines := strings.Split(s, "\n")
	lineSet = make(map[string]struct{}, len(lines))
	for _, l := range lines {
		// Ignore "Data: ..." headers which will never match
		if strings.HasPrefix(l, "Date") {
			continue
		}
		lineSet[l] = struct{}{}
	}
	return lineSet
}

// Starts the background server by calling the below `backgroundServer`
// function, fails the test if the function returns an error
func mustBackgroundServer(t *testing.T) (close func()) {
	close, err := backgroundServer()
	if err != nil {
		t.Fatalf("Server failed to start: %v", err)
	}
	return close
}

// Spins up the echo server in the background, waits 30 sec max for server to
// respond on root url
func backgroundServer() (close func(), err error) {
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

func mockStdoutStderr(t *testing.T) (output func() string) {
	output, err := testutils.MockStdoutStderr()
	if err != nil {
		t.Fatalf("Failed to record stdout/stderr: %v", err)
	}
	return output
}
