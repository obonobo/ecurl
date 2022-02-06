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

	h[http.CanonicalHeaderKey(split[0])] = split[1]
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
	req, err := ecurl.NewRequest(ecurl.GET, params.Url, nil)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	req.Headers.AddAll(params.Headers)
	r, err := ecurl.Do(req)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer r.Body.Close()

	_, err = io.Copy(os.Stdout, r.Body)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}

	return 0
}
