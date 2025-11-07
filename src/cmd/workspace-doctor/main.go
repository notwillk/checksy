package main

import (
    "os"

    "github.com/notwillk/workspace-doctor/internal/cli"
)

func main() {
    root := cli.NewRootCommand(os.Stdout, os.Stderr)
    code := root.Run(os.Args[1:])
    os.Exit(code)
}
