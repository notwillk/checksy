# Rulesy Compose product requirements and implementation design

**Status:** Proposed implementation handoff; not implemented

**Date:** 2026-07-26

**Owner:** Rulesy Compose

Rulesy Compose (`rulesy-compose`) is a host-side CLI that turns pinned base
artifacts, an authenticated Rulesy configuration bundle, and explicit
composition metadata into validated deployable artifacts. This document owns
schemas, the host CLI, variants, artifacts, publishers, validation, tests,
milestones, and risks. Firmware runtime behavior belongs to the separate
[RulesyOS design](../../rulesyos/docs/design.md).

## 1. Product boundary

Compose orchestrates builds; it does not interpret Rulesy configuration.
Actual convergence and compliance validation are performed by a real pinned
Rulesy executable in isolated disposable environments.

Compose owns:

- strict composition and lockfile schemas;
- input resolution and digest pinning;
- build plans and resource-bounded execution;
- variant, artifact, and publisher adapters;
- external inspection and black-box runtime validation;
- sealing, provenance, SBOM coverage, and attestations; and
- explicit publication.

Compose does not:

- become a Rulesy subcommand;
- reimplement checks, fixes, severity, includes, or shell execution;
- run inside production RulesyOS firmware;
- copy its binary, cloud SDKs, credentials, or build cache into firmware;
- silently publish as a side effect of a local build;
- promise reproducibility without measured evidence; or
- claim that successful composition makes arbitrary configuration safe.

Rulesy, RulesyOS, and Compose release independently. Compose owns a Cargo
workspace under `products/rulesy-compose/` when implemented; it does not join
Rulesy's Cargo workspace.

## 2. Contract-freeze gates

Before implementation:

1. **Rulesy pin:** define how a composition resolves an exact released Rulesy
   version, artifact digest, target, and signature.
2. **Configuration closure:** define an authenticated bundle whose root
   configuration and every include, pattern, script, template, and declared
   asset are confined to and covered by the bundle.
3. **Validation result:** bind the documented Rulesy version, overall exit,
   bounded output, and environment to the report. Per-rule structured claims
   require an explicit Rulesy interface; Compose must not scrape prose.
4. **RulesyOS base:** define the immutable base digest, profile, firmware
   contract version, and guest status protocol used by the machine variant.
5. **Artifact identity:** specify when the digest is measured, what bytes it
   covers, and which later packaging/publication steps may change identity.
6. **Credential boundary:** local `build` and `validate` are credential-free.
   Publisher credentials enter only the explicit `publish` process.

## 3. Primary use cases

The MVP creates a service-specific raw/QCOW2 machine image:

1. resolve a released RulesyOS base and released Rulesy executable;
2. verify every input and record immutable digests;
3. attach the signed configuration bundle through RulesyOS's normal seed path;
4. boot a disposable working copy through normal stage zero;
5. let stage zero invoke the pinned Rulesy to convergence;
6. stop and seal the working image;
7. inspect the sealed bytes from the host;
8. validate a disposable copy with real Rulesy check-only;
9. boot a deployment-faithful clone without validation scaffolding;
10. emit artifacts and digest-bound evidence; and
11. optionally publish through a separate explicit command.

Later adapters may create OCI artifacts or publish cloud images, but they are
not prerequisites for the first machine-image release.

## 4. Composition schema

The input document is strict, versioned, and closed to unknown fields. A
representative shape:

```yaml
apiVersion: compose.rulesy.dev/v1alpha1
kind: Composition
metadata:
  name: example-service

rulesy:
  version: 0.8.1
  source:
    artifact: gh-release:notwillk/rulesy@v0.8.1
    digest: sha256:...
  config:
    bundle: ./service-config.tar
    digest: sha256:...
    signature: ./service-config.tar.sig

base:
  kind: rulesyos
  version: 0.1.0
  profile: qemu-x86_64-generic
  artifact: ./rulesyos.raw
  digest: sha256:...

variant:
  kind: rulesyos-machine
  resources:
    cpus: 2
    memory: 2GiB
    disk: 8GiB
    timeout: 15m

artifacts:
  - kind: raw
    path: dist/example.raw
  - kind: qcow2
    path: dist/example.qcow2

validation:
  rulesyCheckOnly: true
  bootProbe:
    timeout: 2m
  inspect:
    forbidUnexpectedSetuid: true
    forbidPrivateKeys: true

provenance:
  sbom: true
  attest: true

publishers: []
```

The schema separates:

- semantic inputs (`rulesy`, configuration, base, variant);
- output encodings (`artifacts`);
- evidence requirements (`validation`, `provenance`); and
- external mutations (`publishers`).

Paths are resolved relative to the composition document. Secrets are never
inline schema values. References to credentials identify an external provider
or process boundary and are redacted from plans and reports.

