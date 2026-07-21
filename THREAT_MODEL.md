# Checksy Threat Model

> **Status: target security contract.** This document defines the intended
> fail-closed behavior of Checksy's planned pull-based convergence agent. The
> current `check` and `install` commands do not yet implement many of these
> controls and must not be treated as safe unattended remote execution. See
> [Current implementation gaps](#current-implementation-gaps).
> Normative policy defaults are recorded in
> [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md). Exact public formats, CLI behavior,
> and resource ceilings are specified in
> [PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md).

## Purpose

Checksy loads definitions that contain shell commands, evaluates those commands,
may run their fixes, and is intended to acquire definitions from local, Git, and
HTTPS sources. A definition is therefore code, not passive configuration. An
authorized definition can exercise every permission available to its selected
execution identity.

This model establishes the trust boundaries and invariants that must hold before
`checksy apply` is suitable for unattended use. It covers acquisition,
authentication, validation, definition selection, execution, and recovery. The
Checksy executable, operating system, shell, Git client, and cryptographic
libraries are assumed to have been obtained through a trusted bootstrap process.
Distribution and installer hardening are tracked separately in the roadmap.

## Protected assets and security goals

| Asset | Security goal |
| --- | --- |
| Host and user data | Prevent an unauthenticated or out-of-policy definition from executing against it. |
| Process-accessible credentials | Do not expose them to definitions that have not passed source authentication and local policy checks. |
| Local trust and enrollment policy | Keep trust keys, allowed signers, source selection, and privilege limits under local control. |
| Verified definitions | Preserve their integrity from authentication through execution. |
| Current and previous selections | Never replace last-known-good with an incomplete, unauthenticated, invalid, or non-convergent definition. |
| Status and audit metadata | Accurately identify attempts, selected revisions, signers, failures, explicit rollback, and offline use. |

The target design prioritizes authenticated definitions, rollback resistance,
strict validation, bounded execution, least privilege, atomic selection, and
recoverability of the selected definition.

## Actors and trust assumptions

The following actors are trusted:

- The local operator or bootstrap administrator who selects sources, provisions
  trust roots, establishes privilege policy, and protects the state directory.
- A locally authorized signer. A signer is trusted to authorize arbitrary code
  within the local privilege policy, not merely to attest to configuration data.
- The local operating system and privileged components that enforce ownership,
  permissions, process identity, and filesystem operations.

The following inputs and systems are untrusted until verified:

- Networks, DNS, proxies, mirrors, redirects, and hosting providers.
- Git repositories, moving refs, tags, and commits not verified against an
  explicitly allowed signer or authenticated selector.
- HTTPS manifest and artifact servers, including Cloudflare Pages or R2.
- Manifests, archives, nested definitions, referenced paths, symlinks, and
  command output.
- Keys, signer identities, or policy advertised only by a remote source.

## Non-goals

Checksy does not attempt to:

- Sandbox rule or fix commands.
- Transactionally roll back arbitrary changes made by shell commands.
- Protect a host whose root account, kernel, trusted toolchain, Checksy binary,
  or locally provisioned trust policy is compromised.
- Protect against malicious code intentionally authorized by a trusted signer.
- Guarantee availability when an authorized command is destructive or consumes
  resources outside the limits Checksy can enforce.
- Build a package-management or configuration-module ecosystem.
- Support native Windows execution.

## Trust boundaries by source and subsystem

| Boundary | Trust decision | Target contract | Current status |
| --- | --- | --- | --- |
| Local file or directory | Explicit operator selection establishes trust in the content. | Canonicalize and strictly validate the complete definition. The default `current` identity is the invoker; any future identity transition remains constrained by local policy. | Legacy YAML receives strict typed validation and the CLI retains per-definition origins. Local file remotes and patterns preserve trusted-workspace external-path and symlink behavior; protected external-root policy is not wired, and there is no execution-identity policy. |
| Git source | TLS, repository ownership, and a ref name are not publisher authentication. | Resolve an immutable commit. Unattended moving refs require an allowed signed commit/tag or selection by a separately signed manifest. Compare annotated tags by their peeled commit. | Git sources are shallow-cloned by ref without allowed-signer verification. The CLI now confines fetched config files and concrete pattern matches to the canonical cached checkout and retains their origins, but this does not authenticate the checkout. |
| HTTPS source | The exact manifest bytes must be authenticated independently of the server. | Verify a detached signature with a locally provisioned pinned key, then verify the artifact SHA-256 and safely extract it inside the bundle root. | HTTPS manifests and artifact authentication are not implemented. |
| State directory | Local ownership and complete verification metadata establish eligibility. | Lock mutations, stage on the same filesystem, and atomically select only completed generations. Protect and retain current and previous selections. | There is no generation state store, advisory lock, or current/previous selection. |
| Rule execution | Crossing this boundary executes arbitrary code. | Execute only an authenticated, strictly validated, locally authorized definition under its selected identity with bounded runtime. | Commands run through Bash with the invoking process's ambient identity and environment and without timeouts. |

Transport security protects a connection; it does not establish that a specific
publisher authorized a definition. A remote source must never establish its own
root of trust.

The current CLI's resolved-definition model prevents a nested definition from
changing the selected root config's legacy cache location, executes each rule
from its defining config directory, and checks structured fetched config and
pattern paths before execution. Clone, refresh, and prune paths also reject
symlink redirects found below the operator-selected cache root during preflight.
These path-based controls remain raceable without a mutation lock and
descriptor-relative no-follow operations. They reduce wrong-origin execution
and path escape through structured fields, but do not make the mutable cache
verified, prevent a local actor from changing it, or constrain paths deliberately
accessed by arbitrary shell code.

## Target security invariants

The planned unattended `apply` path must satisfy all of these invariants:

1. **Authenticate before execution eligibility.** Remote content may be parsed
   and validated only as untrusted staged input; it is not eligible for execution
   or promotion until its required authentication has succeeded. Validation never
   substitutes for authentication. Explicit interactive insecure or unsigned
   exceptions must be locally authorized and auditable; enrollment and
   schedulers never persist them, as specified in
   [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md#insecure-and-unsigned-exceptions).
2. **Validate the whole definition.** Strictly validate configuration structure,
   source locators, paths, hashes, generations, timeouts, identities, nested
   references, and referenced assets before executing a staged definition.
3. **Confine fetched content.** Archive entries, configuration paths, nested
   references, and symlinks must not escape a fetched bundle's root. Any local
   exception must be explicit and unavailable to remote content.
4. **Keep trust local.** Trust keys, allowed signers, source selection, and the
   privilege ceiling come from locally protected bootstrap or enrollment policy,
   never solely from fetched content.
5. **Do not elevate implicitly.** Checksy must not insert or invoke `sudo` on a
   rule's behalf. A rule can still contain arbitrary shell code, so operating
   system authorization remains part of the security boundary.
6. **Bound unattended work.** Source acquisition, checks, and fixes must have
   bounded time, redirects, archive entry counts, and expanded size. Timed-out
   commands must have their process group terminated while preserving captured
   output.
7. **Serialize mutation.** Only one apply or cache mutation may operate on a
   state directory at a time through an operating-system advisory lock.
8. **Promote only after convergence.** Fetch, authentication, extraction,
   validation, checks, fixes, and final checks run against a staged generation.
   `current` changes only after the final check succeeds.
9. **Preserve last-known-good.** Fetch, authentication, extraction, validation,
   convergence, and required pre-promotion metadata failures leave `current`
   unchanged. An interruption during atomic selection must resolve to either the
   old generation or a completely verified new generation, never an incomplete
   one.
10. **Separate source change from machine drift.** An unchanged source, including
    an HTTPS `304 Not Modified`, must still run compliance checks and eligible
    fixes.
11. **Record security-relevant outcomes.** Status metadata must record source,
    immutable revision, generation where applicable, verified signer, attempt,
    success, error, rollback, offline use, and compliance result.

## Freshness, rollback, and offline behavior

For HTTPS sources, a successfully applied manifest generation establishes a
monotonic floor. An ordinary apply must reject an older generation. Rollback must
be explicit, authenticate the requested content through the normal trust path,
and leave an audit record.

A Git commit or tag signature authenticates signer intent for an object, but does
not by itself prove that the object is newer than the selected revision. An
immutable pinned commit avoids ref movement but permits deliberate reuse. Moving
Git refs need either an allowed signature policy plus locally defined freshness
rules or selection by a signed manifest with a monotonic generation.

Offline convergence is permitted only when local policy allows it and the
selected bundle was previously authenticated, completely validated, and remains
within the configured staleness window. Offline operation:

- Uses only the verified `current` bundle.
- Still runs compliance checks and eligible fixes.
- Cannot authenticate or promote new content.
- Records that cached content was used.
- Leaves the selected definition unchanged when policy, expiry, or local
  integrity checks fail.

State paths and ownership, the 24-hour default offline window, rollback
semantics, duration syntax, and retention limits are fixed in
[DESIGN_DECISIONS.md](DESIGN_DECISIONS.md). Exact numeric network, command,
archive, and output limits are fixed in
[PULL_AGENT_CONTRACT.md](PULL_AGENT_CONTRACT.md#resource-limits).

## Privilege and mutation limits

The selected execution identity defines the immediate blast radius. Remote
content must not request an identity or capability above the ceiling established
by locally protected enrollment policy. Checksy must fail clearly when the
requested identity is unavailable and must never elevate automatically.

Rules may read files, environment variables, credentials, and network resources
available to their identity. They may also invoke tools, including `sudo`, if the
host's own policy permits it. Checksy's identity selection is not a sandbox and
cannot compensate for an unsafe operating-system authorization policy.

Last-known-good protects only which definition Checksy selects. A fix may change
packages, files, accounts, services, or other host state before a later command
fails. Checksy cannot generally detect or transactionally undo those changes.
Operators should prefer idempotent fixes and maintain backups appropriate to the
managed system.

## Threats, mitigations, and residual risk

| Threat | Target mitigation | Residual risk |
| --- | --- | --- |
| Compromised network or hosting service | Authenticate exact manifest bytes or allowed Git objects; verify artifact digest. | An attacker can still deny service, delay responses, or observe public metadata. |
| Compromised signing key | Pin trust locally; support local rotation/revocation and record signer identity. | Until revoked, the key can authorize arbitrary code within local policy. |
| Replay or rollback | Enforce monotonic HTTPS generations; require explicit authenticated rollback; use immutable Git revisions or signed selectors. | Git signatures alone provide no total ordering, and an explicit rollback intentionally accepts older code. |
| Artifact or checksum substitution | Bind the artifact SHA-256 inside the authenticated manifest and verify before extraction. | A trusted signer can intentionally authorize harmful content. |
| Archive traversal or link escape | Reject absolute paths, parent traversal, escaping links, special files, and excessive extraction. | Parser defects remain possible; commands can access host paths after authorized execution begins. |
| Local state tampering | Enforce local ownership/permissions, completed metadata, integrity checks, and atomic writes. | A compromised privileged local actor can tamper with the binary, policy, state, or audit data. |
| Concurrent or interrupted updates | Use one advisory lock, sibling staging, atomic selection, and current/previous generations. | Arbitrary commands can race with unrelated host processes outside Checksy. |
| Hung commands or credential prompts | Disable interactive source prompts; enforce timeouts and terminate process groups. | Timeouts are not a full process or resource sandbox, and deliberately detached work may outlive a command. |
| Resource exhaustion | Bound redirects, time, archive entries, and expanded size. | Authorized shell commands can still consume CPU, memory, disk, or network resources allowed by the OS. |
| Privilege misuse | Enforce locally configured execution identities and never auto-elevate. | Authorized code retains every capability of its identity and may exploit permissive host authorization. |
| Partial fix failure | Keep definition selection unchanged, retain diagnostics, and report possible partial mutation. | Arbitrary host mutations cannot be rolled back reliably. |

## Current implementation gaps

At the current repository state:

| Control area | Current gap | Threats left unmitigated |
| --- | --- | --- |
| Remote source contract | There is no `apply`, `status`, HTTPS manifest, signer authentication, enrollment, rollback control, or offline policy implementation. | Compromised transport/hosting, signing-key trust, artifact substitution, and unaudited rollback cannot be handled by the target workflow. |
| Git acquisition and freshness | Cache validity is based on the presence of `.git`; moving refs are not checked against an allowed signer or ordered freshness policy, and the legacy ref-directory encoding is not collision-resistant. | A ref can move to unauthenticated or replayed content, distinct ref spellings can alias one legacy slot, and Git signatures do not currently establish an allowed publisher. |
| Validation and bundle confinement | Runtime YAML rejects unknown fields, invalid scalar types, malformed rule forms, NUL bytes in constrained fields, and invalid patterns. Its generated Draft 7 schema has fixture-tested structural parity; duplicate YAML keys remain parser-owned and the complete Rust glob grammar remains runtime-owned. The CLI retains origins for inline rules and per-config patterns, preflights patterns before commands, and confines fetched Git config/pattern targets to the canonical checkout. The public flat Rust loading/execution compatibility projection omits nested remote pattern groups because it cannot preserve their origins; local definitions retain legacy external-path behavior; HTTPS archive validation and protected local external-root policy are not implemented. | Schema-only consumers cannot detect the two documented layered cases. A mutable or concurrently changed legacy checkout can invalidate path assumptions, trusted local definitions can select external content, archive attacks remain unhandled, and arbitrary shell commands can access paths outside every structured source boundary. |
| State integrity and concurrency | There is no protected generation or durable status/audit metadata, advisory lock, or atomic current/previous selection. | Local state tampering, concurrent mutation, and interrupted updates can leave no trustworthy selection history. |
| Process and resource bounds | Git, checks, and fixes use blocking subprocesses without comprehensive timeouts or process-group termination; Git prompting is not consistently disabled. | Hung descendants, credential prompts, and command resource exhaustion are not bounded by Checksy. |
| Privilege policy | Commands execute through Bash with the invoking process's ambient identity and environment; there is no `runAs` or local privilege-ceiling policy. | A trusted or compromised definition receives all authority of the invoking identity, subject only to host policy. |
| Recovery from failure | Updating a changed Git cache removes the old clone before the replacement succeeds, and arbitrary fixes have no transactional rollback. | Acquisition failure can discard the usable cache, while partial fixes can leave durable host mutations. |

These gaps are roadmap items, not accepted security properties. Until the target
controls are implemented and tested, operators must run only locally reviewed,
trusted definitions and must not deploy current Git remote workflows as an
unattended privileged agent.

## Operator obligations

Operators remain responsible for:

- Provisioning trust keys and allowed signers through an authenticated out-of-band
  channel rather than from the managed source.
- Protecting the Checksy binary, bootstrap policy, trust keys, state directory,
  scheduler definitions, and logs with appropriate local ownership and
  permissions.
- Granting signers only the source and privilege authority they require, and
  rotating or revoking compromised trust locally.
- Reviewing definitions as code, using idempotent checks and fixes, and keeping
  independent backups for important host state.
- Keeping secrets out of public manifests and definition bundles.
- Monitoring status, authentication failures, offline fallback, rollback, and
  repeated convergence failures.

Changes to source authentication, state selection, execution identity, command
isolation, or rollback behavior must update this threat model and their security
tests in the same change.
