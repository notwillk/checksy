# checksy Architecture

## System Overview
checksy is a layered CLI application with clear separation between command handling, configuration management, and execution. The architecture prioritizes:
- **Determinism**: Configs loaded and expanded before execution begins
- **Working directory context**: CLI checks, fixes, and pattern scripts run from the directory of the config that defined them
- **Composability**: Remote configs allow modular, reusable check libraries

## Security Boundary

Checksy definitions contain arbitrary shell commands and are not process
sandboxes. Origin-aware working directories provide path context, not process
containment. The current CLI rejects fetched Git config and pattern paths that
traverse or follow symlinks outside their canonical checkout, but the legacy
cache remains mutable and unauthenticated. Mutation paths reject symlinked
components found below the operator-selected cache root during preflight, but
the current path-based checks remain raceable until locking and descriptor-based
mutation are implemented. Shell commands retain ambient filesystem access.
Current `check` and `install` behavior does not yet
implement the authentication, atomic state, timeout, and privilege controls
required for safe unattended remote execution.
[THREAT_MODEL.md](THREAT_MODEL.md) is the normative target contract for security
invariants and current gaps;
[DESIGN_DECISIONS.md](DESIGN_DECISIONS.md) is the normative policy contract; and
[PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md) freezes the public formats, CLI,
state projection, and resource bounds.

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
  - `schema`: Generate and output the deterministic Draft 7 configuration schema
  - `version`: Output version string
- **Fix mode**: Implements fix/retry logic for failed checks
- **Resolved execution**: Uses the internal origin-aware definition path while
  preserving the public flat Rust API
- **Nested acquisition**: `install` and `check --fix` repeatedly discover Git
  dependencies after newly fetched parents become available
- **Global flags**: `--config`, `--stdin-config` parsing

### config.rs (Configuration Layer)
- **Path resolution**: Auto-detect `.checksy.yaml` or explicit path
- **YAML parsing**: Deserialize with serde_yaml
- **Remote expansion**: Produces origin-aware recursive file and Git inclusions
- **Source boundaries**: Canonicalizes local origins while preserving legacy
  external local references; fetched Git configs cannot escape their checkout
- **Circular detection**: Structured source identity, immutable Git revision,
  and canonical defining path distinguish active cycles from completed includes
- **Default application**: Applies inherited severity defaults
- **Diagnostics**: Successful CLI loads report deprecated non-lowercase severity
  spellings without changing the public library-loading API
- **Git URL parsing**: `parse_git_remote()` handles `git+<url>#<ref>:<path>` format
- **Cache ownership**: Only the selected root config's `cachePath` establishes
  the legacy cache root; nested definitions cannot redirect it

### resolved.rs (Resolved Definition Model)
- **Source identity**: Distinguishes canonical local roots, exact Git locator
  strings/checkouts, and stdin execution without creating persistent source IDs;
  complete provider normalization remains future source-provider work
- **Origin metadata**: Carries defining config path, base directory, optional
  fetched bundle root, source-relative path, and Git revision
- **Execution plan**: Keeps resolved preconditions, rules, and per-config pattern
  groups with their origins
- **Recursion key**: Combines structured source identity, cached revision, and
  canonical defining config path
- **Compatibility projection**: Projects back to the public flat `Config` for
  `load()`; this projection retains only the root pattern group because the
  public type cannot preserve or execute nested groups with their origins

### cache.rs (Cache Management)
- **Directory structure**: Manages `<cache-path>/git/<encoded-repo>/<ref>/`
- **Legacy encoding**: Repository/ref directory names are not persistent,
  collision-resistant source identities; complete normalization is deferred
- **URL encoding**: Sanitizes repo URLs for filesystem (`:/?` → `_`)
- **Cache queries**: `is_cached()`, `get_config_path()`
- **Pruning**: Removes unused cache entries based on used set

### git.rs (Git Operations)
- **Shallow clones**: `git clone --depth 1 --branch <ref>`
- **External dependency**: Requires `git` CLI in PATH
- **Error handling**: Captures stderr from failed clones
- **Transport**: Clone/ref resolution may use network or local Git transports;
  cached HEAD lookup is local

### check.rs (Execution Engine)
- **Rule runner**: Executes shell commands via `std::process::Command`
- **Result collection**: `RuleResult` contains stdout, stderr, exit status
- **Filtering**: `filter_rules()`, `filter_preconditions()` by severity
- **Pattern expansion**: Glob matching for script files (`tests/*.sh`)
- **Origin-aware runner**: Runs resolved checks/fixes in their defining config's
  directory and pattern scripts from their owning pattern group
- **Preflight**: Expands every resolved pattern group before commands run and
  rejects fetched matches outside the bundle root
- **Reporting**: `Report` aggregates results, calculates failures

### schema.rs (Data Definitions)
- **Domain types**: `Config`, `Rule`, `Severity` with strict serde decoding
- **Custom serialization**: `Severity` maps to strings ("warn", "error", etc.)
- **Validation**: Rejects unknown/duplicate fields, invalid scalar types,
  malformed rule forms, empty commands/remotes, NUL bytes in constrained
  fields, and invalid glob patterns
- **Schema generation**: Uses the strict deserialization projection to generate
  a closed Draft 7 schema with an exact remote/executable rule union
