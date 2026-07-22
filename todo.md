# Checksy machine-provisioning roadmap

This roadmap keeps Checksy focused on one job: take an explicitly supplied
configuration and provision the current machine by running checks, fixes, and
final checks.

## Product boundary

- Checksy intentionally executes arbitrary shell commands from trusted
  configuration under the authority of the invoking user.
- Configuration comes from a local file, an auto-discovered local file, or
  stdin.
- Fetching, updating, authenticating, and unpacking configuration are external
  concerns that compose with Checksy through files and pipes.
- `check --fix` remains the provisioning operation; do not add a parallel
  `apply` lifecycle.
- Keep Checksy a CLI. Do not add a daemon, enrollment system, scheduler
  manager, source-provider framework, recorded-status database, generation
  store, trust database, or rollback engine.
- Checksy does not promise transactional rollback of arbitrary fixes and does
  not invoke `sudo` automatically.

## Compatibility guardrails

- Preserve `checksy check`, `checksy check --fix`, local YAML, stdin
  configuration, preconditions, rules, patterns, severities, hints, and local
  file includes.
- Preserve native macOS and Linux support. Native Windows support remains out
  of scope.
- Deprecate Git acquisition deliberately rather than silently changing or
  immediately removing existing configurations.
- Treat configuration acquisition as shell composition. A self-contained YAML
  document may be piped to stdin; a configuration with scripts, Brewfiles,
  templates, or other assets is materialized locally before invoking Checksy.

## P0 — Confirm the reset contract

- [x] Start a clean branch from `origin/main` with a provisioning-only
  roadmap.
- [ ] Add a short architecture decision documenting the product boundary
  above.
- [ ] Define the exact interactive/headless contract:
  - Checks and post-fix checks are always non-interactive.
  - `fix` is a headless-capable command and always receives `/dev/null` as
    stdin, whether or not a controlling terminal is present.
  - `interactive-fix` is an alternative fix command that requires a
    controlling terminal and receives real terminal semantics.
  - A rule may define `fix` or `interactive-fix`, never both.
  - `--non-interactive` explicitly prohibits terminal use but does not prevent
    ordinary `fix` commands from running.
  - `--stdin-config` implicitly selects non-interactive execution. Checksy
    never opens `/dev/tty`, allocates a PTY, or otherwise attaches a terminal
    for a stdin-supplied configuration. An explicit `--non-interactive` flag is
    accepted but redundant in this mode.
  - A missing or prohibited terminal matters only after a rule with
    `interactive-fix` fails its check. Checksy then leaves that fix unexecuted
    and reports that interactive repair is required.
  - A passing rule with `interactive-fix` never requires a terminal.
- [ ] Define conditional rule execution through an optional `skip-if` command:
  - Run `skip-if` once immediately before its rule's initial check.
  - Exit `0` means the skip condition succeeded: mark the rule skipped and do
    not run its check, fix, interactive fix, or final check.
  - Any completed nonzero exit means the skip condition was false: continue
    with the rule's check. Do not assign special meanings to individual
    nonzero exit codes.
  - Failure to start the predicate, timeout, or signal termination is an
    operational error and prevents the rule's check from running.
  - Run the predicate non-interactively with `/dev/null`, the inherited
    environment, and the working directory of the rule's defining config.
  - Report a skipped rule distinctly; it is neither compliant nor failed and
    does not affect severity thresholds.
- [ ] Choose one provisioning-lock namespace and default location that works
  for both file-backed and stdin configurations and does not depend on
  `cachePath`.
- [ ] Define the Git deprecation window and the release in which Git
  acquisition, `install`, and `cachePath` may be removed.

Exit gate: the CLI interaction modes, lock identity, compatibility policy, and
deprecation path are precise enough to test before implementation.

## P1 — Make configuration strict without changing its purpose

