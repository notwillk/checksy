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
│   ├── cache.rs                 # Legacy Git cache layout
│   ├── git.rs                   # Git CLI operations
│   ├── schema.rs                # Config types and generated Draft 7 schema
│   ├── version.rs               # Version constant
│   ├── lib.rs                   # Modules and public compatibility exports
│   ├── Cargo.toml
│   └── Cargo.lock
├── fixtures/
│   ├── strict-config/           # Runtime/schema parity corpus
│   ├── pull-agent-contract/     # Static future-agent contract corpus
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
  → config::load_resolved_with_*
      → strict Config decode
      → DefinitionResolver
          → local/Git remote expansion
          → source identity and cycle tracking
          → ResolvedDefinition with per-item origins
  → check::diagnose_resolved or CLI fix/recheck
      → preflight every pattern group
      → run preconditions from defining config directories
      → run checks/fixes from defining config directories
      → run pattern scripts from defining config directories
  → Report and CLI exit status
```

The selected root config establishes the local source identity and the one
legacy Git cache root. Nested configs may define `cachePath`, but those values do
not redirect acquisition. Local definitions retain legacy external-path
behavior. Fetched Git config files and concrete pattern matches are canonicalized
and must remain inside their checkout. These fetched path boundaries do not
sandbox shell commands.

### `install`

```text
cli::run_install
  → resolve in RefreshOrClone mode
  → refresh each newly discovered Git dependency
  → resolve again to discover remotes inside fetched parents
  → repeat until the dependency graph is complete
  → optionally prune unused entries from the root-anchored cache
```

The legacy updater still compares refs through Git, removes a changed checkout,
and shallow-clones its replacement. Authentication, locking, atomic replacement,
collision-resistant provider identities, and last-known-good state belong to
later roadmap work.

## Source Responsibilities

### `cli.rs`

- Parses legacy global flags and dispatches `check`, `diagnose`, `install`,
  `init`, `schema`, and `version`.
- Uses the resolved-definition path for CLI checking and fix/recheck behavior.
- Iteratively discovers nested Git dependencies for `install` and missing-cache
  repair during `check --fix`.
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
- Expands each pattern group independently, so negations remain origin-scoped.
- Preflights all resolved patterns before executing commands and rejects fetched
  traversal or symlink escapes.
- Aggregates `RuleResult` values into a `Report` and applies severity thresholds.

### `cache.rs` and `git.rs`

- `CacheManager` maps repository/ref pairs into the legacy
  `<cache>/git/<encoded-repository>/<ref>/` layout and supports pruning.
- The historical directory encoding is shared consistently by lookup and prune
  but is not collision-resistant; it is not a persistent source identity.
- `CacheManager::from_root` preserves the root-selected cache anchor while
  callers process nested dependencies.
- `GitCache` invokes the Git CLI for shallow clones, local HEAD lookup, and
  remote-ref lookup.
- A present `.git` directory is still only a legacy cache-presence check, not an
  integrity or authentication proof.

### `schema.rs`

- Owns strict `Config`, `Rule`, and `Severity` deserialization.
- Generates the deterministic Draft 7 config schema from the Rust model.
- Keeps duplicate YAML keys at the parser layer and full glob grammar at the
  runtime layer; the fixture corpus records those narrow parity exceptions.

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
  and patterns, pattern-only configs, and fetched pattern confinement.
- `cli.rs`: dispatch, schema output, diagnostics, resolved fix behavior, and
  install orchestration helpers.
- `cache.rs` and `git.rs`: cache paths/pruning and Git command helpers; network
  tests remain ignored by default.
- `fixtures/strict-config/`: indexed structural, YAML-parser, and runtime-only
  cases.
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
- New command: update dispatch, parser/help text, exit behavior, and CLI tests in
  `cli.rs`.
- Pull-agent public formats or policy: update the normative contract, its
  2020-12 schemas, and `fixtures/pull-agent-contract/`; do not infer those rules
  from the legacy cache implementation.
