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

func TestPrintRuleStatusOutputsHintOnFailure(t *testing.T) {
	tests := []struct {
		name         string
		result       doctor.RuleResult
		includeOutput bool
		wantHint     bool
	}{
		{
			name: "failing rule with hint",
			result: doctor.RuleResult{
				Rule: schema.Rule{Name: "test", Hint: "Try running fix command"},
				Err:  errors.New("failed"),
			},
			includeOutput: true,
			wantHint:      true,
		},
		{
			name: "failing rule without hint",
			result: doctor.RuleResult{
				Rule: schema.Rule{Name: "test"},
				Err:  errors.New("failed"),
			},
			includeOutput: true,
			wantHint:      false,
		},
		{
			name: "passing rule with hint",
			result: doctor.RuleResult{
				Rule: schema.Rule{Name: "test", Hint: "This should not appear"},
				Err:  nil,
			},
			includeOutput: true,
			wantHint:      false,
		},
		{
			name: "failing rule with hint but includeOutput false",
			result: doctor.RuleResult{
				Rule: schema.Rule{Name: "test", Hint: "This should not appear"},
				Err:  errors.New("failed"),
			},
			includeOutput: false,
			wantHint:      false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var stdout bytes.Buffer
			var stderr bytes.Buffer
			r := &RootCommand{stdout: &stdout, stderr: &stderr}

			r.printRuleStatus(tt.result, "❌", tt.includeOutput)

			stderrOutput := stderr.String()
			hasHint := strings.Contains(stderrOutput, "hint:")
			if hasHint != tt.wantHint {
				t.Errorf("hint presence = %v, want %v; stderr: %q", hasHint, tt.wantHint, stderrOutput)
			}

			if tt.wantHint && !strings.Contains(stderrOutput, tt.result.Rule.Hint) {
				t.Errorf("expected hint text %q in stderr, got: %q", tt.result.Rule.Hint, stderrOutput)
			}
		})
	}
}
