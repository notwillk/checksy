package config

import (
	"os"
	"path/filepath"
	"testing"

	schemadef "github.com/notwillk/checksy/schema"
)

func TestResolvePathExplicit(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "cfg.yaml")
	if err := os.WriteFile(path, []byte("rules: []"), 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}

	got, err := ResolvePath(path)
	if err != nil {
		t.Fatalf("ResolvePath returned error: %v", err)
	}
	if got != path {
		t.Fatalf("ResolvePath = %q, want %q", got, path)
	}
}

func TestResolvePathAutoDetect(t *testing.T) {
	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	t.Cleanup(func() { _ = os.Chdir(wd) })

	tmp := t.TempDir()
	if err := os.Chdir(tmp); err != nil {
		t.Fatalf("chdir: %v", err)
	}

	content := []byte("rules:\n  - check: echo ok\n")
	if err := os.WriteFile(filepath.Join(tmp, ".checksy.yaml"), content, 0o644); err != nil {
		t.Fatalf("write auto config: %v", err)
	}

	got, err := ResolvePath("")
	if err != nil {
		t.Fatalf("ResolvePath error: %v", err)
	}
	if got != ".checksy.yaml" {
		t.Fatalf("got %q, want .checksy.yaml", got)
	}
}

func TestApplyRuleDefaults(t *testing.T) {
	cfg := &schemadef.Config{Rules: []schemadef.Rule{
		{Check: "echo hi", Severity: ""},
		{Check: "echo warn", Severity: "warning"},
	}}

	applyRuleDefaults(cfg)

	if cfg.Rules[0].Severity != schemadef.SeverityError {
		t.Fatalf("rule 0 severity = %q, want %q", cfg.Rules[0].Severity, schemadef.SeverityError)
	}
	if cfg.Rules[1].Severity != schemadef.SeverityWarning {
		t.Fatalf("rule 1 severity = %q, want %q", cfg.Rules[1].Severity, schemadef.SeverityWarning)
	}
}

func TestLoadAppliesDefaults(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "config.yaml")
	data := []byte("rules:\n  - name: warn\n    check: echo warn\n    severity: warn\n  - name: default\n    check: echo ok\n")
	if err := os.WriteFile(path, data, 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}

	cfg, err := Load(path)
	if err != nil {
		t.Fatalf("Load returned error: %v", err)
	}

	if got := cfg.Rules[0].Severity; got != schemadef.SeverityWarning {
		t.Fatalf("rule 0 severity = %q, want %q", got, schemadef.SeverityWarning)
	}
	if got := cfg.Rules[1].Severity; got != schemadef.SeverityError {
		t.Fatalf("rule 1 severity = %q, want %q", got, schemadef.SeverityError)
	}
}
