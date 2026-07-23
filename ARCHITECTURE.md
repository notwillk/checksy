# checksy Architecture

## System Overview

checksy is a CLI provisioner: it takes a trusted configuration for the current
machine, runs checks, optionally applies fixes, and re-runs checks to confirm the
result. `check --fix` is the sole provisioning lifecycle. The target
architecture has no `apply` command, daemon, scheduler manager, enrollment
system, source-provider framework, status database, generation store, trust
database, or rollback engine.

Configuration enters through a local file, automatic local discovery, or
stdin. Acquisition, updates, authentication, and unpacking compose outside
Checksy. The current built-in Git cache and `install` command are legacy
compatibility surfaces pending a separate deprecation milestone; they are not a
foundation for new architecture.

## Devcontainer dogfooding

The development container exercises the provisioning lifecycle against a real
machine image. Its [devcontainer definition](.devcontainer/devcontainer.json)
bootstraps Checksy `0.7.6` with Feature `1.0.1` pinned by canonical OCI manifest
digest. Pinning both layers prevents a mutable Feature tag or its default
version from silently changing the bootstrap.

Once the workspace exists, `postCreateCommand` runs
`checksy --config=.devcontainer/checksy.yaml check --fix --non-interactive` as
the remote user. The [flat provisioning definition](.devcontainer/checksy.yaml)
uses [shared version data](.devcontainer/tool-versions.env) and
[repository-local helpers](.devcontainer/scripts/) to provision Entr, Just
`1.57.0`, Rustup `1.29.0` with Rust `1.94.1`, and Dev Container CLI `0.88.0`.
The Rustup bootstrap binary and Just archives use versioned URLs with pinned
architecture-specific SHA-256 values. The Rust toolchain includes `rustfmt` and
`clippy`, and `.cargo/bin` is prepended through the remote environment so
Quality CI and interactive terminals resolve that toolchain. Helpers are
separated into prerequisite, Entr, Just, Rustup, and Dev Container CLI
directories, with one [shared library](.devcontainer/scripts/shared/lib.sh) and
a network-free [test runner](.devcontainer/scripts/tests/run.sh). Their paths
are relative to the selected root configuration's `.devcontainer/` directory,
matching the current root-origin execution model.

This is deliberately a guest-userland boundary. The base image,
Docker-in-Docker, editor customization, and immutable Checksy Feature must exist
before Checksy can run or belong to the container/editor lifecycle, so they
remain declared outside Checksy. Rustup and Rust are provisioned inside that
boundary. Quality CI first converges the same definition and then runs it
check-only before invoking the provisioned `cargo`, `rustfmt`, and `clippy`,
proving provisioning and idempotence through the public CLI.

## Security and mutation boundary

Configured checks and fixes are trusted arbitrary Bash executed with the
invoking user's authority. They are not sandboxed. Checksy does not invoke
`sudo`, cannot transactionally roll back shell mutations, and may leave partial
machine changes when a fix fails. Correct and idempotent fixes remain the
configuration author's responsibility. Process supervision adds no CPU,
memory, filesystem, network, or disk quota. A command that deliberately creates
a different session is outside the managed-descendant guarantee.

## Normative P0 execution contract

This section records the implemented P0 execution contract. Hardened
non-interactive supervision, `skip-if`, `interactive-fix`,
`--non-interactive`, and the provisioning lock are covered individually and by
one integrated public-CLI acceptance gate.

### Interaction modes

- Skip predicates, checks, pattern scripts, ordinary fixes, and final checks are
  non-interactive, receive `/dev/null` as stdin, and run in a detached session
  without a controlling terminal.
- `interactive-fix` is the only terminal-capable command. It is considered only
  during `check --fix` after its check fails; a passing rule never requires a
  terminal. It is mutually exclusive with ordinary `fix`.
- `--non-interactive` prohibits terminal use without disabling ordinary fixes.
- `--stdin-config` implies non-interactive operation and never opens
  `/dev/tty`, allocates a PTY, or otherwise attaches a terminal.
