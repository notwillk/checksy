# Proposed product direction

Documents in this directory describe product work that has been designed but
is not implemented by the current repository.

The released product remains Checksy. Its current command names, configuration
format, paths, packages, and runtime behavior remain authoritative in the root
[README](../../README.md) and [architecture](../../ARCHITECTURE.md).

## Proposals

- [Rulesy product family](rulesy-product-family.md) — proposes renaming Checksy
  to Rulesy and defines the boundaries between Rulesy, RulesyOS, and Rulesy
  Compose.
- [RulesyOS product requirements and implementation design](rulesyos.md) —
  defines a firmware-style Linux substrate and the sibling `rulesy-compose`
  image-composition workflow.

Neither proposal authorizes implementation. Each requires its own explicitly
approved, independently reviewable milestone.
