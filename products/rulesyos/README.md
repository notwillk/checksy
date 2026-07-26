# RulesyOS

**Status:** Proposed; documentation only

RulesyOS is a firmware-style Linux substrate that boots a verified, recoverable
stage zero and invokes a released Rulesy binary against authenticated
configuration on every normal boot.

RulesyOS owns:

- Buildroot integration and board profiles;
- verified boot and an immutable root;
- configuration acquisition, authentication, and generation state;
- boot-time Rulesy invocation;
- firmware update, fallback, and recovery;
- local status, bounded logs, and OS hardening.

It does not own Rulesy configuration semantics or host-side artifact
composition. Rulesy remains the only evaluator. RulesyOS must pin an immutable
released Rulesy artifact rather than use a mutable workspace build in
production.

The complete proposal is in the [RulesyOS design](docs/design.md). Family
boundaries are recorded in the [root architecture](../../ARCHITECTURE.md) and
[monorepo decision](../../docs/decisions/monorepo.md). Artifact composition is
specified separately by [Rulesy Compose](../rulesy-compose/README.md).

No implementation milestone is authorized merely by this directory's
existence. This documentation-only project intentionally has no build, test,
format, or release tasks yet.
