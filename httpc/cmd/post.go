package cmd

import (
	"bytes"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/obonobo/ecurl/ecurl"
)

const POST = "post"

var PostUsage = strings.TrimLeft(`
usage: %v %v [-v] [-h "k:v"]* [-d inline-data] [-f file] [-o file] [-L] URL

%v performs an HTTP POST request on URL

Flags:

	-v, --verbose
		Enables verbose output.

	-h, --header
		Adds a header to your request.

	-d, --data
		Add inline data to the body of your request.

	-f, --file
		Read the body of your request from a file. This flag takes lower
		precedence than the -d, --data flag; if both flags are specified
		the inline data will be used as the body of the request and the
		file will be ignored.

	-o, --output
		Saves the response body to a file. Verbose output will still be
		printed to STDERR, not to the file specified by this flag.

	-L, --location
		Follow redirects up to 5 times.
`, "\n\t\r ")

type PostParams struct {
	GetParams
	InlineData, File string
}

func postCmd(config *Config) (usage func(), action func(args []string) int) {
	postCmd := flag.NewFlagSet(GET, flag.ExitOnError)
	postCmd.Usage = func() {
		fmt.Printf(
			PostUsage,
			config.Command,
			POST,
			strings.ToUpper(POST[:1])+POST[1:])
	}

	// Verbose
	postCmdVerbose := postCmd.Bool("verbose", false, "")
	postCmd.BoolVar(postCmdVerbose, "v", false, "")

	// Headers
	hfv := make(HeadersFlagValue, 10)
	postCmd.Var(&hfv, "h", "")
	postCmd.Var(&hfv, "header", "")

	// Inline data
	postCmdData := postCmd.String("data", "", "")
	postCmd.StringVar(postCmdData, "d", "", "")

	// File
	postCmdFile := postCmd.String("file", "", "")
	postCmd.StringVar(postCmdFile, "f", "", "")

	// Output file
	postCmdOutputFile := postCmd.String("output", "", "")
	postCmd.StringVar(postCmdOutputFile, "o", "", "")

	// Follow redirects
	postCmdFollowRedirects := postCmd.Bool("location", false, "")
	postCmd.BoolVar(postCmdFollowRedirects, "L", false, "")

	return postCmd.Usage, func(args []string) int {
		postCmd.Parse(args)

		params := PostParams{
			InlineData: *postCmdData,
			File:       *postCmdFile,
			GetParams: GetParams{
				FollowRedirects: *postCmdFollowRedirects,
				Url:             postCmd.Arg(0),
				Output:          *postCmdOutputFile,
				Verbose:         *postCmdVerbose,
				Headers:         hfv,
			},
		}

		if exit := checkPostParams(params, config.Command); exit != 0 {
			return exit
		}

		return Post(params)
	}
}

func checkPostParams(params PostParams, command string) (exit int) {
	if params.File != "" && params.InlineData != "" {
		fmt.Fprintln(os.Stderr, "WARNING: flag -f/--file has no "+
			"effect when used alongside flag -d/--data")
	}

	if params.Url == "" {
		fmt.Fprintln(os.Stderr, "Please provide a url.")
		fmt.Fprintf(os.Stderr,
			`%v %v [-v] [-h "k:v"]* [-d inline-data] [-f file] [-o file] [-L] URL`+"\n",
			command, POST)
		return 2
	}

	return 0
}

func Post(params PostParams) (exit int) {
	body, size, err := bodyReader(params)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		return 1
	}
	defer body.Close()
	return makeRequest(params.GetParams, ecurl.POST, body, size)
}

type FailedToReadFileError struct {
	Name string
	Err  error
}

func (e *FailedToReadFileError) Error() string {
	ret := fmt.Sprintf("failed to read file '%v'", e.Name)
	if e.Err != nil {
		ret += fmt.Sprintf(": %v", e.Err)
	}
	return ret
}

func (e *FailedToReadFileError) Unwrap() error {
	return e.Err
}

// Obtains a body reader from the provided params, if the params specify
// inline-data, then the body reader is a io.NopCloser on the data, if the
// params specify a file then the body reader is a file handle.
//
// Return values are: the body reader, the size of the data in the reader, and
// an error if we are reading a file and the file fails to open or we cannot
// determine its size
func bodyReader(params PostParams) (io.ReadCloser, int, error) {
	if params.InlineData != "" || params.File == "" {
		data := bytes.NewBufferString(params.InlineData)
		return io.NopCloser(data), data.Len(), nil
	}
	fh, err := os.Open(params.File)
	if err != nil {
		return nil, 0, &FailedToReadFileError{params.File, err}
	}
	size, err := fileSize(fh)
	if err != nil {
		fh.Close()
		return nil, 0, err
	}
	return fh, size, nil
}

func fileSize(fh *os.File) (int, error) {
	stat, err := fh.Stat()
	if err != nil {
		return -1, fmt.Errorf("error trying to determine file size: %w", err)
	}
	return int(stat.Size()), nil
}