### Configuration bundle declaration

The Rulesy bundle declaration identifies:

- root configuration path;
- exact bundle digest and detached signature;
- allowed signer/key identity;
- complete normalized member list or manifest root;
- byte, file-count, path-depth, and expansion limits; and
- declared non-YAML assets used by configuration.

Compose must not guess opaque dependencies from Bash. The bundle producer is
responsible for declaring closure; Compose verifies the declaration and then
real Rulesy validates behavior.

## 5. Lockfile

`rulesy-compose lock` resolves mutable references into an immutable lockfile.
The lock records:

- composition schema version and normalized digest;
- Compose version;
- Rulesy version, platform artifact, digest, and signature identity;
- base version, profile, artifact digest, and trust metadata;
- configuration bundle and signature digests;
- variant, artifact, validator, and publisher adapter versions;
- builder image/toolchain digests;
- external module or package inputs;
- output format parameters; and
- policy identifiers used by validation.

Lockfiles contain no credentials. A release build requires a complete lock and
fails if resolution would mutate it. `--update` is an explicit lock operation,
not an implicit build side effect.

## 6. Host CLI

The initial command surface:

```text
rulesy-compose schema
rulesy-compose lock <composition>
rulesy-compose plan <composition>
rulesy-compose build <composition>
rulesy-compose validate <artifact-or-report>
rulesy-compose inspect <artifact>
rulesy-compose publish <report> --publisher <name>
```

Requirements:

- `schema` emits the exact supported composition schema.
- `lock` resolves and verifies immutable inputs without building.
- `plan` is side-effect free and reports redacted resolved steps/resources.
- `build` produces local artifacts and evidence without cloud credentials.
- `validate` re-runs allowed external checks against existing bytes.
- `inspect` never boots or mutates the selected master artifact.
- `publish` is the only command authorized to create external provider
  resources.

Every command supports machine-readable output with a versioned envelope and
stable exit classes. Human output is not an API.

## 7. Planning and build lifecycle

The core builds a deterministic action graph from the composition and lock:

1. parse strict input;
2. resolve local paths and select the complete lock;
3. verify signatures and digests;
4. allocate a fresh bounded work directory;
5. copy or snapshot the base into a disposable working artifact;
6. run the selected variant;
7. wait for explicit convergence/status evidence;
8. stop the guest/container cleanly or fail closed after bounded cleanup;
9. seal the working artifact;
10. create each final-format artifact from the sealed source;
11. validate each final format independently;
12. compute final digests;
13. create SBOM/provenance/attestation material; and
14. atomically move completed outputs into their destination.

The sealed master is never used directly for destructive validation.
Validation uses disposable reflinks, snapshots, or full copies. If the host
cannot provide a safe copy primitive, Compose fails rather than mutating the
claimed source.

Resource controls include wall-clock time, virtual disk growth, process count,
memory, CPU, output bytes, downloaded bytes, and work-directory size. Cleanup
must run after interruption and preserve only explicitly requested diagnostic
state.

## 8. Adapter model

Keep three narrow interfaces:

- **Variant:** how configuration convergence occurs in a disposable
  environment.
- **Artifact:** how sealed state is encoded and inspected.
- **Publisher:** how a validated artifact is registered or transferred to an
  external system.

Adapters consume immutable typed inputs and return digest-bound typed evidence.
They do not parse Rulesy YAML or call each other through shell strings.

### Variant: `rulesyos-machine`

The MVP variant:

- consumes a released RulesyOS machine base;
- attaches a signed seed through the normal documented boot path;
- boots normal `rulesyos-stage0` under QEMU;
- confirms the guest's pinned Rulesy version/digest;
- waits for the versioned status protocol;
- requires successful real Rulesy convergence;
- shuts down through the guest protocol; and
- removes transient boot/status data that the sealing policy excludes.

It must not inject a bypass init, replace stage zero, mount the root to edit it
directly, or report success based only on QEMU exit status.

### Variant: OCI

A later OCI variant must explicitly define unavailable machine/kernel
semantics. It converges a disposable root with real Rulesy, maps allowed
persistent payload state into an OCI root filesystem, validates both layers and
the merged view, and runs a deployment-equivalent container.

It cannot claim equivalence to RulesyOS verified boot, firmware update, block
devices, kernel settings, or stage-zero status.

### Artifact adapters

The first artifact adapters are:

- `raw`: exact standalone disk bytes;
- `qcow2`: standalone image with no hidden backing file; and
- validation report/provenance bundles.

Every final format is independently inspected and boot-tested. Success of the
raw source does not prove a converted QCOW2 artifact is valid.

Later adapters may include OCI archive/layout. Each adapter defines content
identity, conversion tools/digests, sparse-file behavior, metadata
normalization, and inspection coverage.

### Publisher adapters

