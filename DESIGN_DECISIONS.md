# Checksy Pull-Agent Design Decisions

> **Status: target policy contract.** Future pull-agent implementation must
> preserve these decisions. Current check and install behavior remains unchanged
> and does not yet provide these guarantees. See
> [THREAT_MODEL.md](THREAT_MODEL.md) for the security rationale.

This document resolves the second P0 roadmap item. The resulting manifest/state
schemas, full CLI grammar, numeric bounds, and process limits are frozen in the
[pull-agent public contract](PULL_AGENT_CONTRACT.md).

Static examples live in the
[pull-agent contract fixtures](fixtures/pull-agent-contract/README.md). They are
future test inputs, not evidence that this behavior is implemented today.

## State roots and ownership

| Scope | Linux | macOS |
| --- | --- | --- |
| User | $XDG_STATE_HOME/checksy, otherwise $HOME/.local/state/checksy | $HOME/Library/Application Support/checksy |
| System | /var/lib/checksy | /Library/Application Support/checksy |

- Relative XDG_STATE_HOME is invalid. Resolution never falls back to the working
  directory. Explicit overrides must be absolute, and enrollment persists the
  resolved absolute path. Scheduled system runs ignore environment overrides.
- A state root must be a real, correctly owned, non-symlink directory and cannot
  be group- or world-writable.
- User state is owned by the enrolled uid/gid; directories use 0700 and mutable
  files use 0600.
- System state is root:root on Linux and root:wheel on macOS. Its root may be
  0755 for named-user traversal. Trust, bootstrap policy, metadata, failures, and
  audit records remain in private 0700 directories with 0600 files.
- Verified system bundle directories are root-owned and non-writable, using 0555
  or 0755 for traversal. Bundle files are 0444, or 0555 when execution is needed.
  This avoids a dedicated group/ACL design.
- Operators MUST NOT put secrets in bundles. Checksy does not detect secrets;
  bundle readability is an operator-visible policy, not a secrecy control.

## Canonical source identity

Checksy stores the original display locator and a structured canonical identity.
The source ID is the full 64-character lowercase hexadecimal SHA-256 digest of:

1. ASCII checksy-source-v1 followed by a zero byte.
2. Each canonical field below, in order, as an unsigned 64-bit big-endian byte
   length followed by the exact field bytes.

URL fields use normalized UTF-8. Local Unix paths use lossless platform bytes and
are never case-folded. Relative config paths use / separators, remove . and
repeated-separator components, and reject empty, absolute, .., or NUL-containing
forms. Local roots and selected config files are physically canonicalized before
the root-relative path is derived; a symlink cannot create a second identity.

| Source | Canonical field order |
| --- | --- |
| Local | local, canonical source root, normalized root-relative config path |
| Git | git, normalized repository endpoint, canonical selector, normalized repository-relative config path |
| HTTPS | https, normalized original manifest URL |

- An explicit local file uses its canonical parent as source root and its
  filename as config path. A selected directory is itself the root.
- Git endpoints lowercase scheme/host, remove default ports and dot segments,
  normalize unreserved percent-encoding, and preserve path case, trailing slash,
  and .git. An scp-style [user@]host:path remains a tagged scp endpoint with a
  remote-home-relative path; it is never collapsed into the distinct absolute
  path semantics of ssh://host/path. Usernames such as git are allowed;
  passwords, tokens, query strings, and fragments are rejected.
- Bare Git refs must resolve uniquely to a full refs/heads/... or refs/tags/...
  selector. Full object IDs become an oid: prefix followed by the lowercase full
  object ID. The peeled commit is revision metadata rather than source identity.
- HTTPS URLs lowercase scheme/host, remove default ports and dot segments, and
  normalize unreserved path percent-encoding; retained escape hex is uppercase.
  An empty path becomes / and a non-root trailing slash remains significant.
  Userinfo and fragments are rejected. Queries preserve their exact encoding,
  order, duplicates, and empty values and are persisted, so operators must not
  put secrets in them. Redirect targets are diagnostic metadata and never change
  source identity.
- Definition-cycle keys additionally include immutable revision and canonical
  config path.

## Legacy cache transition

- Secure apply never imports, executes, or marks .checksy-cache or custom
  cachePath content as verified. Legacy names are collision-prone and their
  content is unauthenticated.
- Secure use fetches and authenticates from scratch into the new state root.
- Legacy caches remain untouched and trigger a manual-cleanup warning only after
  a successful secure apply.
- Existing check and install retain legacy behavior until refactored. cachePath
  is legacy-only and cannot redirect secure state.

## Local source selection and confinement

- An explicit file selects exactly that file. A directory uses explicit
  configPath, otherwise .checksy.yaml or .checksy.yml.
- If both default files exist, hardened apply/enrollment fails and requires
  configPath. Legacy check retains its current .checksy.yaml precedence.
- Configs, nested configs, patterns, and recognized assets resolve
  component-by-component beneath the source root.
- Protected local bootstrap policy may list canonical absolute
  allowedExternalRoots. Targets must remain beneath the source root or an allowed
  root.
