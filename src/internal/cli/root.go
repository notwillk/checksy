package cli

import (
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/notwillk/workspace-doctor/internal/config"
	"github.com/notwillk/workspace-doctor/internal/doctor"
	"github.com/notwillk/workspace-doctor/internal/version"
	"github.com/notwillk/workspace-doctor/schema"
)

const configFlagDescription = "path to workspace config file (defaults to .workspace-doctor.yaml/.yml)"

// RootCommand wires the CLI surface area together.
type RootCommand struct {
	stdout io.Writer
	stderr io.Writer
}

type globalFlags struct {
	configPath string
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
	globals, remaining, err := parseGlobalFlags(args)
	if err != nil {
		fmt.Fprintf(r.stderr, "%v\n", err)
		return 2
	}

	if len(remaining) == 0 {
		r.printUsage()
		return 1
	}

	cmd := remaining[0]
	cmdArgs := remaining[1:]

	switch cmd {
	case "diagnose":
		return r.runDiagnose(cmdArgs, globals)
	case "schema":
		return r.runSchema(cmdArgs)
	case "version", "--version":
		fmt.Fprintf(r.stdout, "workspace-doctor %s\n", version.Version)
		return 0
	case "help", "-h", "--help":
		r.printUsage()
		return 0
	default:
		fmt.Fprintf(r.stderr, "Unknown command %q\n\n", cmd)
		r.printUsage()
		return 2
	}
}

