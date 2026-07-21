# Checksy safe convergence roadmap

This backlog turns the external hardening review into dependency-ordered work for evolving Checksy into a small, safe pull-based convergence agent.

Repository snapshot: current HEAD `1714389` differs from the reviewed baseline `e85246b` only in devcontainer files, so the review findings still apply.

## Compatibility and scope guardrails

- Preserve `checksy check`, `checksy check --fix`, `checksy install`, local YAML, and the existing `git+<repo>#<ref>:<path>` syntax.
- Continue supporting native macOS and Linux targets; native Windows support is out of scope.
- Never elevate privileges or invoke `sudo` automatically.
- Treat remote definitions as arbitrary code. Transport encryption alone is not authentication.
- Last-known-good protects the selected definition, not the machine from partial mutations made by shell fixes.
- Keep Checksy focused on orchestration; compose with idempotent tools instead of building a package/module ecosystem.

## P0 — Set the security and compatibility contracts

- [x] Write the threat model before exposing unattended execution.
  - Identify trust boundaries for local, Git, and HTTPS sources.
  - Document manifest authenticity, signer pinning, replay/rollback behavior, privilege limits, and offline behavior.
  - State explicitly that arbitrary fixes are not transactional and cannot be rolled back by Checksy.

- [x] Resolve and document the open design decisions.
  - Default state directories and ownership for user and system scope.
  - Canonical source identity and migration from the legacy `.checksy-cache` layout.
  - Exact detached-signature location/encoding and allowed Git signer configuration.
  - User-scope trust-key ownership versus the root-owned-key requirement for system enrollment.
  - Apply success threshold, stable exit codes, and the distinct lock-contention result.
  - Whether `status` runs a live check or reports the last recorded compliance result.
  - Offline fallback duration, explicit rollback semantics, and failed-stage/history retention limits.
  - Local-directory config selection and any explicit local-only path-escape policy.
  - Timeout and schedule interval syntax, scheduler log locations, and privilege-policy format.
  - Whether enrollment is ever allowed to persist insecure or unsigned exceptions.

- [x] Specify the public formats and CLI contract.
  - Define the versioned HTTPS manifest schema and its strict validation rules.
  - Define state metadata and version the `status --json` output.
  - Define `apply`, `status`, `enroll`, and `unenroll` flags, errors, and exit codes.
  - Define bounded defaults for HTTP/Git/command timeouts, redirects, archive files/size, kill grace period, and retained failures.

Exit gate: the trust model, state model, manifest/signature format, CLI behavior, and compatibility rules are precise enough to test without relying on implementation details.

## P1 — Make configuration strict and origin-aware

- [x] Replace permissive YAML loading with strict typed validation.
  - Reject unknown fields by default.
  - Require each rule to be exactly one valid form: a remote reference or an executable check.
  - Reject empty rules, `fix` without `check`, remote rules with command fields, invalid paths/URLs/hashes/generations, non-positive timeouts, and unsupported `runAs` values.
  - Preserve all currently valid configuration behavior.
  - If a compatibility `--lenient` mode is necessary, prohibit it for unattended `apply`.

- [x] Generate JSON Schema from the runtime Rust types where practical.
  - Keep `additionalProperties: false`.
  - Add tests proving that runtime validation and emitted schema accept and reject the same fixtures.
  - Remove the hand-maintained schema once parity is established.

- [x] Introduce an internal resolved-definition model.
  - Carry the rule, canonical source identity, defining config path, base directory, and fetched bundle root.
  - Execute inline checks, fixes, patterns, Brewfiles, templates, and other assets relative to the defining config.
  - Merge remote `patterns` as well as preconditions and rules while preserving order/default semantics.
  - Support nested file and Git remotes.
  - Detect cycles by canonical source identity instead of only local path.
  - Reject traversal and symlink escapes from remote bundle roots; allow exceptions only through an explicit local-only policy.

- [x] Add an origin regression bundle.
  - Include `.checksy.yaml`, a nested config, pattern scripts, a `Brewfile`, a template, and an excluded pattern.
  - Prove every asset is resolved from the config that defines it.

