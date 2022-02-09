package cmd

import (
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/obonobo/ecurl/internal/testutils"
)

const (
	url  = "http://localhost:8181/"
	addr = ":8181"
	tool = "ecurl"
)

// Special trim function, should only be used in these tests (because it also
// removes tab + carriage returns from the string)
var trim = func(s string) string {
	return strings.
		ReplaceAll(strings.
			ReplaceAll(strings.
				Trim(s, " \n\r\t"),
				"\t", ""),
			"\r", "")
}

// Tests POST requests with data read from file
func TestPostDataFromFile(t *testing.T) {
	close := testutils.MustBackgroundServer(t)
	defer close()

	// Function for creating the CLI args
	cmd := func(file string, verbose bool) []string {
		ret := make([]string, 0, 5)
		ret = append(ret, []string{tool, POST}...)
		if verbose {
			ret = append(ret, "--verbose")
		}
		ret = append(ret, []string{"--file", file}...)
		ret = append(ret, url)
		return ret
	}

	for _, tc := range []struct {
		name    string
		data    string
		verbose bool
		exit    int
		output  string
	}{
		{
			name:    "Hello World",
			verbose: false,
			exit:    0,
			data:    "Hello World",
			output: `
			POST / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Content-Length: 11
			Connection: close
			User-Agent: ecurl/0.1.0

			Hello World
			`,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			tmp, delete := mustCreateTempFile(t, "tmp-TestPostDataFromFile-*.txt", tc.data)
			defer delete()
			args := cmd(tmp.Name(), tc.verbose)
			assertCliOutput(t, args, tc.exit, tc.output)
		})
	}
}

// Tests some simple GET and POST requests against the EchoServer
func TestGetAndPostSuccess(t *testing.T) {
	close := testutils.MustBackgroundServer(t)
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
			Accept-Encoding: gzip
			Connection: close
			User-Agent: ecurl/0.1.0
			`,
		},
		{
			name: fmt.Sprintf("%v --verbose %v", GET, url),
			args: []string{tool, GET, "--verbose", url},
			exit: 0,
			output: `
			HTTP/1.1 200 OK
			Content-Length: 113
			Content-Type: text/plain; charset=utf-8
			Date: Mon, 07 Feb 2022 18:11:54 GMT

			GET / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Connection: close
			User-Agent: ecurl/0.1.0
			`,
		},

		// POST, no body data
		{
			name: fmt.Sprintf("no body %v %v", POST, url),
			args: []string{tool, POST, url},
			exit: 0,
			output: `
			POST / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Content-Length: 0
			User-Agent: ecurl/0.1.0
			Connection: close
			`,
		},
		{
			name: fmt.Sprintf("no body %v --verbose %v", POST, url),
			args: []string{tool, POST, "--verbose", url},
			exit: 0,
			output: `
			HTTP/1.1 200 OK
			Content-Length: 133
			Content-Type: text/plain; charset=utf-8
			Date: Mon, 07 Feb 2022 18:11:54 GMT

			POST / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Content-Length: 0
			User-Agent: ecurl/0.1.0
			Connection: close
			`,
		},

		// POST, with inline body data
		{
			name: fmt.Sprintf("inline body %v --data 'Hello\\n' %v", POST, url),
			args: []string{tool, POST, "--data", "Hello\n", url},
			exit: 0,
			output: `
			POST / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Content-Length: 6
			Connection: close
			User-Agent: ecurl/0.1.0

			Hello
			`,
		},
		{
			name: fmt.Sprintf("inline body %v --data 'Hello\\n' --verbose %v", POST, url),
			args: []string{tool, POST, "--data", "Hello\n", "--verbose", url},
			exit: 0,
			output: `
			HTTP/1.1 200 OK
			Content-Length: 141
			Content-Type: text/plain; charset=utf-8
			Date: Mon, 07 Feb 2022 18:11:54 GMT

			POST / HTTP/1.1
			Host: localhost
			Accept: */*
			Accept-Encoding: gzip
			Content-Length: 6
			Connection: close
			User-Agent: ecurl/0.1.0

			Hello
			`,
		},
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

	if err := lineSetEqual("expected", "actual", expected, actual); err != nil {
		t.Error(err)
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

// Checks equality of two sets. Returns an error if the sets are not equal.
func lineSetEqual(name1, name2 string, set1, set2 map[string]struct{}) error {
	setOneSubSetTwo := make([]string, 0, len(set1))
	setTwoSubSetOne := make([]string, 0, len(set2))

	// Fill setOneSubSetTwo
	for k := range set1 {
		if _, ok := set2[k]; !ok {
			setOneSubSetTwo = append(setOneSubSetTwo, k)
		}
	}

	// Fill setTwoSubSetOne
	for k := range set2 {
		if _, ok := set1[k]; !ok {
			setTwoSubSetOne = append(setTwoSubSetOne, k)
		}
	}

	toString := func(strs []string) (ret string) {
		ret = "{"
		for _, s := range strs {
			ret += fmt.Sprintf(`"%s", `, s)
		}
		return ret[:len(ret)-2] + "}"
	}

	l1, l2 := len(setOneSubSetTwo), len(setTwoSubSetOne)
	switch {
	case l1 > 0 && l2 > 0:
		return fmt.Errorf(
			"%v is missing %v from %v, %v is missing %v from %v",
			name2, toString(setOneSubSetTwo), name1,
			name1, toString(setTwoSubSetOne), name2)
	case l1 > 0:
		return fmt.Errorf(
			"%v is missing %v from %v",
			name2, toString(setOneSubSetTwo), name1)
	case l2 > 0:
		return fmt.Errorf(
			"%v is missing %v from %v",
			name1, toString(setTwoSubSetOne), name2)
	default:
		return nil
	}
}

func mockStdoutStderr(t *testing.T) (output func() string) {
	output, err := testutils.MockStdoutStderr()
	if err != nil {
		t.Fatalf("Failed to record stdout/stderr: %v", err)
	}
	return output
}

// Creates a temporary file with the provided data, if this operation fails,
// your test will be FailNow-ed with an error. Returns a function that can be
// used for deleting the file
func mustCreateTempFile(
	t *testing.T,
	namePattern, contents string,
) (file *os.File, delete func()) {
	fh, err := os.CreateTemp(".", namePattern)
	if err != nil {
		t.Fatalf("Got an error when trying to create "+
			"file (pattern '%v'): %v", namePattern, err)
	}
	delete = func() {
		fh.Close()
		os.Remove(fh.Name())
	}
	if _, err := fh.Write([]byte(contents)); err != nil {
		delete()
		t.Fatalf("Failed to write data to file '%v'", fh.Name())
	}
	if _, err := fh.Seek(0, 0); err != nil {
		delete()
		t.Fatalf("Failed to seek to beginning of file '%v'", fh.Name())
	}
	return fh, delete
}
