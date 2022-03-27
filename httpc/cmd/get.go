package cmd

import (
	"bytes"
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"

	"github.com/obonobo/ecurl/ecurl"
)

const GET = "get"

var GetUsage = strings.TrimLeft(`
usage: %v %v [-v] [-h "k:v"]* [-o file] [-L] URL

%v performs an HTTP GET request on URL

Flags:

	-v, --verbose
		Enables verbose output.

	-h, --header
		Adds a header to your request.

	-o, --output
		Saves the response body to a file. Verbose output will still be
		printed to STDERR, not to the file specified by this flag.

	-L, --location
		Follow redirects up to 5 times.
`, "\n\r\t ")

type HeadersFlagValue map[string]string

func (h HeadersFlagValue) String() string {
	return ""
}

func (h HeadersFlagValue) Set(value string) error {
	if value == "" {
		return nil
	}
	split := strings.Split(value, ":")
	if len(split) < 2 {
		return fmt.Errorf(`format should be "k:v"`)
	}
	trim := func(s string) string { return strings.Trim(s, " \t\n\r") }
	h[http.CanonicalHeaderKey(trim(split[0]))] = trim(strings.Join(split[1:], ":"))
	return nil
}

type GetParams struct {
	FollowRedirects bool
	Output          string
	Url             string
	Verbose         bool
	Headers         map[string]string
}

func getCmd(config *Config) (usage func(), action func(args []string) int) {
	getCmd := flag.NewFlagSet(GET, flag.ExitOnError)
	getCmd.Usage = func() {
		fmt.Printf(
			GetUsage,
			config.Command,
			GET,
			strings.ToUpper(GET[:1])+GET[1:])
	}

	// Verbose
	getCmdVerbose := getCmd.Bool("verbose", false, "")
	getCmd.BoolVar(getCmdVerbose, "v", false, "")

	// Headers
	hfv := make(HeadersFlagValue, 10)
	getCmd.Var(&hfv, "h", "")
	getCmd.Var(&hfv, "header", "")

	// Output file
	getCmdOutputFile := getCmd.String("output", "", "")
	getCmd.StringVar(getCmdOutputFile, "o", "", "")

	// Follow redirects
	getCmdFollowRedirects := getCmd.Bool("location", false, "")
	getCmd.BoolVar(getCmdFollowRedirects, "L", false, "")

	return getCmd.Usage, func(args []string) int {
		getCmd.Parse(args)

		url := getCmd.Arg(0)

		if url == "" {
			fmt.Fprintln(os.Stderr, "Please provide a url.")
			fmt.Fprintf(os.Stderr,
				`usage: %v %v [-v] [-h "k:v"]* URL`+"\n", config.Command, GET)
			return 2
		}

		return Get(GetParams{
			Url:             url,
			FollowRedirects: *getCmdFollowRedirects,
			Output:          *getCmdOutputFile,
			Verbose:         *getCmdVerbose,
			Headers:         hfv,
		})
	}
}

func Get(params GetParams) (exit int) {
	return makeRequest(params, ecurl.GET, nil, 0)
}

func makeRequest(params GetParams, method string, body io.Reader, length int) (exit int) {
	bodyCopy := []byte{}
	if body != nil {
		cp, err := io.ReadAll(body)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Failed to read request body: %v\n", err)
			return 1
		}
		bodyCopy = cp
	}

	req, err := ecurl.NewRequest(method, params.Url, io.NopCloser(bytes.NewBuffer(bodyCopy)))
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	// Add headers
	req.Headers.AddAll(params.Headers)
	if strings.ToLower(method) != GET {
		req.Headers.Add("Content-Length", fmt.Sprintf("%v", length))
	}

	r, err := ecurl.Do(req)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	rrr := r
	defer rrr.Body.Close()

	bodyOut := os.Stdout
	if params.Output != "" {
		fh, err := os.Create(params.Output)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Failed to open file: %v\n", err)
		}
		defer fh.Close()
		bodyOut = fh
	}

	followRedirects := 5
	if !params.FollowRedirects {
		followRedirects = 0
	}

	for i := 0; ; i++ {
		if followRedirects == 0 {
			break
		}

		rr, needs := needsRedirect(r, req)
		if !needs {
			break
		}
		if i >= followRedirects {
			fmt.Fprintf(os.Stderr,
				"Maximum number of redirects (%v) exceeded...\n", followRedirects)
			return 1
		}

		// If we need to follow a redirect, then print this current response
		// without a body
		clone := r.Clone()
		clone.Body = io.NopCloser(bytes.NewBufferString(""))
		printResponse(bodyOut, nil, clone, params.Verbose)

		rr.Body = io.NopCloser(bytes.NewBuffer(bodyCopy))
		r, err = ecurl.Do(rr)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 1
		}
		rrr := r
		defer rrr.Body.Close()
	}

	return printResponse(bodyOut, nil, r, params.Verbose)
}

// Parse response headers and determine if the response demands a redirect
func needsRedirect(resp *ecurl.Response, req *ecurl.Request) (r *ecurl.Request, needs bool) {
	is300 := resp.StatusCode < 300 || resp.StatusCode > 399
	if is300 {
		return nil, false
	}

	location, ok := resp.Headers["Location"]
	if !ok {
		return nil, false
	}

	rr, err := ecurl.NewRequest(req.Method, location, req.Body)
	if err != nil {
		return nil, false
	}

	return rr, true
}

func printResponse(fh, verboseOut *os.File, r *ecurl.Response, verbose bool) (exit int) {
	if fh == nil {
		fh = os.Stdout
	}
	if verboseOut == nil {
		verboseOut = os.Stderr
	}

	if verbose {
		fmt.Fprintf(verboseOut, "%v %v\n", r.Proto, r.Status)
		fmt.Fprintln(verboseOut, r.Headers.Printout())
	}
	if _, err := io.Copy(fh, r.Body); err != nil {
		fmt.Fprintln(verboseOut, err)
		return 1
	}
	return exit
}
