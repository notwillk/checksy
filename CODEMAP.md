# checksy Code Map

## Directory Structure

```
/workspaces/checksy/
├── src/                    # Rust source code
│   ├── Cargo.toml          # Package manifest
│   ├── Cargo.lock          # Pinned dependency lock
│   ├── Cross.toml          # Digest-pinned Linux cross-build images
│   ├── lib.rs              # Library exports
│   ├── main.rs             # Binary entry point
│   ├── cli.rs              # ~950 lines: Command parsing & orchestration
│   ├── config.rs           # Strict loading, include expansion, and origins
│   ├── cache.rs            # ~270 lines: Cache directory management
│   ├── git.rs              # ~120 lines: Git shallow clone operations
│   ├── check.rs            # ~450 lines: Rule execution & reporting
│   ├── process_runner.rs   # Bounded Linux/macOS command supervision
│   ├── provision_lock.rs   # Per-EUID advisory provisioning semaphore
│   ├── schema.rs           # Strict types, validation & generated schema
│   ├── version.rs          # ~1 line: VERSION constant
│   └── tests/
│       ├── interactive_fix_contract.rs # Compiled-binary PTY/headless tests
│       ├── local_origin_contract.rs # Defining-directory/include tests
│       ├── p0_acceptance.rs        # Integrated public-CLI P0 acceptance gate
│       ├── provisioning_contract.rs
│       ├── provisioning_lock_contract.rs # Compiled-binary semaphore tests
│       ├── process_runner_contract.rs # Compiled-binary supervisor tests
│       ├── skip_if_contract.rs     # Compiled-binary predicate tests
│       └── strict_configuration.rs    # Compiled-binary strict-loading tests
│
├── fixtures/               # Test configurations (YAML)
│   ├── interactive-fix/     # Closed terminal-repair contract corpus
│   ├── p0-acceptance/       # Closed integrated public-CLI P0 corpus
│   ├── process-runner/      # Closed command-supervision contract corpus
│   ├── provisioning-lock/   # Closed per-EUID semaphore contract corpus
│   ├── skip-if/             # Closed conditional-rule contract corpus
│   ├── strict-config/       # Closed runtime/schema parity corpus and CLI assets
│   ├── happy-path/         # Basic severity level tests
│   ├── inline-check/       # Simple inline rule tests
│   ├── preconditions/      # Precondition execution tests
│   ├── fix-behavior/       # Auto-fix functionality tests
│   ├── rule-files/         # Pattern/glob matching tests
│   ├── default-severity/   # Default severity behavior
│   ├── check-logs/         # Log output testing
│   ├── hint-test/          # Hint message testing
│   ├── local-origin/       # Defining-directory end-to-end contract
│   └── remote-config/      # Remote inclusion tests
│       ├── git/            # Git-based remote fixture
│       ├── circular/       # Circular reference handling
│       ├── nested/         # Nested remote expansion
│       └── invalid/        # Validation error tests
│
├── .devcontainer/          # Development environment and Checksy dogfooding
│   ├── devcontainer.json   # Container bootstrap and Checksy Feature pin
│   ├── checksy.yaml        # Entr, Just, Rust, Dev Container CLI, and local Codex convergence
│   ├── tool-versions.env   # Shared exact toolchain pins and checksums
│   └── scripts/
│       ├── prerequisites/          # Required apt-tool provisioning
│       ├── entr/                   # Entr check and apt installation
│       ├── just/                   # Exact Just check and verified install
│       ├── rustup/                 # Rustup and exact Rust toolchain lifecycle
│       ├── devcontainer-cli/       # Node and exact CLI lifecycle
│       ├── codex-cli/              # Local-only exact Codex CLI lifecycle
│       ├── shared/
│       │   └── lib.sh              # Shared version/architecture helpers
│       └── tests/
│           └── run.sh              # Network-free helper contract
│
├── scripts/                # Installation scripts
│   ├── cross-compile.sh    # Pinned cross-build and release packaging
│   ├── install.sh          # Install checksy binary
│   ├── tests/
│   │   ├── release-portability.sh # Pinned target/workflow contract
│   │   └── verify-static-linux-binary.sh # Network-free verifier tests
│   ├── verify-static-linux-binary.sh # Static release contract
│   └── uninstall.sh        # Remove checksy binary
│
├── .github/workflows/      # CI/CD
│   ├── ci.yml              # PR/push tests and devcontainer convergence
│   └── release.yml         # Release automation
│
├── justfile                # Just command runner recipes
├── README.md               # User documentation
├── LICENSE                 # MIT license
├── rust-toolchain.toml     # Rust version specification
└── CODEMAP.md              # This file
```