- **Layered parity**: Duplicate YAML keys remain parser-owned and complete Rust
  glob syntax remains runtime-owned; all other fixture structure is checked
  against both the generated schema and typed deserialization
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
      → load_resolved_with_*() [config.rs]
        → DefinitionResolver            # Recursive file/Git resolution
          → ResolvedDefinition          # Rules and pattern groups retain origins
      → ResolvedOptions
      → diagnose_resolved() [check.rs]
        → expand_resolved_rule_files()  # Preflight all origin-scoped patterns
        → run resolved preconditions    # Defining-config working directory
        → run resolved rules/fixes      # Defining-config working directory
        → run resolved pattern scripts  # Defining-config working directory
      → print_report_results() [cli.rs] # Print ✓/⚠/✗
      → summarize_report()              # Exit code
```

### 2. Install Command Flow
```
run_install() [cli.rs]
  → load_resolved_for_install(RefreshOrClone)
  → refresh discovered Git dependencies
  → repeat resolution                  # Newly cached parents reveal nested Git
  → GitCache::{get_local_sha,get_remote_sha,shallow_clone}()
  → (optional) CacheManager::prune()    # Remove unused
```

## External Dependencies & Integrations

### Required External Tools
- **git**: Required for `install` command (shallow clones)
- **bash**: Required for rule execution (all checks run via `bash -c`)

### Rust Dependencies (Cargo.toml)
- **serde**: Serialization framework
- **serde_yaml**: YAML config parsing
- **serde_json**: JSON schema serialization
- **schemars**: Draft 7 schema generation from configuration types
- **glob**: Pattern matching for rule files
- **jsonschema**: Draft 7 metaschema and fixture validation (dev dependency)
- **tempfile**: Test utilities (dev dependency)

### File System Interactions
- **Config discovery**: Looks for `.checksy.yaml`, `.checksy.yml` in CWD
- **Cache directory**: Creates `<root-config-cache-path>/git/`; nested
  definitions cannot choose another cache anchor
- **Work directory**: Resolved CLI commands use the defining config's directory
- **Glob expansion**: Expands each config's patterns relative to that config;
  fetched matches are canonicalized and confined to the checkout

### Origin Regression Coverage

The checked-in [origin regression fixture](fixtures/origin-regression/README.md)
drives the CLI through a root definition and nested definition with distinct
same-named assets. It locks down defining-config working directories,
root-before-nested execution order, origin-scoped pattern negation, and
exclusion of scripts that must never execute. Temporary-directory and local-Git
unit tests continue to cover cycles, deduplication, fix/recheck behavior, nested
acquisition, and fetched-checkout confinement.

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
  - `diagnose()` - Flat compatibility check execution using one caller-supplied
    work directory
  - `CacheManager`, `GitCache` - Git caching

The resolved types and `diagnose_resolved()` remain crate-private. CLI behavior
is origin-aware; external Rust callers using `load()` plus `diagnose(Options)`
retain the earlier flat contract, including omission of nested remote pattern
groups from the compatibility projection.

## Module Dependencies

```
cli.rs
  ├── config.rs (resolved loading, resolve_path, parse_git_remote)
  ├── resolved.rs (ResolvedLoad and Git dependency descriptors)
  ├── check.rs (ResolvedOptions, resolved/compatibility execution, Report)
  ├── cache.rs (CacheManager)
  ├── git.rs (GitCache)
  ├── schema.rs (Config, Rule, Severity)
  └── version.rs (VERSION)

config.rs
  ├── cache.rs (CacheManager, GitRemote)
  ├── git.rs (cached HEAD revision lookup)
  ├── resolved.rs (origins, identities, execution plan)
  ├── schema.rs (Config, Severity)

check.rs
  ├── resolved.rs (resolved rules and pattern groups)
  └── schema.rs (Config, Rule, Severity)

cache.rs
  └── (self-contained, only std)

git.rs
  └── cache.rs (CacheManager)

schema.rs
  └── serde, serde_yaml, schemars, glob

resolved.rs
  ├── cache.rs (GitRemote dependency descriptors)
  ├── config.rs (diagnostics carried by ResolvedLoad)
  └── schema.rs (Config, Rule, Severity)

lib.rs
  └── (exports from all modules)

main.rs
  └── cli.rs (run)
```

## Design Decisions

### Why Recursive Config Expansion at Load Time?
- Ensures complete config known before execution
- Allows severity filtering without re-parsing
- Preserves each definition's origin while detecting canonical active cycles

### Why One Root-Anchored Legacy Cache?
- Prevents a fetched or nested definition from selecting an acquisition path
- Gives `install`, `check`, and `check --fix` one consistent location for nested Git
- Preserves the selected root config's existing `cachePath` behavior

### Why Separate Install Command?
- Explicit network operations (user consent)
- Avoids implicit network calls during check
- Allows offline operation after install
- Enables pruning of unused cache

### Why Shallow Clones (--depth 1)?
- Minimizes disk usage for cached repos
- Faster clone operations
- Sufficient for single-ref config reads

### Why Bash for Rule Execution?
- Universal availability
- Consistent shell syntax across platforms
- No need to parse shebang lines
