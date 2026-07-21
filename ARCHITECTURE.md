# checksy Architecture

## System Overview
checksy is a layered CLI application with clear separation between command handling, configuration management, and execution. The architecture prioritizes:
- **Determinism**: Configs loaded and expanded before execution begins
- **Working directory context**: Commands currently run in the top-level config's directory; origin-aware execution for nested definitions is planned
- **Composability**: Remote configs allow modular, reusable check libraries

## Security Boundary

Checksy definitions contain arbitrary shell commands and are not process
sandboxes. The working-directory behavior described above provides path context,
not containment. Current `check` and `install` behavior does not yet implement the
authentication, atomic state, timeout, and privilege controls required for safe
unattended remote execution. [THREAT_MODEL.md](THREAT_MODEL.md) is the normative
target contract for security invariants and current gaps;
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
- **Global flags**: `--config`, `--stdin-config` parsing

### config.rs (Configuration Layer)
- **Path resolution**: Auto-detect `.checksy.yaml` or explicit path
- **YAML parsing**: Deserialize with serde_yaml
- **Remote expansion**: Recursive config inclusion (file & git)
- **Circular detection**: HashSet<PathBuf> tracks visited configs
- **Default application**: Applies inherited severity defaults
- **Diagnostics**: Successful CLI loads report deprecated non-lowercase severity
  spellings without changing the public library-loading API
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
      → load() [config.rs]              # Parse & expand
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
  → load_without_remote_expansion()    # Parse only, don't expand
  → collect_git_remotes_recursive()    # Walk config tree
    → parse_git_remote()               # Identify git URLs
  → GitCache::shallow_clone() [git.rs] # Clone each unique (repo, ref)
    → Command::new("git")...           # Execute git CLI
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
- **Cache directory**: Creates `<cache-path>/git/` structure
- **Work directory**: All shell commands run in config file's directory
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
  └── serde, serde_yaml, schemars, glob

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
