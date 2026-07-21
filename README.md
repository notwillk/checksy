# checksy

checksy is a Rust-based command line utility intended to run lightweight health checks against a development workspace.

## Security

Checksy rules and fixes are arbitrary shell code executed with the invoking
process's permissions; they are not sandboxed. Current Git remote caching does
not verify an authorized publisher and is not suitable for unattended privileged
execution. The CLI confines fetched Git config files and pattern matches to their
cached checkout. File-backed `install` and `check --fix` invocations also hold an
operating-system advisory lock on the canonical legacy cache root for their full
run, serializing cooperating Checksy processes. Ordinary `check` and stdin
`check --fix` remain unlocked, and the advisory lock neither authenticates the
mutable cache nor excludes an uncooperative local actor. An authorized shell
command can still access anything its process identity can access. On Linux and
macOS, checks, fixes, pattern scripts, and Git operations use a shared bounded
subprocess runner: stdin is `/dev/null`, each child starts in its own process
group, and a timeout sends `TERM` to that group before a bounded `KILL` fallback.
This is process control, not a sandbox; a background descendant that closes its
capture descriptors before the command leader exits, a deliberately detached
descendant, or work surviving an external signal to Checksy may outlive the
invocation. Run only definitions you trust. The planned fail-closed security
contract and current
implementation gaps are documented in the [threat model](THREAT_MODEL.md). The
frozen target formats, CLI, and resource bounds are specified in the
[pull-agent contract](PULL_AGENT_CONTRACT.md).

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
  process_runner.rs    # Bounded Bash/Git subprocess supervision
  git.rs               # Git operations for caching remotes
  resolved.rs          # Internal source/origin-aware definition model
  state_lock.rs        # Advisory state/cache-directory lock
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

The `check` command executes each configured rule, printing ✅/⚠️/❌ for every check, forwarding any failing command output to stderr, and returning a non-zero exit code when something breaks. Passing `--fix` attempts to run each rule's optional `fix` script to resolve issues before re-running the check. The `schema` command prints a deterministic Draft 7 JSON Schema generated from the strict Rust configuration model for downstream tooling.

On Linux and macOS, Bash rules default to a 15-minute timeout. A rule's optional
`timeout` duration may shorten that default but cannot extend it. Configured
durations use positive fixed units (`ms`, `s`, `m`, `h`, or `d`) and have a
2-hour format-level maximum; the current runner rejects values above its
effective 15-minute command ceiling. Checksy keeps
at most 1 MiB from each stdout/stderr stream by retaining equal head and tail
regions while continuously draining the discarded middle. On timeout it sends
`TERM` to the command's process group, waits up to 5 seconds, and then sends
`KILL`; a timeout or signal termination remains distinct from an ordinary
nonzero exit. Bash's inherited stdin is `/dev/null`, so an ordinary stdin read
gets EOF. Arbitrary shell code can still open `/dev/tty` or another device
explicitly, but the command timeout remains in force. A timeout or signal
termination is an operational failure (exit `2`) and `--no-fail` cannot mask
it; ordinary rule exits continue through the configured severity policy.

Use `--check-severity/--cs` to decide which rules run and `--fail-severity/--fs` to decide which severities cause the command to exit non-zero. The current implementation defaults to `debug` check severity and `error` fail severity, so all rules run and only error-level failures make the command fail. Failing checks below the fail severity threshold still surface with a ⚠️ indicator but no longer abort the run. Configuration severity values are case-insensitive for compatibility, but `debug`, `info`, `warn`, `warning`, and `error` are the canonical spellings; successful CLI loads warn when a recognized non-lowercase spelling is used.

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

Git remotes are cached in the `.checksy-cache/git/` directory (or the path
specified by `cachePath` in the selected root config). Each repository/ref
locator is mapped to a legacy shallow-clone slot (`--depth 1`). The root config alone
chooses this legacy cache location; a nested local or Git definition cannot
redirect acquisition with its own `cachePath`. `install` discovers nested file
and Git remotes iteratively as their parent repositories become available.
Git revision lookups use a 1-minute timeout and clone/fetch work uses a 5-minute
timeout. Git stdin is `/dev/null`; Git terminal prompts and OpenSSH password
prompts are disabled, and credential helpers are requested to run
noninteractively. A nonconforming helper can still open `/dev/tty`, but remains
inside the source timeout. HTTPS acquisition is not implemented yet, so its
separately specified HTTP bounds remain P4 work.