- If a failed rule needs an interactive fix but terminal use is unavailable or
  prohibited, the fix is not run. The rule remains failed at its configured
  severity and reporting continues normally with an actionable diagnostic.
  This remains a compliance result rather than becoming an operational error.

### Supervised command lifecycle

The private Linux/macOS runner starts every configured non-interactive command
as the leader of a new session and process group. It supervises nonblocking
stdout and stderr pipes on a monotonic clock with no supervision interval
longer than 25 milliseconds. Stdin is always `/dev/null`; because the session
has no controlling terminal, opening `/dev/tty` also fails.

Executable rules use a `15m` compiled default or an optional `timeout` matching
`^[1-9][0-9]*(ms|s|m|h)$` in the inclusive range `1ms` through `2h`. A rule gets
a fresh timeout for its skip predicate, initial check, ordinary or interactive
fix, and final recheck. Pattern scripts use `15m`. The complete configuration
is validated before execution, including rejection of malformed, null,
overflowing, over-limit, or include-rule timeouts.

At a deadline the runner sends `TERM` to the whole managed group, permits a
five-second grace, sends `KILL` if anything remains, then permits five more
seconds for final reap and pipe cleanup. A cleanup guard remains armed on error
paths. Stdout and stderr are continuously drained and independently retain at
most 1 MiB. Truncated streams preserve equal head and tail halves separated by
`... N bytes omitted from bounded process output ...`, while the discarded
middle is still drained so a writer cannot block supervision. Reaping a
successful leader is insufficient while its managed group still exists;
in-group descendants remain subject to the same command deadline and cleanup.

Temporary handlers cover `SIGINT`, `SIGTERM`, `SIGHUP`, and `SIGQUIT`. The first
parent signal is forwarded to the group and starts the same grace period; a
second termination signal forces immediate `KILL`. Checksy finishes group and
leader cleanup and flushes retained diagnostics while its temporary handlers
remain active. It then restores the exact saved signal dispositions and
emulates the first signal's platform-default action so its parent observes the
conventional signal status.

Completed nonzero checks and repairs are compliance results and continue
through severity, fix, final-check, and `--no-fail` policy. A completed nonzero
skip predicate is a false condition and proceeds to the check, regardless of
its specific value. Spawn, timeout, child-signal, and supervision failures are
operational: no repair, recheck, or later configured command runs, retained
output is reported, and the CLI returns `2` regardless of severity or
`--no-fail`. Pattern-only configurations execute their scripts; only a
definition with no executable rules or matching patterns is empty. Legacy Git
subprocesses are unchanged and remain outside this runner.

### Conditional rule lifecycle

Each executable precondition or rule may carry one `skip-if`. The engine runs
it once immediately before the initial check, after severity filtering, with
`/dev/null`, the inherited environment, a fresh application of the rule
timeout, and the same effective working directory as that check. Execution
remains a linear list; there are no rule IDs, dependency graph, or dependency
cycle semantics.

Exit `0` produces `RuleOutcome::Skipped`, suppressing the check, either repair,
and final check. Every completed nonzero exit proceeds to the check. Output from
an ordinarily completed predicate is discarded as control output; operational
failures retain bounded output for diagnostics. Spawn, timeout, child-signal,
and supervision failures remain operational and abort before the check. At the
current HEAD the predicate and check both use the selected root configuration
directory; the later origin-correctness milestone will move them together to
their defining configuration directory.

`RuleResult` retains `Passed`, `Skipped`, or `Failed` explicitly. `success()` is
true only for passed results, while failure and threshold calculations consider
only `Failed`; `skipped()` and `Report::skipped_count()` expose the third state.
The CLI prints `⏭️ <name> (skipped)`. When skips exist, the success summary is
`😎 All applicable rules validated; N skipped`, while the failure summary gains
`; N skipped`; zero-skip summaries remain byte-for-byte unchanged. The
[skip-if contract corpus](fixtures/skip-if/README.md) maps these semantics to
compiled-binary file and stdin scenarios.

