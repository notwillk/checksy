# Rulesy

Rulesy provisions the current machine from a trusted YAML configuration. It
runs checks, applies configured fixes when asked, and re-runs checks to confirm
the result.

Configuration may come from an explicit local file, an automatically discovered
`.rulesy.yaml` or `.rulesy.yml`, or stdin. Fetching, updating, authenticating,
and unpacking configuration are responsibilities of the surrounding shell or
automation. Existing Git acquisition remains available temporarily for
compatibility, but it is not part of the target architecture.

## Product family

Rulesy is the implemented provisioner in the
[Rulesy family monorepo](../../README.md). The
[product-family documentation](../../docs/README.md) records the completed
rename, current repository organization, and two proposed adjacent products:

- **RulesyOS** — a firmware-style Buildroot Linux substrate that owns verified
  boot, configuration acquisition and authentication, persistent generations,
  firmware updates, recovery, and boot-time invocation of Rulesy.
- **Rulesy Compose** (`rulesy-compose`) — a host-side artifact composer that
  builds, seals, validates, and explicitly publishes images by invoking the
  real Rulesy evaluator.

The historical [product-family decision](../../docs/decisions/rulesy-product-family.md)
and current [monorepo decision](../../docs/decisions/monorepo.md) preserve
these boundaries. RulesyOS has bootstrap-only build and KVM test
infrastructure, while Rulesy Compose remains documentation-only. Rulesy
remains the same trusted local check/fix CLI; acquisition, authentication,
firmware state, artifact building, and publication stay outside it.

## Provisioning contract

`rulesy check --fix` is Rulesy's only provisioning lifecycle. Rulesy does not
provide a separate `apply` command, daemon, scheduler, enrollment service, or
rollback engine.

Rulesy intentionally executes arbitrary Bash supplied by trusted configuration
with the permissions of the invoking user. Commands are not sandboxed, fixes may
partially mutate the machine before failing, and Rulesy cannot transactionally
undo those mutations. Rulesy never invokes `sudo` automatically.

The following interaction and locking rules are the normative P0 contract.
Supervised commands, `skip-if`, `interactive-fix`, `--non-interactive`, and the
provisioning lock are implemented.

### Interaction modes

- `skip-if` predicates, checks, pattern scripts, ordinary `fix` commands, and
  final checks are non-interactive. They receive `/dev/null` as stdin and run
  in a new session without a controlling terminal, so they cannot fall back to
  `/dev/tty` for input.
- Only `interactive-fix` may use a terminal. It runs only during `check --fix`,
  after its rule fails its check, and never alongside an ordinary `fix`.
- A missing terminal is relevant only when a failed rule needs its
  `interactive-fix`. A passing rule never requires a terminal merely because it
  defines one.
- `--non-interactive` prohibits terminal use but still permits ordinary fixes.
- `--stdin-config` always implies non-interactive execution. A stdin-supplied
  configuration never opens `/dev/tty` or receives a PTY.
- When a needed interactive repair cannot run, Rulesy prints a reason-specific
  diagnostic, leaves the rule failed at its configured severity, and continues
  normal reporting. This is a compliance failure rather than an operational
  failure.

### Command supervision

Every configured command runs through the same Linux/macOS process-supervision
layer. Skip predicates, checks, ordinary fixes, final checks, and pattern
scripts use the non-interactive mode described above. An executable rule may
set `timeout` to a positive duration matching `^[1-9][0-9]*(ms|s|m|h)$`, from
`1ms` through `2h`. The default is `15m`. A rule's timeout applies independently
to its `skip-if`, initial check, ordinary or interactive fix, and final recheck;
pattern scripts always use `15m`.

Each non-interactive command becomes the leader of its own session and managed
process group. At the deadline Rulesy sends `TERM` to the group, waits up to
five seconds, then sends `KILL` and allows up to five more seconds for final
process and pipe cleanup. Rulesy continuously drains non-interactive stdout and
stderr. Each stream retains at most 1 MiB as equal head and tail halves, with
`... N bytes omitted from bounded process output ...` between them when output
was truncated. A successful Bash leader is not complete while another process
remains in its managed group; lingering descendants are supervised through the
same deadline and escalation.

While either kind of command is active, Rulesy catches `SIGINT`, `SIGTERM`,
`SIGHUP`, and `SIGQUIT`. It forwards the first signal to the managed group,
escalates after the same five-second grace, and sends `KILL` immediately after
a second termination signal. After cleanup and captured diagnostics, Rulesy restores
conventional termination behavior and re-raises the first signal so the
invoking shell observes the usual signal status. Internally, each command saves
the exact incoming signal dispositions and restores them before returning. For
an interruption they remain installed through diagnostic flushing, are then
restored, and signal-hook's platform-default action is emulated before control
can return.