## Devcontainer Provisioning

- [`.devcontainer/devcontainer.json`](.devcontainer/devcontainer.json)
  bootstraps the Ubuntu `24.04` base line, Docker-in-Docker, and the canonical
  digest-pinned Checksy Feature. It exposes the user-installed `.local` and
  Rust paths and runs the provisioning definition after workspace creation.
- [`.devcontainer/checksy.yaml`](.devcontainer/checksy.yaml) is the flat
  dogfooding configuration for Entr, Just `1.57.0`, Rustup `1.29.0` and Rust
  `1.94.1`, Dev Container CLI `0.88.0`, and local-only Codex CLI `0.145.0`.
- [`.devcontainer/tool-versions.env`](.devcontainer/tool-versions.env) is the
  single source for exact toolchain pins and architecture-specific checksums.
- [`.devcontainer/scripts/`](.devcontainer/scripts/) separates prerequisite,
  Entr, Just, Rustup, and Dev Container CLI checks and installers by tool.
  [`shared/lib.sh`](.devcontainer/scripts/shared/lib.sh) centralizes common
  validation, while [`tests/run.sh`](.devcontainer/scripts/tests/run.sh)
  provides the network-free contract. The flat configuration addresses helpers
  relative to its own `.devcontainer/` directory.
- [`.github/workflows/ci.yml`](.github/workflows/ci.yml) converges the
  definition and follows it with a check-only pass before code-quality checks.
  It also rebuilds the devcontainer on ARM64 and builds and smoke-tests both
  static Linux release architectures.

## Source Files Deep Dive

### lib.rs (15 lines)
**Role**: Module declaration and public API exports
**Key Exports**:
- Public modules for cache, checks, CLI, config, Git, schema, and version
- Private `process_runner` and `provision_lock` modules used by CLI execution
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
- Conditional workflow: Run `skip-if`, retain skipped outcomes, and suppress
  the complete rule when the predicate succeeds
- Fix workflow: Ordinary and interactive repair, final recheck, and unavailable
  terminal reporting
- Provisioning-lock acquisition and exit `4` handling for every `--fix` path
- Operational-error reporting and parent-signal re-raise after cleanup

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

### config.rs
**Role**: Shared strict loading and origin-preserving include expansion
**Key Functions**:
- `resolve_path()`: Config file discovery
- `load()`: Public flat compatibility entry
- `load_resolved()`: Private CLI entry retaining defining origins
- `decode_config()`: Closed typed decoder shared by CLI ingestion paths
- Recursive resolver: Active-cycle rejection and completed-definition
  deduplication
- `parse_git_remote()`: Git URL parser
- `resolve_remote_path()`: Git cache or file path resolution

**Key Types**:
- `GitRemote { repo, ref_, path }`: Parsed git URL
- `DefinitionOrigin`, `ResolvedRule`, `ResolvedPatternGroup`, and
  `ResolvedDefinition`: Private origin-aware execution projection

**Sections**:
- Lines 1-79: Resolved origin model and resolver state
- Lines 80-150: Path discovery, public compatibility load, and strict decode
- Lines 151-342: Stdin and recursive file/include resolution
- Lines 343-455: Git locator parsing and cache-path resolution
- Lines 456 onward: Loading, origin, cycle, deduplication, and Git tests

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

