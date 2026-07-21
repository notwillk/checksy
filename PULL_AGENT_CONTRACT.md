# Checksy Pull-Agent Public Contract

> **Status: target contract; not implemented.** This document specifies the
> public formats and command behavior of Checksy's planned pull-based
> convergence agent. The current `check` and `install` commands do not implement
> these guarantees, and the current repository must not be deployed as an
> unattended remote-execution agent. The security rationale is in
> [THREAT_MODEL.md](THREAT_MODEL.md), and the governing policy choices are in
> [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md).

The words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, and **MAY** are
normative. A schema check is necessary but not sufficient: every runtime
constraint in this document is also mandatory.

## Contract scope and compatibility

This contract adds `apply`, `status`, `enroll`, and `unenroll`. It does not
change the syntax or behavior of existing `check`, `check --fix`, `install`,
`init`, `schema`, `version`, local YAML discovery, `cachePath`, or
`git+<repository>#<ref>:<path>` locators. Legacy commands continue to use the
legacy cache until their later roadmap refactor. New pull-agent commands reject
legacy global `--config` and `--stdin-config` flags.

Five strict JSON Schema 2020-12 documents define the public and persisted
objects:

- [`manifest-v1.schema.json`](schemas/pull-agent/manifest-v1.schema.json)
- [`policy-v1.schema.json`](schemas/pull-agent/policy-v1.schema.json)
- [`enrollment-v1.schema.json`](schemas/pull-agent/enrollment-v1.schema.json)
- [`state-v1.schema.json`](schemas/pull-agent/state-v1.schema.json)
- [`status-v1.schema.json`](schemas/pull-agent/status-v1.schema.json)

All five objects reject unknown properties. JSON inputs MUST be UTF-8 without a
byte-order mark, MUST contain exactly one top-level object, and MUST reject
duplicate member names and trailing non-whitespace data. YAML
policy input is parsed as YAML 1.2, rejects aliases, merge keys, tags, duplicate
keys, and multiple documents, and is validated as the `policy-v1` logical
object. Persisted policy and enrollment files are JSON.

All hashes are lowercase 64-character SHA-256 hex strings. All public timestamps
are UTC RFC 3339 with exactly millisecond precision, for example
`2026-07-21T14:32:05.123Z`. Provider generations, snapshot sequences, and other
positive IDs are base-10 integers from 1 through `9007199254740991`; count and
byte-count fields explicitly permit zero where their schemas say so. Negative,
fractional, exponential-form, and out-of-range values are invalid. Durations use
`^[1-9][0-9]*(ms|s|m|h|d)$` and fixed units rather than calendar time.

## HTTPS manifest

The exact manifest object defined by
[`manifest-v1.schema.json`](schemas/pull-agent/manifest-v1.schema.json) is:

```json
{
  "schemaVersion": 1,
  "generation": 42,
  "revision": "release-2026-07-21",
  "artifact": {
    "format": "tar.gz",
    "url": "bundle-42.tar.gz",
    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "sizeBytes": 12345
  },
  "configPath": ".checksy.yaml"
}
```

Every shown field is required and no other field is permitted.

- `schemaVersion` is the integer `1`.
- `generation` is a positive JavaScript-safe integer and participates in the
  HTTPS monotonic high-water policy.
- `revision` is 1 through 256 printable ASCII characters. It is diagnostic
  metadata, never a filesystem name or freshness proof.
- `artifact.format` is exactly `tar.gz`.
- `artifact.url` is a 1 through 8192 byte HTTP(S) absolute URL or relative
  URI-reference without userinfo or a fragment. A relative reference resolves
  against the **final redirected manifest response URL**, while source identity
  remains based on the original manifest URL.
- `artifact.sha256` hashes the exact compressed bytes.
- `artifact.sizeBytes` is the exact compressed byte count, from 1 through
  268435456. A shorter or longer response fails before extraction.
- `configPath` is a normalized relative UTF-8 path of at most 1024 bytes. It uses
  `/`, has no empty, `.`, `..`, backslash, control, absolute, or trailing-slash
  component, and must resolve beneath the extracted bundle root.

The detached signature URL is formed by appending `.minisig` to the path of the
final manifest response URL before its query. Checksy verifies the exact response
body bytes, including all whitespace and line endings, before JSON parsing and
rejects a BOM. Minisign comments are verified as required by the standard
format but never authorize a key, source, generation, or revision.