### Interactive command lifecycle

Terminal access is resolved lazily only after a rule actually fails and needs
an `interactive-fix`. For an eligible file-backed run, Checksy opens and
validates `/dev/tty`, requires itself to own the foreground terminal, allocates
an inner PTY, copies terminal state and window size, and launches the repair as
a new session/process-group leader with the PTY slave as its controlling
terminal. No Checksy confirmation is synthesized; the repair command owns its
prompts.

The supervisor relays terminal input and merged output nonblockingly. Output is
shown live and is not replayed by normal result reporting. Window changes are
forwarded to the inner PTY. The existing monotonic deadline, TERM/KILL
escalation, descendant cleanup, and parent-signal re-raise policy also applies
to interactive repairs. Exact outer terminal attributes are restored after
success, completed nonzero exit, setup or relay failure, timeout, and
interruption. Unsupported job-control suspension is cleaned up and reported as
an operational failure rather than allowing supervision to hang. During the
relay, Checksy also guards against outer `SIGTSTP`/`SIGTTIN`/`SIGTTOU`,
revalidates foreground ownership, and fails operationally if terminal ownership
is lost; the child receives default job-control dispositions.

A successful repair gets one non-interactive final check. A completed nonzero
repair preserves the original rule failure, skips the final check, and permits
later rules to run. Setup, timeout, signal, or relay failures stop later
execution as operational exit `2`. Stdin configuration is rejected from this
terminal path before `/dev/tty` is opened; `--stdin-config` takes diagnostic
precedence over an explicit `--non-interactive`.

### Provisioning-lock identity

The lock namespace is `checksy-provision`, keyed only by effective UID. It is
not keyed by configuration, source, working directory, include graph, or
legacy `cachePath`.

| Platform and effective user | Lock file |
| --- | --- |
| Linux non-root | `<account-home>/.local/state/checksy/provision.lock` |
| macOS non-root | `<account-home>/Library/Application Support/checksy/provision.lock` |
| Linux root | `/var/lib/checksy/provision.lock` |
| macOS root | `/Library/Application Support/checksy/provision.lock` |

Non-root account homes come from the operating-system account database rather
than environment variables. All file-backed, auto-discovered, and stdin
`check --fix` runs for the same effective UID contend. Root and non-root users
intentionally have separate namespaces; this is not a cross-UID machine-global
lock.

Lock acquisition is nonblocking. Arguments and locally available configuration
are validated first; the lock is then acquired before missing legacy Git cache
materialization, progress output, or any configured command. The descriptor is
held through initial checks, fixes, final checks, and reporting. Check-only
runs and `install` do not lock.

The final Checksy directory has exact mode `0700`. The persistent
`provision.lock` has exact mode `0600`, is owned by the effective UID, is opened
without following links, and must remain a regular single-link file whose
pathname still identifies the opened inode. A process-local device/inode
registry makes a second acquisition in one process contend consistently with a
separate process. The descriptor is close-on-exec, so configured descendants do
not extend lock ownership. Checksy never reads, writes, truncates, or unlinks
the lock file; descriptor close or process death releases the kernel lock and
stale contents are irrelevant.

Path, account lookup, ownership, permission, type, and other integrity failures
are operational exit `2`. Only a held advisory lock is exit `4`, reported as
`provisioning lock held: another checksy check --fix is already running for this user`.
Neither result can be hidden by `--no-fail`. Unsupported native platforms fail
closed for `check --fix` before a configured command starts.

### Stable exit classes

| Exit | Meaning |
| ---: | --- |
| `0` | Success, including a compliance failure explicitly masked by `--no-fail` |
| `1` | Existing no-command usage fallback only |
| `2` | Invalid arguments/configuration or operational failure |
| `3` | Unmasked rule-compliance failure |
| `4` | Provisioning lock contention |

