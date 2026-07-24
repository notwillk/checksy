# Checksy machine-provisioning roadmap

Checksy has one job: take an explicitly supplied configuration and provision
the current machine by running checks, applicable fixes, and final checks.

## Product boundary

- Checksy intentionally executes arbitrary shell commands from trusted
  configuration under the authority of the invoking user.
- Configuration comes from a local file, an auto-discovered local file, or
  stdin.
- Fetching, updating, authenticating, and unpacking configuration are external
  concerns that compose with Checksy through files and pipes.
- `check --fix` is the provisioning operation. Do not add a parallel `apply`
  lifecycle.
- Checksy does not promise transactional rollback of arbitrary fixes and does
  not invoke `sudo` automatically.
- Keep Checksy a CLI. Do not add a daemon, enrollment system, scheduler
  manager, source-provider framework, recorded-status database, generation
  store, trust database, or rollback engine.

## Compatibility guardrails

- Preserve `checksy check`, `checksy check --fix`, local YAML, stdin
  configuration, preconditions, rules, patterns, severities, hints, and local
  file includes.
- Preserve native macOS and Linux support. Native Windows support remains out
  of scope.
- Deprecate Git acquisition deliberately rather than silently changing or
  immediately removing existing configurations.
- A self-contained YAML document may be piped to stdin. A configuration with
  scripts, Brewfiles, templates, or other assets is materialized locally before
  invoking Checksy.

## Priority and delivery policy

- **P0 — Core:** required for Checksy's provisioning contract and safety.
- **P1 — Important:** meaningful correctness, compatibility, and maintenance
  improvements that are not required for the smallest complete provisioner.
- **P2 — Kinda important:** valuable hardening or cleanup that may follow core
  adoption.
- **P3 — Not important:** optional refinements that should be implemented only
  after real usage demonstrates a need.
- Work on the highest-priority unblocked feature. A lower-priority feature may
  proceed only when every higher-priority remaining feature is genuinely
  blocked.
- Every feature is a vertical slice. Do not check it off until runtime
  behavior, strict configuration/schema handling, reporting, CLI help,
  documentation, deterministic unit/integration tests, formatting, tests, and
  Clippy all pass together.
- Do not land speculative foundations, dormant models, or fixture-only
  contracts for behavior that has no runtime consumer.

## Reset status

- [x] Start `codex/provisioner-reset` from `origin/main` and replace the
  pull-agent roadmap with this provisioning-only roadmap.

## P0 — Core

### Lock the product and CLI contract

- [x] Document the product boundary in README and architecture documentation.
  - State that source acquisition and authentication happen outside Checksy.
  - Keep `check --fix` as the only provisioning lifecycle.
  - Document that commands are trusted arbitrary code and machine mutations
    are not transactionally reversible.
  - Define the exact interaction modes, provisioning-lock namespace, lock
    location, and stable operational exit classes before implementing them.

This item is unblocked and prevents another architecture drift.

### Strict configuration and generated schema

- [x] Implement strict configuration loading end to end.
  - Reject unknown and duplicate fields.
  - Require each rule to be exactly one valid form: an include or an executable
    check. Continue accepting legacy Git include locators until their documented
    removal release.
  - Reject empty checks/includes, invalid severities, invalid patterns, NUL
    bytes, and unsupported explicit nulls.
  - Preserve every currently valid configuration.
  - Generate the JSON Schema from the Rust types where practical, keep objects
    closed, and test structural parity against one fixture corpus.
  - Keep duplicate-key parsing and complete shell/glob validation in their
    documented authoritative layers where JSON Schema cannot express them.
  - Exercise both file-backed and stdin loading through the public CLI.

This item is unblocked. New fields such as `skip-if` and `interactive-fix`
must be added only by their complete runtime slices below, not speculatively.

### Supervised non-interactive command runner

- [x] Route checks, ordinary fixes, final checks, and pattern scripts through
  one hardened runner.
  - Give all non-interactive commands `/dev/null` as stdin.
  - Start non-interactive commands without a controlling terminal so they
    cannot fall back to inherited `/dev/tty` prompting.
  - Apply a bounded global timeout and add an optional positive per-rule timeout
    through this same runtime/schema/documentation slice.
  - Run commands in a managed process group.
  - Send `TERM`, wait a bounded grace period, then send `KILL` to the group.
  - Continuously drain bounded stdout and stderr and retain output emitted
    before failure or timeout.
  - Distinguish spawn failure, timeout, signal termination, and ordinary
    nonzero exit.
  - Never synthesize confirmations or invoke `sudo`.
  - Add network-free process-tree, timeout, output-boundary, and exit-class
    tests.

This item is complete. It does not change ordinary command exit/severity
behavior except where explicitly documented.

### Conditional checks with `skip-if`

