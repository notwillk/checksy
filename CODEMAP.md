# checksy Code Map

## Repository Structure

```text
/workspaces/checksy/
├── src/                         # Rust crate and binary
│   ├── main.rs                  # Process entry point
│   ├── cli.rs                   # Command parsing and orchestration
│   ├── config.rs                # Strict loading and recursive resolution
│   ├── resolved.rs              # Private origin-aware execution model
│   ├── check.rs                 # Compatibility and resolved execution
│   ├── process_runner.rs        # Bounded child-process supervision
│   ├── cache.rs                 # Legacy Git cache layout
│   ├── git.rs                   # Git CLI operations
│   ├── state_lock.rs            # Advisory mutation lock
│   ├── schema.rs                # Config types and generated Draft 7 schema
│   ├── version.rs               # Version constant
│   ├── lib.rs                   # Modules and public compatibility exports
│   ├── Cargo.toml
│   └── Cargo.lock
├── fixtures/
│   ├── strict-config/           # Runtime/schema parity corpus
│   ├── pull-agent-contract/     # Agent contracts plus executable process matrix
│   ├── origin-regression/       # Defining-config asset/command regression
│   ├── remote-config/           # File/Git inclusion examples
│   └── rule-files/              # Pattern-script examples
├── schemas/pull-agent/          # Public 2020-12 pull-agent schemas
├── scripts/                     # Installation scripts
├── README.md
├── ARCHITECTURE.md
├── THREAT_MODEL.md
├── DESIGN_DECISIONS.md
├── PULL_AGENT_CONTRACT.md
└── todo.md
```

## Runtime Flow

### `check` and deprecated `diagnose`

```text
main.rs
  → cli::run
  → run_check
  → config::prepare_resolved_root       # File-backed --fix only
  → acquire cache-root advisory lock    # File-backed --fix only
  → PreparedRoot::resolve or config::load_resolved_with_*
      → strict Config decode
      → DefinitionResolver
          → local/Git remote expansion
          → source identity and cycle tracking
          → ResolvedDefinition with per-item origins
  → check::diagnose_resolved or CLI fix/recheck
      → preflight every pattern group
      → run preconditions from defining config directories through bounded runner
      → run checks/fixes from defining config directories through bounded runner
      → run pattern scripts from defining config directories through bounded runner
  → Report and CLI exit status
```

The selected root config establishes the local source identity and the one
legacy Git cache root. Nested configs may define `cachePath`, but those values do
not redirect acquisition. Local definitions retain legacy external-path
behavior. Fetched Git config files and concrete pattern matches are canonicalized
and must remain inside their checkout. These fetched path boundaries do not
sandbox shell commands. Linux/macOS Bash work defaults to 15 minutes with a
5-second termination grace period; a positive per-rule timeout may shorten but
cannot extend the default.

For a file-backed config, the complete `check --fix` path holds the advisory
lock for the canonical legacy cache root. Ordinary `check` and stdin fix mode do
not acquire it.

### `install`

```text
cli::run_install
  → prepare and freeze the root-selected legacy cache directory
  → acquire the advisory cache-root guard
  → resolve the prepared root in RefreshOrClone mode
  → refresh each newly discovered Git dependency
  → resolve again to discover remotes inside fetched parents
  → repeat until the dependency graph is complete
  → optionally prune unused entries from the root-anchored cache
```

The legacy updater still compares refs through Git, removes a changed checkout,
and shallow-clones its replacement. Its advisory lock serializes cooperating
file-backed Checksy mutations, but does not authenticate the mutable cache or
provide atomic replacement, collision-resistant provider identities, or
last-known-good state. Git revision operations have a 1-minute timeout and
clone/fetch work has a 5-minute timeout; child stdin and standard Git/OpenSSH
prompts are disabled, and credential helpers are requested to be noninteractive.

## Source Responsibilities

### `cli.rs`

- Parses legacy global flags and dispatches `check`, `diagnose`, `install`,
  `init`, `schema`, and `version`.
- Uses the resolved-definition path for CLI checking and fix/recheck behavior.
- Iteratively discovers nested Git dependencies for `install` and missing-cache
  repair during `check --fix`.
