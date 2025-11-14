package cli

import (
	"bytes"
	"strings"
	"testing"
)

func TestRunSchemaWritesJSON(t *testing.T) {
	var out bytes.Buffer
	var errBuf bytes.Buffer
	r := &RootCommand{stdout: &out, stderr: &errBuf}

	if code := r.runSchema([]string{"--pretty"}); code != 0 {
		t.Fatalf("runSchema exit code %d", code)
	}

	if !strings.HasPrefix(out.String(), "{") {
		t.Fatalf("expected JSON output, got %q", out.String())
	}
}
