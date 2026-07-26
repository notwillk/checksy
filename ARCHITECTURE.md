# Rulesy family architecture

## Repository model

This monorepo co-locates three products so their contracts can evolve together
without merging their responsibilities:

| Owner | Owns | Must not own |
| --- | --- | --- |
| Rulesy | Trusted configuration parsing, checks, optional fixes, final checks, reporting, and the public CLI contract | Configuration acquisition, image construction, firmware state, publication, or rollback |
| RulesyOS | Verified stage zero, authenticated configuration selection, boot-time Rulesy invocation, persistent generations, firmware update/recovery, and OS status | A second rule evaluator, host artifact composition, or publisher credentials |
| Rulesy Compose | Host-side schemas, variants, builders, artifact validation, sealing, provenance, and explicit publishers | Firmware runtime behavior, a second Rulesy implementation, or implicit publication |

Each product has independent responsibilities, versioning, acceptance gates,
and releases. Repository proximity does not authorize one product to reach
through another product's private implementation.

## Stable cross-product contracts

The family shares only narrow, versioned surfaces:

- Rulesy's released executable, CLI, configuration schema, exit statuses, and
  structured/human reporting;
- RulesyOS signed-bundle, stage-zero environment, filesystem, status, firmware
  update, and recovery contracts; and
- Rulesy Compose composition document, lockfile, variant, artifact, publisher,
  validation, provenance, and attestation contracts.

Rulesy remains the only evaluator. RulesyOS must pin and package a released
Rulesy artifact rather than linking to workspace source or treating an
unreleased checkout as its production dependency. Rulesy Compose must resolve
and invoke a real pinned Rulesy executable for convergence and check-only
validation. A mock evaluator may support unit tests, but it cannot satisfy
end-to-end acceptance.

Compose is host-side tooling. Its binary, SDKs, cloud credentials, caches, and
publisher implementations must never enter the production RulesyOS root
filesystem.

## Workspace and build boundaries

The repository root is orchestration and documentation, not a shared
application package. Rulesy owns its Rust package and Cargo workspace under
`products/rulesy/`. Future Rust implementations for RulesyOS or Rulesy Compose
must own separate Cargo workspaces in their product directories unless a later
decision explicitly changes that boundary. They must not become members of
Rulesy's Cargo workspace merely because they share this repository.

Repository task orchestration may coordinate products, but product builds and
release artifacts remain independently addressable. Moon `2.4.5` coordinates
tasks through the explicit five-project map in `.moon/workspace.yml`; each
project owns its task definitions in its local `moon.yml`. Rulesy retains its
independent Cargo workspace and product-owned release helpers.

## Dependency direction

```text
Rulesy release
   ├── pinned by RulesyOS firmware builds
   └── invoked by Rulesy Compose validation

RulesyOS release
   └── pinned as an optional Rulesy Compose base artifact

Rulesy Compose
   └── produces artifacts; never becomes firmware runtime
```

Rulesy does not depend on either adjacent product. RulesyOS does not depend on
Compose at runtime. Compose may consume released Rulesy and RulesyOS artifacts,
but never their mutable workspace outputs as a production trust boundary.

## Documentation ownership

- Family boundaries and accepted decisions live in this file and
  [`docs/decisions/`](docs/decisions/).
- Rulesy's detailed runtime architecture lives in
  [`products/rulesy/ARCHITECTURE.md`](products/rulesy/ARCHITECTURE.md).
- RulesyOS owns its [design](products/rulesyos/docs/design.md).
- Rulesy Compose owns its [design](products/rulesy-compose/docs/design.md).

The original product-family decision remains historical. Its former
separate-repository recommendation is explicitly superseded by the monorepo
decision; its behavioral boundaries remain in force.
