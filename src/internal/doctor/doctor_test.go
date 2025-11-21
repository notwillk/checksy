package doctor

import (
	"errors"
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