### check.rs (~650 lines)
**Role**: Rule execution and result collection
**Key Types**:
- `Options { config, workdir, min_severity, fail_severity }`: Public flat
  execution context
- Private resolved helpers: Per-rule and per-pattern defining work directories
- `Report { rules, fail_severity }`: Aggregated results
- `RuleResult { rule, outcome, err, stdout, stderr }`: Passed, skipped, or
  failed rule result
- `RuleOutcome`: Explicit `Passed`, `Skipped`, and `Failed` states

**Key Functions**:
- `diagnose()`: Main execution entry
- `run_rule()`: Execute one Bash check through the supervisor
- `run_rule_file()`: Execute a pattern script through the supervisor
- Internal skip helper: Execute `skip-if` before the initial check using the
  rule timeout and working directory
- Internal checked helpers: Preserve typed operational outcomes for the CLI
- `expand_rule_files()`: Glob pattern expansion
- `filter_rules()`, `filter_preconditions()`: Severity filtering
- `min_severity()`: Severity comparison utility

**Sections**:
- Lines 1-220: Result, error, report, and compatibility types
- Lines 220-450: Supervised execution, pattern expansion, and result mapping
- Lines 450-500: Severity filtering and threshold helpers
- Lines 500-end: Tests

### process_runner.rs
**Role**: Private Linux/macOS supervisor for non-interactive commands and PTY-backed interactive repairs

**Key Types**:
- `ProcessLimits`: Per-command timeout and TERM grace
- `CompletedProcess`: Exit status plus independently captured stdout/stderr
- `CapturedOutput`: Bounded bytes, original count, and truncation state
- `ProcessError`: Spawn, supervision, timeout, child-signal, parent-interrupt,
  and unsupported-platform outcomes
- Interactive terminal context: Validated outer `/dev/tty`, inner PTY, and
  restoration guard

**Behavior**:
- Forces `/dev/null` stdin and starts a new session/process group with `setsid`
- Polls and continuously drains nonblocking stdout/stderr pipes
- Retains at most 1 MiB of equal head/tail output per stream
- Uses a 15-minute default, rule-selected deadlines up to two hours, and a
  five-second TERM-to-KILL grace
- Waits for the full managed group even after a successful leader exits
- Saves and restores exact signal dispositions, forwards parent termination
  signals, escalates a second signal, and completes leader/group cleanup before
  the CLI invokes the first signal's default action
- Lazily validates a foreground controlling terminal, relays an inner PTY
  bidirectionally, forwards window changes, streams merged output live, and
  restores exact outer terminal attributes
- Provides test-only lifecycle events for deterministic process-tree assertions

### provision_lock.rs
**Role**: Private Linux/macOS provisioning semaphore shared by every `check --fix`

**Key Types**:
- `ProvisioningLock`: Non-cloneable RAII owner of the retained lock descriptor
- `ProvisioningLockError`: Held, state/integrity, and unsupported-platform
  outcomes

**Behavior**:
- Selects one documented lock path from platform and effective UID, resolving
  non-root homes through the operating-system account database
- Creates an owner-only `0700` Checksy directory and persistent `0600` lock
  file, then verifies ownership, regular-file type, link count, and pathname
  identity without following links
- Uses a nonblocking exclusive advisory lock plus a process-local device/inode
  registry for consistent same-process contention
- Keeps the descriptor close-on-exec, ignores file contents, and releases by
  descriptor lifetime or process death

### schema.rs
**Role**: Public data definitions, strict private projections, validation, and generated schema
**Key Types**:
- `Severity`: Enum with custom serialization (Error/Warning/Info/Debug)
- `Rule`: Struct with optional fields, validation
- `Config`: Top-level config struct