`--no-fail` affects only exit `3`. It does not convert argument, configuration,
process, state, platform, or contention failures to success. Process spawn,
timeout, child-signal, and supervision failures are implemented operational
exit `2`; parent interruption is cleaned up and re-raised instead.

## Architectural priorities

- **Determinism**: Load and validate the complete configuration before any
  configured command executes.
- **Explicit authority**: Run trusted commands only as the invoking user; never
  escalate automatically.
- **Shell composability**: Let external tools acquire and authenticate input,
  then use local files or stdin as the boundary.
- **Local composition**: Preserve file includes without turning them into a
  source-management subsystem.

## Configuration boundary

All configuration entry paths share one strict typed decoder: explicit and
auto-discovered files, nested local files, cached legacy Git files, stdin,
`check --fix`, and the local include graph inspected by `install`. The decoder
rejects unknown and duplicate fields, unsupported nulls and scalar coercions,
invalid rule unions, blank checks/includes, invalid severities and globs, and
NUL in any configuration string before command execution. Executable-rule
timeouts use the documented syntax and `1ms` through `2h` runtime bound;
include rules cannot declare them. `skip-if` and `interactive-fix` are nonblank
executable commands, require `check`, and cannot appear on an include;
`interactive-fix` also cannot coexist with `fix`.

`Config` and `Rule` remain the public compatibility types. Private raw types
express omission without accepting explicit null and project into those public
types only after validation. Rules form a closed union between a remote-only
include and an executable check. Stdin configurations are required to be
self-contained and cannot include another definition.

`checksy schema` uses Schemars to generate a deterministic Draft 7 schema from
that same strict model. Structural fixture cases must agree between generated
schema validation and typed deserialization. YAML duplicate keys and multiple
documents remain parser-layer errors because they do not yield a unique JSON
instance; complete `glob::Pattern` syntax remains a runtime layer beyond
standard JSON Schema. The
[strict configuration corpus](fixtures/strict-config/README.md) freezes those
narrow boundaries, including structural and runtime-only timeout validation,
and exercises the compiled binary without a public network.

## Component Responsibilities

### main.rs (Entry Point)
- **Single responsibility**: Thin wrapper around `cli::run()`
- Handles stdio stream setup and exit code propagation
- Enables testing by accepting `&mut dyn Write` for stdout/stderr

### cli.rs (Command Layer)
- **Primary orchestrator**: Parses CLI args, dispatches to commands
- **Commands implemented**:
  - `check`: Load config, run checks, print results, return exit code
  - `install`: Cache git remotes with spinner UI
  - `init`: Create starter config file
  - `schema`: Generate the Draft 7 configuration schema
  - `version`: Output version string
- **Fix mode**: Implements ordinary and interactive repair/retry logic for
  failed checks; parses command-local `--non-interactive`
- **Global flags**: `--config`, `--stdin-config` parsing

### config.rs (Configuration Layer)
- **Path resolution**: Auto-detect `.checksy.yaml` or explicit path
- **Strict YAML parsing**: One decoder for root, nested, stdin, fix, and install paths
- **Stream validation**: Reject duplicate keys and multiple YAML documents
- **Remote expansion**: Recursive config inclusion (file & git)
- **Circular detection**: HashSet<PathBuf> tracks visited configs
- **Default application**: Applies inherited severity defaults
- **Git URL parsing**: `parse_git_remote()` handles `git+<url>#<ref>:<path>` format

### cache.rs (Cache Management)
- **Directory structure**: Manages `<cache-path>/git/<encoded-repo>/<ref>/`
- **URL encoding**: Sanitizes repo URLs for filesystem (`:/?` → `_`)
- **Cache queries**: `is_cached()`, `get_config_path()`
- **Pruning**: Removes unused cache entries based on used set

### git.rs (Git Operations)
- **Shallow clones**: `git clone --depth 1 --branch <ref>`
- **External dependency**: Requires `git` CLI in PATH
- **Error handling**: Captures stderr from failed clones
- **Network required**: All operations need network access

