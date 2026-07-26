# RulesyOS KVM bootstrap handoff

**Status:** Pre-rebuild checkpoint
**Branch:** `codex/rulesyos-kvm-devcontainer`
**Base:** `origin/main` at `f99aafe44e0011651dce395a97437731a657f484`
**Initial KVM checkpoint:** `19ce61d` (`Expose host KVM safely in the devcontainer`)

This file exists because rebuilding the development container may replace the
current Codex environment and conversation history. Read it before resuming.
Continue on this branch without creating a worktree.

## Completed before the rebuild

The devcontainer now grants Docker device-cgroup access only to character
device `10:232`, without `--privileged`, a `/dev/kvm` bind, or added
capabilities. Rulesy checks whether the host kernel registers KVM in
`/proc/misc`, warns without failing when it does not, and skips device exposure.

On a KVM host, the guarded helper creates `/dev/kvm` as `10:232` and gives the
remote user's primary group read/write access. It refuses to replace symlinks,
non-devices, wrong device numbers, or separately mounted devices. It may repair
permissions only for an exact `10:232` node on the container-owned `/dev`
`tmpfs`. Both container creation and every container start run the complete
Rulesy configuration directly; the KVM rule's `skip-if` handles non-KVM hosts.

These checks passed before this handoff:

```text
bash -n on the changed shell scripts
JSON parsing of .devcontainer/devcontainer.json
bash .devcontainer/scripts/tests/run.sh
/usr/local/bin/rulesy --config=.devcontainer/rulesy.yaml check --non-interactive
git diff --check
```

The current host does not register KVM and has no `/dev/kvm`. Rulesy emitted
one warning, skipped exposure, and exited successfully. The KVM-capable path
must be verified after rebuilding on a KVM host.

## Rebuild and verify

Use the normal Codex or VS Code “Rebuild Container” action. After reconnecting:

```bash
git branch --show-current
git status --short
bash .devcontainer/scripts/tests/run.sh
/usr/local/bin/rulesy --config=.devcontainer/rulesy.yaml check --fix --non-interactive
/usr/local/bin/rulesy --config=.devcontainer/rulesy.yaml check --non-interactive
```

Then distinguish the two supported outcomes:

```bash
if grep -Eq '^[[:space:]]*232[[:space:]]+kvm$' /proc/misc; then
  test -c /dev/kvm
  test "$(stat -c '%t:%T' /dev/kvm)" = a:e8
  test -r /dev/kvm
  test -w /dev/kvm
else
  test ! -e /dev/kvm
  printf 'KVM unavailable: warning-only skip is expected\n'
fi
```

Do not continue until the rebuilt container follows the applicable outcome.

## Non-negotiable test contract

- Use hardware KVM only. QEMU must be invoked with `-accel kvm`.
- Never silently or automatically fall back to TCG.
- If KVM is unavailable, print a clear warning, skip, and exit zero.
- Enforce a boot timeout and always clean up QEMU on success, failure, or signal.
- Keep all VM tests local for now; do not add them to GitHub Actions.

## Remaining checkpoint 1: known-good image

1. Provision only the QEMU packages needed for headless x86-64 KVM testing.
2. Add a serial-console boot harness under `products/rulesyos/`.
3. Download, but do not commit, CirrOS
   `cirros-0.6.3-x86_64-disk.img` from
   <https://download.cirros-cloud.net/0.6.3/>.
4. Verify SHA-256
   `7d6355852aeb6dbcd191bcda7cd74f1536cfe5cbf8a10495a7283a8396e4b75b`
   before every first use of a cached download.
5. Boot read-only or snapshot-backed with `-accel kvm`, headless serial output,
   a bounded timeout, and trap-based cleanup. Pass only after the expected
   CirrOS boot/login marker appears.
6. Add applicable Moon tasks: `lint`, `test`, and `test-known-good`. At this
   checkpoint, `rulesyos:test` runs the known-good image.
7. Commit as an independently repairable known-good harness checkpoint.

## Remaining checkpoint 2: blank Buildroot

1. Pin the official Buildroot LTS `2025.02.16` archive from
   <https://buildroot.org/downloads/> and record and verify its SHA-256.
2. Add a minimal RulesyOS `BR2_EXTERNAL` x86-64 QEMU configuration. It should
   contain only the stock Buildroot kernel, root filesystem, serial console,
   and shell needed to prove the image boots—no RulesyOS functionality.
3. Keep downloaded sources, Buildroot work output, and generated images out of
   Git; add narrow package-local ignores for those paths.
4. Add `rulesyos:build` and switch `rulesyos:test` to the generated blank image.
   Assert the expected Buildroot boot marker and shell over the same KVM-only
   harness. Retain `rulesyos:test-known-good` as a harness diagnostic.
5. Commit the blank Buildroot image as a separate checkpoint.

No `format` task is needed unless format-capable source is introduced. No
`release` task is appropriate at this stage.

## Stop boundary

Stop after the blank Buildroot image boots through the harness. Do not add the
Rulesy binary, stage-zero behavior, configuration/state management, recovery,
hardening, updates, Compose integration, firmware publishing, or any other
actual RulesyOS functionality.

Do not modify GitHub Actions until the user has reviewed the likely billing
impact. Before the eventual PR, implement every remaining requirement or move
it into `todo.md`, then delete this temporary handoff.
