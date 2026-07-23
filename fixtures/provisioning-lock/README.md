# Provisioning-lock contract fixtures

This closed, network-free corpus exercises the per-user provisioning semaphore
through the compiled `checksy` binary and focused lock tests.
[`cases.yaml`](cases.yaml) maps every checked-in configuration and asset to an
executable Rust test. Tests copy mutable cases to a temporary directory; the
checked-in corpus remains unchanged.

Every file-backed, auto-discovered, or stdin `check --fix` for one effective
UID uses the same nonblocking lock. The lock is independent of configuration
path, working directory, include graph, and `cachePath`. Check-only runs and
`install` do not take it. Contention is exit `4`, even with `--no-fail`, and no
configured command starts for the losing invocation.

Lock ownership depends on the requested lifecycle, not on whether a repair is
eventually needed: a passing `check --fix` still contends, while the same
configuration in check-only mode remains lock-free.

The marker cases expose the complete check/fix/final-check order through paths
provided by the test environment. The blocking repair announces readiness
through a FIFO and waits on a second FIFO, so another invocation can contend
while the first process is known to hold the lock without sleep-based timing.
The stale-content vector proves that lock-file text is neither interpreted nor
rewritten.

The missing-Git case uses a nonexistent fixture-relative repository and a
fixture-relative legacy cache path. Its losing-contender path must return exit
`4` without printing acquisition progress, creating the cache, or invoking Git.
The same test creates a temporary local repository and uses the indexed
blocking Git wrapper to prove that successful clone/materialization occurs
while the semaphore is held. Neither path has a network endpoint.

The focused primitive tests additionally cover exact platform path selection,
account-database home lookup, `0700`/`0600` permissions, ownership, symlink and
hardlink rejection, special files, same-process ownership, cross-process
release, path aliases, and close-on-exec behavior.

Run the executable contract from the crate directory:

```text
cargo test --locked --test provisioning_lock_contract
```

This semaphore coordinates Checksy provisioning only. It does not sandbox
trusted Bash, make partially applied fixes transactional, or prevent an
unrelated process from modifying the machine without taking the advisory lock.