### check.rs (Execution Engine)
- **Rule orchestration**: Routes skip predicates, checks,
  ordinary/interactive fixes, final rechecks, and pattern scripts through the
  private supervisor
- **Result collection**: `RuleResult` retains an explicit passed, skipped, or
  failed outcome plus bounded output
- **Filtering**: `filter_rules()`, `filter_preconditions()` by severity
- **Pattern expansion**: Glob matching for script files (`tests/*.sh`)
- **Reporting**: `Report` aggregates results and calculates failures and skipped
  counts independently

### process_runner.rs (Process Supervisor)
- **Non-interactive isolation**: `/dev/null` stdin plus a new session/process
  group for skip predicates, checks, ordinary fixes, rechecks, and patterns
- **Interactive terminal relay**: Foreground `/dev/tty` validation, inner PTY,
  live bidirectional relay, resize propagation, and exact terminal restoration
- **Deadlines**: Monotonic timeout, five-second TERM grace, bounded KILL/reap
- **Capture**: Continuously drained 1 MiB head/tail limit per output stream
- **Signals**: Temporary parent-signal forwarding, escalation, and cleanup
- **Errors**: Typed completion, timeout, signal, spawn, and supervision outcomes

### schema.rs (Data Definitions)
- **Domain types**: Public `Config`, `Rule`, and `Severity` compatibility types
- **Strict projection**: Closed private raw types reject nulls, coercion, and malformed rule unions
- **Custom serialization**: `Severity` maps to strings ("warn", "error", etc.)
- **Validation**: String, `skip-if`, exact repair union, rule-form,
  timeout-bound, and full glob validation before execution
- **Schema generation**: Deterministic Draft 7 output through Schemars
- **CamelCase mapping**: Config fields use camelCase in YAML

### version.rs
- **Constant**: Single `VERSION` string constant

## Data Flow

### 1. Check Command Flow
```
main.rs
  → cli::run()
    → run_check() [cli.rs]
      → resolve_path() [config.rs]      # Find config file
      → load() [config.rs]              # Strictly decode & expand
        → load_with_context()           # Recursive loading
          → expand_remotes()            # Replace remote rules
            → resolve_remote_path()     # File or git cache path
      → Options { config, workdir, min_severity, fail_severity }
      → diagnose() / check_with_fixes() # Execute checks and requested repairs
        → run_preconditions()           # Filter & run
        → run rules                     # Filter & supervise
          → skip-if                     # Skip or continue before initial check
        → expand_rule_files()           # Glob patterns, including pattern-only configs
        → process_runner                 # Headless pipes or interactive PTY
      → print_report_results() [cli.rs] # Print ✓/⚠/✗
      → summarize_report()              # Exit code
```

### 2. Install Command Flow
```
run_install() [cli.rs]
  → load_without_remote_expansion()    # Strict decode, don't expand
  → collect_git_remotes_recursive()    # Walk config tree
    → parse_git_remote()               # Identify git URLs
  → GitCache::shallow_clone() [git.rs] # Clone each unique (repo, ref)
    → Command::new("git")...           # Execute git CLI
  → (optional) CacheManager::prune()    # Remove unused
```

### Configuration test coverage

- Unit tests validate the strict projection, exact rule union, runtime glob
  layer, generated schema shape, deterministic schema output, and stdout
  write/flush failures.
- The [compiled strict-configuration tests](src/tests/strict_configuration.rs)
  invoke `checksy` for file, auto-discovery, both stdin spellings, nested local
  files, cached legacy Git files, `check --fix`, `install`, `init`, and
  `schema`.
- The indexed fixture corpus is closed: every YAML document under its `valid/`
  and `invalid/` trees is represented exactly once, with explicit structural,
  YAML-parser, or runtime-only ownership.