- Holds the canonical legacy cache-root lock across complete file-backed
  `install` and `check --fix` invocations; ordinary checks and stdin fix mode are
  unlocked.
- Routes Git source operations and CLI Bash execution through one bounded,
  noninteractive subprocess implementation.
- Prints configuration diagnostics, rule outcomes, summaries, and the generated
  schema.

### `config.rs`

- Discovers and strictly decodes YAML configurations.
- Applies inherited check/fail severities and rule defaults.
- Recursively resolves local files and cached Git locators into a
  `ResolvedDefinition`.
- Tracks active and completed definitions by structured identity, revision, and
  canonical defining path: active recursion is an error; a completed include is
  deduplicated.
- Retains each rule and pattern group's defining origin.
- Canonicalizes local file remotes while preserving their legacy ability to
  resolve externally; fetched config paths must remain inside their checkout.
- Validates canonical repository-relative Git entry config paths.
- Preserves the public `load() -> Config` API through a flat compatibility
  projection that retains only the selected root pattern group.
- Prepares file-backed mutation roots once, freezing the decoded root definition
  and root-selected cache path across all locked resolution passes.

### `resolved.rs`

- Defines private `SourceIdentity` variants for local, Git, and stdin sources.
- Uses canonical filesystem locations but exact legacy Git endpoint/ref strings;
  complete provider normalization remains a later source-provider milestone.
- Defines `DefinitionOrigin`, including defining path, base directory,
  source-relative config path, optional fetched bundle root, and revision.
- Defines `DefinitionKey` for active-cycle and completed-include tracking.
- Defines origin-bearing rules, pattern groups, complete definitions, resolver
  modes, and Git dependency descriptors.
- Does not create persistent 64-character source IDs; that belongs to the future
  state/source-provider layer.

### `check.rs`

- Retains the public flat `Options`/`diagnose` execution API.
- Provides crate-private resolved filtering and execution for the CLI.
- Runs inline checks and fixes with the defining config directory as `cwd`.
- Preflights optional positive rule timeouts before command execution, then
  applies them to checks, fixes, and rechecks; a rule can only shorten the
  compiled 15-minute default.
- Expands each pattern group independently, so negations remain origin-scoped.
- Preflights all resolved patterns before executing commands and rejects fetched
  traversal or symlink escapes.
- Aggregates `RuleResult` values into a `Report` and applies severity thresholds.

### `process_runner.rs`

- Defines the private `ProcessLimits`, `CompletedProcess`, bounded capture, and
  typed `ProcessError` model shared by Bash and Git callers.
- Supports Linux/macOS, forces `/dev/null` stdin, drains stdout/stderr
  concurrently, and retains 512 KiB head plus 512 KiB tail per stream with the
  original byte count and truncation state.
- Starts each direct child in its own process group. On timeout it sends `TERM`,
  waits the configured grace period (5 seconds for current callers), then sends
  group `KILL` and performs a bounded reap.
- Treats an ordinary nonzero exit or signal termination as `CompletedProcess`;
  the check layer makes signal termination operational, while spawn,
  supervision, timeout, and unsupported-platform outcomes are runner errors.
- Does not sandbox commands, enforce CPU/memory/disk/network quotas, forward
  parent signals, or guarantee cleanup for background work that closes capture
  descriptors before leader exit or deliberately changes process group/session.

### `cache.rs` and `git.rs`

- `CacheManager` maps repository/ref pairs into the legacy
  `<cache>/git/<encoded-repository>/<ref>/` layout and supports pruning.
- The historical directory encoding is shared consistently by lookup and prune
  but is not collision-resistant; it is not a persistent source identity.
- `CacheManager::from_root` preserves the root-selected cache anchor while
  callers process nested dependencies.
- `GitCache` invokes the Git CLI for shallow clones, local HEAD lookup, and
  remote-ref lookup through the shared bounded runner.
- Git revision work uses a 1-minute budget, clone/fetch work uses a 5-minute
  budget, stdin is `/dev/null`, standard Git/OpenSSH prompts are disabled, and
  credential helpers are requested to be noninteractive.
- A present `.git` directory is still only a legacy cache-presence check, not an
  integrity or authentication proof.