For a file-backed root config, `install` and the complete `check --fix`
invocation acquire a nonblocking operating-system advisory lock anchored at the
canonical legacy cache root's persistent `lock` file. A competing mutation of
that root exits with the distinct lock-held result (`4`) instead of starting
work. The lock is released automatically when its owning process exits, so stale
PID text is not used to decide ownership. Ordinary `check` does not take the
cache lock, and stdin fix mode has no file-backed cache root to lock; configured
commands can still have arbitrary host side effects in either mode.

This cache is still legacy, mutable, and unauthenticated. Its `.git` directory
only indicates that a clone exists; it does not establish an authorized signer
or make the cached content safe for unattended execution. Its historical ref
directory encoding is also not collision-resistant; a future source-provider
layer will replace it rather than treating it as a persistent source identity.

These severity options can also be set in the config file at the top level:

```yaml
checkSeverity: warn
failSeverity: warn
rules:
  - name: "Example"
    check: echo "hello"
```


## Configuration

`checksy --config=path/to/workspace.yaml check` strictly deserializes the provided YAML using the Rust configuration types. Unknown and duplicate fields, incorrectly typed or null values, malformed rule forms, empty commands, NUL bytes in command/path/pattern fields, invalid patterns, and invalid or excessive rule timeouts are rejected before any configured rule command executes. The generated Draft 7 schema has fixture-tested parity for structural validation. Duplicate YAML keys remain a parser-layer check; the complete Rust glob grammar plus duration numeric overflow and the 2-hour hard maximum remain runtime-layer checks. These narrow exceptions are documented in the [strict configuration fixture corpus](fixtures/strict-config/README.md). When the flag is omitted, the command automatically looks for `.checksy.yaml` or `.checksy.yml` in the current working directory so repositories can keep a shared default.

The `check` CLI and its deprecated `diagnose` alias retain an internal origin for
every resolved rule and pattern group. Inline checks and fixes run from the
directory containing the config that defined them; relative shell references to
Brewfiles, templates, and other files therefore use that same directory.
Patterns are also expanded and executed from their defining config's directory.
The checked-in [origin regression fixture](fixtures/origin-regression/README.md)
exercises these rules across a root definition, a nested definition, and their
separately owned auxiliary assets and pattern scripts.
The public Rust `load()` and `diagnose(Options)` interfaces remain flat for
source compatibility and do not expose this per-definition origin model.
Their compatibility projection retains only the selected root config's pattern
list; nested remote pattern groups are available only through the CLI's private
resolved path.

### Inline rules, preconditions, and patterns

- **`preconditions`** — An array of rule objects that run **before** the main rules. They follow the same failure/fix behavior as regular rules. Useful for checks that must pass before proceeding (e.g., verifying dependencies).
- **`rules`** — An array of rule objects, each with `name`, `check`, optional `severity`, `fix`, `hint`, and positive fixed-unit `timeout` (format maximum `2h`, current effective maximum `15m`). These run first in config order. A per-rule timeout can only shorten the 15-minute command default and applies to its check, fix, and post-fix recheck.
- **`patterns`** — An array of glob-style patterns that select script files to run as rules (e.g. `tests/*.sh`). Success and failure are determined by the script's exit code, same as inline rules. There is no fix step for file-based rules. All pattern groups run after inline rules; the selected config's group comes first, followed by nested groups in deterministic discovery order, with files sorted within each group. Patterns are resolved relative to the config that defines them. You can use **positive** patterns (any match is included) and **negated** patterns (prefix with `!` to exclude). Negations apply only within their defining config's group.

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

When a remote rule is expanded, its preconditions and rules are loaded inline,
its patterns are retained for the final pattern phase, and inherited severity
defaults keep their existing behavior. Nested file and Git remotes are supported.
A definition already completed during one load is included once; an active
circular reference is a configuration error instead of being silently skipped.

Local definitions preserve the legacy trusted-workspace behavior: file remotes
may resolve outside the selected config's directory, and local patterns keep
their previous external-path and symlink handling. There is not yet a protected
local external-root policy. Fetched Git config paths, nested file remotes, and
concrete pattern matches must instead remain inside the canonical cached
checkout; traversal and symlink escapes fail before any rule runs. These fetched
path checks constrain structured references only. Before clone, refresh, or
prune, the CLI rejects cache symlink components already present below the
operator-selected root. The advisory cache-root lock serializes cooperating
file-backed `install` and `check --fix` processes, but ordinary checks and
uncooperative local actors do not participate. Descriptor-relative ancestor
opening and a protected generation state store are not implemented, so an actor
that can modify the legacy cache can still race these path-based checks. Checks
and fixes remain arbitrary shell and can deliberately access paths outside the
checkout.

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
