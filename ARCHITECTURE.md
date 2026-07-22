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
configuration author's responsibility.

## Normative P0 execution contract

This section defines target behavior before implementation. At the current
HEAD, `interactive-fix`, `--non-interactive`, hardened process supervision, and
the provisioning lock do not yet exist.

### Interaction modes

- Checks, pattern scripts, ordinary fixes, final checks, and future `skip-if`
  predicates are non-interactive and receive `/dev/null` as stdin.
- `interactive-fix` is the only terminal-capable command. It is considered only
  after its check fails; a passing rule never requires a terminal.
- `--non-interactive` prohibits terminal use without disabling ordinary fixes.
- `--stdin-config` implies non-interactive operation and never opens
  `/dev/tty`, allocates a PTY, or otherwise attaches a terminal.
- If a failed rule needs an interactive fix but terminal use is unavailable or
  prohibited, the fix is not run. The rule remains failed at its configured
  severity and reporting continues normally.

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
timeout, and signal failures will become operational exit `2` when the hardened
runner is implemented; current runner behavior is not yet fully classified.

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
NUL in any configuration string before command execution.

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
narrow boundaries and exercises the compiled binary without a public network.

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
- **Fix mode**: Implements fix/retry logic for failed checks
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
- **Rule runner**: Executes shell commands via `std::process::Command`
- **Result collection**: `RuleResult` contains stdout, stderr, exit status
- **Filtering**: `filter_rules()`, `filter_preconditions()` by severity
- **Pattern expansion**: Glob matching for script files (`tests/*.sh`)
- **Reporting**: `Report` aggregates results, calculates failures

### schema.rs (Data Definitions)
- **Domain types**: Public `Config`, `Rule`, and `Severity` compatibility types
- **Strict projection**: Closed private raw types reject nulls, coercion, and malformed rule unions
- **Custom serialization**: `Severity` maps to strings ("warn", "error", etc.)
- **Validation**: String, rule-form, and full glob validation before execution
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
      → diagnose() [check.rs]          # Execute checks
        → run_preconditions()           # Filter & run
        → run rules                     # Filter & run
        → expand_rule_files()           # Glob patterns
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

config.rs
  ├── cache.rs (CacheManager, GitRemote)
  ├── schema.rs (Config, Severity)
  └── check.rs (uncertain: may have circularity, check carefully)

check.rs
  └── schema.rs (Config, Rule, Severity)

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
