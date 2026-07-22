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

The following interaction and locking rules are the normative P0 contract. The
supervised non-interactive runner is implemented. `interactive-fix`,
`--non-interactive`, and the provisioning lock are not available at the current
HEAD.

### Interaction modes

- Checks, pattern scripts, ordinary `fix` commands, and final checks are
  non-interactive. They receive `/dev/null` as stdin and run in a new session
  without a controlling terminal, so they cannot fall back to `/dev/tty` for
  input. Future `skip-if` predicates will use the same runner.
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

### Command supervision

Every check and ordinary fix runs through the same Linux/macOS process
supervisor. An executable rule may set `timeout` to a positive duration matching
`^[1-9][0-9]*(ms|s|m|h)$`, from `1ms` through `2h`. The default is `15m`. A
rule's timeout applies independently to its initial check, fix, and final
recheck; pattern scripts always use `15m`.

Each command becomes the leader of its own session and managed process group.
At the deadline Checksy sends `TERM` to the group, waits up to five seconds,
then sends `KILL` and allows up to five more seconds for final process and pipe
cleanup. Checksy continuously drains stdout and stderr. Each stream retains at
most 1 MiB as equal head and tail halves, with
`... N bytes omitted from bounded process output ...` between them when output
was truncated. A successful Bash leader is not complete while another process
remains in its managed group; lingering descendants are supervised through the
same deadline and escalation.

While a command is active, Checksy catches `SIGINT`, `SIGTERM`, `SIGHUP`, and
`SIGQUIT`. It forwards the first signal to the managed group, escalates after
the same five-second grace, and sends `KILL` immediately after a second
termination signal. After cleanup and captured diagnostics, Checksy restores
conventional termination behavior and re-raises the first signal so the
invoking shell observes the usual signal status. Internally, each command saves
the exact incoming signal dispositions and restores them before returning. For
an interruption they remain installed through diagnostic flushing, are then
restored, and signal-hook's platform-default action is emulated before control
can return.

An ordinary nonzero command exit remains a compliance result governed by rule
severity, fixes, and `--no-fail`. Spawn, timeout, child-signal, and supervision
failures are operational exit `2`, stop the run immediately, and cannot be
masked by `--no-fail`. A configuration containing only `patterns` still runs
matching scripts; a definition with no executable rules or matching patterns
completes without starting a command. The network-free [process-runner fixture
corpus](fixtures/process-runner/README.md) exercises these guarantees through
the compiled binary. Supervision is not a sandbox: trusted Bash keeps the
invoker's filesystem, network, and process authority; Checksy applies no CPU,
memory, or disk quota; and a command that deliberately creates another session
can escape the managed-descendant guarantee. Legacy Git commands used by
`install` are not routed through this configured-command runner.

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
  config.rs            # Shared strict configuration loading
  check.rs             # Check execution and reporting helpers
  process_runner.rs    # Bounded non-interactive process supervision
  git.rs               # Git operations for caching remotes
  schema.rs            # Strict types and generated Draft 7 schema
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

The `check` command executes each configured rule, printing ✅/⚠️/❌ for every
check, forwarding any failing command output to stderr, and returning a non-zero
exit code when something breaks. Passing `--fix` attempts to run each rule's
optional `fix` script to resolve issues before re-running the check. The
`schema` command deterministically generates the Draft 7 JSON Schema from the
same strict Rust model used at runtime.

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

`checksy --config=path/to/workspace.yaml check` strictly decodes the complete
configuration before any configured command runs. The same decoder is used for
explicit and auto-discovered root files, nested local files, cached legacy Git
files, stdin, `check --fix`, and the local include graph inspected by
`install`. When the flag is omitted, Checksy looks for `.checksy.yaml` or
`.checksy.yml` in the current working directory.

Top-level and rule objects are closed: unknown fields, duplicate YAML keys,
explicit nulls, scalar-to-string coercion, malformed rule forms, blank checks
or includes, invalid severities, invalid globs, and NUL-containing strings are
rejected. `rules` and the other top-level collections may be omitted or empty.
Severity input remains ASCII case-insensitive and accepts both `warn` and
`warning`.

Every rule is exactly one of these forms:

- An include with one nonblank `remote` field and no other fields.
- An executable rule with a nonblank `check` plus optional `name`, `severity`,
  `fix`, `hint`, and `timeout` fields.

Stdin documents must be self-contained; includes require a filesystem context
and are rejected for `--stdin-config` and `--config -`. Shell commands remain
opaque trusted Bash. Duplicate-key and multi-document rejection belongs to the
YAML parser, while complete glob grammar is checked by the runtime after the
generated schema validates its structural constraints. The checked-in
[strict configuration corpus](fixtures/strict-config/README.md) exercises all
three validation layers and the compiled CLI, including timeout syntax,
type/null rejection, the `2h` ceiling, and rejection of timeouts on includes.

`timeout` accepts only `^[1-9][0-9]*(ms|s|m|h)$`, with an inclusive runtime
range from `1ms` through `2h`. Numeric overflow and the upper bound are runtime
validation constraints because Draft 7 cannot compute a mixed-unit duration.
Rules without `timeout` use `15m`. The same timeout starts afresh for the
initial check, an ordinary fix, and its final recheck; pattern scripts use
`15m` and do not have per-pattern overrides.

Commands currently execute relative to the selected root configuration's
directory; preserving the defining directory of every nested local include is
a separate origin-correctness milestone.

### Inline rules, preconditions, and patterns

- **`preconditions`** — An array of rule objects that run **before** the main rules. They follow the same failure/fix behavior as regular rules. Useful for checks that must pass before proceeding (e.g., verifying dependencies).
- **`rules`** — An array of rule objects, each with `name`, `check`, optional `severity`, `fix`, `hint`, and `timeout`. These run first in config order.
- **`patterns`** — An array of glob-style patterns that select script files to run as rules (e.g. `tests/*.sh`). Success and failure are determined by the script's exit code, same as inline rules. There is no fix step or timeout override for file-based rules; they use the 15-minute default and run after inline rules in a deterministic order (alphabetically by file path). Pattern-only configurations execute normally. Patterns are resolved relative to the config file directory. You can use **positive** patterns (any match is included) and **negated** patterns (prefix with `!` to exclude). A file is included only if it matches at least one positive pattern and no negative pattern.

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