Exit gate: strict parsing matches the schema, nested remotes retain their origins, remote patterns work, and fetched definitions cannot reference paths outside their bundle.

## P2 — Build shared locking and process-control primitives

- [x] Implement an operating-system advisory state-directory lock.
  - Allow only one `apply` or cache mutation per state directory.
  - Return the documented distinct result when the lock is held.
  - Add contention and stale-process tests; do not use a PID file as the locking mechanism.

- [x] Build one hardened command runner for checks, fixes, and source operations.
  - Support a global command timeout and optional positive per-rule timeout.
  - Run child commands in their own process group.
  - On timeout, terminate the whole group gracefully, wait a bounded grace period, then force-kill remaining descendants.
  - Preserve stdout/stderr and distinguish timeout from ordinary nonzero exit.
  - Apply bounded timeouts to Git and HTTP work as well as checks/fixes.
  - Disable interactive Git credentials and unattended password prompts.

- [x] Add deterministic process tests.
  - Verify a hanging child and its descendants are terminated.
  - Verify output produced before timeout is preserved.
  - Verify normal nonzero exits remain distinguishable from timeouts.

Exit gate: concurrent mutation is excluded, and a subprocess that remains in
Checksy's runner-managed process group cannot hang indefinitely or leave a known
in-group descendant running after timeout. Deliberate process-group/session
detachment and forwarding signals received by the Checksy parent remain
explicit residuals.

## P3 — Add a recoverable state store and source abstraction

- [ ] Implement the generation-based state layout.

  ```text
  <state-dir>/
    lock
    sources/<source-id>/
      generations/<revision>/
      current
      previous
      state.json
      failures/
  ```

  - Derive collision-resistant source IDs from canonical identities.
  - Write metadata atomically and include a completed/verified marker.
  - Treat a cache entry as valid only when its metadata, config, and referenced assets validate—not merely when `.git` exists.
  - Keep failure diagnostics and generation history within documented bounds.

- [ ] Make staging and promotion atomic.
  - Fetch or copy into a temporary sibling on the same filesystem.
  - Validate the complete staged definition before it is eligible for execution.
  - Converge from the staged definition and switch `current` only after a successful final check.
  - Update `previous` without deleting last-known-good first.
  - Leave `current` unchanged after fetch, authentication, extraction, validation, or convergence failure.
  - Add interruption/fault-injection tests around every promotion boundary.

- [ ] Define a shared source-provider result.
  - Return staged path, canonical source identity, immutable revision, optional generation, signer identity, validators, and whether content changed.
  - Implement local file and directory sources first.
  - Ensure “unchanged” describes source content only and never skips compliance checks.

Exit gate: failed and interrupted updates cannot select an incomplete definition, and a valid previous definition always remains usable.

## P4 — Secure Git and HTTPS acquisition

- [ ] Rework Git acquisition around immutable commits.
  - Preserve `git+<repo>#<ref>:<path>` parsing.
  - Resolve and store the peeled commit SHA for branches, lightweight tags, annotated tags, and full commit SHAs.
  - Fetch full SHAs correctly instead of passing them only to `git clone --branch`.
  - Disable interactive prompting and bound all Git operations.
  - Verify signed commits or tags against explicit locally configured allowed signers.
  - Reject unattended moving refs unless signature verification is configured or a separately signed manifest selects the commit.
  - Retain last-known-good on every Git failure.

- [ ] Define and parse a strict Cloudflare-friendly HTTPS manifest.

  ```json
  {
    "schemaVersion": 1,
    "generation": 42,
    "revision": "immutable-content-or-git-id",
    "artifact": {
      "url": "releases/42.tar.gz",
      "sha256": "..."
    },
    "configPath": ".checksy.yaml"
  }
  ```

  - Resolve relative artifact URLs against the manifest URL.
  - Require HTTPS except for an explicit interactive `--allow-insecure-http` run.
  - Use bounded connection/read/overall timeouts and redirect limits.
  - Send `If-None-Match` and, where available, `If-Modified-Since`; handle `304` by reusing the current verified bundle.

