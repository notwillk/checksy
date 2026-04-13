# checksy Code Map

## Directory Structure

```
/workspaces/checksy/
├── src/                    # Rust source code
│   ├── Cargo.toml          # Package manifest
│   ├── Cargo.lock          # Dependency lock (uncertain if present)
│   ├── lib.rs              # Library exports
│   ├── main.rs             # Binary entry point
│   ├── cli.rs              # ~950 lines: Command parsing & orchestration
│   ├── config.rs           # ~575 lines: Config loading & remote expansion
│   ├── cache.rs            # ~270 lines: Cache directory management
│   ├── git.rs              # ~120 lines: Git shallow clone operations
│   ├── check.rs            # ~450 lines: Rule execution & reporting
│   ├── schema.rs           # ~160 lines: Data structures & serialization
│   └── version.rs          # ~1 line: VERSION constant
│
├── fixtures/               # Test configurations (YAML)
│   ├── happy-path/         # Basic severity level tests
│   ├── inline-check/       # Simple inline rule tests
│   ├── preconditions/      # Precondition execution tests
│   ├── fix-behavior/       # Auto-fix functionality tests
│   ├── rule-files/         # Pattern/glob matching tests
│   ├── default-severity/   # Default severity behavior
│   ├── check-logs/         # Log output testing
│   ├── hint-test/          # Hint message testing
│   └── remote-config/      # Remote inclusion tests
│       ├── git/            # Git-based remote fixture
│       ├── circular/       # Circular reference handling
│       ├── nested/         # Nested remote expansion
│       └── invalid/        # Validation error tests
│
├── scripts/                # Installation scripts
│   ├── install.sh          # Install checksy binary
│   └── uninstall.sh        # Remove checksy binary
│
├── .github/workflows/      # CI/CD
│   └── release.yml         # Release automation
│
├── justfile                # Just command runner recipes
├── README.md               # User documentation
├── LICENSE                 # MIT license
├── rust-toolchain.toml     # Rust version specification
└── CODEMAP.md              # This file
```

## Source Files Deep Dive

### lib.rs (15 lines)
**Role**: Module declaration and public API exports
**Key Exports**:
- `pub mod cache, check, cli, config, git, schema, version`
- `pub use` re-exports for convenient access

### main.rs (25 lines)
**Role**: Thin binary wrapper
**Key Items**:
- `run()` function: Delegates to `cli::run()`
- `main()`: Stdio setup, exit code propagation
- Test: `test_run_help_command()`

### cli.rs (~950 lines)
**Role**: Primary orchestrator - command parsing and dispatch
**Key Functions**:
- `run()`: Entry point, help handling, command dispatch
- `run_check()`: Full check workflow
- `run_install()`: Git remote caching with spinner UI
- `run_init()`: Config file creation
- `run_schema()`: JSON schema output
- `check_with_fixes()`: Fix mode implementation

**Key Types**:
- `GlobalFlags`: `--config`, `--stdin-config`
- Internal parsing functions

**Sections**:
- Lines 1-60: Imports and main dispatch
- Lines 61-115: Global flag parsing
- Lines 200-335: `run_check()` implementation
- Lines 335-560: `run_install()` implementation
- Lines 560-750: Fix mode logic
- Lines 750-950: Helper functions, tests

### config.rs (~575 lines)
**Role**: Configuration loading and remote expansion
**Key Functions**:
- `resolve_path()`: Config file discovery
- `load()`: Main entry for config loading
- `load_with_context()`: Recursive loader with circular detection
- `expand_remotes()`: Remote rule expansion
- `parse_git_remote()`: Git URL parser
- `resolve_remote_path()`: Git cache or file path resolution

**Key Types**:
- `GitRemote { repo, ref_, path }`: Parsed git URL

**Sections**:
- Lines 1-40: Path resolution
- Lines 40-115: Contextual loading with circular detection
- Lines 115-190: Remote expansion
- Lines 190-245: Git URL parsing
- Lines 245-270: Path resolution for git remotes
- Lines 270-575: Tests

### cache.rs (~270 lines)
**Role**: Cache directory structure management
**Key Structs/Methods**:
- `CacheManager { root }`
  - `new()`: Create with config dir and cache path
  - `encode_repo_name()`: URL-safe encoding
  - `ref_cache_path()`: Path to ref directory
  - `is_cached()`: Check if clone exists
  - `get_config_path()`: Path to config within cache
  - `prune()`: Remove unused entries

**Sections**:
- Lines 1-20: Constants and struct
- Lines 22-80: Core methods
- Lines 80-150: Pruning and cleanup
- Lines 150-270: Tests

### git.rs (~120 lines)
**Role**: Git operations via CLI
**Key Structs/Methods**:
- `GitCache`: Stateless struct
  - `shallow_clone()`: `git clone --depth 1 --branch`
  - `ensure_cached()`: Idempotent cache check

