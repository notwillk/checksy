package doctor

import (
	"bytes"
	"errors"
	"os/exec"

	"github.com/notwillk/workspace-doctor/schema"
)

// Options controls how workspace rules are executed.
type Options struct {
	Config  *schema.Config
	WorkDir string
}

// Report contains the outcomes of executing all configured rules.
type Report struct {
	Rules []RuleResult
}

// RuleResult captures stdout/stderr and exit status for a single rule execution.
type RuleResult struct {
	Rule   schema.Rule
	Err    error
	Stdout string
	Stderr string
}

// Success returns true when the command exited cleanly.
func (r RuleResult) Success() bool {
	return r.Err == nil
}

// Name returns the display label for the rule.
func (r RuleResult) Name() string {
	if r.Rule.Name != "" {
		return r.Rule.Name
	}
	return r.Rule.Check
}

// HasFailures returns true when any rule exited unsuccessfully.
func (r Report) HasFailures() bool {
	for _, result := range r.Rules {
		if !result.Success() {
			return true
		}
	}

	return false
}

// Failures returns the subset of rule results that failed.
func (r Report) Failures() []RuleResult {
	var failed []RuleResult
	for _, result := range r.Rules {
		if !result.Success() {
			failed = append(failed, result)
		}
	}
	return failed
}

// Diagnose executes each rule defined in the configuration.
func Diagnose(opts Options) (Report, error) {
	if opts.Config == nil {
		return Report{}, errors.New("no configuration supplied")
	}

	workdir := opts.WorkDir
	if workdir == "" {
		workdir = "."
	}

	results := make([]RuleResult, 0, len(opts.Config.Rules))
	for _, rule := range opts.Config.Rules {
		results = append(results, runRule(rule, workdir))
	}

	return Report{Rules: results}, nil
}

func runRule(rule schema.Rule, workdir string) RuleResult {
	cmd := exec.Command("bash", "-lc", rule.Check)
	cmd.Dir = workdir

	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	err := cmd.Run()

	return RuleResult{
		Rule:   rule,
		Err:    err,
		Stdout: stdout.String(),
		Stderr: stderr.String(),
	}
}