- [ ] Authenticate the exact raw manifest bytes.
  - Verify a detached Ed25519/minisign-style signature with a locally pinned `--trust-key` or enrollment key.
  - Never accept a trust key advertised only by the remote manifest.
  - Fail closed for unattended HTTPS unless authentication succeeds; permit unsigned input only through an explicit interactive exception.
  - Record the verified signer/key identifier.
  - Reject a generation older than the last applied generation unless explicit rollback was requested.

- [ ] Verify and safely extract HTTPS artifacts.
  - Verify SHA-256 before extraction.
  - Reject absolute paths, parent traversal, link escape, special files, excessive entry counts, and excessive expanded size.
  - Ensure `configPath` and all referenced assets remain inside the verified bundle.
  - Preserve a verified cached definition during network failure according to the documented offline policy.
  - Record ETag, Last-Modified, revision, generation, signer, and last-success metadata.

- [ ] Refactor `install` and `check --fix` remote fetching.
  - Route cache mutations through the same lock, staging, validation, Git, and state primitives.
  - Preserve existing interfaces and missing-remote convenience without bypassing authentication or atomicity.

Exit gate: Git and HTTPS providers always identify immutable content, unattended remote execution is fail-closed, and acquisition failures cannot displace last-known-good.

## P5 — Implement convergence and status

- [ ] Add `checksy apply --source <SOURCE>`.
  - Acquire the exclusive lock.
  - Inspect/fetch and authenticate the source.
  - Stage and strictly validate the complete definition and assets.
  - Run check, fix where needed, and final check from the staged definition.
  - Promote only after successful convergence.
  - Record source, revision, generation, signer, last attempt, last success, last error, and compliance result.
  - Re-run convergence on `304` or any other unchanged-source result so machine drift can still be repaired.
  - Report clearly when fixes may have partially changed the machine even though promotion was rejected.

- [ ] Add `checksy status [--json]`.
  - Report source, current revision, previous revision, signer, last attempt, last success, last error, and current compliance state.
  - Keep JSON stable and machine-readable with documented timestamps and nullability.
  - Ensure status checks never mutate the machine.

- [ ] Preserve legacy workflows with regression tests.
  - Cover `check`, `check --fix`, `install`, local YAML, Git locators, preconditions, rules, patterns, severities, and hints.
  - Document any state migration or deliberately changed error behavior.

Exit gate: first apply converges and promotes, unchanged-source apply still repairs drift, failed convergence leaves selection unchanged, and status accurately explains the resulting state.

## P6 — Enforce execution identity and local privilege policy

- [ ] Add optional rule field `runAs` with `current` as the default.
  - Support `current`, `root`, `user:<name>`, and `console-user` on macOS.
  - Reject unsupported identities during strict validation.

- [ ] Implement privilege transitions safely.
  - If `root` is requested while Checksy is not root, fail clearly without invoking `sudo`.
  - When root runs a user rule, set UID, GID, supplementary groups, `HOME`, `USER`, and `LOGNAME` correctly.
  - Resolve the macOS console user safely and reject login-window or non-user identities.
  - Apply the same identity to check and fix commands.

- [ ] Enforce the locally established privilege ceiling.
  - Persist allowed identities/actions in enrollment or bootstrap policy.
  - Reject fetched definitions that request more privilege than local policy allows before executing any rule.
  - Add platform-conditional tests for allowed transitions and fail-closed cases.

Exit gate: remote content cannot expand its privilege beyond local policy, and all supported identity transitions are covered on their relevant platforms.

## P7 — Add explicit enrollment and scheduling

- [ ] Add `checksy enroll --source ... --scope user|system --interval ...`.
  - Persist source, state path, trust key, and privilege policy in a securely owned local bootstrap file.
  - Use absolute executable, state, bootstrap, and log paths.
  - Validate the requested schedule and policy before writing anything.

