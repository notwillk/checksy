# checksy

checksy provisions the current machine from a trusted YAML configuration. It
runs checks, applies configured fixes when asked, and re-runs checks to confirm
the result.

Configuration may come from an explicit local file, an automatically discovered
`.checksy.yaml` or `.checksy.yml`, or stdin. Fetching, updating, authenticating,
and unpacking configuration are responsibilities of the surrounding shell or
automation. Existing Git acquisition remains available temporarily for
compatibility, but it is not part of the target architecture.

## Provisioning contract

`checksy check --fix` is Checksy's only provisioning lifecycle. Checksy does not
provide a separate `apply` command, daemon, scheduler, enrollment service, or
rollback engine.

Checksy intentionally executes arbitrary Bash supplied by trusted configuration
with the permissions of the invoking user. Commands are not sandboxed, fixes may
partially mutate the machine before failing, and Checksy cannot transactionally
undo those mutations. Checksy never invokes `sudo` automatically.

The following interaction and locking rules are the normative P0 contract. They
are documented before implementation; `interactive-fix`, `--non-interactive`,
and the provisioning lock are not available at the current HEAD.

### Interaction modes

- Checks, pattern scripts, ordinary `fix` commands, final checks, and future
  `skip-if` predicates are non-interactive and receive `/dev/null` as stdin.
- Only the future `interactive-fix` command may use a terminal. It runs only
  after its rule fails its check.
- A missing terminal is relevant only when a failed rule needs its
  `interactive-fix`. A passing rule never requires a terminal merely because it
  defines one.
- The future `--non-interactive` flag prohibits terminal use but still permits
  ordinary fixes.
- `--stdin-config` always implies non-interactive execution. A stdin-supplied
  configuration never opens `/dev/tty` or receives a PTY.
- When a needed interactive repair cannot run, the rule remains failed at its
  configured severity and Checksy continues normal reporting.

### Provisioning lock

Every future `check --fix` invocation will take one nonblocking advisory lock
for its effective user. The namespace is independent of the configuration path,
working directory, stdin versus file input, includes, and legacy `cachePath`.

| Platform and effective user | Lock file |
| --- | --- |
| Linux non-root | `<account-home>/.local/state/checksy/provision.lock` |
| macOS non-root | `<account-home>/Library/Application Support/checksy/provision.lock` |
| Linux root | `/var/lib/checksy/provision.lock` |
| macOS root | `/Library/Application Support/checksy/provision.lock` |

The account home is resolved from the operating-system account database, not
from `$HOME` or XDG environment variables. File-backed, auto-discovered, and
stdin provisioning under the same effective UID therefore contend on the same
lock; root and non-root users intentionally use separate namespaces. Check-only
runs remain lock-free. Lock contents are never interpreted, and descriptor
lifetime—not PID text or file removal—determines ownership.

### Exit status

| Exit | Meaning |
| ---: | --- |
| `0` | Successful run, or a compliance failure masked by `--no-fail` |
| `1` | Existing no-command usage fallback |
| `2` | Invalid invocation/configuration or an operational failure |
| `3` | Unmasked rule-compliance failure |
| `4` | Provisioning lock contention; reserved until locking is implemented |

`--no-fail` masks only rule-compliance exit `3`. It never masks an operational
failure or lock contention.

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
  check.rs             # Check execution and reporting helpers
  git.rs               # Git operations for caching remotes
  schema.rs            # Configuration schema definitions
  version.rs           # Centralized version string
```

## Building

`just build`

The resulting binary can be copied anywhere on your `PATH` if desired. Cargo
commands run from `src/`; the root `justfile` provides the common project tasks.

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
checksy check --check-severity warn --fail-severity error
```

The `check` command executes each configured rule, printing ✅/⚠️/❌ for every check, forwarding any failing command output to stderr, and returning a non-zero exit code when something breaks. Passing `--fix` attempts to run each rule's optional `fix` script to resolve issues before re-running the check. The `schema` command outputs the current machine-readable JSON Schema; generating that schema from strict runtime types is a separate P0 milestone.

Use `--check-severity/--cs` to decide which rules run and `--fail-severity/--fs` to decide which severities cause the command to exit non-zero. When omitted, checks currently run at every severity and the command only fails for error-level rules. Failing checks below the fail severity threshold still surface with a ⚠️ indicator but do not make the run fail.

### Legacy Git-based remote configs

The following built-in acquisition behavior is retained for compatibility. New
automation should acquire and authenticate configuration outside Checksy, then
pass a local file or self-contained stdin document. Runtime deprecation belongs
to the later Git-compatibility milestone and is not introduced here.

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

`checksy --config=path/to/workspace.yaml check` loads the provided YAML and
aborts if it cannot be deserialized. Strict runtime/schema parity is the next P0
configuration milestone. When the flag is omitted, the command automatically
looks for `.checksy.yaml` or `.checksy.yml` in the current working directory.
Commands currently execute relative to the selected root configuration's
directory; preserving the defining directory of every nested local include is
a separate origin-correctness milestone.

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