- [ ] Replace permissive YAML loading with strict typed deserialization.
  - Reject unknown and duplicate fields.
  - Require every rule to be exactly one valid form: a local include or an
    executable check.
  - Reject empty checks/includes, `fix` or `interactive-fix` without `check`,
    `skip-if` without `check`, rules defining both fix forms, invalid
    severities, invalid patterns, NUL bytes, explicit nulls where unsupported,
    and invalid timeout values.
  - Preserve valid existing configurations.
- [ ] Generate the configuration JSON Schema from the Rust types where
  practical.
  - Keep objects closed with `additionalProperties: false`.
  - Test structural parity between typed deserialization and the schema.
  - Document parser-only and runtime-only validation narrowly.
- [ ] Preserve deterministic diagnostics for file and stdin configurations.

Exit gate: valid legacy configurations still load, malformed configurations
fail before any configured command executes, and schema/runtime behavior is
covered by one fixture corpus.

## P2 — Preserve local configuration origins

- [ ] Carry each rule and pattern group with its defining configuration and
  working directory.
- [ ] Execute skip predicates, checks, fixes, post-fix checks, patterns, and
  referenced assets relative to the configuration that defines them.
- [ ] Preserve ordering and inherited severity semantics across local file
  includes.
- [ ] Detect active include cycles deterministically and deduplicate completed
  repeated includes.
- [ ] Keep stdin configuration rooted at the caller's current working
  directory.
- [ ] Add one checked-in origin regression fixture containing a root config,
  local include, scripts, a Brewfile, a template, and an excluded pattern.

Exit gate: local composition is predictable, every asset uses the correct
working directory, and no Git acquisition machinery is required to resolve a
definition.

## P3 — Separate interactive and headless execution

- [ ] Build one supervised command runner shared by checks, fixes, and final
  checks.
  - Apply bounded global and optional positive per-rule timeouts.
  - Run commands in a managed process group.
  - Send `TERM`, wait a bounded grace period, then send `KILL` to the group.
  - Continuously drain bounded stdout and stderr and preserve output emitted
    before failure or timeout.
  - Distinguish spawn failure, timeout, signal termination, and ordinary
    nonzero exit.
- [ ] Run `skip-if` through the same supervision and bounded-output machinery.
  - Always provide `/dev/null` as stdin; skip predicates are never interactive.
  - Preserve ordinary shell truth semantics: zero skips and every completed
    nonzero exit continues to the check.
  - Treat runner failures, timeouts, and signals as operational errors rather
    than predicate results.
- [ ] Implement headless-capable fixes.
  - Give ordinary `fix` commands `/dev/null` as stdin regardless of terminal
    availability.
  - Let `--non-interactive` force this mode even when a terminal exists.
  - Never synthesize confirmation answers or invoke `sudo`.
  - Keep timeouts and process-group cleanup active.
- [ ] Implement correct interactive fix terminal handling.
  - Execute `interactive-fix` only when its check fails and terminal use is
    available and permitted.
  - When interactive repair is required in headless mode, execute no fix for
    that rule and report a distinct actionable failure.
  - Give the fix a real controlling terminal using a PTY or correct foreground
    process-group handoff; merely inheriting a terminal file descriptor is not
    sufficient.
  - Restore terminal ownership and modes after success, failure, timeout, or
    interruption.
  - Forward or correctly route terminal signals while retaining child cleanup.
  - Restrict terminal-backed fixes to file-backed configurations; stdin mode
    remains non-interactive even when the Checksy process has a controlling
    terminal.
- [ ] Add deterministic tests for ordinary headless fixes, interactive
  prompting, deferred terminal requirements, headless EOF, terminal absence,
  stdin configuration, timeout escalation, descendants, bounded output,
  signal handling, and ordinary nonzero exits.

Exit gate: headless provisioning cannot wait for input indefinitely, and an
operator-run fix behaves like a normal terminal command without giving up
timeout or descendant cleanup.

## P4 — Serialize provisioning mutations

- [ ] Implement an operating-system advisory provisioning lock on macOS and
  Linux.
  - Acquire it for the complete `check --fix` operation, including initial
    checks and final rechecks.
  - Cover both file-backed and stdin configuration.
  - Keep ordinary check-only runs lock-free.
  - Return a distinct documented result on contention.
  - Use a securely opened regular lock file; reject symlink and special-file
    substitutions.
  - Release through descriptor lifetime and process death rather than PID-file
    interpretation.
