package doctor

import (
	"errors"
	"os"
	"path/filepath"
	"testing"

	"github.com/notwillk/checksy/schema"
)

func TestFilterRulesBySeverity(t *testing.T) {
	cfg := &schema.Config{Rules: []schema.Rule{
		{Name: "debug", Check: "true", Severity: schema.SeverityDebug},
		{Name: "info", Check: "true", Severity: schema.SeverityInfo},
		{Name: "warn", Check: "true", Severity: schema.SeverityWarning},
	}}

	rules := FilterRules(cfg, schema.SeverityWarning)
	if len(rules) != 1 || rules[0].Name != "warn" {
		t.Fatalf("unexpected rules: %+v", rules)
	}
}

func TestRuleResultShouldFail(t *testing.T) {
	tests := []struct {
		name   string
		result RuleResult
		failOn schema.Severity
		want   bool
	}{
		{"success", RuleResult{Rule: schema.Rule{Severity: schema.SeverityWarning}}, schema.SeverityWarning, false},
		{"below threshold", RuleResult{Rule: schema.Rule{Severity: schema.SeverityWarning}, Err: errors.New("boom")}, schema.SeverityError, false},
		{"at threshold", RuleResult{Rule: schema.Rule{Severity: schema.SeverityWarning}, Err: errors.New("boom")}, schema.SeverityWarning, true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := tt.result.ShouldFail(tt.failOn); got != tt.want {
				t.Fatalf("ShouldFail = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestReportAggregatesFailures(t *testing.T) {
	results := []RuleResult{
		{Rule: schema.Rule{Name: "warn", Severity: schema.SeverityWarning}, Err: errors.New("boom")},
		{Rule: schema.Rule{Name: "error", Severity: schema.SeverityError}, Err: errors.New("boom")},
	}
	report := Report{Rules: results, FailSeverity: schema.SeverityError}

	if !report.HasFailures() {
		t.Fatalf("expected failures")
	}
	if got := report.Failures(); len(got) != 1 || got[0].Rule.Name != "error" {
		t.Fatalf("unexpected failures: %+v", got)
	}
}

func TestDiagnoseRunsRules(t *testing.T) {
	cfg := &schema.Config{Rules: []schema.Rule{
		{Name: "ok", Check: "exit 0", Severity: schema.SeverityDebug},
		{Name: "bad", Check: "exit 1", Severity: schema.SeverityError},
	}}

	report, err := Diagnose(Options{Config: cfg, WorkDir: t.TempDir(), MinSeverity: schema.SeverityDebug, FailSeverity: schema.SeverityError})
	if err != nil {
		t.Fatalf("Diagnose error: %v", err)
	}
	if len(report.Rules) != 2 {
		t.Fatalf("expected 2 rules, got %d", len(report.Rules))
	}
	if !report.Rules[1].ShouldFail(schema.SeverityError) {
		t.Fatalf("expected failing rule")
	}
}

func TestMinSeverity(t *testing.T) {
	tests := []struct {
		a, b schema.Severity
		want schema.Severity
	}{
		{schema.SeverityDebug, schema.SeverityWarning, schema.SeverityDebug},
		{schema.SeverityError, schema.SeverityWarning, schema.SeverityWarning},
	}

	for _, tt := range tests {
		if got := MinSeverity(tt.a, tt.b); got != tt.want {
			t.Fatalf("MinSeverity(%q,%q)=%q, want %q", tt.a, tt.b, got, tt.want)
		}
	}
}

func TestExpandRuleFiles(t *testing.T) {
	workdir := t.TempDir()
	sub := filepath.Join(workdir, "sub")
	if err := os.Mkdir(sub, 0o755); err != nil {
		t.Fatalf("mkdir sub: %v", err)
	}
	for _, name := range []string{"a.sh", "b.sh", "skip.sh", filepath.Join("sub", "c.sh")} {
		p := filepath.Join(workdir, name)
		if err := os.WriteFile(p, []byte("exit 0"), 0o644); err != nil {
			t.Fatalf("write %s: %v", name, err)
		}
	}

	t.Run("positive only", func(t *testing.T) {
		got, err := ExpandRuleFiles(workdir, []string{"*.sh"})
		if err != nil {
			t.Fatalf("ExpandRuleFiles: %v", err)
		}
		want := []string{"a.sh", "b.sh", "skip.sh"}
		if len(got) != len(want) {
			t.Fatalf("got %d paths, want %d: %v", len(got), len(want), got)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("got[%d] = %q, want %q", i, got[i], want[i])
			}
		}
	})

	t.Run("positive and negative", func(t *testing.T) {
		got, err := ExpandRuleFiles(workdir, []string{"*.sh", "!skip.sh"})
		if err != nil {
			t.Fatalf("ExpandRuleFiles: %v", err)
		}
		want := []string{"a.sh", "b.sh"}
		if len(got) != len(want) {
			t.Fatalf("got %d paths, want %d: %v", len(got), len(want), got)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("got[%d] = %q, want %q", i, got[i], want[i])
			}
		}
	})

	t.Run("multiple positives and negative", func(t *testing.T) {
		got, err := ExpandRuleFiles(workdir, []string{"*.sh", "sub/*.sh", "!skip.sh"})
		if err != nil {
			t.Fatalf("ExpandRuleFiles: %v", err)
		}
		// Sorted: a.sh, b.sh, sub/c.sh
		want := []string{"a.sh", "b.sh", "sub/c.sh"}
		if len(got) != len(want) {
			t.Fatalf("got %d paths, want %d: %v", len(got), len(want), got)
		}
		for i := range want {
			if got[i] != want[i] {
				t.Fatalf("got[%d] = %q, want %q", i, got[i], want[i])
			}
		}
	})

	t.Run("empty and nil", func(t *testing.T) {
		got, err := ExpandRuleFiles(workdir, nil)
		if err != nil {
			t.Fatalf("ExpandRuleFiles(nil): %v", err)
		}
		if got != nil {
			t.Fatalf("ExpandRuleFiles(nil) = %v, want nil", got)
		}
		got, err = ExpandRuleFiles(workdir, []string{})
		if err != nil {
			t.Fatalf("ExpandRuleFiles([]): %v", err)
		}
		if got != nil {
			t.Fatalf("ExpandRuleFiles([]) = %v, want nil", got)
		}
	})
}

func TestRunRuleFile(t *testing.T) {
	workdir := t.TempDir()
	passPath := filepath.Join(workdir, "pass.sh")
	failPath := filepath.Join(workdir, "fail.sh")
	if err := os.WriteFile(passPath, []byte("exit 0"), 0o644); err != nil {
		t.Fatalf("write pass.sh: %v", err)
	}
	if err := os.WriteFile(failPath, []byte("exit 1"), 0o644); err != nil {
		t.Fatalf("write fail.sh: %v", err)
	}

	passResult := RunRuleFile(workdir, "pass.sh")
	if !passResult.Success() {
		t.Fatalf("pass.sh should succeed: %v", passResult.Err)
	}
	if passResult.Name() != "pass.sh" {
		t.Fatalf("Name = %q, want pass.sh", passResult.Name())
	}

	failResult := RunRuleFile(workdir, "fail.sh")
	if failResult.Success() {
		t.Fatalf("fail.sh should fail")
	}
	if !failResult.ShouldFail(schema.SeverityError) {
		t.Fatalf("file rules should fail at error severity")
	}
}

func TestDiagnoseRunsRuleFiles(t *testing.T) {
	workdir := t.TempDir()
	if err := os.WriteFile(filepath.Join(workdir, "pass.sh"), []byte("exit 0"), 0o644); err != nil {
		t.Fatalf("write pass.sh: %v", err)
	}
	if err := os.WriteFile(filepath.Join(workdir, "fail.sh"), []byte("exit 1"), 0o644); err != nil {
		t.Fatalf("write fail.sh: %v", err)
	}

	cfg := &schema.Config{
		Rules:     []schema.Rule{{Name: "inline", Check: "exit 0", Severity: schema.SeverityError}},
		Patterns: []string{"*.sh"},
	}
	report, err := Diagnose(Options{Config: cfg, WorkDir: workdir, MinSeverity: schema.SeverityDebug, FailSeverity: schema.SeverityError})
	if err != nil {
		t.Fatalf("Diagnose: %v", err)
	}
	// 1 inline + 2 file rules (fail.sh, pass.sh sorted) = 3
	if len(report.Rules) != 3 {
		t.Fatalf("expected 3 rules, got %d", len(report.Rules))
	}
	if report.Rules[0].Name() != "inline" {
		t.Fatalf("first rule should be inline, got %q", report.Rules[0].Name())
	}
	// File rules in alphabetical order: fail.sh, pass.sh
	if report.Rules[1].Name() != "fail.sh" || report.Rules[2].Name() != "pass.sh" {
		t.Fatalf("file rules order: got %q, %q", report.Rules[1].Name(), report.Rules[2].Name())
	}
	if !report.HasFailures() {
		t.Fatalf("expected failure from fail.sh")
	}
}
