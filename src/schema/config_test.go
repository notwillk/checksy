package schema

import "testing"

func TestSeverityJSONSchema(t *testing.T) {
	schema := Severity("unused").JSONSchema()
	if schema.Type != "string" {
		t.Fatalf("unexpected schema type: %s", schema.Type)
	}
	wantEnum := []string{string(SeverityError), string(SeverityWarning), string(SeverityInfo), string(SeverityDebug)}
	for i, want := range wantEnum {
		if got, ok := schema.Enum[i].(string); !ok || got != want {
			t.Fatalf("enum[%d] = %v, want %q", i, schema.Enum[i], want)
		}
	}
	if schema.Default != string(SeverityError) {
		t.Fatalf("default = %v, want %q", schema.Default, SeverityError)
	}
}

func TestNormalizeSeverity(t *testing.T) {
	tests := []struct {
		name  string
		value Severity
		want  Severity
		valid bool
	}{
		{"error", "error", SeverityError, true},
		{"warn alias", "warning", SeverityWarning, true},
		{"mixed case", "Warn", SeverityWarning, true},
		{"info", "info", SeverityInfo, true},
		{"debug", "debug", SeverityDebug, true},
		{"invalid", "nope", "nope", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, ok := NormalizeSeverity(tt.value)
			if ok != tt.valid {
				t.Fatalf("valid=%v, want %v", ok, tt.valid)
			}
			if got != tt.want {
				t.Fatalf("got %q, want %q", got, tt.want)
			}
		})
	}
}
