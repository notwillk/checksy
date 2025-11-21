package main

import (
	"io"
	"os"

	"github.com/notwillk/checksy/internal/cli"
)

func run(args []string, stdout, stderr io.Writer) int {
	root := cli.NewRootCommand(stdout, stderr)
	return root.Run(args)
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
