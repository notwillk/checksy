# Rulesy product-family proposal

**Status:** Proposed; not implemented

**Date:** 2026-07-25

**Current released product:** Checksy `0.7.7`

## Decision

Rename Checksy to **Rulesy** and use that name for the focused machine
provisioner. Introduce two adjacent products without expanding the
provisioner's responsibility:

| Product | Responsibility | Explicit boundary |
| --- | --- | --- |
| Rulesy | Run trusted configuration on the current machine through the existing check, optional fix, and final-check lifecycle. | No acquisition framework, image builder, firmware updater, enrollment service, package model, rollback engine, or operating-system policy. |
| RulesyOS | Boot a verified, recoverable stage-zero Linux substrate; acquire and authenticate a local Rulesy configuration; run Rulesy on every normal boot; retain configuration generations and firmware recovery state. | Separate repository and executable surfaces. It invokes Rulesy rather than adding OS behavior to Rulesy. |
| Rulesy Compose (`rulesy-compose`) | Build, seal, externally validate, and optionally publish deployable artifacts from pinned bases, ordinary Rulesy configuration, and composition metadata. | Host-side sibling tool. It runs the real Rulesy evaluator and is not installed in the production RulesyOS firmware. |

The full RulesyOS and Rulesy Compose design is recorded in the
[implementation handoff](rulesyos.md).

## Current versus proposed names

The rename has not happened. Until a dedicated rename milestone lands:

- the executable and Rust crate are `checksy`;
- configuration discovery uses `.checksy.yaml` and `.checksy.yml`;
- documentation examples invoke `checksy`;
- installer, archive, OCI Feature, repository, and release coordinates retain
  their Checksy names;
- the provisioning-lock namespace retains its Checksy path; and
- current public Rust types and CLI compatibility promises remain unchanged.

The proposed target vocabulary is:

| Current | Proposed target |
| --- | --- |
| Checksy | Rulesy |
| `checksy` | `rulesy` |
| ChecksyOS (former working name) | RulesyOS |
| image-composition sibling | Rulesy Compose / `rulesy-compose` |

Documentation may use “Rulesy, formerly Checksy” when referring to the future
product family. It must not present proposed command names as currently
available.

## Rename invariants

The rename is a compatibility migration, not an opportunity to redesign the
provisioning lifecycle.

1. Preserve `check`, `check --fix`, `--non-interactive`, local configuration,
   stdin configuration, strict decoding, severity behavior, `skip-if`,
   `interactive-fix`, process supervision, configuration-relative execution,
   and provisioning-lock semantics.
2. Do not add a parallel `apply` command, daemon, scheduler, source provider,
   state database, image builder, or OS abstraction to Rulesy.
3. Keep configuration acquisition and authentication external to Rulesy.
4. Keep trusted configuration as arbitrary Bash with the invoking identity's
   authority.
5. Do not claim transactional rollback of arbitrary fixes.
6. Do not publish a release containing an ambiguous mixture of user-facing
   `checksy` and `rulesy` paths.
7. Keep RulesyOS state, trust, update, and recovery formats outside the Rulesy
   configuration schema.
8. Keep composition variants, artifacts, publishers, provenance, and
   validation metadata outside the Rulesy configuration schema.

## Required rename plan

Before implementation, freeze one migration plan covering all externally
observable names together:

- repository and package naming;
- executable and Rust crate naming;
- one release-version authority shared by tags, CLI output, Cargo metadata,
  installers, and packaged artifacts;
- help/version output;
- installer and uninstaller URLs;
- release archive and checksum names;
- OCI Feature coordinates and options;
- default configuration filenames and discovery order;
- environment variables, generated schema identifiers, and documentation
  examples;
- user and root provisioning-lock paths;
- development-container bootstrap references;
- compatibility aliases, warnings, and their removal release;
- Git-acquisition deprecation sequencing; and
- downstream RulesyOS Buildroot package pins.

The provisioning-lock migration requires special care. Old and new executables
must not acquire different files and therefore run concurrent provisioning
operations during a compatibility window.

The plan must state whether `.checksy.yaml`, the `checksy` executable, and
existing package coordinates receive temporary aliases, how long those aliases
remain, and how conflicts are diagnosed. Tests must cover mixed-version
invocations and prove that aliases preserve one provisioning semaphore.

## Repository boundaries

This repository remains the focused provisioner repository through the rename.
RulesyOS starts in its own repository after the rename is complete or pinned.
Rulesy Compose is a sibling host-side project and may share a workspace with
RulesyOS, but it is not compiled into the firmware and does not become a Rulesy
subcommand.

Cross-repository contracts should be versioned and narrow:

- Rulesy CLI, exit statuses, configuration format, and runtime semantics;
- RulesyOS stage-zero filesystem, environment, status, and signed-bundle
  contracts; and
- Rulesy Compose composition, lockfile, provenance, validation, artifact, and
  publication contracts.

## Approval gates

No rename or add-on implementation begins from this document alone.

1. Approve the complete rename and compatibility plan.
2. Implement and release the rename as a focused vertical slice.
3. Freeze the Rulesy CLI snapshot consumed by RulesyOS.
4. Start the RulesyOS repository at milestone 0 of its
   [implementation handoff](rulesyos.md).
5. Start Rulesy Compose only after the signed-seed, structured-status, and
   check-only validation protocols it consumes are stable.