**Key Methods**:
- `Severity::parse()`: String parsing
- `Rule::is_remote()`: Check if remote rule
- `Rule::validate_remote_only()`: Enforce remote-only constraint
- Skip validation: Require nonblank `skip-if` only on executable rules
- Repair validation: Require exactly zero or one of `fix` and `interactive-fix`
- Timeout parsing: Enforce `1ms` through `2h` executable-rule bounds
- `configuration_schema()`: Deterministically generate Draft 7 from the strict model

**Sections**:
- Strict optional/string wrappers and reusable schema constraints
- Severity compatibility and schema definition
- Exact include/executable rule union
- Closed top-level config projection and runtime validation
- Generated-schema and structural-parity unit tests

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

**local-origin/** (Defining-origin contract)
- Root and included configs own distinct same-named assets
- Exercises predicate, check, repair, final recheck, and pattern working
  directories
- Proves root-first pattern groups, group-local negation, cycle rejection, and
  completed deduplication

**Pattern fixtures**
- `rule-files/`: Tests glob pattern matching
- `preconditions/`: Tests preconditions execution order

**strict-config/** (Strict configuration contract)
- `cases.yaml`: Closed index of accepted/rejected structural, YAML-parser, and runtime-only cases
- `valid/` and `invalid/`: Runtime/generated-schema parity documents
- `integration/`: Marker-based file, stdin, nested, fix, install, and cached-Git CLI assets
- `README.md`: Validation-layer ownership and compatibility rules

**process-runner/** (Supervised command contract)
- `cases.yaml`: Closed fixture-to-test index
- YAML and shell assets: EOF, timeout, signal, bounded-output, fail-fast, PTY,
  and parent-interruption cases
- `README.md`: Exact executable coverage and residual session-escape boundary

**interactive-fix/** (Terminal repair contract)
- `cases.yaml`: Closed fixture-to-test index for PTY and headless modes
- YAML and shell assets: Prompting, stdin prohibition, explicit
  non-interactive operation, fix outcomes, terminal restoration, timeout,
  suspension, and parent interruption
- `README.md`: Interaction lifecycle, test mapping, and terminal boundary

**provisioning-lock/** (Provisioning semaphore contract)
- `cases.yaml`: Closed fixture-to-test index for file, auto-discovered, stdin,
  alias, passing, failure, and contention paths
- YAML and shell assets: Marker order, FIFO readiness, cache-path independence,
  invalid preflight, and stale lock-file content
- `README.md`: Per-EUID namespace, lifecycle, filesystem boundary, and residual
  advisory-lock risk

**skip-if/** (Conditional-rule contract)
- `cases.yaml`: Closed fixture-to-test index for file and stdin predicates
- YAML assets: Command and environment gates, completed exits, summary/report
  states, fix suppression, workdir/timeout behavior, and operational failures
- `README.md`: Exact predicate, reporting, and process-supervision contract

**p0-acceptance/** (Integrated P0 acceptance contract)
- `cases.yaml`: Closed index of the complete public-CLI provisioning scenarios
- YAML configurations: File/stdin repair, skip predicates, interactive
  lifecycle, provisioning contention, bounded descendant cleanup, and strict
  preflight; PTY, FIFO, and process-tree helpers live in the compiled test
- `README.md`: End-to-end expectations and mapping to the compiled acceptance
  tests; focused feature corpora remain authoritative for edge cases

## Test Locations

### Unit Tests
Inline at bottom of each source file:
- `config.rs`: ~300 lines of tests (config loading, git parsing, remotes)
- `cache.rs`: ~120 lines of tests (encoding, paths, pruning)
- `cli.rs`: ~100 lines of tests (severity parsing, rule names)
- `check.rs`: ~150 lines of tests (filtering, results, severity)
- `process_runner.rs`: Process outcomes, timeouts, output bounds, signals,
  `/dev/null`, and deterministic descendant cleanup
- `provision_lock.rs`: Path selection, integrity checks, same/cross-process
  contention, descriptor inheritance, and release

### Integration Tests
- `main.rs`: Single test for help command
- `tests/local_origin_contract.rs`: Defining-directory, pattern-group, cycle,
  completed-deduplication, and stdin contract
- `tests/provisioning_contract.rs`: Public help, exit, and documentation contract
- `tests/strict_configuration.rs`: Actual compiled-binary strict-loading and schema tests
- `tests/process_runner_contract.rs`: Actual compiled-binary process-supervision tests
- `tests/interactive_fix_contract.rs`: Actual compiled-binary PTY/headless
  interactive-repair tests
- `tests/p0_acceptance.rs`: Integrated, network-free public-CLI P0 acceptance
  gate
- `tests/provisioning_lock_contract.rs`: Actual compiled-binary provisioning
  semaphore tests
- `tests/skip_if_contract.rs`: Actual compiled-binary conditional-rule tests
- `fixtures/strict-config/`: Fully indexed strict model plus checked-in CLI assets
- `fixtures/process-runner/`: Closed network-free command-runner scenarios
- `fixtures/p0-acceptance/`: Closed network-free integrated P0 scenarios
- `fixtures/skip-if/`: Closed network-free conditional-rule scenarios
- `fixtures/interactive-fix/`: Closed network-free interactive-repair scenarios
- `fixtures/provisioning-lock/`: Closed network-free semaphore scenarios
- `fixtures/local-origin/`: Closed network-free defining-origin scenarios

Provisioning-lock unit tests cover path derivation, exact modes and ownership,
same-process and cross-process contention, stale contents, close-on-exec,
descriptor-lifetime release, and malicious filesystem entries. The compiled
contract tests exercise file-backed, auto-discovered, aliased, and stdin
provisioning with FIFO readiness and command markers, plus lock-free check-only
runs and fail-fast exits `2` and `4`.

## Dependency Map

### Data Flow Direction
```
Config YAML
  ↓ (config.rs parse)
Config struct
  ↓ (remote expansion)
Expanded Config
  ↓ (provision_lock.rs for --fix)
Per-EUID provisioning lock
  ↓ (Options creation)
Options
  ↓ (severity filter, then skip-if)
Applicable checks
  ↓ (diagnose/fix execution)
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
  ├── provision_lock.rs
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
  ├── schema.rs
  └── process_runner.rs

process_runner.rs
  ├── libc (temporary sigaction save/install/restore and terminal ioctls)
  ├── rustix (process, poll, PTY, and terminal primitives)
  └── signal-hook

provision_lock.rs
  ├── libc (effective-user account lookup)
  └── rustix (descriptor-relative filesystem and advisory-lock primitives)

schema.rs
  ├── serde / serde_yaml
  ├── schemars
  └── glob

lib.rs
  └── (public modules plus private process_runner and provision_lock)
```

## Entry Points for Modifications

### Add Config Field
1. Implement the field's complete runtime behavior; do not add dormant fields.
2. `schema.rs`: Add it to the public type and strict raw projection with validation.
3. Update `fixtures/strict-config/cases.yaml` and positive/negative fixtures.
4. Add compiled-binary coverage for every ingestion path the field affects.
5. `checksy schema` updates automatically from the Rust model; assert parity rather than editing JSON.

### Add Command
1. `cli.rs`: Add to `run()` match
2. `cli.rs`: Implement `run_<command>()`
3. `cli.rs`: Update `print_usage()`

### Add Rule Behavior
1. `schema.rs`: Add to `Rule` struct (if data)
2. `check.rs`: Route execution through the typed supervised path
3. `process_runner.rs`: Change lifecycle behavior only when the rule feature
   changes process supervision
4. `cli.rs`: Update reporting and operational-error behavior (if output)
5. Add strict fixtures plus the feature's closed compiled-binary contract cases

### Change Git Caching
1. `git.rs`: Modify clone command
2. `cache.rs`: Update path structure
3. `config.rs`: Update path resolution

## Uncertain Areas

1. **Windows support**: Native Windows is outside the current product boundary
