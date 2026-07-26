# Monorepo conventions

## Ownership

- Put product implementation, fixtures, tests, and detailed documentation
  beneath the owning `products/<name>/` directory.
- Keep family-level decisions and cross-product contracts under `docs/`.
- Keep repository automation at the root only when it genuinely coordinates
  multiple products or preserves an existing public repository path.
- Do not make RulesyOS or Rulesy Compose a Rulesy subcommand.
- Do not share private implementation modules across products. Share a
  versioned contract or released artifact instead.

## Build and release boundaries

- Rulesy owns the Cargo package under `products/rulesy/`.
- Any future Rust implementation in another product owns a separate Cargo
  workspace in that product directory.
- Products version and release independently.
- RulesyOS pins a released Rulesy artifact.
- Rulesy Compose invokes a real pinned Rulesy executable and remains host-side.
- Publication is always explicit; a local build must not silently publish.

## Task runner

Use checked-in Moon tasks through `moon run <project>:<task>` and pass runtime
arguments after `--`. Define tasks in the owning project's `moon.yml`; the
repository root is not a sixth project. Rulesy-managed Rustup and system Rust
remain the sole Rust toolchain authority, so do not add Proto or a
Moon-managed Rust toolchain. Keep dynamic, stateful, destructive, and
validation tasks uncached until their inputs and outputs are modeled
deliberately.

## Documentation

- Use repository-relative Markdown links.
- Link to the owning product document instead of duplicating a detailed
  contract at the root.
- Mark proposed behavior as proposed; do not describe documentation-only
  products as implemented.
- Preserve accepted decisions as history. Add a superseding decision and an
  explicit note instead of rewriting the old record.
- Update `CODEMAP.md`, the owning product README, and affected decisions when a
  path or responsibility changes.

## Changes

- Keep a change within one product when possible.
- When a change crosses products, name the contract being changed and test both
  producer and consumer behavior.
- Avoid speculative shared libraries, dormant schemas, and placeholder runtime
  code.
- Preserve unrelated work in the shared worktree.
- Run product-specific tests plus repository link/path checks before review.

Rulesy-specific Rust and CLI conventions remain in
[`products/rulesy/CONVENTIONS.md`](products/rulesy/CONVENTIONS.md).
