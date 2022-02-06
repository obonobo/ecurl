package cmd

import (
	"flag"
	"fmt"
	"os"
	"path"
)

const ROOT = "root"

type Config struct {
	Command    string
	Subcommand string
}

// Runs the CLI tools on all os.Args and then calls os.Exit. Call this as the
// only function in your main
func RunAndExit() {
	os.Exit(Run(os.Args))
}

// Runs the CLI tool and returns the program exit code. The main function will
// need to manually exit with this code.
func Run(args []string) (exitCode int) {
	config := &Config{Command: path.Base(args[0])}
	rootCmd := flag.NewFlagSet(ROOT, flag.ExitOnError)
	helpFlag := rootCmd.Bool("help", false, "")
	rootCmd.BoolVar(helpFlag, "h", false, "")
	rootCmd.Usage = func() { printHelp(config) }
	rootCmd.Parse(args[1:])
	if *helpFlag || len(args) < 2 {
		rootCmd.Usage()
		return 1
	}

	getUsage, get := getCmd(config)
	postUsage, post := postCmd(config)
	help := helpCmd(config, map[string]func(){
		GET:  getUsage,
		POST: postUsage,
	})

	config.Subcommand = args[1]
	rest := args[2:]

	switch config.Subcommand {
	case HELP:
		return help(rest)
	case GET:
		return get(rest)
	case POST:
		return post(rest)
	default:
		fmt.Println(unknownCommand(config.Command, config.Subcommand))
		return 1
	}
}