Publishers are explicit external mutations and accept only a completed,
validated report whose artifact digest still matches local bytes.

The initial cloud publisher may target AWS through the no-modification
`ImportSnapshot` then `RegisterImage` route. It records staging object,
snapshot, image, and region identities; applies tags from explicit metadata;
and cleans partial resources on failure.

Publisher results distinguish:

- published from a validated local source;
- provider accepted/registered;
- provider-side isolated boot/probe passed; and
- provider-side bytes are or are not independently digest-verifiable.

Do not claim the registered AMI itself was validated when only its local source
was.

## 9. External validation

Validation has three layers.

### Rulesy compliance

Boot or start a disposable validation copy and invoke the exact pinned Rulesy
binary in check-only, non-interactive mode against the retained authenticated
configuration. Bind the overall exit, bounded output, binary digest,
configuration digest, environment, and guest/container status to the report.

A failed check blocks artifact success. Compose never downgrades severity or
reinterprets a rule.

### Artifact inspection

Inspect final bytes without trusting the build environment. Depending on
format, verify:

- partition/filesystem structure and size;
- expected boot artifacts and firmware metadata;
- no hidden backing files;
- file ownership, modes, capabilities, and setuid/setgid inventory;
- absence of development keys, credentials, validation scaffolding, and host
  paths;
- expected Rulesy binary/version;
- package/file inventory and SBOM correspondence;
- bootloader configuration and root integrity metadata; and
- no unexpected writable stage-zero content.

Mount or parse artifacts read-only. Use isolated helper processes and reject
host-device/path traversal.

### Black-box runtime validation

Boot a deployment-faithful clone with no Compose-only channel and assert:

- normal RulesyOS stage zero runs;
- retained configuration authenticates;
- real Rulesy check-only succeeds;
- the configured foreground payload hook starts;
- no unexpected listener or debug console exists; and
- shutdown and subsequent reboot preserve intended state.

Validation evidence is digest-bound. Any byte change after validation invalidates
the report and requires revalidation.

## 10. Sealing

Sealing is profile-specific and explicit. A machine-image profile may:

- remove temporary acquisition material and one-boot seed media;
- clear host-specific machine IDs, DHCP leases, random seeds, boot IDs, and
  transient logs when appropriate for cloning;
- retain the authenticated active configuration and project state required at
  deployment;
- normalize explicitly permitted metadata;
- flush filesystems and shut down cleanly; and
- ensure all final output formats are standalone.

Sealing must not remove state required by the normal RulesyOS trust or boot
path. It must never make the artifact appear unprovisioned if the declared
variant requires retained convergence.

Profiles state whether artifacts are:

- clone-ready templates;
- machine-bound instances;
- factory images awaiting first-boot identity; or
- update payloads.

## 11. Outputs, provenance, and SBOM

Each successful build emits:

- selected final artifacts and SHA-256 digests;
- immutable lockfile;
- normalized composition digest;
- build and validation reports;
- exact Rulesy and base versions/digests;
- tool, adapter, and builder-image versions/digests;
- captured bounded logs;
- SBOM with a declared coverage boundary;
- license material where available;
- provenance statement; and
- optional detached signature or attestation created only after validation.

The report states which evidence covers:

- base firmware packages;
- Rulesy and stage-zero binaries;
- configuration bundle members;
- project artifacts installed by opaque Bash;
- generated filesystem contents; and
- provider-side resources.

Do not imply full SBOM coverage for opaque downloaded/build outputs that were
not observed or declared.

### Reproducibility terminology

Compose is an orchestrator, not proof of deterministic compilation. Reports
distinguish:

- input-pinned;
- rebuild compared;
- bit-for-bit reproducible;
- functionally equivalent under named tests; and
- not assessed.

Never call an artifact reproducible solely because all input references were
locked.

## 12. Security boundary

Rulesy configuration is trusted arbitrary code. Compose assumes the bundle
signer is authorized to mutate the disposable build environment. Isolation
protects the host from accidents and limits authority; it does not make the
configuration untrusted-safe without a separately defined sandbox.

Requirements:

- use unprivileged or tightly scoped helpers where possible;
- isolate network access by phase and default it off during validation;
- never mount host secrets or Docker sockets into the convergence guest;
- pass publisher credentials only to the publisher process;
- redact URLs, tokens, environment secrets, and cloud responses;
- validate all archive and path inputs before extraction;
- bind reports to input and output digests;
- sign/attest only after all required validation; and
- clean interrupted external resources deterministically.

Threats include hostile archives, path traversal, oversized images, crafted
filesystems, QEMU/helper escape, credential leakage, validation/build
disagreement, post-validation mutation, and publisher partial failure.

## 13. Tests

### Schema and planning

- reject unknown fields, duplicate keys, unsupported versions, nulls, and
  ambiguous adapter selections;
