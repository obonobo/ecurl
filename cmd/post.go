package cmd

import (
	"flag"
	"fmt"
	"os"
	"strings"
)

const POST = "post"

const PostUsage = `usage: %v %v [-v] [-h "k:v"]* [-d inline-data] [-f file] URL

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

`

type PostParams struct {
	GetParams
	InlineData, File string
}

func postCmd(config *Config) (usage func(), action func(args []string) int) {
	postCmd := flag.NewFlagSet(GET, flag.ExitOnError)
	postCmd.Usage = func() {
		fmt.Printf(
			GetUsage,
			config.Command,
			GET,
			strings.ToUpper(GET[:1])+GET[1:])
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

	return postCmd.Usage, func(args []string) int {
		postCmd.Parse(args)

		params := PostParams{
			InlineData: *postCmdData,
			File:       *postCmdFile,
			GetParams: GetParams{
				Url:     postCmd.Arg(0),
				Verbose: *postCmdVerbose,
				Headers: hfv,
			},
		}

		if exit, msg := checkPostParams(params); exit != 0 {
			fmt.Println(msg)
			return exit
		}

		return Post(params)
	}
}

func checkPostParams(params PostParams) (exit int, msg string) {
	if params.File != "" && params.InlineData != "" {
		fmt.Fprintln(os.Stderr, "WARNING: flag -f/--file has no "+
			"effect when used alongside flag -d/--data")
	}
	return 0, ""
}

func Post(params PostParams) (exit int) {
	fmt.Println(params)
	return 1
}