- [ ] Add same-process and cross-process contention, release, stale-content,
  path-integrity, and stdin-mode tests without sleeps.

Exit gate: two cooperating Checksy processes cannot provision through the same
lock namespace concurrently.

## P5 — Deprecate built-in Git acquisition

- [ ] Emit a clear deprecation warning for `checksy install`, `git+...`
  locators, Git remote rules, and `cachePath`.
- [ ] Keep deprecated behavior compatible during the documented transition
  window; do not expand it with new authentication or state machinery.
- [ ] Document external composition examples:
  - Checkout or update Git content, then invoke `checksy --config`.
  - Fetch and verify self-contained YAML, then pipe it to
    `checksy --stdin-config`.
  - Fetch, verify, and unpack a multi-file bundle, then invoke the local config.
- [ ] Preserve local file includes; consider renaming `remote` to `include` only
  in a separately planned compatibility change.
- [ ] Remove Git acquisition, its mutable cache, and unused dependencies in the
  planned breaking release.

Exit gate: existing users receive an actionable migration path, while new
documentation no longer recommends built-in source acquisition.

## P6 — Harden installation and CI

- [ ] Harden `scripts/install.sh`.
  - Avoid a mutable `main` bootstrap in the recommended command.
  - Use an independently pinned release key or fingerprint when verification
    is requested.
  - Require an exact checksum match and fail closed on verification failure.
  - Support stock macOS and Linux checksum tools.
  - Stage and atomically replace the binary while retaining the old binary on
    failure.
- [ ] Add required CI for formatting, Clippy, tests without public-network
  dependencies, supported macOS/Linux builds, and installer smoke tests.
- [ ] Gate release publishing on the required test workflow.
- [ ] Remove stale Go and GoReleaser documentation.

## Required deterministic scenarios

- [ ] A local configuration performs check, fix, and successful final check.
- [ ] A stdin configuration performs the same flow with an ordinary fix and no
  terminal.
- [ ] An interactive fix can prompt through a controlling terminal.
- [ ] A stdin configuration never opens `/dev/tty` or a PTY, even when the
  Checksy process has a controlling terminal.
- [ ] An ordinary headless fix receives EOF and cannot hang waiting for
  terminal input.
- [ ] A passing rule with `interactive-fix` succeeds headlessly without
  requiring a terminal.
- [ ] A failing rule with `interactive-fix` reports interactive repair is
  required without executing that fix when headless.
- [ ] `skip-if` exit `0` reports a skipped rule and executes no check or fix.
- [ ] Every completed nonzero `skip-if` exit, including values greater than
  `1`, proceeds to the check.
- [ ] A `skip-if` predicate can gate a rule on command availability and an
  environment variable in both file-backed and stdin configurations.
- [ ] A timed-out, signaled, or unspawnable `skip-if` predicate is an
  operational failure and does not run the check.
- [ ] Concurrent file-backed and stdin provisioning contend on the same lock.
- [ ] A hung command and its in-group descendants are terminated within the
  configured bound while prior output is retained.
- [ ] Unknown YAML fields and malformed rules fail before command execution.
- [ ] Included rules and pattern assets execute from their defining local
  directories.
- [ ] Git acquisition paths emit the documented deprecation warning.
- [ ] Default tests require no public network access.

## Definition of done

- Checksy provisions from either a local configuration tree or stdin.
- Interactive and headless fixes have explicit, tested terminal semantics.
- Provisioning mutations are serialized independently of source or cache.
- Strict validation and origin-relative execution remain compatible with valid
  existing configurations.
- Built-in Git acquisition has a documented deprecation and removal path.
- Checksy contains no pull-agent state, remote trust, enrollment, scheduling,
  or recorded-status subsystem.
- Formatting, lint, deterministic tests, supported builds, and installer checks
  pass.
