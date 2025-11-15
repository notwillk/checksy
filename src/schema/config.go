package schema

import (
	"strings"

	invjsonschema "github.com/invopop/jsonschema"
)

type Severity string

const (
	SeverityError   Severity = "error"
	SeverityWarning Severity = "warn"
	SeverityInfo    Severity = "info"
	SeverityDebug   Severity = "debug"
)

func (Severity) JSONSchema() *invjsonschema.Schema {
	return &invjsonschema.Schema{
		Type: "string",
		Enum: []any{
			string(SeverityError),
			string(SeverityWarning),
			string(SeverityInfo),
			string(SeverityDebug),
		},
		Default: string(SeverityError),
	}
}

// NormalizeSeverity returns the canonical severity for the provided value,
// accepting historical aliases such as "warning".
func NormalizeSeverity(value Severity) (Severity, bool) {
	switch strings.ToLower(strings.TrimSpace(string(value))) {
	case string(SeverityError):
		return SeverityError, true
	case string(SeverityWarning), "warning":
		return SeverityWarning, true
	case string(SeverityInfo):
		return SeverityInfo, true
	case string(SeverityDebug):
		return SeverityDebug, true
	default:
		return value, false
	}
}

type Rule struct {
	Name     string   `json:"name,omitempty" yaml:"name,omitempty"`
	Check    string   `json:"check" yaml:"check"`
	Severity Severity `json:"severity,omitempty" yaml:"severity,omitempty"`
	Fix      string   `json:"fix,omitempty" yaml:"fix,omitempty"`
	Hint     string   `json:"hint,omitempty" yaml:"hint,omitempty"`
}

type Config struct {
	Rules []Rule `json:"rules" yaml:"rules"`
}
