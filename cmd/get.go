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

	getCmdVerbose := getCmd.Bool("verbose", false, "")
	getCmd.BoolVar(getCmdVerbose, "v", false, "")

	hfv := make(HeadersFlagValue, 10)
	getCmd.Var(&hfv, "h", "")
	getCmd.Var(&hfv, "header", "")

	return getCmd.Usage, func(args []string) int {
		getCmd.Parse(args)
		return Get(GetParams{
			Url:     getCmd.Arg(0),
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
	defer r.Body.Close()
	return printResponse(r, params.Verbose)
}

func printResponse(r *ecurl.Response, verbose bool) (exit int) {
	if verbose {
		fmt.Printf("%v %v\n", r.Proto, r.Status)
		fmt.Println(r.Headers.Printout())
	}
	if _, err := io.Copy(os.Stdout, r.Body); err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	return exit
}