## Generation identity and bundle digest

`revision`, `generationId`, and `bundleSha256` are distinct:

- `revision` is provider metadata: the canonical full Git commit object ID, the
  signed manifest revision, or the local snapshot label.
- `bundleSha256` protects the staged content eligible for execution.
- `generationId` is a source-scoped, filesystem-safe identity for an immutable
  staged generation.

### Canonical bundle digest

Checksy computes `bundleSha256` over the validated staged definition and every
recognized staged asset, excluding transport metadata, `.git`, state metadata,
and temporary files. All archive links and special files are rejected before
digesting. The digest input is:

1. ASCII `checksy-bundle-v1` followed by a zero byte.
2. Each directory and regular file, ordered by the raw UTF-8 bytes of its
   normalized root-relative path.
3. For each entry, four fields encoded as an unsigned 64-bit big-endian byte
   length followed by the field bytes: entry kind (`directory` or `file`), path,
   executable marker (`0` or `1`; directories use `0`), and contents
   (directories use an empty field).

The root itself is omitted. File ownership, group, timestamps, archive header
order, extended attributes, and non-executable permission bits are ignored.
Empty directories are included. Local-only allowlisted assets are copied into
the staged resolved definition under collision-free logical paths and are
included. Recomputing the digest before execution or offline reuse MUST produce
the stored value.

### Generation ID

`generationId` is the lowercase SHA-256 of ASCII
`checksy-generation-v1` plus a zero byte, followed by each field below encoded
with the same unsigned 64-bit big-endian length prefix used for source IDs:

| Provider | Ordered fields after the domain prefix |
| --- | --- |
| Local | `sourceId`, `local`, `bundleSha256`, `configPath` |
| Git | `sourceId`, `git`, object format (`sha1` or `sha256`), canonical full peeled commit object ID, `configPath` |
| HTTPS | `sourceId`, `https`, `manifestSha256`, `configPath` |

The generation directory name is `generationId`, never raw `revision`. For
HTTPS, `manifestSha256` hashes the exact signed manifest response bytes. The
manifest binds `artifactSha256`, so a changed signed manifest produces a new
generation ID even when the extracted tree happens to be identical.

## Protected policy and enrollment

### Protected policy format

[`policy-v1.schema.json`](schemas/pull-agent/policy-v1.schema.json) defines this
exact top-level shape:

```yaml
schemaVersion: 1
privilegePolicy:
  schemaVersion: 1
  allowedRunAs: [current]
  allowedActions: [check, fix]
allowedExternalRoots: []
failSeverityCeiling: error
offline:
  enabled: true
  maxAge: 24h
limits: {}
```

