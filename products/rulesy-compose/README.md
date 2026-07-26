# Rulesy Compose

**Status:** Proposed; documentation only

Rulesy Compose (`rulesy-compose`) is a host-side CLI for building, converging,
validating, sealing, and explicitly publishing deployable artifacts from
pinned inputs.

Compose owns:

- versioned composition and lockfile schemas;
- host CLI planning and execution;
- variants, artifact adapters, and publisher adapters;
- external artifact inspection and black-box validation;
- provenance, SBOM coverage, signatures, and attestations.

Compose must invoke a real pinned Rulesy executable; it must never reimplement
Rulesy's evaluator. Compose is not part of production RulesyOS firmware, and
its SDKs, credentials, caches, or publisher code must never be copied into the
firmware root.

The complete proposal is in the
[Rulesy Compose design](docs/design.md). Family boundaries are recorded in the
[root architecture](../../ARCHITECTURE.md) and
[monorepo decision](../../docs/decisions/monorepo.md). The firmware runtime it
may consume is specified separately by [RulesyOS](../rulesyos/README.md).

No implementation milestone is authorized merely by this directory's
existence. This documentation-only project intentionally has no build, test,
format, or release tasks yet.