- [x] Add `skip-if` as an optional executable-rule command and implement it end
  to end.
  - Keep rule execution linear; do not add rule IDs, `dependsOn`, dependency
    graphs, or dependency-cycle semantics.
  - Run it once immediately before the rule's initial check.
  - Exit `0` means the condition succeeded: report the rule as skipped and run
    no check, fix, interactive fix, or final check.
  - Every completed nonzero exit means the condition was false: continue with
    the rule's check. Do not assign special meanings to individual nonzero
    values.
  - Spawn failure, timeout, or signal termination is an operational error and
    prevents the rule's check from running.
  - Run it with `/dev/null`, the inherited environment, and the same effective
    working directory as the associated check. File-backed rules use their
    defining configuration's directory; stdin uses the caller's current
    directory.
  - A skipped rule is neither compliant nor failed and does not affect severity
    thresholds.
  - Reject empty `skip-if`, `skip-if` without `check`, and `skip-if` on an
    include rule.
  - Cover command-availability and environment-variable gates in file-backed
    and stdin configurations.

This item is complete. Rule execution remains linear; no dependency graph or
special nonzero predicate exits were added.

### Interactive repairs with `interactive-fix`

- [x] Add `interactive-fix` as an alternative fix command and implement it end
  to end.
  - A rule may define `fix` or `interactive-fix`, never both; either requires a
    `check`.
  - Ordinary `fix` is headless-capable and always receives `/dev/null`, whether
    or not a terminal exists.
  - Run `interactive-fix` only after its check fails and only when terminal use
    is available and permitted.
  - A passing rule with `interactive-fix` never requires a terminal.
  - In headless mode, leave a needed interactive fix unexecuted and report a
    distinct actionable interactive-repair requirement. Continue processing
    other rules according to the normal severity policy.
  - `--non-interactive` prohibits terminal use but does not prevent ordinary
    fixes from running.
  - `--stdin-config` implicitly selects non-interactive execution. Never open
    `/dev/tty`, allocate a PTY, or otherwise attach a terminal for a
    stdin-supplied configuration; an explicit `--non-interactive` is accepted
    but redundant.
  - For file-backed interactive fixes, provide real terminal semantics through
    a PTY or correct foreground process-group handoff. Restore terminal state
    after success, failure, timeout, signal, or interruption.
  - Retain timeout and descendant cleanup while correctly routing terminal
    signals.
  - Add deterministic PTY tests for prompting, terminal absence, forced
    non-interactive mode, passing interactive rules, and failed repairs.

This item is complete. It does not make stdin configuration interactive and it
does not change ordinary headless `fix` behavior.

### Provisioning semaphore

- [x] Serialize complete `check --fix` operations with an operating-system
  advisory lock on macOS and Linux.
  - Use one documented provisioning namespace independent of configuration
    source and legacy `cachePath`.
  - Cover file-backed and stdin provisioning.
  - Acquire before initial checks and hold through fixes, final checks, and
    reporting.
  - Keep check-only runs lock-free.
  - Return a distinct documented result on contention.
  - Securely open a regular lock file and reject symlink, hardlink, ownership,
    permission, and special-file substitutions.
  - Release through descriptor lifetime and process death rather than PID-file
    interpretation.
  - Test same-process and cross-process contention, release, stale contents,
    path integrity, aliases, and stdin/file interaction without sleeps.

This item is complete. The semaphore is per effective UID, covers every
configuration-ingestion form, and remains a cooperative Checksy boundary rather
than a sandbox or cross-UID machine-global lock.

### P0 integrated acceptance gate

- [x] Prove the complete core workflow through public CLI tests.
  - A local configuration performs check, ordinary fix, and successful final
    check.
  - A stdin configuration performs the same flow without terminal access.
  - `skip-if` exit `0` skips all rule work; every completed nonzero value runs
    the check; predicate runner failures are operational errors.
  - A passing `interactive-fix` rule succeeds headlessly without needing a
    terminal.
  - A needed `interactive-fix` prompts for a file-backed terminal run and is
    left unexecuted with an actionable result in stdin or headless mode.
  - Concurrent file-backed and stdin provisioning contend on the same lock.
  - A hung process and its managed descendants terminate within the bound
    while prior output is retained.
  - Invalid configuration fails before any configured command executes.
  - The default suite uses no public network.

This gate is complete. Its closed, network-free corpus proves the combined
public-CLI lifecycle; focused tests remain authoritative for each P0 runtime
feature's edge cases.

## P1 — Important

### Dogfood Checksy in the development container

