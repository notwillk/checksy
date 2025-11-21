# checksy

checksy is a Go-based command line utility intended to run lightweight health checks against a development workspace. The initial scaffolding provides a `diagnose` command that demonstrates how to add future checks and subcommands.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/install.sh | bash
```

## Uninstallation

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/checksy/main/scripts/uninstall.sh | bash
```

## Project Layout

```
src/
  go.mod               # Go module root for the CLI
  cmd/checksy # CLI entry point (main package)
  internal/cli         # Argument parsing and command wiring
  internal/doctor      # Diagnostic checks and reporting helpers
  internal/version     # Centralized version string
```

## Building

`build`

The resulting binary can be copied anywhere on your `PATH` if desired. Running `go` commands from the `src/` directory keeps import paths consistent with the module definition.

## Running

```bash
# Show help
checksy help

# Run the workspace validation rules
checksy --config=path/to/.checksy.yaml diagnose

# Attempt to auto-fix failures when fixes are defined
checksy --config=path/to/.checksy.yaml diagnose --fix

# Emit the configuration JSON schema
checksy schema > dist/config.schema.json

# Emit the configuration JSON schema (prettily)
checksy schema --pretty > dist/config.schema.json

# Only execute warn+ rules but fail only on errors
checksy diagnose --check-severity=warn --fail-severity=error
```

The `diagnose` command executes each configured rule, printing ✅/⚠️/❌ for every check, forwarding any failing command output to stderr, and returning a non-zero exit code when something breaks. Passing `--fix` attempts to run each rule's optional `fix` script to resolve issues before re-running the check. The `schema` command reflects over the configuration struct in `schema/config.go` and outputs a machine-readable JSON Schema definition that downstream tooling can validate against.

Use `--check-severity/--cs` to decide which rules run and `--fail-severity/--fs` to decide which severities cause the command to exit non-zero. When omitted, checks default to running for warn+ rules and the command only fails for error-level rules. Failing checks below the fail severity threshold still surface with a ⚠️ indicator but no longer abort the run.


## Configuration

`checksy --config=path/to/workspace.yaml diagnose` loads the provided YAML, validates it against the same JSON Schema emitted by the `schema` command, and aborts if validation fails. When the flag is omitted, the command automatically looks for `.checksy.yaml` or `.checksy.yml` in the current working directory so repositories can keep a shared default. Every rule's command executes relative to the directory containing the resolved config file, so you can point the CLI at any workspace path while keeping rule definitions portable.
