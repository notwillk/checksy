# Moon migration handoff

**Status:** Pre-rebuild checkpoint
**Branch:** `codex/migrate-just-to-moon`
**Base:** `origin/main` at `6a7523aa19fa72ef2767eee7fcdcaf9dcf322971`
**Goal:** Replace Just with Moon as Rulesy's repository task runner before
starting RulesyOS work.

This file exists because rebuilding the development container replaces the
current Codex environment and its conversation history. Read this file before
continuing. Keep working directly on the branch above without creating a
worktree.

## Completed before the rebuild

### Project-local Moon skills

Two complete Agent Skills were vendored under `.agents/skills/`:

- `moon`: general Moon v2 setup, configuration, and migration guidance from
  `hyperb1iss/moonrepo-skill` at
  `574052bd06a265d472461513c9365264334082b4`; registry page:
  <https://www.skills.sh/hyperb1iss/moonrepo-skill/moon>.
- `debug-task`: Moonrepo's official task-debugging skill from Moon `v2.4.5`
  (`84d7413c68576fc65fd86d950aeedf1bb1a789a7`); registry page:
  <https://www.skills.sh/moonrepo/moon/debug-task>.

Their immutable sources and licenses are recorded in
`.agents/skills/SOURCES.md`. The community `moon` skill is useful guidance, but
Moon's official documentation and the installed CLI remain authoritative.

On the first post-rebuild turn, read both `SKILL.md` files completely before
using them:

```text
.agents/skills/moon/SKILL.md
.agents/skills/debug-task/SKILL.md
```

### Moon devcontainer provisioning

Moon is temporarily installed alongside Just. Do not remove Just until every
repository task and caller has a tested Moon replacement.

The Rulesy devcontainer definition now provisions:

```text
moon 2.4.5
moon-exec 2.4.5
```

It uses Moon's official static-musl archives:

```text
x86_64:
  moon_cli-x86_64-unknown-linux-musl.tar.xz
  sha256 627f99ec29e7f52829daef9c48dfb70840313e01980d297d09e58fd9dbe1a6e9

aarch64:
  moon_cli-aarch64-unknown-linux-musl.tar.xz
  sha256 41cca0fcca0a63de1f7c4d94d275f55c2b26ef559bf19cf7d5bbf29c2ae5df53
```

The pinned assets avoid host/container glibc coupling. Both installed binaries
were confirmed with `ldd` to be statically linked.

Files added or changed for provisioning:

```text
.devcontainer/rulesy.yaml
.devcontainer/tool-versions.env
.devcontainer/scripts/moon/check.sh
.devcontainer/scripts/moon/install.sh
.devcontainer/scripts/prerequisites/check.sh
.devcontainer/scripts/prerequisites/install.sh
.devcontainer/scripts/shared/lib.sh
.devcontainer/scripts/tests/run.sh
```

The prerequisite rule now ensures `xz` is available, installing Ubuntu's
`xz-utils` when necessary. The Moon installer verifies the pinned SHA-256,
extracts the `.tar.xz`, and stages `moon` and `moonx` into `/usr/local/bin`.

### Verification already completed

These checks passed before the handoff:

```text
bash -n on all changed provisioning scripts
bash .devcontainer/scripts/tests/run.sh
git diff --check
real x86_64 Moon download, checksum verification, extraction, and installation
bash .devcontainer/scripts/moon/check.sh
moon --version       => moon 2.4.5
moonx --version      => moon-exec 2.4.5
ldd moon/moonx       => statically linked
```

Running the complete devcontainer Rulesy definition confirmed that the new
Moon rule passes. The current container predates the latest bootstrap and is
not a valid full-suite result:

```text
rulesy --version       => rulesy 0.8.0 (the definition requires 0.8.1)
devcontainer --version => 0.87.0 (the definition requires 0.88.0)
codex --version        => 0.144.6 (the definition requires 0.145.0)
```

The current sandbox also makes the installed `rustup 1.29.0` abort while
probing its compiler. Do not diagnose that from this stale container. Rebuild
first and evaluate the fresh lifecycle result.

## Rebuild and resume

Rebuild the development container from this branch using the normal Codex or
VS Code “Rebuild Container” action. The checked-out workspace and uncommitted
branch changes must remain mounted into the new container.

Immediately after the rebuild:

```bash
git branch --show-current
git status --short
rulesy --version
moon --version
moonx --version
bash .devcontainer/scripts/tests/run.sh
rulesy --config=.devcontainer/rulesy.yaml check --fix --non-interactive
rulesy --config=.devcontainer/rulesy.yaml check --non-interactive
```

Expected versions:

```text
rulesy 0.8.1
moon 2.4.5
moon-exec 2.4.5
```

If the lifecycle provisioning fails, fix the provisioning slice before
starting the task migration. Do not silently install an unpinned Moon version,
pipe a mutable installer into a shell, or switch to a dynamically linked GNU
asset.

## Remaining migration

Complete the migration as one end-to-end, reviewable feature.

1. Inspect the installed Moon version and official schemas, then define the
   smallest single-project workspace for this repository's nonstandard layout:
   the repository root owns scripts and `dist/`, while Cargo's manifest is
   `src/Cargo.toml`.
2. Replace every recipe in `justfile` with a tested Moon task:
   `build`, `compile`, `dev`, `cross-compile`, `release`, `test`,
   `get-version`, and `ensure-tag-matches-version`.
3. Preserve `scripts/cross-compile.sh` and `scripts/release.sh` as the
   implementations behind their tasks. Do not redesign release behavior as
   part of the task-runner migration.
4. Disable caching for stateful, dynamic, or destructive tasks until correct
   inputs and outputs are intentionally modeled. In particular, `release`,
   `dev`, dynamic cross-compilation, and tag validation must never be skipped
   because of a cache hit.
5. Keep the existing Rulesy-managed Rustup/Rust toolchain. Do not introduce
   Proto or a second Rust version authority merely because Moon supports
   toolchain management.
6. Replace the release workflow's Just installation and invocations with an
   exactly pinned Moon setup that works on Linux and macOS. Pin any GitHub
   Action by immutable commit.
7. Update CI to run at least one real Moon task and assert the exact Moon
   version on both x86_64 and ARM64 devcontainer paths.
8. Replace Just commands and current-state descriptions in:

   ```text
   README.md
   ARCHITECTURE.md
   CODEMAP.md
   todo.md
   release-procedure.md
   skills/rulesy-workflow/SKILL.md
   skills/rulesy-workflow/references/best-practices-guide.md
   .github/workflows/release.yml
   .devcontainer/devcontainer.json
   ```

   `release-procedure.md` contains adjacent stale GoReleaser and Go-version
   claims; correct those existing false statements while preserving actual
   release semantics.
9. Once no task, workflow, documentation, or test refers to Just, remove:

   ```text
   justfile
   .devcontainer/scripts/just/
   JUST_VERSION and JUST_*_SHA256
   skellock.just
   ```

10. Add the actual Moon cache/runtime paths to `.gitignore` while keeping
    checked-in `.moon` configuration visible.
11. Extend network-free tests for the final closed helper/configuration tree,
    task mapping, argument forwarding, caching policy, version/tag validation,
    and release workflow pins.
12. Run every Moon task that replaces a Just recipe, the complete Rust suite,
    Clippy, shell tests, Rulesy convergence/check-only, documentation-link
    checks, and `git diff --check`.

Do not begin RulesyOS work on this branch. Finish and land the Moon migration
first. Delete this temporary handoff file only after the migration is complete
and its information has been incorporated into permanent documentation and
tests.
