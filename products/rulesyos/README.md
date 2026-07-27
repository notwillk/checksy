# RulesyOS

**Status:** First baked-configuration functional reference image implemented;
production trust, updates, and external configuration remain proposed

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

The production design is in the [RulesyOS design](docs/design.md). Family
boundaries are recorded in the [root architecture](../../ARCHITECTURE.md) and
[monorepo decision](../../docs/decisions/monorepo.md). Artifact composition is
specified separately by [Rulesy Compose](../rulesy-compose/README.md).

The current x86-64 reference image uses Buildroot `2025.02.16`, Linux
`6.12.27`, the glibc target toolchain, released Rulesy `0.8.3`, and the
independent Rust `rulesyos-stage0` `0.1.0` workspace. The
[Rulesy release lock](rulesy-release.lock) pins and verifies the published
archive and extracted binary rather than using a workspace build.

The image boots its kernel directly, mounts its root read-only, and provides a
separate 64 MiB ext4 state partition in `rulesyos.img`. BusyBox init disables
root login and the serial getty, prepares tmpfs-backed runtime storage, and
runs stage zero once per boot. Stage zero validates the platform and pinned
Rulesy binary, materializes the baked configuration, invokes Rulesy, and
durably records schema-v1 status plus at most eight root-only per-boot logs.
The KVM acceptance test boots one disk twice, proving that the first boot
creates the baked state marker and the second boot runs Rulesy without
repeating the already-satisfied fix.

UEFI, a bootloader, dm-verity, signed external bundles, configuration
generations, payload handoff, production hardening, firmware updates,
recovery, RulesyOS releases, and Compose integration remain deferred.

Run the project checks and reference-image tests with:

```sh
moon run rulesyos:format
moon run rulesyos:lint
moon run rulesyos:build
moon run rulesyos:test
moon run rulesyos:test-known-good
```

The image build and KVM tasks are local-only. VM tests require x86-64 KVM;
when KVM is unavailable they warn, skip, and return success, never falling
back to QEMU software emulation. `test-known-good` retains pinned CirrOS as an
independent diagnostic of the runtime-test path.

Pinned Buildroot and CirrOS inputs are downloaded into the ignored
project-local `.cache/` directory and verified with SHA-256 before use.
Buildroot work and the generated `bzImage`, `rootfs.ext2`, and `rulesyos.img`
remain in the ignored project-local `output/` directory.
