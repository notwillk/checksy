# checksy

checksy is a Rust-based command line utility intended to run lightweight health checks against a development workspace.

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
  Cargo.toml           # Rust package definition
  main.rs              # CLI entry point
  cache.rs             # Git remote cache management
  cli.rs               # Argument parsing and command wiring
  config.rs            # Configuration loading
  doctor.rs            # Diagnostic checks and reporting helpers
  git.rs               # Git operations for caching remotes
  schema.rs            # Configuration schema definitions
  version.rs           # Centralized version string
```

## Building

`just build`

The resulting binary can be copied anywhere on your `PATH` if desired. Running `go` commands from the `src/` directory keeps import paths consistent with the module definition.

### Cross-compiling

`just cross-compile <target>`

Cross-compile for a different architecture/target (e.g., `aarch64-unknown-linux-gnu`). The binary is output to `dist/checksy_<OS>_<ARCH>`.

## Running

```bash
# Show help
checksy help

# Run the workspace validation rules
checksy --config=path/to/.checksy.yaml check

# Run with config from stdin
cat path/to/.checksy.yaml | checksy --stdin-config check

# Attempt to auto-fix failures when fixes are defined
checksy --config=path/to/.checksy.yaml check --fix

# Emit the configuration JSON schema
checksy schema > dist/config.schema.json

# Only execute warn+ rules but fail only on errors
checksy check --check-severity=warn --fail-severity=error
```

The `check` command executes each configured rule, printing ✅/⚠️/❌ for every check, forwarding any failing command output to stderr, and returning a non-zero exit code when something breaks. Passing `--fix` attempts to run each rule's optional `fix` script to resolve issues before re-running the check. The `schema` command reflects over the configuration struct and outputs a machine-readable JSON Schema definition that downstream tooling can validate against.

Use `--check-severity/--cs` to decide which rules run and `--fail-severity/--fs` to decide which severities cause the command to exit non-zero. When omitted, checks default to running for warn+ rules and the command only fails for error-level rules. Failing checks below the fail severity threshold still surface with a ⚠️ indicator but no longer abort the run.

### Git-based Remote Configs

Remote configs can reference git repositories using the format:
```
git+<repo-url>#<ref>:<path>
```

- `repo-url`: The git repository URL (e.g., `https://github.com/org/repo.git`)
- `ref` (optional): Branch, tag, or commit (defaults to `main`)
- `path` (optional): Path to config file within the repo (defaults to `.checksy.yaml`)

Examples:
```yaml
# Default ref (main) and path (.checksy.yaml)
remote: git+https://github.com/org/shared-checks.git

# Specific branch
remote: git+https://github.com/org/shared-checks.git#develop

# Specific tag and custom config path
remote: git+https://github.com/org/shared-checks.git#v1.0.0:configs/dev.yaml
```

**Caching git remotes:** Before using git-based remotes, you must cache them:
```bash
# Cache all git remotes referenced in the config
checksy install

# Cache with pruning of unused refs
checksy install --prune
```

Git remotes are cached in the `.checksy-cache/git/` directory (or the path specified by `cachePath` in your config). Each unique repository and ref combination gets a shallow clone (`--depth 1`).

These severity options can also be set in the config file at the top level:

```yaml
checkSeverity: warn
failSeverity: warn
rules:
  - name: "Example"
    check: echo "hello"
```


## Configuration

`checksy --config=path/to/workspace.yaml check` loads the provided YAML, validates it against the same JSON Schema emitted by the `schema` command, and aborts if validation fails. When the flag is omitted, the command automatically looks for `.checksy.yaml` or `.checksy.yml` in the current working directory so repositories can keep a shared default. Every rule's command executes relative to the directory containing the resolved config file, so you can point the CLI at any workspace path while keeping rule definitions portable.

### Inline rules, preconditions, and patterns

- **`preconditions`** — An array of rule objects that run **before** the main rules. They follow the same failure/fix behavior as regular rules. Useful for checks that must pass before proceeding (e.g., verifying dependencies).
- **`rules`** — An array of rule objects, each with `name`, `check`, optional `severity`, `fix`, and `hint`. These run first in config order.
- **`patterns`** — An array of glob-style patterns that select script files to run as rules (e.g. `tests/*.sh`). Success and failure are determined by the script's exit code, same as inline rules. There is no fix step for file-based rules; they run after inline rules in a deterministic order (alphabetically by file path). Patterns are resolved relative to the config file directory. You can use **positive** patterns (any match is included) and **negated** patterns (prefix with `!` to exclude). A file is included only if it matches at least one positive pattern and no negative pattern.

### Remote Config References

Rules can reference other config files using the `remote` property. This enables modular, reusable check configurations:

```yaml
rules:
  # Reference a local file (relative to this config)
  - remote: shared/team-checks.yaml
  
  # Reference a git repository (requires `checksy install` first)
  - remote: git+https://github.com/org/shared-checks.git#main
  
  # Your local rules follow...
  - name: "Project-specific check"
    check: echo "hello"
```

When a remote rule is expanded, all its preconditions and rules are loaded inline and inherit the parent config's defaults. Circular references are automatically detected and skipped.

Example with preconditions:

```yaml
preconditions:
  - name: "Prerequisite check"
    check: test -f required-file.txt
rules:
  - name: "Example rule"
    check: echo "optional check"
patterns:
  - "scripts/check-*.sh"
  - "!scripts/check-skip.sh"
```