An ordinary nonzero check or repair remains a compliance result governed by
rule severity, fixes, and `--no-fail`. A completed nonzero `skip-if` instead
means its condition did not match, so Rulesy proceeds to the check; individual
nonzero values have no special meanings. Spawn, timeout, child-signal, and
supervision failures are operational exit `2`, stop the run immediately, and
cannot be masked by `--no-fail`. A configuration containing only `patterns`
still runs matching scripts; a definition with no executable rules or matching
patterns completes without starting a command. The network-free
[process-runner fixture corpus](fixtures/process-runner/README.md) exercises
these guarantees through the compiled binary. Supervision is not a sandbox:
trusted Bash keeps the invoker's filesystem, network, and process authority;
Rulesy applies no CPU, memory, or disk quota; and a command that deliberately
creates another session can escape the managed-descendant guarantee. Legacy Git
commands used by `install` are not routed through this configured-command
runner.

### Conditional execution

An executable rule may set `skip-if` to a nonblank, NUL-free Bash command.
After severity filtering, Rulesy runs it once immediately before that rule's
initial check in both check-only and `--fix` modes. The predicate inherits
Rulesy's environment, receives `/dev/null`, uses a fresh application of the
rule's timeout, and runs in the same effective working directory as the
associated check.

Predicate exit `0` reports exactly `⏭️ <name> (skipped)` and suppresses the
check, ordinary or interactive repair, and final recheck. Every completed
nonzero exit proceeds to the check. Output from an ordinarily completed
predicate is control output and is not printed or retained as rule output. A
skipped outcome is retained in the report but is neither passed nor failed,
does not affect severity thresholds, and is not masked or changed by
`--no-fail`. When at least one rule skips, a successful run uses the summary
`😎 All applicable rules validated; N skipped`; a failure summary appends
`; N skipped`. Summaries are unchanged when nothing skips. Predicate spawn,
timeout, signal, or supervision failures are operational exit `2`, preserve
available bounded output, and prevent the check and all later commands from
running. The closed, network-free [skip-if fixture
corpus](fixtures/skip-if/README.md) covers command-availability and environment
gates plus file-backed and stdin configuration.

For an eligible file-backed `interactive-fix`, Rulesy validates the caller's
foreground controlling terminal, creates an inner PTY, and runs the repair as a
new session and process-group leader with that PTY as its controlling terminal.
Input and merged terminal output are relayed live; interactive output is not
duplicated later in the normal report. Rulesy adds no confirmation prompt—the
configured command owns its interaction. Timeout, descendant cleanup, and
parent-signal forwarding use the same bounded lifecycle as non-interactive
commands, and the caller's terminal attributes are restored on every return
path. Outer job-control signals and foreground-terminal loss are cleaned up as
operational failures instead of leaving Rulesy suspended with a live repair.
A completed nonzero repair leaves the original rule failure in place,
skips the final check, and continues; a terminal-supervision failure is
operational exit `2` and stops later commands. The closed, network-free
[interactive-fix fixture corpus](fixtures/interactive-fix/README.md) exercises
these PTY and headless contracts through the compiled binary.

### Provisioning lock

Every `check --fix` invocation takes one nonblocking advisory lock for its
effective user. The namespace is independent of the configuration path,
working directory, stdin versus file input, includes, and legacy `cachePath`.

| Platform and effective user | Lock file |
| --- | --- |
| Linux non-root | `<account-home>/.local/state/rulesy/provision.lock` |
| macOS non-root | `<account-home>/Library/Application Support/rulesy/provision.lock` |
| Linux root | `/var/lib/rulesy/provision.lock` |
| macOS root | `/Library/Application Support/rulesy/provision.lock` |

The account home is resolved from the operating-system account database, not
from `$HOME` or XDG environment variables. File-backed, auto-discovered, and
stdin provisioning under the same effective UID therefore contend on the same
lock; root and non-root users intentionally use separate namespaces. Check-only
runs and `install` remain lock-free.

Rulesy validates the invocation and locally available configuration before
locking. It then acquires the lock before any missing legacy Git cache is
materialized, progress is printed, or configured command starts, and holds it
through initial checks, repairs, final checks, and reporting. Contention fails
immediately with exit `4`; `--no-fail` cannot mask it.