**External Dependency**: Requires `git` CLI in PATH

**Sections**:
- Lines 1-80: Clone implementation
- Lines 80-120: Tests (network-dependent, ignored)

### check.rs (~450 lines)
**Role**: Rule execution and result collection
**Key Types**:
- `Options { config, workdir, min_severity, fail_severity }`: Execution context
- `Report { rules, fail_severity }`: Aggregated results
- `RuleResult { rule, err, stdout, stderr }`: Single check result

**Key Functions**:
- `diagnose()`: Main execution entry
- `run_rule()`: Execute single rule via bash
- `run_rule_file()`: Execute script file
- `expand_rule_files()`: Glob pattern expansion
- `filter_rules()`, `filter_preconditions()`: Severity filtering
- `min_severity()`: Severity comparison utility

**Sections**:
- Lines 1-90: Types and implementations
- Lines 90-200: `diagnose()` and execution flow
- Lines 200-300: Rule file pattern expansion
- Lines 300-450: Tests

### schema.rs (~160 lines)
**Role**: Data definitions with serde
**Key Types**:
- `Severity`: Enum with custom serialization (Error/Warning/Info/Debug)
- `Rule`: Struct with optional fields, validation
- `Config`: Top-level config struct

**Key Methods**:
- `Severity::parse()`: String parsing
- `Rule::is_remote()`: Check if remote rule
- `Rule::validate_remote_only()`: Enforce remote-only constraint

**Sections**:
- Lines 1-70: Severity enum
- Lines 70-130: Rule struct and validation
- Lines 130-160: Config struct

### version.rs (1 line)
**Role**: Single constant
**Content**: `pub const VERSION: &str = "0.7.0";`

## Fixture Structure

### Organization
Fixtures organized by feature/scenario:
- Each directory contains `.checksy.yaml` configs
- May include shell scripts (`.sh`) for rule files
- `README.md` explains fixture purpose

### Key Fixtures

**happy-path/** (Basic functionality)
- Tests all severity levels (debug, info, warn, error)
- Contains `pass.sh` and `fail.sh` helper scripts

**remote-config/** (Remote inclusion)
- `.checksy.yaml`: Main config with file remote
- `shared.yaml`: Included config
- `inherit-parent.yaml`: Tests default inheritance
- `circular/`: A→B→C→A circular reference test
- `nested/`: A→B→C linear chain test
- `invalid/`: Validation error tests
- `git/`: Git-based remote with real clone

**Pattern fixtures**
- `rule-files/`: Tests glob pattern matching
- `preconditions/`: Tests preconditions execution order

## Test Locations

### Unit Tests
Inline at bottom of each source file:
- `config.rs`: ~300 lines of tests (config loading, git parsing, remotes)
- `cache.rs`: ~120 lines of tests (encoding, paths, pruning)
- `cli.rs`: ~100 lines of tests (severity parsing, rule names)
- `check.rs`: ~150 lines of tests (filtering, results, severity)

### Integration Tests
- `main.rs`: Single test for help command
- `fixtures/`: Real config scenarios tested manually or via CI

## Dependency Map

### Data Flow Direction
```
Config YAML
  ↓ (config.rs parse)
Config struct
  ↓ (remote expansion)
Expanded Config
  ↓ (Options creation)
Options
  ↓ (diagnose execution)
Report
  ↓ (CLI output)
Exit code + stdout/stderr
```

### Module Import Graph
```
main.rs
  └── cli.rs (run)

cli.rs
  ├── cache.rs
  ├── check.rs
  ├── config.rs
  ├── git.rs
  ├── schema.rs
  └── version.rs

config.rs
  ├── cache.rs
  └── schema.rs

cache.rs
  └── (std only)

git.rs
  ├── cache.rs
  └── (std only)

check.rs
  └── schema.rs

schema.rs
  └── (serde only)

lib.rs
  └── (all modules)
```

## Entry Points for Modifications

### Add Config Field
1. `schema.rs`: Add to `Config` struct
2. `config.rs`: Use in loading if needed
3. `cli.rs`: Update JSON schema in `run_schema()`

### Add Command
1. `cli.rs`: Add to `run()` match
2. `cli.rs`: Implement `run_<command>()`
3. `cli.rs`: Update `print_usage()`

### Add Rule Behavior
1. `schema.rs`: Add to `Rule` struct (if data)
2. `check.rs`: Modify `run_rule()` (if execution)
3. `cli.rs`: Update reporting (if output)

### Change Git Caching
1. `git.rs`: Modify clone command
2. `cache.rs`: Update path structure
3. `config.rs`: Update path resolution

## Uncertain Areas

1. **Exact dependency versions**: Check `Cargo.toml` for precise versions
2. **CI/CD details**: `.github/workflows/release.yml` specifics not examined
3. **Cross-compilation**: `justfile` recipes for cross-comp not detailed
4. **Windows support**: Uncertain if fully tested (paths use `/` separator)
