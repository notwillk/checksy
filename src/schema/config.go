package schema

type Severity string

const (
	SeverityError   Severity = "error"
	SeverityWarning Severity = "warning"
	SeverityInfo    Severity = "info"
	SeverityDebug   Severity = "debug"
)

type Rule struct {
	Name     string   `json:"name,omitempty" yaml:"name,omitempty"`
	Check    string   `json:"check" yaml:"check"`
	Severity Severity `json:"severity,omitempty" yaml:"severity,omitempty"`
	Fix      string   `json:"fix,omitempty" yaml:"fix,omitempty"`
}

type Config struct {
	Rules []Rule `json:"rules" yaml:"rules"`
}
