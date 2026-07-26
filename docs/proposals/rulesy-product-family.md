# Rulesy product-family decision

**Status:** Rulesy rename implemented; adjacent products proposed

**Date:** 2026-07-25

**Release baseline:** Rulesy `0.7.7`

## Decision

Use **Rulesy** as the name for the focused machine provisioner. Define two
adjacent products without expanding the provisioner's responsibility:

| Product | Responsibility | Explicit boundary |
| --- | --- | --- |
| Rulesy | Run trusted configuration on the current machine through the existing check, optional fix, and final-check lifecycle. | No acquisition framework, image builder, firmware updater, enrollment service, package model, rollback engine, or operating-system policy. |
| RulesyOS | Boot a verified, recoverable stage-zero Linux substrate; acquire and authenticate a local Rulesy configuration; run Rulesy on every normal boot; retain configuration generations and firmware recovery state. | Separate repository and executable surfaces. It invokes Rulesy rather than adding OS behavior to Rulesy. |
| Rulesy Compose (`rulesy-compose`) | Build, seal, externally validate, and optionally publish deployable artifacts from pinned bases, ordinary Rulesy configuration, and composition metadata. | Host-side sibling tool. It runs the real Rulesy evaluator and is not installed in the production RulesyOS firmware. |

The full RulesyOS and Rulesy Compose design is recorded in the
[implementation handoff](rulesyos.md).

## Public vocabulary

The clean cutover uses Rulesy consistently:

| Surface | Name |
| --- | --- |
| Product | Rulesy |
| Executable and crate | `rulesy` |
| Default configuration | `.rulesy.yaml`, `.rulesy.yml` |
| Default cache | `.rulesy-cache` |
| Firmware-style sibling | RulesyOS |
| Image-composition sibling | Rulesy Compose / `rulesy-compose` |

Current commands, configuration names, paths, packages, and documentation use
Rulesy without compatibility aliases.

## Rename invariants

The rename changes public names, not the provisioning lifecycle.

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
6. Use Rulesy names consistently across every user-facing path.
7. Keep RulesyOS state, trust, update, and recovery formats outside the Rulesy
   configuration schema.
8. Keep composition variants, artifacts, publishers, provenance, and
   validation metadata outside the Rulesy configuration schema.

## Repository boundaries

This repository remains the focused provisioner repository. RulesyOS starts in
its own repository after an immutable Rulesy revision is available.
Rulesy Compose is a sibling host-side project and may share a workspace with
RulesyOS, but it is not compiled into the firmware and does not become a Rulesy
subcommand.

Cross-repository contracts should be versioned and narrow:

- Rulesy CLI, exit statuses, configuration format, and runtime semantics;
- RulesyOS stage-zero filesystem, environment, status, and signed-bundle
  contracts; and
- Rulesy Compose composition, lockfile, provenance, validation, artifact, and
  publication contracts.

## Adjacent-product approval gates

The rename decision does not authorize the proposed add-ons.

1. Freeze the Rulesy CLI snapshot consumed by RulesyOS.
2. Start the RulesyOS repository at milestone 0 of its
   [implementation handoff](rulesyos.md).
3. Start Rulesy Compose only after the signed-seed, structured-status, and
   check-only validation protocols it consumes are stable.