- All strict-loading integration paths are network-free. Temporary legacy Git
  cache layouts and a sentinel `git` executable cover compatibility behavior.

### Process-supervision test coverage

- Unit tests exercise exact and bounded output, ordinary exit status, spawn and
  child-signal classification, timeout escalation, `/dev/null`, simultaneous
  stream draining, and default versus per-rule deadlines.
- The compiled [process-runner contract tests](src/tests/process_runner_contract.rs)
  map to the closed [process-runner fixture
  corpus](fixtures/process-runner/README.md). They cover checks, fixes, final
  rechecks, pattern-only definitions, output retained on timeout, fail-fast
  operational errors, a real controlling PTY, and parent interruption.
- The isolated process-tree harness uses readiness handshakes, descendant-held
  advisory locks, and bounded watchdogs to prove TERM-resistant leaders,
  children, and grandchildren terminate and the leader is reaped without a
  public network.

### Conditional-rule test coverage

- The compiled skip-predicate tests map to the closed, network-free
  [skip-if fixture corpus](fixtures/skip-if/README.md).
- Cases cover command-availability and environment gates, file and stdin
  entrypoints, exit `0` versus arbitrary nonzero values, exact reporting and
  summaries, shared working directory, a fresh timeout application, and
  suppression of checks and both repair forms.
- Operational spawn, timeout, and signal cases retain available output, stop
  before the check and later commands, return exit `2`, and remain unmasked by
  `--no-fail`.

### Interactive-repair test coverage

- The compiled interactive-repair tests map to the closed, network-free
  [interactive-fix fixture corpus](fixtures/interactive-fix/README.md).
- Real controlling-PTY cases cover stdin and `/dev/tty` prompting, live output,
  successful final recheck, completed nonzero repair, exact outer-terminal
  restoration, timeout escalation, job-control suspension, descendant cleanup,
  and parent interruption.
- Headless cases cover passing rules without terminal probing, file-backed
  `--non-interactive`, both stdin spellings, stdin diagnostic precedence,
  ordinary fixes, continued reporting, severity policy, and `--no-fail`.

### Provisioning-lock test coverage

- Focused primitive tests cover platform path selection, account-database home
  lookup, exact modes and ownership, same-process and cross-process contention,
  release, stale contents, close-on-exec, and link/type/path substitution.
- The compiled provisioning-lock tests map to the closed, network-free
  [provisioning-lock fixture corpus](fixtures/provisioning-lock/README.md).
  File-backed, auto-discovered, aliased, and stdin provisioning contend in one
  per-EUID namespace, while check-only invocations remain usable.
- FIFO readiness and bounded subprocess watchdogs prove contention and
  process-death release without sleeps. Marker cases prove that contention and
  integrity failures occur before configured commands and that the descriptor
  remains held through reporting.

### Integrated P0 acceptance coverage

- The compiled [P0 acceptance tests](src/tests/p0_acceptance.rs) map to the
  closed, network-free [P0 acceptance fixture
  corpus](fixtures/p0-acceptance/README.md).
- The suite composes file and stdin repair/recheck, skip predicates, passing and
  needed interactive repairs, headless and PTY execution, cross-ingestion lock
  contention, predicate timeout with descendant cleanup, and strict preflight
  before commands or lock acquisition.
- Focused feature corpora remain authoritative for edge conditions. This suite
  proves the complete P0 contract through the public CLI rather than replacing
  those narrower tests.

### Devcontainer provisioning test coverage

- The network-free [helper tests](.devcontainer/scripts/tests/run.sh) cover
  version loading, supported architecture mapping, exact Just release
  selection, checksum rejection, Rustup/toolchain/component selection, and the
  Node.js requirement for Dev Container CLI.
- Quality CI runs the provisioning definition with ordinary fixes, immediately
  runs it check-only, and only then proceeds to formatting, Clippy, and
  installer syntax checks.
