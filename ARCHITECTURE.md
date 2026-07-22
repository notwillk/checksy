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

## Security and mutation boundary

Configured checks and fixes are trusted arbitrary Bash executed with the
invoking user's authority. They are not sandboxed. Checksy does not invoke
`sudo`, cannot transactionally roll back shell mutations, and may leave partial
machine changes when a fix fails. Correct and idempotent fixes remain the
configuration author's responsibility. Process supervision adds no CPU,
memory, filesystem, network, or disk quota. A command that deliberately creates
a different session is outside the managed-descendant guarantee.

## Normative P0 execution contract

This section records both implemented behavior and the remaining target. At the
current HEAD, hardened non-interactive supervision, `interactive-fix`, and
`--non-interactive` exist; `skip-if` and the provisioning lock do not.

### Interaction modes

- Checks, pattern scripts, ordinary fixes, and final checks are non-interactive,
  receive `/dev/null` as stdin, and run in a detached session without a
  controlling terminal. Future `skip-if` predicates will use the same runner.
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
a fresh timeout for its initial check, ordinary or interactive fix, and final
recheck. Pattern scripts use `15m`. The complete configuration is validated
before execution, including rejection of malformed, null, overflowing,
over-limit, or include-rule timeouts.

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

Completed nonzero exits are compliance results and continue through severity,
fix, final-check, and `--no-fail` policy. Spawn, timeout, child-signal, and
supervision failures are operational: no fix, recheck, or later configured
command runs, retained output is reported, and the CLI returns `2` regardless
of severity or `--no-fail`. Pattern-only configurations execute their scripts;
only a definition with no executable rules or matching patterns is empty.
Legacy Git subprocesses are unchanged and remain outside this runner.

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

Lock acquisition is nonblocking and occurs after invocation and configuration
validation but before any configured command starts. The descriptor is held
through initial checks, fixes, final checks, and reporting. Check-only runs do
not lock. The future implementation will use a private owner-only directory and
file, reject link/type/ownership/mode substitution, never interpret lock-file
contents, and rely on descriptor close or process death for release.

### Stable exit classes

| Exit | Meaning |
| ---: | --- |
| `0` | Success, including a compliance failure explicitly masked by `--no-fail` |
| `1` | Existing no-command usage fallback only |
| `2` | Invalid arguments/configuration or operational failure |
| `3` | Unmasked rule-compliance failure |
| `4` | Provisioning lock contention; reserved until the lock is implemented |

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
include rules cannot declare them. `interactive-fix` is a nonblank executable
command, requires `check`, cannot coexist with `fix`, and cannot appear on an
include.

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
- **Rule orchestration**: Routes checks, ordinary/interactive fixes, final
  rechecks, and pattern scripts through the private supervisor
- **Result collection**: `RuleResult` contains stdout, stderr, exit status
- **Filtering**: `filter_rules()`, `filter_preconditions()` by severity
- **Pattern expansion**: Glob matching for script files (`tests/*.sh`)
- **Reporting**: `Report` aggregates results, calculates failures

### process_runner.rs (Process Supervisor)
- **Non-interactive isolation**: `/dev/null` stdin plus a new session/process
  group
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
- **Validation**: String, exact repair union, rule-form, timeout-bound, and full
  glob validation before execution
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
- **rustix**: Session/process-group, nonblocking pipe, polling, signal, PTY, and
  terminal-state primitives
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
