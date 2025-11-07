package doctor

import (
	"bytes"
	"errors"
	"os/exec"
	"strings"

	"github.com/notwillk/workspace-doctor/schema"
)

var severityOrder = map[schema.Severity]int{
	schema.SeverityDebug:   0,
	schema.SeverityInfo:    1,
	schema.SeverityWarning: 2,
	schema.SeverityError:   3,
}

const defaultRuleSeverity = schema.SeverityInfo

// Options controls how workspace rules are executed.
type Options struct {
	Config      *schema.Config
	WorkDir     string
	MinSeverity schema.Severity
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

	minSeverity := normalizeMinSeverity(opts.MinSeverity)

	results := make([]RuleResult, 0, len(opts.Config.Rules))
	for _, rule := range opts.Config.Rules {
		if !ruleMeetsSeverity(rule, minSeverity) {
			continue
		}
		results = append(results, runRule(rule, workdir))
	}

	return Report{Rules: results}, nil
}

func runRule(rule schema.Rule, workdir string) RuleResult {
	script := rule.Check
	if script == "" {
		script = "true"
	}
	if !strings.HasSuffix(script, "\n") {
		script += "\n"
	}

	cmd := exec.Command("bash")
	cmd.Dir = workdir
	cmd.Stdin = strings.NewReader(script)

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

func ruleMeetsSeverity(rule schema.Rule, min schema.Severity) bool {
	ruleSeverity := normalizeRuleSeverity(rule.Severity)
	return severityOrder[ruleSeverity] >= severityOrder[min]
}

func normalizeRuleSeverity(value schema.Severity) schema.Severity {
	if _, ok := severityOrder[value]; ok {
		return value
	}
	return defaultRuleSeverity
}

func normalizeMinSeverity(value schema.Severity) schema.Severity {
	if value == "" {
		return schema.SeverityDebug
	}
	if _, ok := severityOrder[value]; ok {
		return value
	}
	return schema.SeverityDebug
}
