package cmd

import (
	"flag"
	"fmt"
	"path"
)

const HELP = "help"

const USAGE = `%v performs HTTP requests

Usage:
	%v <command> [options] URL

The commands are:

	get	perform an HTTP GET request
	post	perform an HTTP POST request

Use "%v help <command>" for more information about a command.
`

func helpCmd(config *Config, usages map[string]func()) (action func(args []string) int) {
	helpCmd := flag.NewFlagSet(HELP, flag.ExitOnError)
	return func(args []string) int {
		helpCmd.Parse(args)
		if len(args) < 1 {
			printHelp(config)
			return 1
		}
		needHelpWith := helpCmd.Arg(0)
		if printUsage, ok := usages[needHelpWith]; ok {
			printUsage()
			return 1
		}
		c := path.Base(config.Command)
		fmt.Printf(
			"%v %v %v: unknown command. Run '%v %v'.\n",
			c, HELP, needHelpWith, c, HELP)
		return 1
	}
}

func unknownCommand(command, subcommand string) string {
	return fmt.Sprintf(
		"%v %v: unknown command\nRun '%v help' for usage.",
		command, subcommand, command)
}

func usage(command string) string {
	cmd := path.Base(command)
	return fmt.Sprintf(USAGE, cmd, cmd, cmd)
}

func printHelp(config *Config) {
	fmt.Print(usage(config.Command))
}
