# Monorepo organization decision

**Status:** Accepted

**Date:** 2026-07-26

**Supersedes:** Repository-placement clauses in
[Rulesy product-family decision](rulesy-product-family.md)

## Decision

Co-locate Rulesy, RulesyOS, and Rulesy Compose in one repository under
`products/`, while preserving their independent product boundaries.

| Product | Repository path | Release boundary |
| --- | --- | --- |
| Rulesy | `products/rulesy/` | Independent CLI/crate and release artifacts |
| RulesyOS | `products/rulesyos/` | Independent firmware version and artifacts |
| Rulesy Compose | `products/rulesy-compose/` | Independent host CLI and artifacts |

The root owns family documentation, cross-product decisions, task
orchestration, development-container integration, and stable compatibility
paths that genuinely span the repository.

## Required boundaries

1. Each product owns its implementation, tests, detailed documentation,
   versioning, and release acceptance.
2. Rulesy's Cargo package/workspace stays under `products/rulesy/`.
3. Future Rust implementations for RulesyOS and Rulesy Compose use independent
   Cargo workspaces in their own product directories unless a later decision
   explicitly approves another layout.
4. Rulesy remains the only evaluator of Rulesy configuration.
5. RulesyOS pins a released Rulesy artifact. Production firmware must not
   depend on mutable workspace output or an unreleased Rulesy checkout.
6. Rulesy Compose invokes a real, pinned Rulesy executable for convergence and
   check-only validation. It must not reimplement rule semantics.
7. Rulesy Compose is host-side tooling. Its executable, build SDKs, cloud
   credentials, caches, and publisher adapters never enter production
   RulesyOS firmware.
8. Products release independently. A repository commit may change more than
   one product, but that does not force a shared version or release train.
9. Shared behavior crosses product boundaries only through explicit,
   versioned contracts or released artifacts.

## Dependency direction

Rulesy does not depend on either adjacent product. RulesyOS consumes Rulesy.
Rulesy Compose consumes Rulesy and may consume a released RulesyOS base.
RulesyOS does not require Compose at runtime.

## Consequences

- Cross-product contract changes can be reviewed and tested atomically.
- Product source remains discoverable without pretending the products share
  one runtime or package graph.
- Root documentation must remain concise and route detail to the owning
  product.
- CI may coordinate products, but build and release jobs must preserve
  independently addressable product results.
- The original family decision remains useful history; only its physical
  repository-placement clause is superseded.