### `state_lock.rs`

- Defines private `StateDirectoryLock` and structured `LockError` acquisition
  results for contention, state failures, and unsupported platforms.
- Canonicalizes the selected root and uses Linux/macOS `rustix` operations to
  open its persistent `lock` leaf without following that leaf.
- Requires a single-link regular file owned by the effective uid/gid with exact
  mode `0600`; contention remains distinct from lock-file integrity failure.
- Relies on the operating system to release ownership on descriptor close or
  process death instead of treating recorded PID text as the lock.
- Serializes cooperating processes only. It does not prevent direct mutation by
  another local actor, authenticate cached content, provide descriptor-relative
  ancestor traversal, or implement atomic promotion.

### `schema.rs`

- Owns strict `Config`, `Rule`, and `Severity` deserialization.
- Generates the deterministic Draft 7 config schema from the Rust model.
- Validates and exposes optional positive per-rule timeout durations without
  allowing a rule to increase the compiled command timeout.
- Keeps duplicate YAML keys at the parser layer and full glob grammar plus
  duration numeric overflow/hard-maximum checks at the runtime layer; the
  fixture corpus records those narrow parity exceptions.

### `lib.rs` and `main.rs`

- `main.rs` connects process stdio and exit status to `cli::run`.
- `lib.rs` keeps the existing flat configuration/check APIs public.
- The resolved-definition module and origin-aware executor are deliberately
  crate-private; external Rust callers using `load()` plus `diagnose(Options)`
  keep the legacy single-workdir behavior.

## Test and Fixture Map

- `schema.rs` and `config.rs`: generated-schema validity, strict fixture parity,
  typed loading, diagnostics, defaults, remote expansion, identities, cycles,
  nested Git discovery, and config confinement.
- `check.rs`: severity behavior, compatibility execution, origin-relative rules
  and patterns, pattern-only configs, fetched pattern confinement, and focused
  timeout/result propagation.
- `cli.rs`: dispatch, schema output, diagnostics, resolved fix behavior, and
  install orchestration, prepared-root, and mutation-lock behavior.
- `cache.rs` and `git.rs`: cache paths/pruning and Git command helpers; network
  tests remain ignored by default.
- `process_runner.rs`: deterministic Linux/macOS tests cover forced timeout of
  a TERM-resistant managed leader/child/grandchild tree, pre-timeout output,
  ordinary nonzero completion, exact/max-plus-one capture, and continuous
  output draining.
- `state_lock.rs`: owner/mode/type validation, contention, release, stale-process
  recovery, and descriptor-inheritance behavior.
- `fixtures/strict-config/`: indexed structural, YAML-parser, and runtime-only
  cases.
- `fixtures/pull-agent-contract/process/`: executable case-to-test mapping for
  the completed P2 process scenarios; its boundary is the runner-managed
  process group, not deliberately detached work or parent-signal forwarding.
- `fixtures/origin-regression/`: network-free CLI regression proving root and
  nested rules, assets, and pattern scripts use their defining config's
  directory and that origin-scoped exclusions never execute.
- `fixtures/remote-config/` and `fixtures/rule-files/`: human-readable legacy
  examples.

## Change Routing

- Configuration syntax or validation: update `schema.rs`, generated-schema tests,
  and `fixtures/strict-config/`.
- Remote resolution, origins, or cycle behavior: update `config.rs` and the
  private types in `resolved.rs`.
- Check/fix/pattern execution: update `check.rs` and the CLI fix path.
- Git cache discovery or refresh: update `cli.rs`, `cache.rs`, and `git.rs`.
- Timeout, capture, process-group, or child-stdin semantics: update
  `process_runner.rs`, its deterministic tests, and the executable process
  contract index.
- Mutation serialization or lock-file safety: update `state_lock.rs` and the
  complete mutation scopes in `cli.rs`.
- New command: update dispatch, parser/help text, exit behavior, and CLI tests in
  `cli.rs`.
- Pull-agent public formats or policy: update the normative contract, its
  2020-12 schemas, and `fixtures/pull-agent-contract/`; do not infer those rules
  from the legacy cache implementation.
