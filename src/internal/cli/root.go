package cli

import (
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"github.com/notwillk/workspace-doctor/internal/config"
	"github.com/notwillk/workspace-doctor/internal/doctor"
	"github.com/notwillk/workspace-doctor/internal/version"
)

// RootCommand wires the CLI surface area together.
type RootCommand struct {
	stdout io.Writer
	stderr io.Writer
}

// NewRootCommand returns a ready-to-run command tree.
func NewRootCommand(stdout, stderr io.Writer) *RootCommand {
	if stdout == nil {
		stdout = os.Stdout
	}
	if stderr == nil {
		stderr = os.Stderr
	}

	return &RootCommand{stdout: stdout, stderr: stderr}
}

// Run executes the CLI for the provided arguments and returns an exit code.
func (r *RootCommand) Run(args []string) int {
	if len(args) == 0 {
		r.printUsage()
		return 1
	}

	switch args[0] {
	case "diagnose":
		return r.runDiagnose(args[1:])
	case "schema":
		return r.runSchema(args[1:])
	case "version", "--version":
		fmt.Fprintf(r.stdout, "workspace-doctor %s\n", version.Version)
		return 0
	case "help", "-h", "--help":
		r.printUsage()
		return 0
	default:
		fmt.Fprintf(r.stderr, "Unknown command %q\n\n", args[0])
		r.printUsage()
		return 2
	}
}

func (r *RootCommand) runDiagnose(args []string) int {
	flags := flag.NewFlagSet("diagnose", flag.ContinueOnError)
	flags.SetOutput(r.stderr)

	var configPath string
	flags.StringVar(&configPath, "config", "", "path to workspace config file (defaults to .workspace-doctor.yaml/.yml)")

	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}

	resolvedConfigPath, err := config.ResolvePath(configPath)
	if err != nil {
		fmt.Fprintln(r.stderr, err)
		return 2
	}
	if resolvedConfigPath == "" {
		fmt.Fprintln(r.stderr, "no configuration file found; specify --config or add .workspace-doctor.yaml/.yml to the workspace")
		return 2
	}

	absConfigPath, err := filepath.Abs(resolvedConfigPath)
	if err != nil {
		fmt.Fprintf(r.stderr, "unable to resolve config path: %v\n", err)
		return 2
	}

	cfg, err := config.Load(absConfigPath)
	if err != nil {
		fmt.Fprintf(r.stderr, "failed to load config %q: %v\n", absConfigPath, err)
		return 2
	}

	report, err := doctor.Diagnose(doctor.Options{
		Config:  cfg,
		WorkDir: filepath.Dir(absConfigPath),
	})
	if err != nil {
		fmt.Fprintf(r.stderr, "diagnose failed: %v\n", err)
		return 2
	}

	for _, result := range report.Rules {
		icon := "✅"
		if !result.Success() {
			icon = "❌"
		}
		fmt.Fprintf(r.stdout, "%s %s\n", icon, result.Name())
		if !result.Success() {
			if result.Stdout != "" {
				fmt.Fprintf(r.stderr, "%s stdout:\n%s\n", result.Name(), result.Stdout)
			}
			if result.Stderr != "" {
				fmt.Fprintf(r.stderr, "%s stderr:\n%s\n", result.Name(), result.Stderr)
			}
			if result.Stdout == "" && result.Stderr == "" && result.Err != nil {
				fmt.Fprintf(r.stderr, "%s error: %v\n", result.Name(), result.Err)
			}
		}
	}

	if !report.HasFailures() {
		fmt.Fprintln(r.stdout, "All rules validated 😎")
		return 0
	}

	failures := report.Failures()
	fmt.Fprintf(r.stdout, "%d rules failed validation 😭\n", len(failures))
	for _, failure := range failures {
		fmt.Fprintf(r.stdout, "- %s\n", failure.Name())
	}

	return 3
}

func (r *RootCommand) printUsage() {
	fmt.Fprintln(r.stdout, "workspace-doctor - inspect and troubleshoot development environments")
	fmt.Fprintln(r.stdout)
	fmt.Fprintln(r.stdout, "Usage:")
	fmt.Fprintln(r.stdout, "  workspace-doctor <command> [flags]")
	fmt.Fprintln(r.stdout)
	fmt.Fprintln(r.stdout, "Available Commands:")
	fmt.Fprintln(r.stdout, "  diagnose   Validate the workspace using config-defined rules")
	fmt.Fprintln(r.stdout, "  schema     Print the JSON schema for workspace configuration")
	fmt.Fprintln(r.stdout, "  version    Print the current build version")
	fmt.Fprintln(r.stdout, "  help       Show this message")
}