The Rulesy-owned lock directory is `0700` and `provision.lock` is a persistent
`0600` regular, single-link file owned by the effective UID. Rulesy opens it
without following links, rejects ownership, mode, type, hardlink, and pathname
substitution, and prevents the descriptor from reaching configured commands.
Lock contents are never interpreted or rewritten. Descriptor lifetime—not PID
text or file removal—determines ownership, so close or process death releases
the lock and stale contents do not block a later run. The closed, network-free
[provisioning-lock fixture corpus](fixtures/provisioning-lock/README.md)
exercises the CLI and filesystem contract.

This is cooperative serialization for Rulesy itself, not a machine-wide
security boundary. Trusted commands and unrelated programs can mutate the host
without participating in the advisory lock.

### Integrated P0 acceptance coverage

The closed, network-free [P0 acceptance fixture
corpus](fixtures/p0-acceptance/README.md) exercises the complete provisioning
lifecycle through the compiled public CLI. It combines local and stdin ordinary
repairs, skip predicates, terminal and headless interactive repairs, semaphore
contention, bounded descendant cleanup, and invalid-configuration preflight.
The focused fixture corpora above remain authoritative for each feature's edge
cases; the P0 corpus proves that those contracts compose end to end.

### Exit status

| Exit | Meaning |
| ---: | --- |
| `0` | Successful run, or a compliance failure masked by `--no-fail` |
| `1` | Existing no-command usage fallback |
| `2` | Invalid invocation/configuration or an operational failure |
| `3` | Unmasked rule-compliance failure |
| `4` | Provisioning lock contention |

`--no-fail` masks only rule-compliance exit `3`. It never masks an operational
failure or lock contention.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/rulesy/main/scripts/install.sh | bash
```

Beginning with Rulesy `0.7.7`, official Linux release archives are statically
linked musl executables for `x86_64` and `aarch64`; they do not inherit a glibc
version requirement from the release runner. Archive names remain
`rulesy_linux_x86_64.tar.gz` and `rulesy_linux_aarch64.tar.gz`.

The static builds resolve provisioning-lock homes for root and users present in
the machine's local `/etc/passwd` database. Accounts available only through
glibc NSS modules such as LDAP or SSSD are not supported by the official
binary; build Rulesy from source for a GNU target when that integration is
required.

## Uninstallation

```bash
curl -fsSL https://raw.githubusercontent.com/notwillk/rulesy/main/scripts/uninstall.sh | bash
```

## Project Layout

```text
products/rulesy/
  Cargo.toml           # Rust package definition
  Cargo.lock           # Pinned Rust dependency graph
  src/
    main.rs            # CLI entry point
    cache.rs           # Git remote cache management
    cli.rs             # Argument parsing and command wiring
    config.rs          # Shared strict configuration loading
    check.rs           # Check execution and reporting helpers
    process_runner.rs  # Bounded headless and PTY process supervision
    provision_lock.rs  # Per-effective-user provisioning semaphore
    git.rs             # Git operations for caching remotes
    schema.rs          # Strict types and generated Draft 7 schema
    version.rs         # Centralized version string
  tests/               # Compiled-binary integration contracts
  fixtures/            # Closed network-free fixture corpora
  scripts/             # Cross-build and release implementations
  docs/                # Product-owned operational docs
