# `skip-if` contract fixtures

This closed, network-free corpus exercises `skip-if` through the compiled
`checksy` binary. [`cases.yaml`](cases.yaml) maps every YAML configuration and
shell asset in this directory to an executable test in
`src/tests/skip_if_contract.rs`. Tests copy the corpus to a temporary directory
before commands create marker files, leaving the checked-in fixtures unchanged.

The corpus treats `skip-if` as a Bash predicate evaluated once after severity
filtering and immediately before the initial check. Exit `0` retains the rule as
skipped and suppresses its check, either repair form, and final check. Any
completed nonzero exit (`1`, `23`, or shell-level `127`) suppresses predicate
output and proceeds with the check. Predicate stdin is `/dev/null`; the inherited
environment, associated check working directory, and rule timeout are reused.
Skipped rules print `⏭️ <name> (skipped)`. A successful run with skips ends in
`😎 All applicable rules validated; N skipped`, while a failing summary appends
`; N skipped`. Runs without skips keep the existing summary unchanged.

Timeout, child-signal, Bash-spawn, and supervision failures are operational:
bounded captured output is retained, no later command runs, the exit is `2`, and
`--no-fail` cannot mask it. Other cases cover preconditions and ordinary rules,
severity filtering, high-severity skips, command and environment gates, explicit
files, auto-discovery, both stdin spellings, check-only and `--fix`, and the
deprecated `diagnose` alias. The `--fix` contract tests serialize through the
same per-user provisioning semaphore helper used by the other compiled-binary
test suites.

All commands use local Bash and make no network requests. Run the suite from the
crate directory:

```text
cargo test --locked --test skip_if_contract
```
