# Deterministic process contract

[cases.yaml](cases.yaml) is the executable P2 process-runner matrix for Linux
and macOS. It maps each contract case to one exact Rust test under
`process_runner::tests`; no case needs public network access.

| Case | Rust test | Contract evidence |
| --- | --- | --- |
| `managed-tree-forced-timeout` | `process_runner::tests::managed_tree_forced_timeout` | A TERM-resistant leader, child, and grandchild remain in one runner-created process group; timeout performs group TERM, grace, group KILL, and leader reap. Distinct child/grandchild state-directory locks become reacquirable after return. |
| `pre-timeout-output-retained` | `process_runner::tests::pre_timeout_output_retained` | A timeout result preserves `PRE_TIMEOUT_STDOUT:<nonce>` and `PRE_TIMEOUT_STDERR:<nonce>` emitted after readiness. |
| `ordinary-nonzero-distinct` | `process_runner::tests::ordinary_nonzero_distinct` | Exit `23` is a completed process, not a timeout, and does not enter TERM/KILL cleanup. |
| `capture-exact-limit` | `process_runner::tests::capture_exact_limit` | Exactly 1,048,576 bytes on each stdout/stderr stream are retained without truncation. |
| `capture-max-plus-one` | `process_runner::tests::capture_max_plus_one` | 1,048,577 bytes on each stdout/stderr stream record the original size, set truncation, and retain 524,288-byte head and tail halves. |
| `continuous-output-drain` | `process_runner::tests::continuous_output_drain` | Continuous output is drained without pipe deadlock or deadline starvation, and timeout metadata remains available. |

Run the mapped tests with:

~~~bash
cd src
cargo test --locked process_runner::tests
~~~

The managed-tree test uses a nonce-bearing readiness handshake to report
process identities, but PID polling is not its liveness authority. The child
and grandchild each hold a distinct `StateDirectoryLock`; reacquiring both locks
after the runner returns proves those managed descendants are dead. Both capture
boundary cases exercise stdout and stderr independently at the same boundary.

## Boundary

The completed P2 guarantee applies to the process group created and supervised
by the runner. A command that deliberately changes process group or session, a
background descendant that closes the inherited capture descriptors before an
ordinary leader exit, and signals received by the Checksy parent are outside
these six executable cases. Those residuals remain documented security limits,
not implied managed-tree guarantees.