All six top-level fields are required. `limits` may omit individual properties;
omissions materialize the defaults in [Resource limits](#resource-limits).
Supplied values may only tighten the compiled maximums. The materialized policy,
including defaults, is what enrollment persists and fingerprints.

`privilegePolicy` permits only exact `current`, `root`, `user:<name>`, and, on
macOS, `console-user` selectors and exact `check` and `fix` actions. Wildcards,
groups, executable hooks, and Linux `console-user` are invalid. A system policy
has no implicit identity; listing `current` explicitly acknowledges that it
executes as the bound root scheduler identity.

`allowedExternalRoots` contains unique authoritative path objects of the form
`{"bytesBase64Url":"...","display":"..."}`. The first field is unpadded
base64url for at most 4096 lossless canonical platform path bytes; `display` is
non-authoritative. The list MUST be empty for Git and HTTPS.

`failSeverityCeiling` is nullable. When it is non-null, the effective apply
threshold is the stricter of this local ceiling and the definition's
`failSeverity`, using severity order `debug < info < warn < error`; the
threshold alone does not reject the definition. Null adds no local threshold.
The resolved user default is null.

`offline.enabled` defaults to `true`; when enabled, `maxAge` defaults to `24h`,
must be at least `1m`, and may not exceed `7d`. When disabled, `maxAge` is
`null`. There is no CLI flag
that enables or extends offline use. Offline fallback retains the eligibility
and failure matrix in [DESIGN_DECISIONS.md](DESIGN_DECISIONS.md#offline-policy).

### Enrollment format and paths

[`enrollment-v1.schema.json`](schemas/pull-agent/enrollment-v1.schema.json)
defines one protected enrollment per scope with these exact required fields:

```json
{
  "schemaVersion": 1,
  "scope": "user",
  "source": {},
  "stateDir": {"bytesBase64Url": "...", "display": "..."},
  "expectedExecutor": {"uid": 1000, "gid": 1000, "username": "alice"},
  "trust": {},
  "policy": {},
  "schedule": {
    "interval": "1h",
    "stdoutPath": {"bytesBase64Url": "...", "display": "..."},
    "stderrPath": {"bytesBase64Url": "...", "display": "..."}
  },
  "createdAt": "2026-07-21T14:32:05.123Z",
  "updatedAt": "2026-07-21T14:32:05.123Z"
}
```

No field is optional. `source` is exactly one of:

| Kind | Exact fields |
| --- | --- |
| Local | `kind: local`, canonical `root` path object, normalized `configPath` |
| Git | `kind: git`, canonical `repository`, resolved `selector`, normalized `configPath` |
| HTTPS | `kind: https`, canonical original `manifestUrl` |

Git `selector` is a unique `refs/heads/...`, `refs/tags/...`, or
`oid:<full-lowercase-object-id>`. Object IDs are 40 or 64 lowercase hex
characters. HTTPS enrollment accepts `https://` only; an insecure exception is
never persisted.

`trust` must match the source:

| Source | Exact trust object |
| --- | --- |
| Local | `{ "kind": "local-operator" }` |
| HTTPS | `kind: minisign`, authoritative `keyPath`, 16-hex `keyId`, and `publicKeySha256` |
| Moving Git ref/tag | `kind: ssh`, authoritative `allowedSignersPath`, and `fileSha256` |
| Protected Git object pin | `kind: git-content-pin` and exact `objectId` matching the `oid:` selector |

The user enrollment binds a non-root uid/gid and username. The system enrollment
binds exactly uid 0, gid 0, username `root`. Every execution validates the bound
effective identity before loading rules.

Enrollment files are always discoverable independently of a custom state root:

| Scope | Linux | macOS |
| --- | --- | --- |
| User | `$XDG_CONFIG_HOME/checksy/enrollment.json`, otherwise `$HOME/.config/checksy/enrollment.json` | `$HOME/Library/Application Support/checksy/enrollment.json` |
| System | `/etc/checksy/enrollment.json` | `/Library/Application Support/checksy/enrollment.json` |

Enrollment stores absolute, resolved state and scheduler-log paths. User
enrollment files and their Checksy-owned parent are user-owned with `0600` and
`0700` modes. System enrollment files are root-owned (`root:root` Linux,
`root:wheel` macOS) and `0600`. Linux `/etc/checksy` is `0700`; the macOS system
file shares the `0755` state root required for named-user bundle traversal, while
its private trust and policy children remain `0700`. Trust and policy inputs are
opened without following symlinks, checked from the same descriptor that is
copied, and rejected if non-regular or writable by an identity other than their
trusted owner.

## State metadata

[`state-v1.schema.json`](schemas/pull-agent/state-v1.schema.json) defines the
atomically published snapshot for one source. Its exact top-level fields are:

```json
{
  "schemaVersion": 1,
  "snapshotSequence": 1,
  "source": {},
  "selection": {"current": null, "previous": null, "additional": []},
  "freshness": {},
  "lastAttempt": null,
  "lastSuccess": null,
  "lastError": null,
  "recordedCompliance": null,
  "updatedAt": "2026-07-21T14:32:05.123Z"
}
```

All fields are required; absence is represented by explicit `null`. The snapshot
sequence starts at 1 and strictly increases for every atomic publication.
`source` contains exact `id`, `kind`, display locator, and canonical source
object; Checksy recomputes the ID and rejects mismatches.

`selection.current` and `selection.previous` are nullable generation objects;
`selection.additional` contains at most one. A generation contains exactly:

- `generationId`, `revision`, `configPath`, `bundleSha256`, `signer`,
  `verifiedAt`, and `promotedAt`;
- `providerGeneration`, `manifestSha256`, and `artifactSha256`, which are
  non-null only for HTTPS and explicit `null` otherwise.

The signer union is `local-operator`, `minisign` with key ID/fingerprint, `ssh`
with exact principal/key fingerprint and signed-object kind (`commit` or `tag`),
or `git-content-pin` with object ID.
Selection rejects duplicate generation IDs, incomplete payloads, or
previous/additional values without a current value.
A non-null current selection also requires non-null `lastSuccess` and
`recordedCompliance`, because content cannot become current before a successful
final convergence check.

`freshness` is a source-matched union:

- Local stores `kind: local` and nullable `snapshotSha256`.
- Git stores `kind: git`, selector, nullable accepted commit and tag object, and
  nullable acceptance timestamp.
- HTTPS stores `kind: https`, nullable high-water generation, manifest/revision/
  artifact identity, ETag, Last-Modified text, and last online contact.

`lastAttempt`, `lastSuccess`, `lastError`, and `recordedCompliance` use the
strict `$defs` in the state schema. Attempts identify start/end, one of
`success`, `degraded`, `noncompliant`, or `error`, exit, revision/generation,
online versus offline operation, rollback, source change, promotion, error, and
bounded output. Compliance uses exactly `compliant`, `degraded`, or
`noncompliant`, plus revision, generation, timestamp, effective fail severity,
offline marker, aggregate totals, and exact debug/info/warn/error counts.
Absence of a compliance result is represented by a null containing field.

`lastError` describes only the latest failed attempt and is cleared to null by
the next successful or degraded attempt. A failed candidate updates attempt,
error, failure, and audit metadata atomically while preserving the selected
current generation and that current generation's recorded compliance.

Audit and failure files use `state-v1.schema.json#/$defs/auditRecord` and
`#/$defs/failureRecord`. They are never executable payloads. A lock-held result
creates no attempt. Captured stdout and stderr retain exact-byte, unpadded
base64url head/tail segments, original byte counts, and truncation markers;
failure persistence is bounded independently from live capture.

### State layout and atomicity

The logical layout is:

```text
<state-dir>/
  lock
  sources/<source-id>/
    state.json
    trust/
    policy.json
    generations/<generation-id>/
    staging/
    failures/
    audit/
```

All mutation takes the OS advisory lock. Staging is a private sibling on the same
filesystem. A generation becomes eligible only after authentication, complete
validation, bundle digest, durable metadata, convergence, and a successful final
check. Promotion atomically publishes one new snapshot; `current` is never a
partially updated symlink or path. A failed or interrupted operation may
atomically publish diagnostic metadata, but it leaves the prior current
selection and its recorded compliance authoritative.

Plain status reads one atomically published snapshot. Live status resolves one
immutable completed current generation from that snapshot and holds a
non-mutating operating-system reference for the whole check. Garbage collection
MUST NOT remove a generation while such a reference is active. Status takes no
mutation lock and writes no state.

## Status JSON

[`status-v1.schema.json`](schemas/pull-agent/status-v1.schema.json) defines the
only output of `status --json`. Exactly one compact, single-line object followed
by one newline is written to stdout; spinners, warnings, and human text never
share stdout, and human diagnostics go to stderr.

```json
{
  "schemaVersion": 1,
  "result": "status",
  "mode": "recorded",
  "generatedAt": "2026-07-21T14:32:05.123Z",
  "status": {
    "source": {
      "id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "kind": "local",
      "display": "/srv/checksy/source"
    },
    "current": null,
    "previous": null,
    "lastAttempt": null,
    "lastSuccess": null,
    "lastError": null,
    "recordedCompliance": null,
    "observedCompliance": null
  },
  "error": null
}
```

All fields, including nullable fields, are required. For `result: status`, mode
is `recorded` or `live`, `status` is non-null, and `error` is null. Recorded mode
always has null `observedCompliance`; live mode has a non-null observed summary.
The status payload exposes the source ID/kind/display; current and previous
summaries with exact `revision`, nullable `providerGeneration`, `signer`, and
`verifiedAt`; attempt and success summaries; `lastError`; and recorded and
observed compliance. It deliberately excludes generation IDs, bundle hashes,
canonical path bytes, internal paths, HTTP validators, policy, captured command
output, and private audit data.

For `result: error`, `error` is exactly `{code, message, retryable}` with a
stable code and non-stable message. Mode is nullable. Non-live errors have null
status; a live operational error may use `mode: live` and include the recorded
snapshot while leaving observed compliance null. `compliance-failed` is never
an error result: noncompliance remains `result: status` and exit 3. Once a
`status --json` invocation is recognized, argument, lookup, state, and
live-check failures still emit exactly this one JSON object. Failure to
serialize or write stdout is the only case where a complete object cannot be
promised.

Recorded `compliant` or `degraded` and completed live `compliant` or `degraded`
exit 0. Known `noncompliant` exits 3. Missing, corrupt, or incomplete state and
incomplete live checks exit 2. A live check executes arbitrary rule
commands and can have side effects even though Checksy performs no fetch, fix,
promotion, lock, or state write.

## CLI contract

New commands are strict. They accept value flags as `--flag value` or
`--flag=value`; boolean flags are bare only. They reject unknown flags,
positionals, the `--` token itself, and a missing or empty value. Repeating a
singleton flag is invalid even with the same value.
`checksy <command> --help`, `-h`, and `checksy help <command>` print that
command's help and exit 0 without side effects.

`SOURCE` is exactly one of:

- a local file or directory path;
- the compatible `git+<repository>#<ref>:<path>` locator; or
- an `https://` manifest URL, with `http://` accepted only for an interactive
  direct apply using `--allow-insecure-http`.

### Apply

```text
checksy apply --source <SOURCE>
  [--scope user|system] [--state-dir <ABS>] [--config-path <REL>]
  [--trust-key <ABS> | --allowed-signers <ABS>] [--policy <ABS>]
  [--replace-trust] [--replace-policy]
  [--rollback-to <TARGET>] [--rollback-reason <TEXT>]
  [--allow-insecure-http] [--allow-unsigned] [--non-interactive]

checksy apply --enrollment <ABS>
  [--rollback-to <TARGET>] [--rollback-reason <TEXT>]
  [--non-interactive]
```

Exactly one of `--source` and `--enrollment` is required. An enrollment path,
state directory, trust path, signer path, and policy path must be absolute.
Direct scope defaults to `user` only for a non-root caller. Root must explicitly
select `--scope system`; root user-scope and non-root system-scope execution
fail with `permission-denied`, and Checksy never invokes `sudo`. System scope
requires an explicit system policy. `--config-path` is allowed only for a local directory. A local
file already selects its config. HTTPS takes `--trust-key`; moving Git takes
`--allowed-signers`; local rejects both. A protected enrolled Git `oid:` source
may use `git-content-pin`; a one-off direct object ID still requires an allowed
signer because invocation text is not persisted protected policy.

Every authenticated direct HTTPS apply requires `--trust-key`; the sole
exception is explicit unsigned check-only mode. Every direct Git apply,
including a full object-ID selector, requires `--allowed-signers`. The supplied
trust and any supplied/default policy are copied and fingerprinted in source
state. Supplying a different fingerprint fails unless the matching
`--replace-trust` or `--replace-policy` is also supplied; each replace flag
requires its input. Replacement is staged under the lock and becomes
authoritative only with successful authentication and convergence. Failure
preserves the old trust, policy, and current generation.

`--rollback-to` is one-run and never persisted. HTTPS uses
`generation:<positive-integer>`; Git and local use
`revision:<printable-ASCII-id>`. A mismatched target kind is invalid.
`--rollback-reason` is optional only when a target is present, is at most 512
UTF-8 bytes, and is recorded. Retained content may be used; otherwise the target
is reacquired through the normal provider and authentication path. Rollback
never lowers a high-water mark.

`--allow-insecure-http` and `--allow-unsigned` are direct, interactive-only
exceptions. Insecure HTTP still requires a valid signature and may promote.
Unsigned content is check-only: no fixes, promotion, reusable state, offline
eligibility, enrollment, trust input, or rollback. It is incompatible with
`--trust-key`, `--allowed-signers`, `--rollback-to`, and replacement flags.
Because insecure HTTP always requires valid authentication, HTTP plus unsigned
is rejected rather than composing the exceptions. Checks remain arbitrary code.

Apply locks source mutation, revalidates the complete staged definition against
policy, checks, fixes eligible failures, and rechecks. It promotes only when the
final effective threshold passes. An unchanged source and an HTTPS 304 still run
this convergence sequence. Ordinary nonzero fix results are retained as
diagnostics and a passing final check exits 0; spawn, timeout, termination, or
runner failure exits 2 and cannot promote.

### Status

```text
checksy status
  [--enrollment <ABS> | --source <SOURCE> | --source-id <64HEX>]
  [--state-dir <ABS>] [--config-path <REL>] [--live] [--json]
```

At most one selector is accepted. With none, status uses the caller-scope
default enrollment and exits 2 if none exists. `--source` canonicalizes only
enough to locate existing state and never fetches; `--source-id` is exactly 64
lowercase hex characters. `--state-dir` is valid only with `--source` or
`--source-id`; `--config-path` is valid only with a local directory `--source`.
Default status is recorded and executes nothing. `--live` has the behavior and
side-effect warning specified under [Status JSON](#status-json).

### Enroll

```text
checksy enroll --source <SOURCE> --scope user|system --interval <DURATION>
  [--state-dir <ABS>] [--config-path <REL>]
  [--trust-key <ABS> | --allowed-signers <ABS>] [--policy <ABS>]
  [--replace] [--apply-now] [--non-interactive]
```

Interval is required, uses only `m`, `h`, or `d`, and is from `5m` through `30d`
inclusive. Enrollment permits only HTTPS, never HTTP. Trust-source compatibility
matches direct apply. A full Git object ID may become a protected content pin;
moving refs require allowed signers. User scope materializes the default user
policy when `--policy` is absent. System scope requires root and `--policy`.

There is one enrollment per scope. Identical enrollment is idempotent and exits
0. An interval-only change does not require `--replace`; a source, state, trust,
or policy change does. Enrollment and scheduler replacement are one atomic
logical change; any pre-commit scheduler or file failure retains the old
enrollment and scheduler. Without `--apply-now`, enroll validates paths, trust,
policy, ownership, and scheduler configuration but performs no remote fetch and
runs no definition command.

With `--apply-now`, Checksy stages the new enrollment, applies it, and publishes
the enrollment and scheduler only after successful convergence. Failure returns
the apply exit, leaves a prior enrollment/scheduler unchanged (or leaves none
for first enrollment), and may retain bounded attempt diagnostics and host-side
partial mutations. Successful enrollment records absolute executable,
enrollment, state, and log paths. Generated schedulers invoke the exact
enrollment using `apply --enrollment <ABS> --non-interactive` at login/boot and
the interval.

### Unenroll

```text
checksy unenroll --scope user|system [--delete-state] [--non-interactive]
```

Unenroll is idempotent. It atomically removes the scope's scheduler and
enrollment; a scheduler-removal failure retains both. By default it preserves
all state and logs. `--delete-state` additionally deletes only the enrolled
source's state, copied trust/policy, verified bundles, and scheduler logs after
scheduler removal, never unrelated direct-source state. System unenrollment
requires root. Every deletion target is resolved from protected enrollment
metadata before any path is removed.

### Headless execution

Headless mutation requires explicit `--non-interactive`. The flag reads neither
stdin nor a terminal, disables Git credential
and password prompts, never implies consent, and never weakens validation.
Schedulers always use it. `--allow-insecure-http` and `--allow-unsigned` are
incompatible with it. If an exception requires confirmation and no usable TTY is
available on both stdin and stderr, Checksy fails with `interactive-required`
before fetching or executing. Environment variables cannot acknowledge an
exception.

Explicit mutation flags (`--replace-trust`, `--replace-policy`, `--rollback-to`,
`--replace`, `--apply-now`, and `--delete-state`) are sufficient authorization
in non-interactive mode after normal ownership, scope, and policy checks.

Apply, enroll, and unenroll take the mutation lock and may return exit 4 with
`lock-held`. Status is always lock-free.

## Resource limits

All boundaries are inclusive. Bytes are octets; MiB/GiB below use powers of
1024. Duration enforcement uses a monotonic clock. A protected policy may select
values above or below a compiled default but never above the hard maximum;
missing values use compiled defaults. Zero, integer overflow, and
maximum-plus-one resource values fail before fetch or execution. Definitions
may only shorten the protected effective `commandTimeout`; CLI flags cannot
raise or bypass policy, and no granular resource-limit flags are exposed.

| Policy field or fixed control | Default | Maximum |
| --- | ---: | ---: |
| `httpConnectTimeout` | `10s` | `60s` |
| `httpReadIdleTimeout` | `30s` | `5m` |
| `httpManifestTimeout` (combined manifest and signature overall budget) | `2m` | `5m` |
| `httpArtifactTimeout` | `10m` | `30m` |
| `gitResolveTimeout` (resolution and signature verification) | `1m` | `5m` |
| `gitFetchTimeout` (clone/fetch) | `5m` | `30m` |
| `commandTimeout` (check/fix) | `15m` | `2h` |
| `killGracePeriod` (TERM-to-KILL) | `5s` | `30s` |
| `maxRedirects` per HTTP response chain | 5 | 5 |
| `maxManifestBytes` | 1 MiB | 1 MiB |
| `maxSignatureBytes` | 16 KiB | 16 KiB |
| `maxTrustOrPolicyBytes` (each trust or policy input) | 64 KiB | 64 KiB |
| `maxArtifactBytes` compressed | 256 MiB | 256 MiB |
| `maxArchiveEntries` | 10,000 | 10,000 |
| `maxArchiveSingleFileBytes` expanded | 512 MiB | 512 MiB |
| `maxArchiveExpandedBytes` total | 2 GiB | 2 GiB |
| `maxArchivePathBytes` normalized UTF-8 | 4,096 | 4,096 |
| `maxCapturedOutputBytes` per child stream | 1 MiB | 1 MiB |
| `maxPersistedOutputBytes` per failure stream | 64 KiB | 64 KiB |
| `maxErrorBytes` | 16 KiB | 16 KiB |
| `maxFailureRecords` / `failureRetention` | 10 / `30d` | 10 / `30d` |
| `maxAuditRecords` / `auditRetention` | 100 / `90d` | 100 / `90d` |
| `maxSchedulerLogBytes` per stream | 10 MiB | 10 MiB |
| `maxSchedulerLogFiles` older per stream | 5 | 5 |

Manifest and signature fetches share one overall budget. Connect and read-idle
limits also apply within every overall HTTP budget. `artifact.sizeBytes` must not
exceed the effective compressed-artifact cap. Output over a live capture cap is
drained but not retained. Stored output keeps equal-size exact-byte head and
tail portions, encodes them as unpadded base64url, and records original byte
count and truncation. Persisted failure output is independently reduced to 64
KiB per stream. Human rendering may be lossy, but persisted bytes are exact.

Scheduler intervals have no default: enrollment requires one from `5m` through
`30d`. Each stdout and stderr log rotates independently before an append would
exceed 10 MiB. Checksy retains the active file and five older files per stream,
for six files total. Rotation preserves the ownership and modes below.

Verified retention is current, previous, and one newest additional generation.
Failure retention is at most 10 records and 30 days. Ordinary audit retention is
at most 100 records and 90 days while records identifying current, previous, and
the latest rollback remain protected as defined in the policy contract. Ties
are ordered deterministically by timestamp descending and then stable record ID
ascending; pruning removes the last unprotected record in that order.

## HTTP and archive rules

Every HTTP request sends `Accept-Encoding: identity`; any other content encoding
is rejected. Userinfo and fragments are forbidden. Queries retain their exact
encoding and order. At most five redirects are followed per manifest, sidecar,
or artifact chain. Cross-host redirects are allowed because content trust is
cryptographic, but redirect targets never change source identity. HTTPS-to-HTTP
and HTTP-to-HTTP requests are rejected unless the interactive direct run has
confirmed `--allow-insecure-http`; an HTTP redirect can never be enrolled or
used unattended. Redirects cannot supply trust. Checksy sends no ambient
credentials or cookies on manifest, sidecar, artifact, or redirected requests;
it never forwards them across a redirect.

A `304 Not Modified` is usable only for a conditional request whose stored
validator is bound to the current exact authenticated manifest bytes. It
refreshes online-contact time, does not express new signer intent, and still
runs compliance and eligible fixes. Authentication, checksum, policy, archive,
rollback, equivocation, 401, 403, 404, and local-integrity failures never trigger
offline fallback.

For `tar.gz`, Checksy verifies compressed length and SHA-256 before extraction,
then enforces expanded limits while streaming into a private same-filesystem
staging directory. Exactly one gzip member and one tar stream are accepted;
trailing compressed or tar data is rejected. Only POSIX ustar/PAX directories
and regular files are accepted. Global PAX, GNU sparse data, GNU or unknown
extension records, symlinks, hard links, devices, FIFOs, sockets, and all other
special entries are rejected.

Entry paths must be UTF-8, relative, `/`-separated, component-normalized, within
the path limit, and confined beneath the bundle root. Absolute paths, empty
components, `.`, `..`, backslashes, NUL/control bytes, duplicate normalized
paths, file/directory collisions, and filesystem-equivalent collisions are
rejected. Every tar header counts toward the entry cap, including local PAX
headers. The complete archive is preflighted before any executable staging
content is written. Safe local PAX path and size records may describe the next
directory or regular file; ownership, timestamps, ACLs, xattrs, capabilities,
setuid/setgid/sticky bits, and other metadata are discarded.

Promoted user bundle directories/files are user-owned and mode `0500`/`0400`,
with executable regular files `0500`. Promoted system bundles are root-owned and
mode `0555`/`0444`, with executable regular files `0555`. The executable bit is
the sole preserved permission bit. System bundle ancestors needed by named-user
rules are root-owned `0755`; private trust, policy, state, failure, and audit
directories remain `0700` with mutable files `0600`.

## State, logging, and deletion permissions

User state roots are owned by the enrolled user, use `0700` directories, and use
`0600` mutable files. System state roots and traversable source/generation
ancestors are root-owned (`root:root` Linux, `root:wheel` macOS) and `0755`;
private children are `0700`. State paths and every ancestor are opened without
following symlinks and rejected if owned incorrectly or writable by an
unauthorized principal.

User log directories/files are `0700`/`0600`. System log directories/files are
root-owned `0750`/`0640`. Logs may contain command output and are private but not
tamper-evident against their owner. Bundles MUST NOT contain secrets; Checksy
cannot detect them, and system bundles may be readable by explicitly authorized
named-user rules.

Failed executable staging payloads are deleted. Failure and audit records retain
only bounded diagnostics. `unenroll --delete-state` is the only enrollment
operation that deletes verified source state and that enrollment's scheduler
logs. It never follows symlinks, crosses the resolved enrolled-source directory,
or deletes unrelated sources.

## Errors and exits

Human diagnostics use stable symbolic codes but non-normative wording. Internal
state records phase and whether arbitrary fixes may have partially mutated the
host; public status errors expose only code, message, and retryability.

| Exit | Stable error codes and meaning |
| ---: | --- |
| 0 | Success, compliant, or degraded below threshold |
| 1 | Existing missing-subcommand usage fallback only; explicit help is 0 |
| 2 | `invalid-arguments`, `interactive-required`, `permission-denied`, `unsupported-platform`, `unsupported-schema-version`, `source-unavailable`, `offline-unavailable`, `authentication-failed`, `checksum-mismatch`, `rollback-rejected`, `equivocation`, `manifest-invalid`, `archive-invalid`, `definition-invalid`, `policy-denied`, `state-failed`, `operation-timeout`, `scheduler-failed` |
| 3 | `compliance-failed` |
| 4 | `lock-held`; no attempt started |

No other public error code is valid in schema version 1. The first deterministic
failure in pipeline order supplies the code. Errors before any rule runs set
`partialMutationPossible: false`; after a check or fix begins it is true unless
the runner can prove no command began. Lock contention is advisory contention,
not a stale PID-file decision. Lock open, ownership, or I/O failure is
`state-failed`, exit 2.

Transient source failures, operation timeouts, and lock contention are
retryable. Authentication, checksum, manifest/archive/definition validation,
policy, rollback, equivocation, and state-integrity failures are not retryable.

## Deferred runtime work

This change freezes contracts only. It does not add parsers, state mutation,
network acquisition, signature verification, extraction, process control,
identity changes, scheduler installation, or any of the four new commands.
Those remain P1 through P7 implementation work and MUST use these schemas and
fixtures without weakening the contract. OpenPGP Git signatures, native Windows
execution, shell sandboxing, and transactional rollback of arbitrary host
changes remain explicitly out of scope.

Until those implementations and their deterministic tests land, operators must
continue treating remote definitions as arbitrary unsandboxed shell code and
must not use current Checksy for unattended privileged convergence.
