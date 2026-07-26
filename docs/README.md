# Product-family documentation

This directory records cross-product decisions for the Rulesy monorepo.
Product-specific runtime and design documentation lives with its owner under
[`products/`](../products/).

## Documents

- [Rulesy product family](decisions/rulesy-product-family.md) — historical
  rename and product-boundary decision.
- [Monorepo organization](decisions/monorepo.md) — supersedes the former
  separate-repository placement while preserving independent ownership and
  releases.
- [RulesyOS design](../products/rulesyos/docs/design.md) — verified stage zero,
  Buildroot, boot, state, recovery, and hardening.
- [Rulesy Compose design](../products/rulesy-compose/docs/design.md) — host-side
  schemas, variants, artifacts, validation, provenance, and publishers.

RulesyOS has an implemented baked-configuration reference image; its production
trust, update, and recovery milestones remain deferred. Rulesy Compose remains
an unimplemented proposal. Each further milestone requires its own explicitly
approved, independently reviewable slice.
