package cli

import (
	"bytes"
	"errors"
	"strings"
	"testing"

	"github.com/notwillk/workspace-doctor/internal/doctor"
	"github.com/notwillk/workspace-doctor/schema"
)

func TestParseSeverityFlag(t *testing.T) {
	tests := map[string]schema.Severity{
		"debug":   schema.SeverityDebug,
		"info":    schema.SeverityInfo,
		"warn":    schema.SeverityWarning,
		"warning": schema.SeverityWarning,
		"":        schema.SeverityDebug,
	}

	for value, want := range tests {
		got, err := parseSeverityFlag(value)
		if err != nil {
			t.Fatalf("parseSeverityFlag(%q) error: %v", value, err)
		}
		if got != want {
			t.Fatalf("parseSeverityFlag(%q)=%q, want %q", value, got, want)
		}
	}
}

func TestRuleDisplayName(t *testing.T) {
	rule := schema.Rule{Check: "echo hi"}
	if got := ruleDisplayName(rule); !strings.Contains(got, "echo hi") {
		t.Fatalf("unexpected display name: %q", got)
	}

	rule = schema.Rule{Name: "custom", Check: "echo hi"}
	if got := ruleDisplayName(rule); got != "custom" {
		t.Fatalf("expected explicit name, got %q", got)
	}
}

func TestPrintRuleOutcomeUsesWarningIconBelowThreshold(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer
	r := &RootCommand{stdout: &stdout, stderr: &stderr}
	result := doctor.RuleResult{Rule: schema.Rule{Name: "warn", Severity: schema.SeverityWarning}, Err: errors.New("boom")}

	r.printRuleOutcome(result, schema.SeverityError)
	if got := stdout.String(); !strings.Contains(got, "⚠️") {
		t.Fatalf("expected warning icon, got %q", got)
	}
}
