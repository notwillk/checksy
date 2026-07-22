# Process-runner contract fixtures

This closed, network-free corpus exercises the supervised command runner through the
compiled `checksy` binary. [`cases.yaml`](cases.yaml) maps every checked-in YAML
configuration and shell asset to an executable Rust integration test in
`src/tests/process_runner_contract.rs`.

The cases cover `/dev/null` stdin for checks, fixes, final rechecks, and pattern
scripts; pattern-only execution; ordinary compliance failures; spawn, timeout, and
child-signal operational failures; bounded output; fail-fast fix timeouts; parent
signal forwarding; successful leaders with still-running managed descendants;
and validation before execution. Tests copy the corpus to a temporary directory
before running cases that write marker files, so this directory remains unchanged.

All commands are local Bash and require no public network. The parent-interruption
case invokes an ignored helper in the Rust integration-test executable. Its child
and grandchild hold advisory locks, announce readiness with pipe and Unix-datagram
handshakes, and use bounded watchdogs; the test does not use sleeps for readiness.
The TTY case starts Checksy with a real controlling pseudoterminal and proves that a
runner child has neither readable stdin nor access to `/dev/tty`.

Run the complete executable contract from the crate directory:

```text
cargo test --locked --test process_runner_contract
```

These fixtures verify process supervision. They do not make trusted Bash safe or
sandbox it, and a command that deliberately creates a new session escapes the
managed process group.