- Git and HTTPS content cannot add, inherit, or use external-root exceptions.
- Confinement applies to recognized paths, not arbitrary shell access. Authorized
  commands retain all filesystem access granted by their OS identity.

## Retention

- Keep three verified payloads per source: current, previous, and the newest
  remaining historical generation. Never prune current or previous.
- Cleanup runs under the state-directory lock after durable promotion metadata.
- Delete staging payloads after handled failures. Locked startup deletes orphan
  staging from interrupted runs; staging content is never reused.
- Failure records contain bounded metadata/output, never executable source.
  Retain the newest 10 and no record older than 30 days.
- Retain the newest 100 audit attempts and no ordinary attempt older than 90
  days, while preserving records for current, previous, and the latest rollback.
- Reacquiring pruned rollback content requires normal authentication. Exact
  output byte limits are specified in
  [PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md#resource-limits).

## Trust material storage

These rules apply equally to Minisign public keys and source-scoped OpenSSH
allowed_signers files:

- Enrollment validates and atomically copies trust material into a source-scoped
  0700 directory with 0600 files. Runtime never trusts an arbitrary external
  path.
- User input may be user- or root-owned; its stored copy is user-owned. System
  input and storage must be root-owned. Every input path component and the leaf
  is opened without following symlinks, checked from the opened descriptor, and
  must not be writable by any identity other than its trusted owner. The leaf
  must be a regular file, and Checksy copies bytes from that same descriptor.
- Owner/root write access is intentionally retained for controlled local
  rotation; "writable trust material" means writable by any other principal.
  A user-owned trust copy therefore trusts that enrolled user.
- Stored ownership, mode, and fingerprint are rechecked on every use.

## HTTPS authentication

- Ordinary and unattended HTTPS requires a standard Minisign signature. Append
  .minisig to the path component of the final redirected manifest URL, before
  any query. The original URL remains source identity.
- Send Accept-Encoding: identity and reject other content encoding. Reject a
  UTF-8 BOM, and verify exact body octets including line endings before parsing
  or normalization.
- Accept one standard ASCII signature and one standard public-key file. Reject
  malformed/trailing records. Verify the complete Minisign structure; neither
  trusted nor untrusted comments authorize content.
- Record the key ID and SHA-256 fingerprint of the decoded standard Minisign
  public-key payload. File comments and textual wrapping are excluded.
- A validator-bound 304 reuses previously signed exact bytes. It confirms online
  revalidation, not new signer intent.

## Git authorization and freshness

Git v1 supports SSH signatures only:

- Each source has a protected copied OpenSSH allowed_signers file.
- Exact principals/keys are accepted; wildcard principals and cert-authority
  entries are rejected.
- Repository files, global Git configuration, and ambient trust are never
  authoritative.
- Moving branches and lightweight tags require an allowed-signed commit.
- An annotated tag may instead be authorized by its signed tag object; revision
  is the peeled commit.
- A full object ID selected through protected local policy is local authorization
  by content pin and need not also be signed.
- Branch updates must be unchanged or fast-forward descendants of the last
  accepted head. Non-fast-forward movement is rollback. Observed tags are
  immutable without an explicit authenticated one-run rollback.
- Commit timestamps never establish freshness. OpenPGP support is deferred beyond
  v1.

## Rollback and high-water marks

- HTTPS maintains the highest successfully promoted generation. Equal generation
  is accepted only when authenticated manifest digest, revision, and artifact
  hash are identical; divergence is equivocation.
- Git maintains accepted branch ancestry or tag binding independently of current.
- Rollback is an authenticated, audited, one-run request. It may use retained
  verified content or normally authenticated historical content.
- Rollback records actor, time, source, from/to revisions, and optional reason.
  It changes current/previous but never lowers a generation or ancestry
  high-water mark.
- Enrollment and schedulers cannot persist rollback intent.

## Offline policy

Offline eligibility lasts for the protected policy's configured `maxAge`, which
defaults to 24 hours and may be set from 1 minute through 7 days, from successful
online source contact:

- An authenticated 200 starts the window. A validator-bound 304 refreshes contact
  time without representing new signer freshness.
- Content is eligible when age is no greater than the configured `maxAge`.
- Offline convergence uses only integrity-checked completed current content,
  still checks/fixes/rechecks, records offline use, and never fetches/promotes.
- Fallback is allowed for DNS/connect/read failures and HTTP 408, 429, or 5xx.
- Fallback is forbidden after bad signature/checksum, malformed input, rollback
  or equivocation, policy denial, HTTP 401/403/404, or local integrity failure.
  Ineligible content fails before commands execute.

## Apply, status, and exits

- Apply uses failSeverity, defaulting to error. Promotion requires the final
  recheck to have no failure at/above the threshold. Lower failures produce
  degraded state but permit promotion.
- Acquisition, authentication, strict validation, and state errors always block
  promotion.
- An ordinary nonzero fix result is recorded and followed by a final recheck. If
  that recheck satisfies the threshold, apply must promote and exit 0 while
  retaining the fix diagnostic. A fix spawn, timeout, termination, or runner
  failure is operational: it exits 2 and cannot promote even if a later
  diagnostic check appears to pass.

| Exit | Stable meaning |
| --- | --- |
| 0 | Success or known recorded/live compliance |
| 1 | Existing missing-subcommand fallback; explicit help remains 0 |
| 2 | Invocation, source/authentication/validation/state, or operational failure |
| 3 | Known compliance/convergence failure at effective threshold |
| 4 | Advisory lock held; no attempt started |

Lock open/I/O failure is exit 2, not contention.

- Plain status reads one atomically published metadata snapshot only, derives all
  displayed fields from that snapshot, and performs no fetch, command, fix, lock,
  or state write.
- Future status --live atomically resolves one completed immutable current
  generation and keeps that generation available through a non-mutating pin for
  the whole run. It takes no state lock, fetches nothing, fixes nothing, and
  writes no state. Checks remain arbitrary code and can have side effects.
- Recorded/live pass exits 0, known failure exits 3, and missing/corrupt state or
  incomplete live checks exit 2. Output includes compliance revision/timestamp.

## Durations, scheduling, and logs

- Duration grammar is ^[1-9][0-9]*(ms|s|m|h|d)$.
- Zero, signs, fractions, whitespace, compounds, uppercase, infinity, and
  overflow are invalid. Scheduler intervals accept only m, h, or d.
- Units mean milliseconds, seconds, 60-second minutes, 3,600-second hours, and
  fixed 86,400-second days; they are never calendar durations.
- Every unattended network/process operation has finite defaults and local hard
  maximums, which per-rule and per-source values cannot exceed. Exact values and
  interval bounds are specified in
  [PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md#resource-limits).
- Enrollment resolves absolute log paths; generated schedulers contain no shell
  or environment expressions.

| Scope | stdout | stderr |
| --- | --- | --- |
| Linux user | $XDG_STATE_HOME/checksy/logs/apply.stdout.log, otherwise $HOME/.local/state/checksy/logs/apply.stdout.log | $XDG_STATE_HOME/checksy/logs/apply.stderr.log, otherwise $HOME/.local/state/checksy/logs/apply.stderr.log |
| Linux system | /var/log/checksy/apply.stdout.log | /var/log/checksy/apply.stderr.log |
| macOS user | $HOME/Library/Logs/checksy/apply.stdout.log | $HOME/Library/Logs/checksy/apply.stderr.log |
| macOS system | /Library/Logs/checksy/apply.stdout.log | /Library/Logs/checksy/apply.stderr.log |

The Linux user paths use the resolved state root and every scheduler receives its
materialized absolute path. User log dirs/files are 0700/0600. System log
dirs/files are 0750/0640 and owned by root:root on Linux or root:wheel on macOS.
Rotation limits are fixed in
[PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md#resource-limits); scheduler
implementation remains P7.

## Privilege policy

Privilege policy is a strict versioned YAML object embedded in protected
bootstrap policy. This snippet is the object itself; the containing bootstrap
schema is specified in
[PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md#protected-policy-and-enrollment):

~~~yaml
schemaVersion: 1
allowedRunAs:
  - current
allowedActions:
  - check
  - fix
~~~

- Exact identities are current, root, user:<name>, and, on macOS only,
  console-user. Linux rejects console-user. Exact actions are check and fix.
  Wildcards, groups, and executable hooks are invalid.
- User scope defaults to current with check/fix.
- System scope has no implicit identity list. Listing current explicitly
  acknowledges that the scheduler identity normalizes to root.
- Enrollment binds the expected executor uid/gid: the enrolled user for user
  scope and root for system scope. Execution paths reject an unexpected effective
  identity before loading rules. current resolves only to that bound executor.
- Authorization compares normalized selectors exactly before identity
  resolution: current matches only current and root matches only root. Named
  users and console-user require their exact selector. Pattern scripts count as
  check; a rule's check/fix use the same identity.
- Reject the whole staged definition before execution if any identity/action
  exceeds policy.
- This is orchestration policy, not a sandbox. Checks can mutate, and root code
  can change identity.

## Insecure and unsigned exceptions

- Enrollment/schedulers never persist insecure HTTP, unsigned, or rollback
  exceptions.
- Interactive insecure HTTP requires independent confirmation and a valid
  signature; signed content may promote normally.
- Interactive unsigned input is check-only: no fixes, promotion, reusable cache,
  offline eligibility, or unattended eligibility.
- Unsigned checks remain arbitrary code and may mutate or exfiltrate. Check-only
  is not a sandbox. Unsigned mode is incompatible with trust and rollback;
  insecure HTTP still requires valid authentication, so the two exceptions
  cannot be combined.

## Public contract

[PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md) defines the complete v1
manifest, policy, enrollment, state, and status formats; exact new-command CLI
grammar; and network, process, archive, output, and scheduler bounds. Existing
check, check --fix, install, local discovery, cachePath, and Git locator behavior
remains unchanged until its scheduled implementation work.