- [x] Provision the development container's userland tools through Checksy.
  - Bootstrap Checksy `0.7.6` through Feature `1.0.1` at its immutable
    canonical OCI digest.
  - Provision Entr, Just `1.57.0`, Rustup `1.29.0` with the exact Rust `1.94.1`
    toolchain and required `rustfmt`/`clippy` components, Dev Container CLI
    `0.88.0`, and local-development Codex CLI `0.145.0` from one checked-in
    configuration. Skip Codex CLI when GitHub Actions is provisioning CI.
  - Run non-interactive convergence during container creation and in CI, then
    run the same configuration check-only to prove idempotence.
  - Keep the base image, Docker-in-Docker, editor customization, and immutable
    Checksy Feature outside Checksy as the deliberate bootstrap boundary.
  - Organize helpers by provisioned tool and cover version parsing,
    architectures, download selection, checksum rejection, Rust toolchain
    selection, and Node.js compatibility with network-free tests.

This item is complete. Checksy now provisions its own development userland,
including its Rust quality toolchain, without absorbing container bootstrap or
editor lifecycle concerns.

### Correct local configuration origins

- [x] Preserve the defining origin of every local rule and pattern group.
  - Execute skip predicates, checks, fixes, final checks, patterns, scripts,
    Brewfiles, templates, and other assets relative to their defining config.
  - Preserve local include ordering and inherited severity semantics.
  - Detect active include cycles deterministically and deduplicate completed
    repeated includes.
  - Keep stdin configuration rooted at the caller's current working directory.
  - Add one checked-in root/include fixture with distinct assets and excluded
    patterns, plus end-to-end CLI assertions.

This is complete. File-backed includes now retain their defining working
directories, pattern groups remain origin-scoped, active cycles fail before
execution, and completed repeats are deduplicated. Stdin remains rooted at the
caller's current working directory.

### Deprecate built-in Git acquisition

- [ ] Ship a complete Git-acquisition deprecation slice.
  - Warn for `checksy install`, `git+...` locators, Git include rules, and
    `cachePath` without changing their existing behavior during the transition
    window.
  - Document the removal release and provide actionable migration diagnostics.
  - Document external composition: checkout Git then use `--config`; fetch and
    verify YAML then use `--stdin-config`; fetch, verify, and unpack a bundle
    then use its local config.
  - Preserve local file includes and avoid adding new Git authentication,
    caching, or state behavior.
  - Test every warning and legacy workflow without public-network access.

This item is unblocked and may proceed as soon as all unblocked P0 work is
complete.

### Required continuous integration

- [ ] Add a required PR/push workflow for formatting, Clippy, deterministic
  tests, supported macOS/Linux builds, and installer smoke tests where
  practical.
- [ ] Gate release publishing on the required workflow and align displayed and
  packaged versions behind one source of truth.

This item is technically unblocked but follows P0 under the priority policy.

## P2 — Kinda important

### Harden the release installer

- [ ] Harden `scripts/install.sh` end to end.
  - Avoid recommending a bootstrap fetched from mutable `main`.
  - Use an independently pinned release key or fingerprint when verification
    is requested; never trust a key downloaded beside the artifact.
  - Require an exact checksum match and fail closed when requested verification
    cannot be performed.
  - Support stock macOS and Linux checksum tools.
  - Stage and atomically replace the binary while retaining/restoring the old
    binary on failure.
  - Add macOS/Linux integration tests and update installation documentation.

### Remove deprecated Git acquisition

- [ ] In the documented breaking release, remove `install`, `git+...`
  acquisition, the mutable Git cache, `cachePath`, and unused dependencies.
  - Preserve local file includes.
  - Add migration tests proving local and stdin provisioning remain intact.
  - Remove stale Git-acquisition documentation and fixtures.

Blocked by completion of the P1 deprecation window.

### Documentation cleanup

- [ ] Remove stale Go and GoReleaser references and ensure every supported CLI
  example is exercised by a smoke test.

## P3 — Not important

These are deliberately optional and should remain deferred unless real usage
justifies them.

- [ ] Consider renaming the legacy local `remote` property to `include` in a
  future major release after Git acquisition is gone.
- [ ] Consider an optional human-readable skip reason only if rule names and
  the standard skipped report prove insufficient.
- [ ] Consider a configurable lock namespace/path override only if the default
  provisioning semaphore cannot support a demonstrated workflow.

## Definition of done

- Checksy provisions from a local configuration tree or stdin through
  `check --fix`.
- `skip-if`, ordinary fixes, and interactive fixes have deterministic,
  documented execution semantics.
- Interactive and headless modes are safe and fully tested.
- Provisioning mutations are serialized independently of source and cache.
- Strict validation and local origin behavior remain compatible with valid
  existing configurations.
- Built-in Git acquisition has a documented migration and removal path.
- Checksy contains no pull-agent state, remote trust, enrollment, scheduling,
  or recorded-status subsystem.
- Every completed feature includes implementation, schema/config support,
  reporting, documentation, deterministic tests, formatting, Clippy, and
  supported builds.