```

## Building

```bash
moon run rulesy:build
```

The resulting binary can be copied anywhere on your `PATH` if desired. Cargo
commands use `products/rulesy/Cargo.toml`. Rulesy's tasks are defined in
[`moon.yml`](moon.yml); Moon resolves them from the repository root and
executes them in the Rulesy project.

### Devcontainer provisioning

This repository dogfoods Rulesy for its development environment. The
[devcontainer configuration](../../.devcontainer/devcontainer.json) bootstraps
Rulesy `0.8.1` through Feature `1.0.1`, referenced by its immutable canonical
OCI manifest digest. The digest pins the Feature implementation, while its
`version` option pins the Rulesy release selected by that implementation. The
base is kept on the Ubuntu `26.04` release line so security rebuilds remain
available without silently changing the container's Ubuntu release.

After the workspace is created, the
[Rulesy configuration](../../.devcontainer/rulesy.yaml) provisions Entr, Moon
`2.4.5` with Moon executor `2.4.5`, Rustup `1.29.0` with the exact Rust
`1.94.1` toolchain, and Dev Container CLI `0.88.0`. For local development it
also installs Codex CLI `0.145.0`; that rule skips when GitHub Actions sets
`GITHUB_ACTIONS=true`, so CI does not install an unused interactive
development tool. The Moon and Rustup release assets use versioned URLs with
pinned architecture-specific SHA-256 values. The Rust toolchain includes the
`rustfmt` and `clippy` components used by Quality CI. Shared pins live in
[tool-versions.env](../../.devcontainer/tool-versions.env), and checks and fixes are
grouped by tool under the focused
[provisioning helpers](../../.devcontainer/scripts/). Shared behavior lives in
[`shared/lib.sh`](../../.devcontainer/scripts/shared/lib.sh), with network-free
coverage in [`tests/run.sh`](../../.devcontainer/scripts/tests/run.sh).
The provisioned `codex` command lives under `~/.local/opt/codex-cli`;
authenticate it on first use with `codex login`.

The container lifecycle runs provisioning automatically through the direct
`postCreateCommand` and `postStartCommand` entries in
[`devcontainer.json`](../../.devcontainer/devcontainer.json). Both invoke the
digest-pinned Rulesy binary. From the repository root, use these exact commands
to converge it again or verify it without applying fixes:

```bash
rulesy --config=.devcontainer/rulesy.yaml check --fix --non-interactive
rulesy --config=.devcontainer/rulesy.yaml check --non-interactive
```

The base image, Docker-in-Docker, editor settings, and immutable Rulesy Feature
remain in `devcontainer.json`. They establish the container and bootstrap
Rulesy itself; Rulesy owns Rustup, Rust, and the other guest userland tools
provisioned after that environment and workspace exist. The remote environment
prepends the dedicated Codex CLI prefix, `/home/vscode/.local/bin`, and
`/home/vscode/.cargo/bin` so the user-owned tools are available to lifecycle
commands and terminals.

### Cross-compiling

```bash
moon run rulesy:cross-compile -- <target>
```

Cross-compile for a different architecture/target (for example,
`aarch64-unknown-linux-musl`). Linux release targets must use musl; artifacts
are written under `products/rulesy/dist/`.

## Running

```bash
# Show help
rulesy help

# Run the workspace validation rules
rulesy --config=path/to/.rulesy.yaml check

# Run with config from stdin
cat path/to/.rulesy.yaml | rulesy --stdin-config check

# Attempt to auto-fix failures when fixes are defined
rulesy --config=path/to/.rulesy.yaml check --fix

# Permit ordinary fixes while explicitly prohibiting terminal repairs
rulesy --config=path/to/.rulesy.yaml check --fix --non-interactive

# Emit the configuration JSON schema
rulesy schema > products/rulesy/dist/config.schema.json

# Only execute warn+ rules but fail only on errors
rulesy check --check-severity warn --fail-severity error
```

The `check` command executes each configured rule, printing ✅/⚠️/❌ for every
check, forwarding any failing command output to stderr, and returning a non-zero
exit code when something breaks. Passing `--fix` attempts the rule's optional
ordinary `fix` or terminal-capable `interactive-fix`, then re-runs the check
after a successful repair. `--non-interactive` disables only
`interactive-fix`; it is accepted with or without `--fix`. The `schema` command
deterministically generates the Draft 7 JSON Schema from the same strict Rust
model used at runtime.

Use `--check-severity/--cs` to decide which rules run and `--fail-severity/--fs` to decide which severities cause the command to exit non-zero. When omitted, checks currently run at every severity and the command only fails for error-level rules. Failing checks below the fail severity threshold still surface with a ⚠️ indicator but do not make the run fail.

### Legacy Git-based remote configs

The following built-in acquisition behavior is retained for compatibility. New
automation should acquire and authenticate configuration outside Rulesy, then
pass a local file or self-contained stdin document. Runtime deprecation belongs
to the later Git-compatibility milestone and is not introduced here.

Remote configs can reference git repositories using the format:
```
git+<repo-url>#<ref>:<path>
```

- `repo-url`: The git repository URL (e.g., `https://github.com/org/repo.git`)
- `ref` (optional): Branch, tag, or commit (defaults to `main`)
- `path` (optional): Path to config file within the repo (defaults to `.rulesy.yaml`)

Examples:
```yaml
# Default ref (main) and path (.rulesy.yaml)
remote: git+https://github.com/org/shared-checks.git

# Specific branch
remote: git+https://github.com/org/shared-checks.git#develop

# Specific tag and custom config path
remote: git+https://github.com/org/shared-checks.git#v1.0.0:configs/dev.yaml
```

**Caching git remotes:** Before using git-based remotes, you must cache them:
```bash
# Cache all git remotes referenced in the config
rulesy install

# Cache with pruning of unused refs
rulesy install --prune
```

Git remotes are cached in the `.rulesy-cache/git/` directory (or the path specified by `cachePath` in your config). Each unique repository and ref combination gets a shallow clone (`--depth 1`).