- Fresh-container validation asserts Checksy `0.7.6`, Entr availability, Just
  `1.57.0`, Rust `1.94.1` with `rustfmt` and `clippy`, Dev Container CLI
  `0.88.0`, and Node.js 20 or newer. A second convergence and check-only pass
  prove the fixes are idempotent.

## External Dependencies & Integrations

### Required External Tools
- **git**: Required for `install` command (shallow clones)
- **bash**: Required for rule execution (all checks run via `bash -c`)

### Rust Dependencies (Cargo.toml)
- **serde**: Serialization framework
- **serde_yaml**: YAML config parsing
- **serde_json**: Deterministic JSON output
- **schemars**: Draft 7 schema generation from the strict Rust model
- **glob**: Pattern matching for rule files
- **rustix**: Session/process-group, nonblocking pipe, polling, signal, PTY,
  terminal-state, descriptor-relative filesystem, and advisory-lock primitives
- **signal-hook**: Portable signal constants and default-action emulation
- **libc**: Exact save/install/restore of native signal dispositions
- **jsonschema**: Draft 7 metaschema and fixture parity tests (dev dependency)
- **tempfile**: Test utilities (dev dependency)

### File System Interactions
- **Config discovery**: Looks for `.checksy.yaml`, `.checksy.yml` in CWD
- **Cache directory**: Creates `<cache-path>/git/` structure
- **Work directory**: Commands currently run in the selected root config's directory; nested defining origins are a pending correction
- **Glob expansion**: Expands patterns relative to config directory

## Entry Points

### CLI Entry (Primary)
- **Binary**: `checksy` (main.rs → cli::run)
- **Global flags**: `--config PATH`, `--stdin-config`
- **Check/diagnose flags**: `--fix`, `--non-interactive`, severity controls, and
  `--no-fail`
- **Commands**: check, install, init, schema, version, diagnose (deprecated)

### Library Entry (Rust API)
- **Crate**: `checksy` (lib.rs)
- **Public exports**:
  - `run()` - CLI entry
  - `load()` - Config loading
  - `diagnose()` - Check execution
  - `CacheManager`, `GitCache` - Git caching

## Module Dependencies

```
cli.rs
  ├── config.rs (load, resolve_path, parse_git_remote)
  ├── check.rs (diagnose, Options, Report)
  ├── cache.rs (CacheManager)
  ├── git.rs (GitCache)
  ├── schema.rs (Config, Rule, Severity)
  └── version.rs (VERSION)

process_runner.rs
  ├── libc (exact signal dispositions and terminal ioctls)
  ├── rustix (process groups, pipes, poll, PTY, terminal state)
  └── signal-hook (signal constants and default-action emulation)

config.rs
  ├── cache.rs (CacheManager, GitRemote)
  ├── schema.rs (Config, Severity)
  └── check.rs (uncertain: may have circularity, check carefully)

check.rs
  ├── schema.rs (Config, Rule, Severity, repairs, timeout)
  └── process_runner.rs (headless and PTY-backed command execution)

cache.rs
  └── (self-contained, only std)

git.rs
  └── cache.rs (CacheManager)

schema.rs
  ├── serde / serde_yaml
  ├── schemars
  └── glob

lib.rs
  └── (exports from all modules)

main.rs
  └── cli.rs (run)
```

## Design Decisions

### Why Recursive Config Expansion at Load Time?
- Ensures complete config known before execution
- Allows severity filtering without re-parsing
- Simplifies circular reference detection

### Why Is the Install Command Still Present?
- It preserves compatibility with existing Git-based configurations.
- `check --fix` can currently clone a missing Git remote, so checks are not yet
  guaranteed network-free.
- The P1 Git-deprecation slice will define the warning window and eventual
  removal while new workflows use external acquisition.

### Why Shallow Clones (--depth 1)?
- Minimizes disk usage for cached repos
- Faster clone operations
- Sufficient for single-ref config reads

### Why Bash for Rule Execution?
- Universal availability
- Consistent shell syntax across platforms
- No need to parse shebang lines