- resolve paths relative to the document;
- freeze mutable references in the lock;
- prove release mode never mutates a lock;
- redact secrets from plan and machine-readable output; and
- property/fuzz test sizes, paths, archives, and normalized identities.

### Adapter contracts

- each adapter accepts/returns the typed core contract;
- no adapter interprets Rulesy YAML;
- declared resource bounds are enforced;
- interruption cleans child processes and work state;
- raw/QCOW2 outputs are standalone;
- artifact inspection is read-only; and
- publishers reject stale or unvalidated reports.

### Machine-image end to end

1. Build from a released pinned RulesyOS base.
2. Boot through normal `rulesyos-stage0`.
3. Prove real pinned Rulesy performs convergence.
4. Prove a failed Rulesy check blocks success.
5. Seal without bypassing the guest trust path.
6. Validate a disposable copy with real Rulesy check-only.
7. Boot a clean deployment-faithful clone without Compose scaffolding.
8. Verify the payload hook and reboot behavior.
9. Inspect final raw and QCOW2 bytes independently.
10. Bind reports to final digests.
11. Detect a one-byte post-validation change.
12. Prove no Compose binary, cloud SDK, credentials, or private key entered
    firmware.

### OCI end to end

- validate each layer and merged root;
- run a deployment-equivalent container;
- state unavailable machine semantics;
- verify runtime user, entrypoint, mounts, and capabilities; and
- document SBOM coverage.

### Publisher tests

- build and validate without any cloud credentials;
- create external state only under `publish`;
- use fixture servers or provider mocks for deterministic failures;
- clean staging resources on every failure point;
- record provider identities and region/account without secrets;
- distinguish source validation from provider-side validation; and
- run a quarantined integration account for opt-in provider acceptance.

## 14. Acceptance criteria

The machine-image MVP is complete when:

- strict composition and lock schemas are published;
- every input resolves to a verified immutable digest;
- a normal RulesyOS boot invokes the real pinned Rulesy;
- raw and QCOW2 artifacts each pass inspection and deployment-faithful boot;
- validation never mutates the sealed master;
- reports bind inputs, outputs, Rulesy evidence, and adapter versions;
- local build needs no publisher credential;
- signature/attestation occurs only after validation; and
- isolation, hostile-input, resource-limit, secret-canary, and interruption
  tests pass.

OCI and AWS are independent later acceptance gates. They do not block the first
useful machine-image release.

## 15. Milestones

1. **Contract freeze:** Rulesy pin, bundle closure, result envelope, base/status
   protocol, artifact identity, and credentials.
2. **Core schemas and planner:** strict composition, lock, plan, reporting, and
   typed adapter contracts.
3. **RulesyOS machine variant:** disposable QEMU convergence through normal
   stage zero using real pinned Rulesy.
4. **Machine artifacts and validation:** raw/QCOW2, sealing, read-only
   inspection, check-only validation, deployment-faithful boot, and evidence.
5. **Provenance and release:** SBOM coverage, attestations, signed outputs, and
   independent Compose release.
6. **OCI adapter:** explicit container semantics and validation.
7. **AWS publisher:** no-modification import/register route, cleanup, and
   provider-side probes.

Compose implementation may begin only after the RulesyOS QEMU base, signed seed
path, and guest status protocol it consumes are stable. It is not required for
the first bootable RulesyOS release.

## 16. Principal risks

- **Second evaluator:** parsing Rulesy YAML or synthesizing outcomes would fork
  semantics. Always invoke real Rulesy.
- **Firmware contamination:** host CLI, SDKs, caches, and credentials must
  never enter the target root.
- **Trust-path bypass:** direct offline edits can produce an artifact that
  never exercised normal RulesyOS stage zero.
- **Validation mutation:** validation must use disposable copies and verify the
  sealed digest afterward.
- **Incomplete closure:** opaque Bash can depend on undeclared assets or
  downloads; reports must state coverage honestly.
- **Reproducibility overclaim:** pinned inputs are evidence, not bit-for-bit
  proof.
- **Format drift:** raw, QCOW2, OCI, and cloud resources have different
  identity and inspection boundaries.
- **Credential leakage:** publisher authority must be process- and phase-local.
- **Partial publication:** every external resource needs recorded identity and
  cleanup policy.
- **Host escape:** QEMU, filesystem parsers, conversion tools, and hostile
  images require isolation and patch management.
- **Status overinterpretation:** overall Rulesy exit does not authorize
  invented per-rule structured claims.

## 17. Implementation rule

Start with the strict schema/lock core and the QEMU machine variant. Keep all
provider SDKs behind publisher adapters. Do not implement OCI or AWS until raw
and QCOW2 artifacts pass real Rulesy convergence, external inspection, and
deployment-faithful validation end to end.
