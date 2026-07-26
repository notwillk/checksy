# RulesyOS

**Status:** Bootstrap test infrastructure only; product functionality proposed

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

The current implementation is deliberately limited to KVM-only boot testing.
It uses the runtime-test emulator bundled with pinned Buildroot `2025.02.16`
and validates that test path with pinned CirrOS before a blank Buildroot image
becomes the primary target. Neither path contains RulesyOS product
functionality.

Run the current known-good diagnostic with:

```sh
moon run rulesyos:test-known-good
```

The test requires x86-64 KVM. When KVM is unavailable it warns, skips, and
returns success; it never falls back to QEMU software emulation. The pinned
Buildroot source and CirrOS disk, kernel, and initramfs are downloaded into the
ignored project-local `.cache/` directory and verified with SHA-256 before use.