- [ ] Generate native schedulers.
  - macOS LaunchAgent for user scope and LaunchDaemon for system scope.
  - systemd user timer/service and system system timer/service.
  - Run `checksy apply` at login/boot and periodically with persistent logging.
  - Document the exact manual “apply now” command.

- [ ] Add `checksy unenroll --scope user|system`.
  - Make enroll repeatable and updates atomic.
  - Make unenroll safe and idempotent while preserving state unless explicit deletion is requested.
  - Never install a daemon or request elevation merely because `apply` was run.

Exit gate: user/system enrollment is explicit, secure, repeatable, removable, and invokes the same supported `apply` path on macOS and Linux.

## P8 — Harden installation, CI, and releases

- [ ] Harden `scripts/install.sh`.
  - Use a versioned or immutable bootstrap route instead of mutable `main` in the recommended command.
  - Use a pinned key/fingerprint distributed independently of the artifact; never trust a sibling-downloaded key as the root of trust.
  - Fail closed when requested signature verification is unavailable or fails.
  - Require exactly one checksum entry for the selected archive.
  - Support Linux `sha256sum` and stock macOS `shasum -a 256`.
  - Stage and atomically replace the binary while retaining/restoring the previous binary on failure.
  - Add Linux and macOS installer integration/smoke tests.

- [ ] Add a required PR/push CI workflow.
  - Run `cargo fmt --check`.
  - Run `cargo clippy --all-targets -- -D warnings`.
  - Run `cargo test` with no public-network dependency.
  - Build release binaries for supported macOS/Linux targets.
  - Run installer smoke tests where practical.

- [ ] Gate publishing on the full test workflow.
  - Make the release workflow depend on the required checks before creating a release or publishing the devcontainer feature.
  - Align the crate version and displayed version behind one source of truth.

Exit gate: no release can publish without formatting, lint, tests, supported builds, and applicable installer checks passing.

## Required deterministic integration scenarios

Use temporary local Git repositories and local HTTP servers; default tests must not require public network access.

- [ ] First HTTPS apply with a valid detached signature.
- [ ] `304 Not Modified` followed by a compliance check.
- [ ] Local drift repaired while the source is unchanged.
- [ ] Changed ETag/revision validated, converged, and promoted.
- [ ] Bad signature leaves `current` unchanged.
- [ ] Bad checksum and malformed/hostile archive leave `current` unchanged.
- [ ] Failed Git clone/fetch retains last-known-good.
- [ ] Concurrent mutations are rejected through the advisory lock.
- [ ] Hung child and descendants time out and captured output is retained.
- [ ] Unknown YAML fields and structurally invalid rules fail strict validation.
- [ ] Remote assets and pattern scripts execute from their defining directories.
- [ ] Nested remotes work and canonical cycles are rejected safely.
- [ ] Annotated tags and immutable full commit refs resolve to the correct commit.
- [ ] Privilege transitions and privilege-policy violations behave correctly.
- [ ] Interrupted updates never make an incomplete generation current.

## Documentation and handoff

- [ ] Update README, architecture, schema documentation, CLI help, and release procedure.
  - Document the final CLI and manifest/signature formats.
  - Explain state layout, last-known-good, offline/rollback policy, scheduling, trust, and privilege policy.
  - Show composition with Homebrew Bundle and optionally chezmoi.
  - Remove stale Go and GoReleaser references.

- [ ] At implementation handoff, report:
  - Final CLI and manifest format.
  - Files and architecture changed.
  - Threat-model decisions.
  - Verification commands and results.
  - Backward-compatibility notes.
  - Remaining risks and intentionally deferred work.

## Definition of done

- A signed manifest served by a local Cloudflare-like test server can be applied.
- A second `304` run still checks compliance.
- Deliberate machine drift is repaired without a source change.
- Invalid, failed, or interrupted updates cannot displace last-known-good.
- Unattended remote execution is fail-closed by default.
- Existing Checksy workflows remain functional.
- Formatting, lint, unit, integration, installer, and platform-relevant tests pass.
