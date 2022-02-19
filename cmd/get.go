package cmd

import (
	"flag"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"

	"github.com/obonobo/ecurl/ecurl"
)

const GET = "get"

const GetUsage = `usage: %v %v [-v] [-h "k:v"]* URL

%v performs an HTTP GET request on URL

Flags:

	-v, --verbose
		Enables verbose output.

	-h, --header
		Adds a header to your request.

	-o, --output
		Saves the response body to a file. Verbose output will still be
		printed to STDERR, not to the file specified by this flag.
`

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
	Output  string
	Url     string
	Verbose bool
	Headers map[string]string
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

	return getCmd.Usage, func(args []string) int {
		getCmd.Parse(args)
		return Get(GetParams{
			Url:     getCmd.Arg(0),
			Output:  *getCmdOutputFile,
			Verbose: *getCmdVerbose,
			Headers: hfv,
		})
	}
}

func Get(params GetParams) (exit int) {
	return makeRequest(params, ecurl.GET, nil, 0)
}

func makeRequest(params GetParams, method string, body io.Reader, length int) (exit int) {
	req, err := ecurl.NewRequest(method, params.Url, body)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	// Add headers``
	req.Headers.AddAll(params.Headers)
	if strings.ToLower(method) != GET {
		req.Headers.Add("Content-Length", fmt.Sprintf("%v", length))
	}

	r, err := ecurl.Do(req)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer r.Body.Close()

	bodyOut := os.Stdout
	if params.Output != "" {
		fh, err := os.Create(params.Output)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Failed to open file: %v\n", err)
		}
		defer fh.Close()
		bodyOut = fh
	}

	return printResponse(bodyOut, nil, r, params.Verbose)
}

func printResponse(
	fh, verboseOut *os.File,
	r *ecurl.Response,
	verbose bool,
) (exit int) {
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
