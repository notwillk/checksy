# workspace-doctor

workspace-doctor is a Go-based command line utility intended to run lightweight health checks against a development workspace. The initial scaffolding provides a `diagnose` command that demonstrates how to add future checks and subcommands.

## Project Layout

```
src/
  go.mod               # Go module root for the CLI
  cmd/workspace-doctor # CLI entry point (main package)
  internal/cli         # Argument parsing and command wiring
  internal/doctor      # Diagnostic checks and reporting helpers
  internal/version     # Centralized version string
```

## Building

`build`

The resulting binary can be copied anywhere on your `PATH` if desired. Running `go` commands from the `src/` directory keeps import paths consistent with the module definition.

## Running

```
# Show help
workspace-doctor help

# Run the workspace validation rules
workspace-doctor diagnose --config path/to/.workspace-doctor.yaml

# Attempt to auto-fix failures when fixes are defined
workspace-doctor diagnose --fix --config path/to/.workspace-doctor.yaml

# Emit the configuration JSON schema
workspace-doctor schema --pretty > dist/config.schema.json
```

The `diagnose` command executes each configured rule, printing ✅/❌ for every check, forwarding any failing command output to stderr, and returning a non-zero exit code when something breaks. Passing `--fix` attempts to run each rule's optional `fix` script to resolve issues before re-running the check. The `schema` command reflects over the configuration struct in `schema/config.go` and outputs a machine-readable JSON Schema definition that downstream tooling can validate against.


## Configuration

`workspace-doctor diagnose --config path/to/workspace.yaml` loads the provided YAML, validates it against the same JSON Schema emitted by the `schema` command, and aborts if validation fails. When the flag is omitted, the command automatically looks for `.workspace-doctor.yaml` or `.workspace-doctor.yml` in the current working directory so repositories can keep a shared default. Every rule's command executes relative to the directory containing the resolved config file, so you can point the CLI at any workspace path while keeping rule definitions portable.

## Next Steps

- Add additional checks inside `internal/doctor` for the tools and conventions your team cares about.
- Introduce more subcommands in `internal/cli` to automate other workspace-related workflows.