Origin-aware execution does not redesign legacy Git cache selection or
acquisition. Once a cached definition resolves, its commands use that
definition's directory; acquisition behavior remains unchanged and is tracked
for deprecation separately.

These severity options can also be set in the config file at the top level:

```yaml
checkSeverity: warn
failSeverity: warn
rules:
  - name: "Example"
    check: echo "hello"
```


## Configuration

`rulesy --config=path/to/workspace.yaml check` strictly decodes the complete
configuration before any configured command runs. The same decoder is used for
explicit and auto-discovered root files, nested local files, cached legacy Git
files, stdin, `check --fix`, and the local include graph inspected by
`install`. When the flag is omitted, Rulesy looks for `.rulesy.yaml` or
`.rulesy.yml` in the current working directory.

Top-level and rule objects are closed: unknown fields, duplicate YAML keys,
explicit nulls, scalar-to-string coercion, malformed rule forms, blank checks
or includes, invalid severities, invalid globs, and NUL-containing strings are
rejected. `rules` and the other top-level collections may be omitted or empty.
Severity input remains ASCII case-insensitive and accepts both `warn` and
`warning`.

Every rule is exactly one of these forms:

- An include with one nonblank `remote` field and no other fields.
- An executable rule with a nonblank `check` plus optional `name`, `severity`,
  `skip-if`, `fix`, `interactive-fix`, `hint`, and `timeout` fields. `fix` and
  `interactive-fix` are mutually exclusive.

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
`skip-if`, initial check, an ordinary or interactive fix, and its final recheck;
pattern scripts use `15m` and do not have per-pattern overrides.

`skip-if` must contain a nonblank, NUL-free command, requires `check`, and
cannot appear on an include. Predicate exit `0` skips the complete rule;
completed nonzero exits run it normally. Predicate process failures are
operational errors rather than skip decisions.

`interactive-fix` must contain a nonblank, NUL-free command and cannot appear
with `fix` or on an include. It is ignored during check-only runs and when its
check passes. During `check --fix`, stdin configuration is always
non-interactive: both `--stdin-config` and `--config -` prohibit opening
`/dev/tty` or allocating a PTY, even if Rulesy itself has a controlling
terminal. If a failed rule needs an interactive repair in that mode, with
`--non-interactive`, or without a usable foreground terminal, Rulesy leaves the
repair unexecuted and explains how to proceed.

For configuration loaded through the CLI, every file-backed `skip-if`, check,
repair, final recheck, and pattern script executes relative to the directory
of the configuration that defines it. A predicate and its associated check
always share that working directory. Stdin configuration remains
self-contained and uses the caller's current working directory. The public
Rust `load()` and `diagnose(Options)` compatibility APIs remain flat;
`diagnose` uses the single working directory supplied by its caller.

### Inline rules, preconditions, and patterns

- **`preconditions`** — An array of rule objects that run **before** the main rules. They support the same conditional, failure, and ordinary/interactive repair behavior as regular rules. Useful for checks that apply only after a prerequisite condition is met.
- **`rules`** — An array of rule objects, each with `name`, `check`, optional `skip-if`, `severity`, one of `fix` or `interactive-fix`, `hint`, and `timeout`. These run first in config order.
- **`patterns`** — An array of glob-style patterns that select script files to run as rules (e.g. `tests/*.sh`). Success and failure are determined by the script's exit code, same as inline rules. There is no fix step or timeout override for file-based rules; they use the 15-minute default and run after inline rules. Pattern groups run root-first and then in first-seen depth-first include order; matches are alphabetical within each group, and negations apply only to their defining group. Pattern-only configurations execute normally. Patterns are resolved relative to the defining config file directory. You can use **positive** patterns (any match is included) and **negated** patterns (prefix with `!` to exclude). A file is included only if it matches at least one positive pattern and no negative pattern.

### Remote Config References

Rules can reference other config files using the `remote` property. This enables modular, reusable check configurations:

```yaml
rules:
  # Reference a local file (relative to this config)
  - remote: shared/team-checks.yaml
  
  # Reference a git repository (requires `rulesy install` first)
  - remote: git+https://github.com/org/shared-checks.git#main
  
  # Your local rules follow...
  - name: "Project-specific check"
    check: echo "hello"
```

When an include is expanded, all its preconditions and rules are loaded inline,
inherit the parent config's defaults, and retain the included file's defining
directory. Active include cycles fail before any configured command runs and
report the ordered include chain. A definition reached again after its first
complete expansion is deduplicated. The network-free
[local-origin contract](fixtures/local-origin/README.md) exercises these
behaviors through the compiled CLI.

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
