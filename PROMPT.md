# checksy - AI Agent Context

## What This System Does
checksy is a Rust CLI tool for running health checks in development environments. It reads YAML configuration files containing shell command-based rules, executes them, and reports pass/fail status with severity levels (debug, info, warn, error).

**Key Features:**
- YAML-based rule definitions with shell commands
- Hierarchical severity levels (debug → info → warn → error)
- Remote config inclusion via file paths or git repositories
- Git-based remote caching with shallow clones
- Fix mode: auto-attempt repairs for failed checks
- Pattern-based rule files (glob matching)

## Core Domain Model

### Config
```rust
struct Config {
    cache_path: Option<String>,      // Default: ".checksy-cache"
    check_severity: Option<Severity>, // Min severity to run
    fail_severity: Option<Severity>,  // Min severity to fail
    preconditions: Vec<Rule>,         // Run before main rules
    rules: Vec<Rule>,                 // Main checks
    patterns: Vec<String>,              // Glob patterns for script files
}
```

### Rule
```rust
struct Rule {
    name: Option<String>,
    check: Option<String>,      // Shell command to execute
    severity: Option<Severity>, // Default: Error
    fix: Option<String>,        // Auto-fix command
    hint: Option<String>,       // Failure message hint
    remote: Option<String>,     // Config file to include
}
```

**Remote Rule Types:**
- **File remote**: `remote: path/to/config.yaml` - Relative path
- **Git remote**: `git+<url>#<ref>:<path>` - e.g., `git+https://github.com/org/repo.git#main:.checksy.yaml`

**Important:** Remote rules can ONLY have the `remote` property set (no name, check, severity, fix, hint allowed).

### Severity (Enum)
- `Debug` (0) - Lowest, for verbose output
- `Info` (1) - Informational checks
- `Warning` (2) - Non-failing issues
- `Error` (3) - Failing issues (default)

## High-Level Architecture

### Execution Flow
1. **CLI Parsing** (`cli.rs`)
   - Parse global flags (`--config`, `--stdin-config`)
   - Dispatch to command handler (check, install, init, schema, version)

2. **Config Loading** (`config.rs`)
   - Resolve config path (explicit or auto-detect `.checksy.yaml`)
   - Parse YAML, apply defaults
   - **Expand remotes recursively** (preventing circular refs via visited HashSet)
   - For git remotes: verify cache exists or error with "Run 'checksy install'"

3. **Git Caching** (`install` command → `cli.rs`)
   - Parse config without remote expansion to collect all git remotes
   - For each unique `(repo, ref)`: shallow clone to cache directory
   - Support `--prune` to remove unused cache entries

4. **Check Execution** (`check.rs`)
   - Run preconditions first (in order)
   - Run rules (in order)
   - Run pattern-matched script files (alphabetically)
   - Collect results into `Report`
   - Exit code: 0 (success) or 3 (failures) or 2 (error)

### Cache Directory Structure
```
<config-dir>/
└── <cache-path>/              # e.g., ".checksy-cache"
    └── git/                  # Fixed subdirectory
        └── <encoded-repo>/   # URL-safe repo name
            └── <ref>/         # Branch/tag name
                └── <files>   # Shallow clone contents
```

## Directory Map
```
src/
├── lib.rs          # Module exports
├── main.rs         # CLI entry point (thin wrapper)
├── cli.rs          # Command parsing & dispatch (main logic)
├── config.rs       # Config loading & remote expansion
├── cache.rs        # Cache directory management
├── git.rs          # Git shallow clone operations
├── check.rs        # Rule execution & reporting
├── schema.rs       # Data structures (Config, Rule, Severity)
└── version.rs      # VERSION constant

fixtures/           # Test configurations
├── happy-path/
├── remote-config/  # Remote inclusion examples
│   ├── git/        # Git-based remote fixture
│   ├── circular/   # Circular ref test
│   └── nested/     # Nested remote test
└── ...
```

## Critical Invariants / Rules

### 1. Remote Rule Validation
Remote rules MUST only have `remote` property. Any other property (name, check, severity, fix, hint) causes error.

### 2. Circular Reference Prevention
Remote configs tracked via `visited: HashSet<PathBuf>` during expansion. Re-visiting same canonical path returns empty config (graceful skip).

### 3. Git Remote Caching Requirement
Git remotes must be cached via `checksy install` BEFORE running `checksy check`. Uncached git remotes error with clear message.

### 4. Severity Inheritance
- Rules without severity inherit from `check_severity` config field
- Default severity: `Error`
- Remote configs inherit parent's `check_severity`/`fail_severity` defaults

### 5. Work Directory Execution
All shell commands execute relative to the config file's directory (not CWD).

### 6. All-or-Nothing Install
`checksy install` fails entirely if ANY git remote fails to clone.

## Common Tasks

### Add a New Command
1. Add case in `run()` match statement (`cli.rs`)
2. Implement `run_<command>()` function
3. Update `print_usage()` with command description
4. Update `run_schema()` JSON schema if config-related

### Add a Config Field
1. Add field to `Config` struct in `schema.rs` with serde attributes
2. Update `run_schema()` JSON schema output
3. Use field in appropriate module (config loading, check execution, etc.)
4. Update test fixtures if behavior changes

### Add Git Remote Support to New Feature
1. Parse git URL via `parse_git_remote()` in `config.rs`
2. Check cache via `CacheManager::is_cached()`
3. Get cached path via `CacheManager::get_config_path()`
4. Error if not cached: "Run 'checksy install' first"

### Add New Rule Property
1. Add to `Rule` struct in `schema.rs`
2. Update validation in `Rule::validate_remote_only()` if remote-restricted
3. Use in `check.rs` (execution) or `cli.rs` (reporting)
4. Update JSON schema in `run_schema()`

### Test Changes
- Unit tests in each module's `#[cfg(test)]` section
- Integration tests via fixtures in `fixtures/` directory
- Run `cargo test` from `src/` directory
