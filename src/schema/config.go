package schema

import invjsonschema "github.com/invopop/jsonschema"

type Severity string

const (
	SeverityError   Severity = "error"
	SeverityWarning Severity = "warning"
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

type Rule struct {
	Name     string   `json:"name,omitempty" yaml:"name,omitempty"`
	Check    string   `json:"check" yaml:"check"`
	Severity Severity `json:"severity,omitempty" yaml:"severity,omitempty"`
	Fix      string   `json:"fix,omitempty" yaml:"fix,omitempty"`
}

type Config struct {
	Rules []Rule `json:"rules" yaml:"rules"`
}
