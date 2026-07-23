# P0 acceptance contract

This closed, network-free corpus proves the complete P0 provisioning workflow
through the compiled `checksy` CLI. [`cases.yaml`](cases.yaml) maps each of the
five configurations to its executable scenario in
[`p0_acceptance.rs`](../../src/tests/p0_acceptance.rs). Tests copy configurations
to temporary writable directories; the checked-in corpus remains unchanged.

The core case runs the same configuration from a file and from standard input.
It combines a skipped rule, completed nonzero predicates, an ordinary repair
and final check, and a passing rule whose interactive repair must never probe
for a terminal. Predicate output is intentionally hidden while trace files
prove exact execution order and once-only predicate evaluation.

The interactive case separates the three terminal contracts. A file-backed PTY
run accepts `approve`, repairs, and rechecks. A detached file run and a stdin
configuration run under an otherwise real controlling terminal leave the
interactive repair unapplied, continue to a later rule, and report their
reason-specific compliance failures.

The contention case uses FIFO readiness and release handshakes, never sleeps.
It proves that file and stdin provisioning use the same per-user semaphore in
both holder/contender directions. A losing `--no-fail` contender must exit `4`
before any configured command or progress output.

The timeout case delegates its predicate to an ignored Rust test helper that
forms a TERM-resistant leader, child, and grandchild. Each process holds a
separate advisory lock and announces readiness over a local Unix datagram only
after the complete tree exists. Timeout output is retained, later commands do
not run, and all locks must be immediately reacquirable after cleanup.

The invalid-preflight case puts a marker-producing valid rule before a later
rule with a blank `skip-if`. File, stdin, and already-held-lock runs must reject
the complete configuration before taking the semaphore or executing either
rule.

Run the integrated acceptance gate from the crate directory:

```text
cargo test --locked --test p0_acceptance
```

This corpus is integration evidence for the combined public CLI contract. The
focused strict-configuration, process-runner, interactive-fix,
provisioning-lock, and skip-if corpora remain authoritative for individual
edge cases.