func (r *RootCommand) runDiagnose(args []string, globals globalFlags) int {
	flags := flag.NewFlagSet("diagnose", flag.ContinueOnError)
	flags.SetOutput(r.stderr)

	var localConfigPath string
	var noFail bool
	var severityFloor string
	var applyFixes bool
	flags.StringVar(&localConfigPath, "config", "", configFlagDescription)
	flags.BoolVar(&noFail, "no-fail", false, "always exit zero even when rules fail")
	flags.StringVar(&severityFloor, "severity", "debug", "minimum rule severity to run (debug|info|warning|error)")
	flags.BoolVar(&applyFixes, "fix", false, "attempt to run fixes for failing rules when available")

	if err := flags.Parse(args); err != nil {
		if err == flag.ErrHelp {
			return 0
		}
		return 2
	}

	configPath := globals.configPath
	if localConfigPath != "" {
		configPath = localConfigPath
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

	minSeverity, err := parseSeverityFlag(severityFloor)
	if err != nil {
		fmt.Fprintf(r.stderr, "%v\n", err)
		return 2
	}

	opts := doctor.Options{
		Config:      cfg,
		WorkDir:     filepath.Dir(absConfigPath),
		MinSeverity: minSeverity,
	}

	var report doctor.Report
	if applyFixes {
		report, err = r.diagnoseWithFixes(opts)
	} else {
		report, err = doctor.Diagnose(opts)
	}
	if err != nil {
		fmt.Fprintf(r.stderr, "diagnose failed: %v\n", err)
		return 2
	}

	if !applyFixes {
		r.printReportResults(report)
	}

	return r.summarizeReport(report, noFail)
}

func (r *RootCommand) printUsage() {
	fmt.Fprintln(r.stdout, "workspace-doctor - inspect and troubleshoot development environments")
	fmt.Fprintln(r.stdout)
	fmt.Fprintln(r.stdout, "Usage:")
	fmt.Fprintln(r.stdout, "  workspace-doctor [global flags] <command> [command flags]")
	fmt.Fprintln(r.stdout)
	fmt.Fprintln(r.stdout, "Global Flags:")
	fmt.Fprintf(r.stdout, "  --config string   %s\n", configFlagDescription)
	fmt.Fprintln(r.stdout)
	fmt.Fprintln(r.stdout, "Available Commands:")
	fmt.Fprintln(r.stdout, "  diagnose   Validate the workspace using config-defined rules")
	fmt.Fprintln(r.stdout, "  schema     Print the JSON schema for workspace configuration")
	fmt.Fprintln(r.stdout, "  version    Print the current build version")
	fmt.Fprintln(r.stdout, "  help       Show this message")
}

func parseGlobalFlags(args []string) (globalFlags, []string, error) {
	globals := globalFlags{}
	remaining := make([]string, 0, len(args))

	for i := 0; i < len(args); i++ {
		arg := args[i]

		switch {
		case arg == "--config" || arg == "-config":
			if i+1 >= len(args) {
				return globals, nil, fmt.Errorf("--config flag requires a value")
			}
			globals.configPath = args[i+1]
			i++
			continue
		case strings.HasPrefix(arg, "--config="):
			globals.configPath = strings.TrimPrefix(arg, "--config=")
			continue
		case strings.HasPrefix(arg, "-config="):
			globals.configPath = strings.TrimPrefix(arg, "-config=")
			continue
		}

		remaining = append(remaining, arg)
	}

	return globals, remaining, nil
}

func parseSeverityFlag(value string) (schema.Severity, error) {
	switch strings.ToLower(strings.TrimSpace(value)) {
	case "", "debug":
		return schema.SeverityDebug, nil
	case "info":
		return schema.SeverityInfo, nil
	case "warning":
		return schema.SeverityWarning, nil
	case "error":
		return schema.SeverityError, nil
	default:
		return "", fmt.Errorf("invalid severity %q: must be one of debug, info, warning, error", value)
	}
}

func (r *RootCommand) printReportResults(report doctor.Report) {
	for _, result := range report.Rules {
		if result.Success() {
			r.printRuleSuccess(result)
			continue
		}
		r.printRuleFailure(result)
	}
}

func (r *RootCommand) printRuleStatus(result doctor.RuleResult, icon string, includeOutput bool) {
	fmt.Fprintf(r.stdout, "%s %s\n", icon, result.Name())
	if !includeOutput || result.Success() {
		return
	}
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

func (r *RootCommand) printRuleSuccess(result doctor.RuleResult) {
	r.printRuleStatus(result, "✅", false)
}

func (r *RootCommand) printRuleFailure(result doctor.RuleResult) {
	r.printRuleStatus(result, "❌", true)
}

func (r *RootCommand) summarizeReport(report doctor.Report, noFail bool) int {
	if !report.HasFailures() {
		fmt.Fprintln(r.stdout, "All rules validated 😎")
		return 0
	}

	failures := report.Failures()
	fmt.Fprintf(r.stdout, "%d rules failed validation 😭\n", len(failures))
	for _, failure := range failures {
		fmt.Fprintf(r.stdout, "- %s\n", failure.Name())
	}

	if noFail {
		return 0
	}

	return 3
}

func (r *RootCommand) diagnoseWithFixes(opts doctor.Options) (doctor.Report, error) {
	if opts.Config == nil {
		return doctor.Report{}, errors.New("no configuration supplied")
	}

	workdir := opts.WorkDir
	if workdir == "" {
		workdir = "."
	}

	rules := doctor.FilterRules(opts.Config, opts.MinSeverity)
	results := make([]doctor.RuleResult, 0, len(rules))

	for _, rule := range rules {
		result := doctor.RunRule(rule, workdir)
		if result.Success() {
			r.printRuleSuccess(result)
			results = append(results, result)
			continue
		}

		if strings.TrimSpace(rule.Fix) == "" {
			r.printRuleFailure(result)
			results = append(results, result)
			continue
		}

		r.printRuleStatus(result, "⚠️", false)

		fixRule := schema.Rule{
			Name:  fmt.Sprintf("%s fix", ruleDisplayName(rule)),
			Check: rule.Fix,
		}
		fixResult := doctor.RunRule(fixRule, workdir)
		if !fixResult.Success() {
			r.printRuleFailure(fixResult)
			r.printRuleFailure(result)
			results = append(results, result)
			continue
		}

		r.printRuleSuccess(fixResult)

		result = doctor.RunRule(rule, workdir)
		if result.Success() {
			r.printRuleSuccess(result)
		} else {
			r.printRuleFailure(result)
		}

		results = append(results, result)
	}

	return doctor.Report{Rules: results}, nil
}

func ruleDisplayName(rule schema.Rule) string {
	if rule.Name != "" {
		return rule.Name
	}
	trimmed := strings.TrimSpace(rule.Check)
	if trimmed != "" {
		return trimmed
	}
	return "rule"
}
