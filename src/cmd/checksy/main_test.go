package main

import (
	"bytes"
	"testing"
)

func TestRunHelpCommand(t *testing.T) {
	var stdout bytes.Buffer
	var stderr bytes.Buffer

	code := run([]string{"help"}, &stdout, &stderr)
	if code != 0 {
		t.Fatalf("expected zero exit code, got %d", code)
	}
	if stdout.Len() == 0 {
		t.Fatalf("expected usage output")
	}
}
