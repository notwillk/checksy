# Rulesy family agent context

This repository is a monorepo for three separately released products.

## Read first

1. [`README.md`](README.md)
2. [`ARCHITECTURE.md`](ARCHITECTURE.md)
3. [`CODEMAP.md`](CODEMAP.md)
4. [`docs/decisions/monorepo.md`](docs/decisions/monorepo.md)
5. The README, architecture/design, and conventions for the product being
   changed

## Product boundaries

- **Rulesy** is the implemented Rust CLI. It provisions the current machine
  from trusted local configuration or stdin through `check` and optional
  `check --fix`.
- **RulesyOS** implements the first x86-64 baked-configuration reference image:
  a released Rulesy binary, permanent Rust stage zero, persistent state and
  status, and two-boot KVM acceptance. It owns Buildroot, boot, authenticated
  configuration, firmware update, recovery, and hardening; those production
  trust and lifecycle features remain deferred.
- **Rulesy Compose** is documentation-only. It owns host-side composition
  schemas, variants, artifact generation, external validation, provenance, and
  explicit publishers.

Rulesy is the only evaluator. RulesyOS pins a released Rulesy binary. Compose
invokes a real pinned Rulesy binary and never enters production firmware.

## Repository constraints

- Keep product implementations and detailed docs in `products/<product>/`.
- Keep independent Cargo workspaces and releases.
- Treat root files as family orchestration or stable public compatibility
  paths.
- Do not infer implementation authorization from a design document.
- Do not add a shared abstraction until at least two implemented consumers
  require the same stable contract.
- Use the checked-in Moon tasks for repository orchestration and preserve their
  project boundaries and behavior.
- Keep Rulesy-managed Rustup and system Rust as the sole Rust toolchain
  authority; do not introduce Proto.

For Rulesy implementation work, continue with
[`products/rulesy/PROMPT.md`](products/rulesy/PROMPT.md).
