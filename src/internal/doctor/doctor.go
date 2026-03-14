package doctor

import (
	"bytes"
	"errors"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"

	"github.com/notwillk/checksy/schema"
)

var severityOrder = map[schema.Severity]int{
	schema.SeverityDebug:   0,
	schema.SeverityInfo:    1,
	schema.SeverityWarning: 2,
	schema.SeverityError:   3,
}

const defaultRuleSeverity = schema.SeverityError

// Options controls how rules are executed.
type Options struct {
	Config       *schema.Config
	WorkDir      string
	MinSeverity  schema.Severity
	FailSeverity schema.Severity
}

// Report contains the outcomes of executing all configured rules.
type Report struct {
	Rules        []RuleResult
	FailSeverity schema.Severity
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

// Severity returns the normalized severity for the rule.
func (r RuleResult) Severity() schema.Severity {
	return normalizeRuleSeverity(r.Rule.Severity)
}

// ShouldFail reports whether the rule result should be treated as a failure for
// the provided fail severity threshold.
func (r RuleResult) ShouldFail(threshold schema.Severity) bool {
	if r.Success() {
		return false
	}
	normalized := normalizeFailSeverity(threshold)
	return severityOrder[r.Severity()] >= severityOrder[normalized]
}

// HasFailures returns true when any rule exited unsuccessfully.
func (r Report) HasFailures() bool {
	failureThreshold := normalizeFailSeverity(r.FailSeverity)
	for _, result := range r.Rules {
		if result.ShouldFail(failureThreshold) {
			return true
		}
	}

	return false
}

// Failures returns the subset of rule results that failed.
func (r Report) Failures() []RuleResult {
	failureThreshold := normalizeFailSeverity(r.FailSeverity)
	var failed []RuleResult
	for _, result := range r.Rules {
		if result.ShouldFail(failureThreshold) {
			failed = append(failed, result)
		}
	}
	return failed
}

// Diagnose executes each rule defined in the configuration (inline rules first,
// then patterns in alphabetical order). File-based rules have no fix
// and are treated as error severity for failure reporting.
func Diagnose(opts Options) (Report, error) {
	if opts.Config == nil {
		return Report{}, errors.New("no configuration supplied")
	}

	workdir := opts.WorkDir
	if workdir == "" {
		workdir = "."
	}

	rules := FilterRules(opts.Config, opts.MinSeverity)
	results := make([]RuleResult, 0, len(rules))
	for _, rule := range rules {
		results = append(results, RunRule(rule, workdir))
	}

	filePaths, err := ExpandRuleFiles(workdir, opts.Config.Patterns)
	if err != nil {
		return Report{}, err
	}
	for _, relPath := range filePaths {
		results = append(results, RunRuleFile(workdir, relPath))
	}

	return Report{Rules: results, FailSeverity: opts.FailSeverity}, nil
}

// FilterRules returns the subset of rules that meet the provided minimum severity.
func FilterRules(cfg *schema.Config, min schema.Severity) []schema.Rule {
	if cfg == nil {
		return nil
	}

	minSeverity := normalizeMinSeverity(min)
	selected := make([]schema.Rule, 0, len(cfg.Rules))
	for _, rule := range cfg.Rules {
		if ruleMeetsSeverity(rule, minSeverity) {
			selected = append(selected, rule)
		}
	}

	return selected
}

// RunRule executes a single rule and captures its output.
func RunRule(rule schema.Rule, workdir string) RuleResult {
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

// ExpandRuleFiles resolves patterns (glob patterns) relative to workdir.
// Positive patterns include matching files; negated patterns (prefix "!") exclude matches.
// A file is included if it matches any positive pattern and no negative pattern.
// Returns paths relative to workdir, sorted alphabetically. Only regular files are included.
func ExpandRuleFiles(workdir string, patterns []string) ([]string, error) {
	if len(patterns) == 0 {
		return nil, nil
	}

	var positive, negative []string
	for _, p := range patterns {
		s := strings.TrimSpace(p)
		if s == "" {
			continue
		}
		if strings.HasPrefix(s, "!") {
			negative = append(negative, strings.TrimPrefix(s, "!"))
		} else {
			positive = append(positive, s)
		}
	}

	if len(positive) == 0 {
		return nil, nil
	}

	included := make(map[string]struct{})
	for _, pat := range positive {
		globPath := filepath.Join(workdir, pat)
		matches, err := filepath.Glob(globPath)
		if err != nil {
			return nil, err
		}
		for _, abs := range matches {
			info, err := os.Stat(abs)
			if err != nil || !info.Mode().IsRegular() {
				continue
			}
			rel, err := filepath.Rel(workdir, abs)
			if err != nil {
				continue
			}
			rel = filepath.ToSlash(rel)
			included[rel] = struct{}{}
		}
	}

	for _, pat := range negative {
		globPath := filepath.Join(workdir, strings.TrimSpace(pat))
		matches, err := filepath.Glob(globPath)
		if err != nil {
			return nil, err
		}
		for _, abs := range matches {
			rel, err := filepath.Rel(workdir, abs)
			if err != nil {
				continue
			}
			rel = filepath.ToSlash(rel)
			delete(included, rel)
		}
	}

	out := make([]string, 0, len(included))
	for p := range included {
		out = append(out, p)
	}
	sort.Strings(out)
	return out, nil
}

// RunRuleFile runs a single script file as a rule (bash ./path) and captures its output.
// The rule is treated as error severity; no fix is available. Name is the relative path.
func RunRuleFile(workdir, relPath string) RuleResult {
	rule := schema.Rule{
		Name:     relPath,
		Check:    relPath,
		Severity: schema.SeverityError,
	}
	scriptPath := relPath
	if !strings.HasPrefix(scriptPath, "./") {
		scriptPath = "./" + scriptPath
	}
	cmd := exec.Command("bash", scriptPath)
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

func ruleMeetsSeverity(rule schema.Rule, min schema.Severity) bool {
	ruleSeverity := normalizeRuleSeverity(rule.Severity)
	return severityOrder[ruleSeverity] >= severityOrder[min]
}

func normalizeRuleSeverity(value schema.Severity) schema.Severity {
	if normalized, ok := schema.NormalizeSeverity(value); ok {
		return normalized
	}
	if _, ok := severityOrder[value]; ok {
		return value
	}
	return defaultRuleSeverity
}

func normalizeMinSeverity(value schema.Severity) schema.Severity {
	if value == "" {
		return schema.SeverityDebug
	}
	if normalized, ok := schema.NormalizeSeverity(value); ok {
		return normalized
	}
	if _, ok := severityOrder[value]; ok {
		return value
	}
	return schema.SeverityDebug
}

func normalizeFailSeverity(value schema.Severity) schema.Severity {
	if value == "" {
		return schema.SeverityError
	}
	if normalized, ok := schema.NormalizeSeverity(value); ok {
		return normalized
	}
	if _, ok := severityOrder[value]; ok {
		return value
	}
	return schema.SeverityError
}

// MinSeverity returns the less strict (numerically lower) of the provided
// severities after normalization.
func MinSeverity(a, b schema.Severity) schema.Severity {
	a = normalizeMinSeverity(a)
	b = normalizeMinSeverity(b)
	if severityOrder[a] <= severityOrder[b] {
		return a
	}
	return b
}
